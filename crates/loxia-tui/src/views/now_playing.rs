//! Now Playing tab (`07-06`, `docs/07-ui-spec.md` §8): the queue (or history) on the left, track
//! detail and transport on the right.

use loxia_core::keymap::ActionId;
use loxia_core::state::queue::QueueSource;
use loxia_core::state::{AppState, NowPlayingSub};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget};
use crate::style;
use crate::text;

/// Below this width the right pane collapses entirely and the left pane takes the full canvas —
/// the queue is the more useful of the two when space is scarce.
const RIGHT_PANE_MIN_TOTAL_WIDTH: u16 = 100;
/// Left pane's share of the canvas once both panes fit.
const LEFT_PANE_PERCENT: u32 = 45;

pub fn render(
    f: &mut Frame,
    canvas: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
    art: &mut crate::widgets::album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    if canvas.width == 0 || canvas.height == 0 {
        return Vec::new();
    }

    if canvas.width < RIGHT_PANE_MIN_TOTAL_WIDTH {
        render_left_pane(f, canvas, state, theme, hits);
        return Vec::new();
    }

    let left_w = ((u32::from(canvas.width) * LEFT_PANE_PERCENT) / 100) as u16;
    let left_area = Rect::new(canvas.x, canvas.y, left_w, canvas.height);
    let right_area = Rect::new(
        canvas.x + left_w,
        canvas.y,
        canvas.width - left_w,
        canvas.height,
    );
    render_left_pane(f, left_area, state, theme, hits);
    render_right_pane(f, right_area, state, theme, hits, art)
}

