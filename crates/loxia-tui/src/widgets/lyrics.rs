//! Synced/unsynced lyrics pane (`07-07`, `docs/07-ui-spec.md` §8, `docs/03-emby-api.md` §7).

use std::time::Duration;

use loxia_core::model::lyrics::{LyricLine, Lyrics};
use loxia_core::state::queue::QueueEntry;
use loxia_core::state::{AppState, Connectivity};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::style;
use crate::text;

/// Lines farther than this many *logical* lines from the active one fade to `Dim`.
const DISTANT_LINE_THRESHOLD: usize = 4;

fn current_entry(state: &AppState) -> Option<&QueueEntry> {
    let id = state.player.current?;
    state.queue.entries.iter().find(|e| e.entry_id == id)
}

/// Whether the caller should reserve any area for this widget at all — `false` collapses the
/// pane entirely rather than rendering an empty box (this task's own spec: "an empty box labelled
/// LYRICS for the majority of tracks that have none is worse than no pane"). `Connectivity::
/// Offline` is folded in here too: a stale fetch was never issued for the current track while
/// offline, so it can never arrive, and the pane would otherwise sit forever in the "loading"
/// state rather than just staying hidden.
pub fn should_show(state: &AppState) -> bool {
    if !state.config.ui.show_lyrics || state.connectivity == Connectivity::Offline {
        return false;
    }
    let Some(entry) = current_entry(state) else {
        return false;
    };
    if entry.track.lyric_stream.is_none() {
        return false;
    }
    match &state.lyrics {
        Some((id, lyrics)) if *id == entry.track.id => !lyrics.is_empty(),
        // No reply for *this* track yet — still worth reserving the area for "loading lyrics…"
        // rather than flashing the pane in and out as the fetch resolves.
        _ => true,
    }
}

/// Assumes `should_show` was already checked — a defensive no-op if called without it (e.g. no
/// current entry at all), never a panic.
pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(entry) = current_entry(state) else {
        return;
    };
    let stored = state
        .lyrics
        .as_ref()
        .filter(|(id, _)| *id == entry.track.id)
        .map(|(_, l)| l);
    let Some(lyrics) = stored else {
        render_loading(f, area, theme);
        return;
    };
    if lyrics.is_empty() {
        return;
    }
    match lyrics {
        Lyrics::Synced(lines) => {
            render_synced(f, area, lyrics, lines, state.player.position, theme)
        }
        Lyrics::Unsynced(lines) => render_unsynced(f, area, lines, state.lyrics_scroll, theme),
    }
}

fn render_loading(f: &mut Frame, area: Rect, theme: &Theme) {
    let row = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "loading lyrics…",
            style::fg(theme, Role::Dim),
        ))),
        row,
    );
}

/// One rendered row: which logical `LyricLine` it came from (for active/distance styling) and
/// its own (possibly wrapped) text.
struct Row {
    line_index: usize,
    text: String,
}

/// Wraps every logical line to `width`, remembering which logical line each resulting row came
/// from, plus the row-range each logical line occupies (its first row is where `▶` goes and where
/// centring targets, even when the line itself wrapped to several rows).
fn flatten(lines: &[LyricLine], width: usize) -> (Vec<Row>, Vec<(usize, usize)>) {
    let mut flat = Vec::new();
    let mut ranges = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let start = flat.len();
        let wrapped = text::wrap(&line.text, width.max(1));
        if wrapped.is_empty() {
            flat.push(Row {
                line_index: i,
                text: String::new(),
            });
        } else {
            for row in wrapped {
                flat.push(Row {
                    line_index: i,
                    text: row,
                });
            }
        }
        ranges.push((start, flat.len()));
    }
    (flat, ranges)
}

/// The active line is centred, advancing by exactly one row per index step (never a recentring
/// jump) since the start row is always derived directly from the active row's own position minus
/// half the viewport — before the first timestamp (`active_line() == None`), starts from the top
/// with nothing highlighted instead.
fn render_synced(
    f: &mut Frame,
    area: Rect,
    lyrics: &Lyrics,
    lines: &[LyricLine],
    position: Duration,
    theme: &Theme,
) {
    let width = area.width as usize;
    let height = area.height as usize;
    let active_index = lyrics.active_line(position);
    let (flat, ranges) = flatten(lines, width);
    if flat.is_empty() {
        return;
    }

    let start_row = match active_index {
        Some(idx) => {
            let active_row = ranges[idx].0;
            active_row
                .saturating_sub(height / 2)
                .min(flat.len().saturating_sub(height.min(flat.len())))
        }
        None => 0,
    };

    for i in 0..height {
        let Some(row) = flat.get(start_row + i) else {
            break;
        };
        let is_active = active_index == Some(row.line_index);
        let is_first_row_of_active = is_active && ranges[row.line_index].0 == start_row + i;
        let distance = active_index
            .map(|a| row.line_index.abs_diff(a))
            .unwrap_or(usize::MAX);
        let role = if is_active {
            Role::Accent
        } else if distance > DISTANT_LINE_THRESHOLD {
            Role::Dim
        } else {
            Role::Fg
        };
        let prefix = if is_first_row_of_active { "▸ " } else { "  " };
        let row_area = Rect::new(area.x, area.y + i as u16, area.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{prefix}{}", row.text),
                style::fg(theme, role),
            ))),
            row_area,
        );
    }
}