fn format_mmss(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// The first row index to render so `cursor` stays on screen when the list is taller than `height`.
/// Centres the cursor (clamped at both ends) — a real bug found in the field: the Queue/History
/// panes rendered from index 0 with a bare `.take(height)`, so anything past the first screenful
/// (including the current track once the queue grew) was simply invisible and unreachable on screen
/// even as `↑`/`↓` moved an off-screen cursor over it (`docs/12-decisions.md`).
/// The first visible row: the offset the reducer stores, clamped into range.
///
/// It used to be derived from the cursor by centring it, with no stored offset at all — so every
/// cursor move recentred the list, a click scrolled the row out from under the pointer, and the
/// second click of a double-click landed on a different track (`docs/12-decisions.md`).
///
/// The cursor is still *nudged* into view here, by the minimum amount and never by centring,
/// because the reducer maintains the offset against a fixed `ASSUMED_VIEWPORT_ROWS` rather than
/// this pane's real height — on a much shorter pane the stored offset can leave the cursor just
/// off-screen.
fn scroll_start(len: usize, cursor: usize, height: usize, stored: usize) -> usize {
    if height == 0 || len <= height {
        return 0;
    }
    let max_start = len - height;
    let start = stored.min(max_start);
    if cursor < start {
        cursor
    } else if cursor >= start + height {
        (cursor + 1 - height).min(max_start)
    } else {
        start
    }
}

/// The queue pane's title.
///
/// Every row used to carry a `[album]`/`[artist]`/`[manual]` source badge. It repeated the same
/// word down the whole list and told the user nothing they did not already know from having queued
/// it — the one case where the source is genuinely worth stating is an instant mix, whose seed is
/// otherwise invisible. So the badge is gone from the rows and the mix is named once, here, where
/// "what is this queue?" belongs (`docs/12-decisions.md`).
///
/// The title only claims a mix while the queue is *still* that mix: a mix replaces the queue
/// wholesale, so anything appended afterwards makes it something else, and every entry's source is
/// checked rather than trusting the stored name to have been cleared.
fn left_pane_title(state: &AppState) -> String {
    let hint = state.keymap.hint_for(ActionId::ToggleHistory);
    match state.now_playing_subview {
        NowPlayingSub::Queue => match mix_title(state) {
            Some(title) => format!("{title}  [{hint}] History"),
            None => format!("PLAY QUEUE  [{hint}] History"),
        },
        NowPlayingSub::History => format!("HISTORY  [{hint}] Queue"),
    }
}

fn mix_title(state: &AppState) -> Option<String> {
    let name = state.queue.mix_name.as_deref()?;
    let all_mix = !state.queue.entries.is_empty()
        && state
            .queue
            .entries
            .iter()
            .all(|e| matches!(e.source, QueueSource::InstantMix { .. }));
    all_mix.then(|| format!("MIX FOR {name}"))
}

fn render_left_pane(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::border(theme, true))
        .title(left_pane_title(state));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Before the rows, so a click still resolves to the `QueueEntry` beneath the pointer (`hit`
    // takes the *last* match) while the wheel has a target everywhere in the pane.
    hits.push(inner, HitTarget::NowPlayingList);

    match state.now_playing_subview {
        NowPlayingSub::Queue => render_queue_rows(f, inner, state, theme, hits),
        NowPlayingSub::History => render_history_rows(f, inner, state, theme, hits),
    }
}

fn render_queue_rows(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
) {
    if state.queue.play_order.is_empty() {
        let hint = state.keymap.hint_for(ActionId::QueueArtistOnly);
        let message = format!("queue is empty — press {hint} on an album to start");
        render_empty_message(f, area, &message, theme);
        return;
    }
    let width = area.width as usize;
    let start = scroll_start(
        state.queue.play_order.len(),
        state.now_playing_cursor,
        area.height as usize,
        state.now_playing_scroll,
    );
    for (i, &entry_index) in state
        .queue
        .play_order
        .iter()
        .enumerate()
        .skip(start)
        .take(area.height as usize)
    {
        let Some(entry) = state.queue.entries.get(entry_index) else {
            continue;
        };
        let row_area = Rect::new(area.x, area.y + (i - start) as u16, area.width, 1);
        hits.push(row_area, HitTarget::QueueEntry(entry.entry_id));

        let is_current = i == state.queue.position;
        let is_cursor = i == state.now_playing_cursor;
        let marker = if is_current { "▸" } else { " " };
        let role = if is_current {
            Role::Accent
        } else if i < state.queue.position {
            Role::Dim
        } else {
            Role::Fg
        };
        let base_style = if is_cursor {
            style::selection(theme, true)
        } else {
            style::fg(theme, role)
        };

        let track = &entry.track;
        let artist = track.artist_names.join(", ");
        let duration = format_mmss(track.duration);
        // A fixed-width slot either way, so favouriting a row never shifts the column beside it.
        // Without *some* marker here, `f` in this view changes nothing on screen and reads as a
        // no-op — which is exactly how it read while it genuinely was one
        // (`docs/12-decisions.md`).
        let favourite = if track.is_favorite { "♡ " } else { "  " };
        let right = format!("{favourite}{artist}  {duration}");
        let right_w = text::width(&right);

        let left_prefix = format!("{marker} {}. ", i + 1);
        let name_budget = width.saturating_sub(text::width(&left_prefix) + right_w + 1);
        let name = text::ellipsize(&track.name, name_budget);
        let left = format!("{left_prefix}{name}");
        let padded = text::pad_to(&left, width.saturating_sub(right_w));
        let line_text = format!("{padded}{right}");

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(line_text, base_style))),
            row_area,
        );
    }
}

fn render_history_rows(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    _hits: &mut HitMap,
) {
    if state.history.is_empty() {
        render_empty_message(f, area, "nothing played yet", theme);
        return;
    }
    let width = area.width as usize;
    let sorted = state.history_sorted();
    let start = scroll_start(
        sorted.len(),
        state.now_playing_cursor,
        area.height as usize,
        state.now_playing_scroll,
    );
    for (i, entry) in sorted
        .iter()
        .enumerate()
        .skip(start)
        .take(area.height as usize)
    {
        let row_area = Rect::new(area.x, area.y + (i - start) as u16, area.width, 1);
        let is_cursor = i == state.now_playing_cursor;
        let base_style = if is_cursor {
            style::selection(theme, true)
        } else {
            style::style(theme, Role::Fg)
        };

        let (hour, minute) = loxia_core::local_hour_minute(entry.played_at);
        let duration = format_mmss(entry.track.duration);
        let artist = entry.track.artist_names.join(", ");
        let left = format!("• {hour:02}:{minute:02}  {artist} — {}", entry.track.name);
        let right_w = text::width(&duration);
        let name_budget = width.saturating_sub(right_w + 1);
        let left = text::ellipsize(&left, name_budget);
        let padded = text::pad_to(&left, width.saturating_sub(right_w));
        let line_text = format!("{padded}{duration}");

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(line_text, base_style))),
            row_area,
        );
    }
}

fn render_right_pane(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    _hits: &mut HitMap,
    art: &mut crate::widgets::album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    let mut effects: Vec<loxia_core::effect::Effect> = Vec::new();
    if area.width == 0 || area.height == 0 {
        return effects;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::border(theme, false));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return effects;
    }

    let Some(entry_id) = state.player.current else {
        render_nothing_playing(f, inner, theme);
        return effects;
    };
    let Some(entry) = state.queue.entries.iter().find(|e| e.entry_id == entry_id) else {
        render_nothing_playing(f, inner, theme);
        return effects;
    };
    let track = &entry.track;

    let mut y = inner.y;
    let row = |y: u16| Rect::new(inner.x, y, inner.width, 1);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            track.name.clone(),
            style::fg(theme, Role::Accent).add_modifier(Modifier::BOLD),
        ))),
        row(y),
    );
    y += 1;
    if y >= inner.y + inner.height {
        return effects;
    }

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            track.artist_names.join(", "),
            style::style(theme, Role::Fg),
        ))),
        row(y),
    );
    y += 1;
    if y >= inner.y + inner.height {
        return effects;
    }

    let year = track
        .year
        .map(|y| y.to_string())
        .unwrap_or_else(|| "—".to_string());
    let genre = track
        .genres
        .first()
        .cloned()
        .unwrap_or_else(|| "—".to_string());
    let meta = format!("Album: {} ({year}) | Genre: {genre}", track.album_name);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(meta, style::fg(theme, Role::Dim)))),
        row(y),
    );
    y += 2;
    if y >= inner.y + inner.height {
        return effects;
    }

    // The progress bar and transport controls used to live here too — both now sit in the global
    // player bar, where they are visible from *every* tab rather than only this one, and the space
    // they freed goes to album art below (`docs/12-decisions.md`).

    // `07-07`: hidden entirely (no reserved area at all) when the pane is off, the track has no
    // lyric stream, or a fetch resolved to genuinely nothing — "an empty box labelled LYRICS for
    // the majority of tracks that have none is worse than no pane" (this task's own spec).
    let mut remaining = (inner.y + inner.height).saturating_sub(y);
    let show_lyrics = crate::widgets::lyrics::should_show(state);
    // The art's share is reserved from whether this *track has* a lyric stream, never from whether
    // the pane is currently toggled on — keying it off the toggle made the cover jump and resize
    // every time lyrics were shown or hidden (`docs/12-decisions.md`). Toggling now only fills or
    // empties the area beneath it, leaving the art exactly where it was.
    let track_has_lyrics = state
        .current_entry()
        .is_some_and(|e| e.track.lyric_stream.is_some());

    if remaining > 0 {
        let art_rows = if track_has_lyrics {
            remaining / 2
        } else {
            remaining
        };
        if art_rows > 0 {
            let art_area = crate::widgets::album_art::art_rect(
                crate::widgets::album_art::ArtSizeContext::Inspector,
                Rect::new(inner.x, y, inner.width, art_rows),
            );
            effects.extend(crate::widgets::album_art::render(
                f,
                art_area,
                crate::widgets::album_art::ArtSizeContext::Inspector,
                art,
                current_media_item(state).as_ref(),
                theme,
            ));
            y += art_rows;
            remaining = remaining.saturating_sub(art_rows);
        }
    }

    // `07-07`: hidden entirely (no reserved area at all) when the pane is off, the track has no
    // lyric stream, or a fetch resolved to genuinely nothing — "an empty box labelled LYRICS for
    // the majority of tracks that have none is worse than no pane" (this task's own spec).
    if show_lyrics && remaining > 0 {
        crate::widgets::lyrics::render(
            f,
            Rect::new(inner.x, y, inner.width, remaining),
            state,
            theme,
        );
    }
    effects
}

/// The currently-playing entry as a `MediaItem`, which is what `album_art` resolves an image id
/// from. Built here rather than stored: the queue already holds the `Track`.
fn current_media_item(state: &AppState) -> Option<loxia_core::model::MediaItem> {
    let entry_id = state.player.current?;
    let entry = state
        .queue
        .entries
        .iter()
        .find(|e| e.entry_id == entry_id)?;
    Some(loxia_core::model::MediaItem::Track(entry.track.clone()))
}

fn render_empty_message(f: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let lines = text::wrap(message, area.width as usize);
    let total = lines.len() as u16;
    let start_y = area.y + area.height.saturating_sub(total) / 2;
    for (i, line) in lines.iter().enumerate() {
        let y = start_y + i as u16;
        if y >= area.y + area.height {
            break;
        }
        let row = Rect::new(area.x, y, area.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                line.clone(),
                style::fg(theme, Role::Dim),
            )))
            .alignment(Alignment::Center),
            row,
        );
    }
}