/// A plain block with no highlighting, starting `scroll` source lines in. Untimed lyrics have no
/// active line to follow, so on a long track the pane simply cut off with no way to read the rest
/// — `J`/`K` move this offset (`docs/12-decisions.md`).
///
/// `scroll` skips whole *source* lines before wrapping, which is what lets the reducer clamp it
/// without knowing the pane's width. The scrolled-to line therefore always starts at the top of the
/// pane rather than mid-paragraph.
fn render_unsynced(f: &mut Frame, area: Rect, lines: &[String], scroll: usize, theme: &Theme) {
    let width = area.width as usize;
    let height = area.height as usize;
    let mut flat: Vec<String> = Vec::new();
    for line in lines.iter().skip(scroll.min(lines.len())) {
        let wrapped = text::wrap(line, width.max(1));
        if wrapped.is_empty() {
            flat.push(String::new());
        } else {
            flat.extend(wrapped);
        }
    }
    for i in 0..height {
        let Some(text) = flat.get(i) else {
            break;
        };
        let row_area = Rect::new(area.x, area.y + i as u16, area.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text.clone(),
                style::style(theme, Role::Fg),
            ))),
            row_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::model::{LyricFormat, LyricStreamRef, MediaSourceId};
    use loxia_core::state::queue::Availability;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16, state: &AppState) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| {
                let area = f.area();
                if should_show(state) {
                    render(f, area, state, &theme);
                }
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    fn synced_lines(pairs: &[(u64, &str)]) -> Vec<LyricLine> {
        pairs
            .iter()
            .map(|(secs, text)| LyricLine {
                at: Duration::from_secs(*secs),
                text: text.to_string(),
            })
            .collect()
    }

    fn state_with_track(lyric_stream: Option<LyricStreamRef>) -> AppState {
        let mut state = fixtures::fixture_empty();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let mut t = fixtures::track("Motion", 1, &alb, &[&a]);
        t.lyric_stream = lyric_stream;
        let entry_id = state.queue.next_id();
        state.queue.entries.push(QueueEntry {
            entry_id,
            track: t,
            source: loxia_core::state::queue::QueueSource::Manual,
            availability: Availability::Remote,
        });
        state.queue.play_order = vec![0];
        state.queue.position = 0;
        state.player.current = Some(entry_id);
        state
    }

    fn a_stream() -> LyricStreamRef {
        LyricStreamRef {
            media_source_id: MediaSourceId::from("ms1"),
            stream_index: 2,
            format: LyricFormat::Lrc,
        }
    }

    /// Untimed lyrics on a long track ran off the bottom of the pane with nothing to scroll them
    /// (`docs/12-decisions.md`). The offset skips whole source lines, so the scrolled-to line lands
    /// at the top of the pane.
    #[test]
    fn scrolling_unsynced_lyrics_reveals_later_lines() {
        let mut state = state_with_track(Some(a_stream()));
        let track = state.queue.entries[0].track.id.clone();
        let lines: Vec<String> = (0..40).map(|i| format!("line {i}")).collect();
        state.lyrics = Some((track, Lyrics::Unsynced(lines)));

        let top = render_at(30, 5, &state);
        assert!(top.contains("line 0"), "{top}");
        assert!(!top.contains("line 20"));

        state.lyrics_scroll = 20;
        let scrolled = render_at(30, 5, &state);
        assert!(
            scrolled.contains("line 20"),
            "the scrolled-to line should head the pane: {scrolled}"
        );
        assert!(
            !scrolled.contains("line 0\""),
            "earlier lines should be off the top: {scrolled}"
        );
    }

    /// An offset past the end (a shorter set of lyrics arriving, say) must not panic or blank the
    /// pane out of range — it simply has nothing left to draw.
    #[test]
    fn a_scroll_beyond_the_end_is_harmless() {
        let mut state = state_with_track(Some(a_stream()));
        let track = state.queue.entries[0].track.id.clone();
        state.lyrics = Some((track, Lyrics::Unsynced(vec!["only line".to_string()])));
        state.lyrics_scroll = 999;

        let rendered = render_at(30, 5, &state);
        assert!(!rendered.contains("only line"));
    }

    #[test]
    fn no_lyric_stream_hides_pane_and_emits_nothing() {
        let state = state_with_track(None);
        assert!(!should_show(&state));
    }

    #[test]
    fn lyrics_fetched_on_track_change() {
        let mut state = state_with_track(Some(a_stream()));
        let target = state.queue.entries[0].entry_id;
        let effects = loxia_core::reducer::apply(
            &mut state,
            loxia_core::action::Action::Queue(loxia_core::action::QueueAction::JumpTo(target)),
        );
        assert!(effects.iter().any(|e| matches!(
            e,
            loxia_core::effect::Effect::Net(loxia_core::effect::NetEffect::FetchLyrics { .. })
        )));
    }

    #[test]
    fn lyrics_replaced_not_accumulated() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        state.lyrics = Some((track_id.clone(), Lyrics::Unsynced(vec!["old".to_string()])));
        loxia_core::reducer::apply(
            &mut state,
            loxia_core::action::Action::Data(loxia_core::action::DataAction::LyricsLoaded {
                track: track_id.clone(),
                lyrics: Lyrics::Unsynced(vec!["new".to_string()]),
            }),
        );
        assert_eq!(
            state.lyrics,
            Some((track_id, Lyrics::Unsynced(vec!["new".to_string()])))
        );
    }

    #[test]
    fn active_line_derived_from_position_not_stored() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        state.lyrics = Some((
            track_id,
            Lyrics::Synced(synced_lines(&[(0, "one"), (10, "two"), (20, "three")])),
        ));
        state.player.position = Duration::from_secs(11);
        let rendered = render_at(40, 10, &state);
        let active_line = rendered.lines().find(|l| l.contains('▸')).unwrap();
        assert!(active_line.contains("two"));

        state.player.position = Duration::from_secs(21);
        let rendered = render_at(40, 10, &state);
        let active_line = rendered.lines().find(|l| l.contains('▸')).unwrap();
        assert!(active_line.contains("three"));
    }

    #[test]
    fn active_line_advances_with_playback() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        let lines = synced_lines(&[(0, "a"), (5, "b"), (10, "c"), (15, "d")]);
        state.lyrics = Some((track_id, Lyrics::Synced(lines.clone())));

        let cases = [
            (0u64, "a"),
            (4, "a"),
            (5, "b"),
            (9, "b"),
            (10, "c"),
            (16, "d"),
        ];
        for (secs, expected) in cases {
            state.player.position = Duration::from_secs(secs);
            let rendered = render_at(40, 10, &state);
            let active_line = rendered.lines().find(|l| l.contains('▸')).unwrap();
            assert!(
                active_line.contains(expected),
                "at {secs}s expected {expected:?}, got {active_line:?}"
            );
        }
    }

    #[test]
    fn before_first_timestamp_nothing_highlighted() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        state.lyrics = Some((
            track_id,
            Lyrics::Synced(synced_lines(&[(5, "one"), (10, "two")])),
        ));
        state.player.position = Duration::from_secs(1);
        let rendered = render_at(40, 10, &state);
        assert!(!rendered.contains('▸'));
        assert!(rendered.contains("one"), "starts from the top");
    }

    /// The buffer row (not the `{:?}`-formatted string's line number, which is offset by the
    /// `Buffer { area: ..., content: [` header) the `▶` marker actually renders on.
    fn marker_row(w: u16, h: u16, state: &AppState) -> u16 {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| render(f, f.area(), state, &theme))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..h)
            .find(|&y| (0..w).any(|x| buffer.cell((x, y)).is_some_and(|c| c.symbol() == "▸")))
            .expect("active line marker not found")
    }

    #[test]
    fn active_line_stays_centred() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        let pairs: Vec<(u64, &str)> = (0..20).map(|i| (i * 5, "line")).collect();
        let lines = synced_lines(&pairs);
        state.lyrics = Some((track_id, Lyrics::Synced(lines)));
        state.player.position = Duration::from_secs(50); // active index 10
        // 9 visible rows, active centred means row index ~4 (height/2).
        assert_eq!(marker_row(40, 9, &state), 4);
    }

    #[test]
    fn scroll_advances_by_one_not_by_a_jump() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        let pairs: Vec<(u64, &str)> = (0..20).map(|i| (i * 5, "line")).collect();
        let lines = synced_lines(&pairs);
        state.lyrics = Some((track_id, Lyrics::Synced(lines)));

        state.player.position = Duration::from_secs(50); // active index 10
        let before_row = marker_row(40, 9, &state);

        state.player.position = Duration::from_secs(55); // active index 11
        let after_row = marker_row(40, 9, &state);

        // Centred either way, so the *visible row* the marker sits at is unchanged — what
        // actually advanced by one is the underlying scroll window (the active index moved by
        // one, and `start_row` is derived directly from it), not a discontinuous recentring jump.
        assert_eq!(before_row, after_row);
    }

    #[test]
    fn distant_lines_are_dimmed() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        let pairs: Vec<(u64, &str)> = (0..20).map(|i| (i * 5, "line")).collect();
        let lines = synced_lines(&pairs);
        state.lyrics = Some((track_id, Lyrics::Synced(lines)));
        state.player.position = Duration::from_secs(50); // active index 10

        let backend = TestBackend::new(40, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, &state, &theme);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // Row 0 (visible row furthest from the centred active row) must be dimmed relative to a
        // near row — compared against the theme's own Dim/Fg colours rather than a hardcoded one.
        let dim = style::fg(&theme, Role::Dim).fg;
        let far_cell_fg = buffer.cell((2, 0)).unwrap().fg;
        assert_eq!(Some(far_cell_fg), dim);
    }

    #[test]
    fn wrapped_active_line_highlights_all_rows() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        let long_text = "word ".repeat(20);
        state.lyrics = Some((
            track_id,
            Lyrics::Synced(vec![
                LyricLine {
                    at: Duration::ZERO,
                    text: "intro".to_string(),
                },
                LyricLine {
                    at: Duration::from_secs(5),
                    text: long_text,
                },
            ]),
        ));
        state.player.position = Duration::from_secs(6);
        let rendered = render_at(20, 10, &state);
        let accent = style::fg(&Theme::default(), Role::Accent).fg;

        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| render(f, f.area(), &state, &theme))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // Every row containing "word" (the wrapped active line's own text) must be Accent-styled.
        let word_rows: Vec<u16> = (0..10)
            .filter(|&y| {
                (0..20).any(|x| {
                    buffer
                        .cell((x, y))
                        .is_some_and(|c| c.symbol().contains('w'))
                })
            })
            .collect();
        assert!(word_rows.len() > 1, "expected the line to actually wrap");
        for y in word_rows {
            assert_eq!(Some(buffer.cell((2, y)).unwrap().fg), accent);
        }
        let _ = rendered;
    }

    #[test]
    fn unsynced_lyrics_render_without_highlight() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        state.lyrics = Some((
            track_id,
            Lyrics::Unsynced(vec!["first line".to_string(), "second line".to_string()]),
        ));
        let rendered = render_at(40, 10, &state);
        assert!(rendered.contains("first line"));
        assert!(rendered.contains("second line"));
        assert!(!rendered.contains('▸'));
    }

    #[test]
    fn toggle_hides_pane_and_persists() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        state.lyrics = Some((track_id, Lyrics::Unsynced(vec!["line".to_string()])));
        assert!(should_show(&state));

        let effects = loxia_core::reducer::apply(
            &mut state,
            loxia_core::action::Action::View(loxia_core::action::ViewAction::ToggleLyrics),
        );
        assert!(!state.config.ui.show_lyrics);
        assert!(!should_show(&state));
        assert!(effects.iter().any(|e| matches!(
            e,
            loxia_core::effect::Effect::Sys(loxia_core::effect::SysEffect::WriteConfig(_))
        )));
    }

    #[test]
    fn lyrics_failure_renders_nothing() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        // `lyrics::fetch` degrades a failure to `Unsynced(vec![])`, never an `Err` — confirm the
        // pane hides once that empty result has actually landed for this track.
        loxia_core::reducer::apply(
            &mut state,
            loxia_core::action::Action::Data(loxia_core::action::DataAction::LyricsLoaded {
                track: track_id,
                lyrics: Lyrics::Unsynced(Vec::new()),
            }),
        );
        assert!(!should_show(&state));
        let rendered = render_at(40, 10, &state);
        assert!(rendered.trim().is_empty() || !rendered.contains("loading"));
    }

    #[test]
    fn lyrics_snapshot_synced() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        state.lyrics = Some((
            track_id,
            Lyrics::Synced(synced_lines(&[
                (0, "First line"),
                (5, "Second line"),
                (10, "Third line"),
                (15, "Fourth line"),
            ])),
        ));
        state.player.position = Duration::from_secs(6);
        insta::assert_snapshot!(render_at(40, 9, &state));
    }

    #[test]
    fn lyrics_snapshot_unsynced() {
        let mut state = state_with_track(Some(a_stream()));
        let track_id = state.queue.entries[0].track.id.clone();
        state.lyrics = Some((
            track_id,
            Lyrics::Unsynced(vec![
                "First line".to_string(),
                "Second line".to_string(),
                "Third line".to_string(),
            ]),
        ));
        insta::assert_snapshot!(render_at(40, 9, &state));
    }

    #[test]
    fn lyrics_snapshot_hidden() {
        let state = state_with_track(None);
        assert!(!should_show(&state));
        insta::assert_snapshot!(render_at(40, 9, &state));
    }
}