fn render_nothing_playing(f: &mut Frame, area: Rect, theme: &Theme) {
    let row_y = area.y + area.height / 2;
    let row = Rect::new(area.x, row_y, area.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "nothing playing",
            style::fg(theme, Role::Dim),
        )))
        .alignment(Alignment::Center),
        row,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::keymap::KeyMap;
    use loxia_core::state::nav::Tab;
    use loxia_core::state::queue::HistoryEntry;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::Duration;

    fn render_at(w: u16, h: u16, state: &AppState) -> (String, HitMap) {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                let mut off = crate::widgets::album_art::ArtOff::default();
                render(f, area, state, &theme, &mut hits, &mut off.art());
            })
            .unwrap();
        (format!("{:?}", terminal.backend().buffer()), hits)
    }

    fn now_playing_state() -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        state.nav.active_tab = Tab::NowPlaying;
        state.keymap = KeyMap::defaults();
        state.player.duration = Duration::from_secs(211);
        state.player.position = Duration::from_secs(90);
        state
    }

    #[test]
    fn queue_numbering_follows_play_order_when_shuffled() {
        let mut state = now_playing_state();
        // Reverse the play order without touching `entries` — numbering must follow this, not
        // the underlying entry index.
        state.queue.play_order.reverse();
        state.queue.position = 0;
        let first_entry_id = state.queue.entries[state.queue.play_order[0]].entry_id;
        let (rendered, hits) = render_at(120, 30, &state);
        assert!(rendered.contains("1. Track 10"), "{rendered}");
        // The first queue row sits at the left pane's first inner row: y=1 (inside the top
        // border), somewhere along its width.
        assert!(
            (0..40).any(|x| matches!(
                hits.hit(x, 1),
                Some(HitTarget::QueueEntry(id)) if *id == first_entry_id
            )),
            "missing QueueEntry hit target for the first row"
        );
    }

    #[test]
    fn queue_scrolls_to_keep_the_cursor_visible() {
        // 10 entries, a viewport far too short to show them all — with the cursor near the end, the
        // last track must be on screen and the first must have scrolled off (the bug: the pane
        // rendered from index 0 and clipped everything past the first screenful).
        let mut state = now_playing_state();
        state.now_playing_cursor = 9;
        // Height 8: after the pane's top/bottom borders + the player bar, only a few rows remain.
        let (rendered, _) = render_at(120, 8, &state);
        assert!(
            rendered.contains("Track 10"),
            "the cursor's row must be visible: {rendered}"
        );
        assert!(
            !rendered.contains("1. Track 1 "),
            "the first row must have scrolled off: {rendered}"
        );
    }

    #[test]
    fn scroll_start_honours_the_stored_offset_and_clamps() {
        assert_eq!(scroll_start(5, 0, 10, 0), 0, "fits entirely -> no scroll");
        assert_eq!(scroll_start(100, 0, 20, 0), 0, "cursor at top clamps to 0");
        assert_eq!(
            scroll_start(100, 99, 20, 999),
            80,
            "a stored offset past the end clamps to the last page"
        );
    }

    /// The whole point of storing the offset: a cursor that is already on screen must not move the
    /// view. Clicking a row used to recentre the list, sliding it out from under the pointer so the
    /// second click of a double-click landed on a different track (`docs/12-decisions.md`).
    #[test]
    fn a_visible_cursor_never_moves_the_view() {
        // Rows 40..59 are on screen; every cursor within them keeps the offset at 40.
        for cursor in 40..60 {
            assert_eq!(
                scroll_start(100, cursor, 20, 40),
                40,
                "cursor {cursor} is already visible and must not scroll the pane"
            );
        }
        // Only a cursor genuinely off-screen nudges it, and by the minimum.
        assert_eq!(scroll_start(100, 60, 20, 40), 41, "one row past the bottom");
        assert_eq!(scroll_start(100, 39, 20, 40), 39, "one row above the top");
    }

    #[test]
    fn current_entry_marked_and_styled() {
        let state = now_playing_state();
        let (rendered, _) = render_at(120, 30, &state);
        let current_line = rendered
            .lines()
            .find(|l| l.contains('▸'))
            .expect("current entry marker not found");
        assert!(current_line.contains(&state.queue.entries[state.queue.play_order[3]].track.name));
    }

    #[test]
    fn played_entries_dimmed() {
        let state = now_playing_state();
        assert_eq!(state.queue.position, 3, "fixture assumption");
        let (rendered, _) = render_at(120, 30, &state);
        // Entries before position 3 (Track 1..3) must render, and this is exercised more directly
        // at the styling layer than the plain-text buffer can assert — this locks in that the
        // rows themselves are present and in the right order.
        assert!(rendered.contains("1. Track 1"));
        assert!(rendered.contains("4. Track 4"));
    }

    #[test]
    /// The per-row source badge is gone: it repeated one word down the whole list and said nothing
    /// the user did not already know from having queued it (`docs/12-decisions.md`).
    fn no_per_row_source_badge() {
        let mut state = now_playing_state();
        state.queue.entries[0].source = QueueSource::Playlist {
            id: loxia_core::model::PlaylistId::from("p1"),
        };
        let (rendered, _) = render_at(120, 30, &state);
        for badge in ["[playlist]", "[mix]", "[album]", "[artist]", "[manual]"] {
            assert!(!rendered.contains(badge), "{badge} still rendered");
        }
    }

    /// Without a marker on the row, `f` in this view changes nothing visible — which is exactly how
    /// it looked while it genuinely did nothing (`docs/12-decisions.md`). The slot is fixed width
    /// so favouriting never shifts the columns beside it.
    #[test]
    fn a_favourited_queue_row_is_marked_without_shifting_the_row() {
        let mut state = now_playing_state();
        let plain = render_at(120, 30, &state).0;
        // Columns, not bytes — `♡` is three bytes wide and one cell wide.
        let artist_col = |s: &str| {
            s.lines()
                .find_map(|l| {
                    l.find("Boy Harsher")
                        .map(|byte| crate::text::width(&l[..byte]))
                })
                .expect("artist column")
        };

        let index = state.queue.play_order[0];
        state.queue.entries[index].track.is_favorite = true;
        let marked = render_at(120, 30, &state).0;

        assert!(marked.contains('♡'), "no favourite marker: {marked}");
        assert_eq!(
            artist_col(&plain),
            artist_col(&marked),
            "the columns beside the marker must not move"
        );
    }

    /// The one source worth naming is an instant mix, whose seed is otherwise invisible — said once
    /// in the pane title rather than on every row.
    #[test]
    fn an_instant_mix_names_its_seed_in_the_title() {
        let mut state = now_playing_state();
        let (rendered, _) = render_at(120, 30, &state);
        assert!(rendered.contains("PLAY QUEUE"));
        assert!(!rendered.contains("MIX FOR"));

        for entry in &mut state.queue.entries {
            entry.source = QueueSource::InstantMix {
                seed: loxia_core::model::ItemId::from("seed"),
            };
        }
        state.queue.mix_name = Some("Boy Harsher".to_string());
        let (rendered, _) = render_at(120, 30, &state);
        assert!(rendered.contains("MIX FOR Boy Harsher"), "{rendered}");
        assert!(!rendered.contains("PLAY QUEUE"));

        // Queue something else on top and it stops being that mix.
        state.queue.entries[0].source = QueueSource::Manual;
        let (rendered, _) = render_at(120, 30, &state);
        assert!(rendered.contains("PLAY QUEUE"));
        assert!(!rendered.contains("MIX FOR"));
    }

    #[test]
    fn right_pane_collapses_under_100_columns() {
        let state = now_playing_state();
        let wide = render_at(120, 30, &state).0;
        assert!(wide.contains("nothing playing") || wide.matches('┌').count() >= 2);
        let narrow = render_at(99, 30, &state).0;
        // Only the left pane's own single bordered block — no second, right-pane border.
        let border_count = narrow.matches("┌").count();
        assert_eq!(
            border_count, 1,
            "right pane must not render under 100 columns"
        );
    }

    #[test]
    fn nothing_playing_state() {
        let mut state = now_playing_state();
        state.player.current = None;
        let (rendered, _) = render_at(120, 30, &state);
        assert!(rendered.contains("nothing playing"));
    }

    /// `10-13`: `empty_states_use_keymap_hints` (the queue half).
    #[test]
    fn queue_empty_state_shows_keymap_hint() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::NowPlaying;
        state.keymap = KeyMap::defaults();
        let (rendered, _) = render_at(120, 30, &state);
        assert!(rendered.contains("queue is empty"));
        let hint = state.keymap.hint_for(ActionId::QueueArtistOnly);
        assert!(rendered.contains(&hint));
    }

    #[test]
    fn history_empty_state() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::NowPlaying;
        state.keymap = KeyMap::defaults();
        state.now_playing_subview = NowPlayingSub::History;
        let (rendered, _) = render_at(120, 30, &state);
        assert!(rendered.contains("nothing played yet"));
    }

    #[test]
    fn history_toggle_and_border_hint() {
        let mut state = now_playing_state();
        let (rendered, _) = render_at(120, 30, &state);
        assert!(rendered.contains("PLAY QUEUE"));
        assert!(rendered.contains("History"));

        state.now_playing_subview = NowPlayingSub::History;
        let (rendered, _) = render_at(120, 30, &state);
        assert!(rendered.contains("HISTORY"));
        assert!(rendered.contains("Queue"));
    }

    fn history_state() -> AppState {
        let mut state = now_playing_state();
        state.now_playing_subview = NowPlayingSub::History;
        state.history.clear();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        for n in 0..3u32 {
            state.history.push_back(HistoryEntry {
                track: fixtures::track(&format!("Hist {n}"), n, &alb, &[&a]),
                played_at: fixtures::fixed_epoch() + Duration::from_secs(u64::from(n) * 60),
                completed: true,
            });
        }
        state
    }

    #[test]
    fn history_is_newest_first() {
        let state = history_state();
        let (rendered, _) = render_at(120, 30, &state);
        let hist2_pos = rendered.find("Hist 2").unwrap();
        let hist0_pos = rendered.find("Hist 0").unwrap();
        assert!(
            hist2_pos < hist0_pos,
            "the newest entry (Hist 2) must render above the oldest (Hist 0)"
        );
    }

    #[test]
    fn requeue_from_history() {
        use loxia_core::action::{Action, QueueAction};
        let mut state = history_state();
        let sorted = state.history_sorted();
        let newest = sorted[0].track.clone();
        let before_len = state.queue.entries.len();

        loxia_core::reducer::apply(
            &mut state,
            Action::Queue(QueueAction::RequeueTrack(Box::new(newest.clone()))),
        );
        assert_eq!(state.queue.entries.len(), before_len + 1);
        assert!(
            state
                .queue
                .entries
                .iter()
                .any(|e| e.track.id == newest.id && e.source == QueueSource::Manual)
        );
    }

    #[test]
    fn auto_scroll_follows_current_track() {
        let mut state = now_playing_state();
        state.now_playing_cursor = 0;
        state.now_playing_user_scrolled = true;
        // A fresh load (JumpTo) is the "track change" moment that resumes auto-follow.
        let target = state.queue.entries[state.queue.play_order[1]].entry_id;
        loxia_core::reducer::apply(
            &mut state,
            loxia_core::action::Action::Queue(loxia_core::action::QueueAction::JumpTo(target)),
        );
        assert!(!state.now_playing_user_scrolled);
        assert_eq!(state.now_playing_cursor, state.queue.position);
    }

    #[test]
    fn auto_scroll_suppressed_while_user_scrolling() {
        let mut state = now_playing_state();
        let before_cursor = state.now_playing_cursor;
        loxia_core::reducer::apply(
            &mut state,
            loxia_core::action::Action::Nav(loxia_core::action::NavAction::MoveDown { n: 1 }),
        );
        assert!(state.now_playing_user_scrolled);
        assert_eq!(state.now_playing_cursor, before_cursor + 1);
    }

    #[test]
    fn now_playing_snapshot_queue() {
        let state = now_playing_state();
        insta::assert_snapshot!(render_at(120, 30, &state).0);
    }

    #[test]
    fn now_playing_snapshot_history() {
        let state = history_state();
        insta::assert_snapshot!(render_at(120, 30, &state).0);
    }

    #[test]
    fn now_playing_snapshot_narrow() {
        let state = now_playing_state();
        insta::assert_snapshot!(render_at(90, 24, &state).0);
    }

    #[test]
    fn now_playing_snapshot_nothing_playing() {
        let mut state = now_playing_state();
        state.player.current = None;
        state.queue.entries.clear();
        state.queue.play_order.clear();
        insta::assert_snapshot!(render_at(120, 30, &state).0);
    }
}
