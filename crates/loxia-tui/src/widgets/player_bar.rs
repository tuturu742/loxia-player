//! The persistent three-line player bar (`docs/07-ui-spec.md` §7): now-playing line, progress
//! bar, and audio inspector line. Always visible, so every line degrades gracefully at any width.
//! Reads `player.position`/`player.duration` from state — never extrapolates from a wall clock.

use std::time::Duration;

use loxia_core::state::AppState;
use loxia_core::state::player::PlayStatus;
use loxia_core::state::queue::{Availability, RepeatMode};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget, TransportButton};
use crate::style;
use crate::text;
use crate::widgets::progress::render_progress;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // A top border plus one column of side padding, so the bar reads as its own panel and its
    // controls aren't jammed against the terminal edge (`docs/12-decisions.md`). Only the top edge
    // is drawn: a full box would cost two more rows the three content lines need.
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(style::border(theme, false));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let area = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    if area.width == 0 || area.height == 0 {
        return;
    }

    render_line1(f, row(area, 0), state, theme);
    if area.height < 2 {
        return;
    }
    render_line2(f, row(area, 1), state, theme, hits);
    if area.height < 3 {
        return;
    }
    render_line3(f, row(area, 2), state, theme);
}

fn row(area: Rect, i: u16) -> Rect {
    Rect::new(area.x, area.y + i, area.width, 1)
}

/// `pub(crate)`: `10-02`'s own Zen view reuses this for its own progress row's flanking
/// timestamps, rather than re-deriving the same `mm:ss`/`h:mm:ss` formatting.
pub(crate) fn format_time(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn status_glyph(status: PlayStatus) -> char {
    match status {
        PlayStatus::Playing => '▸',
        PlayStatus::Paused => '‖',
        PlayStatus::Stopped => '■',
        PlayStatus::Buffering => '⋯',
        PlayStatus::Loading => '◌',
    }
}

/// `docs/07-ui-spec.md` §7 shows a bare "[availability glyph]" placeholder with no literal
/// mapping — chosen to echo the same vocabulary `04-06`'s column status glyphs already
/// established (`↓` downloaded, `△` unavailable), plus a distinct mark for a cached-but-not-
/// pinned file; the normal remote case gets no glyph at all.
fn availability_glyph(a: Availability) -> String {
    match a {
        Availability::Remote => String::new(),
        Availability::Cached => text::narrow_glyph('\u{25D0}'),
        Availability::Downloaded => text::narrow_glyph('\u{2193}'),
        Availability::Unavailable => text::narrow_glyph('\u{25B3}'),
    }
}

fn render_line1(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let Some(entry_id) = state.player.current else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "nothing playing",
                style::fg(theme, Role::Dim),
            ))),
            area,
        );
        return;
    };
    let Some(entry) = state.queue.entries.iter().find(|e| e.entry_id == entry_id) else {
        return;
    };
    let track = &entry.track;

    let glyph = text::narrow_glyph(status_glyph(state.player.status));
    // The line's own layout is a user template (`ui.now_playing_format`) — see
    // `loxia_core::model::format_now_playing` for the placeholder set.
    let mut text = format!(
        "{glyph} {}",
        loxia_core::model::format_now_playing(&state.config.ui.now_playing_format, track)
    );

    // `11-06`: `restored_unloaded` is the real "restored, not yet resumed" flag — replacing the
    // old `Stopped`-plus-nonzero-position guess, which was both wrong (a paused restore reads
    // `Paused`, not `Stopped`) and a false positive for any *unrelated* stop that happened to leave
    // a nonzero position. The `!is_zero()` guard stays, purely cosmetic: "resumed at 0:00" is not
    // worth showing even in the (rare) case a session was saved at the very start of a track.
    if state.player.restored_unloaded && !state.player.position.is_zero() {
        text.push_str(&format!(
            " — resumed at {}",
            format_time(state.player.position)
        ));
    }

    let availability = availability_glyph(entry.availability);
    let availability = availability.as_str();
    let width = area.width as usize;
    let left = text::ellipsize(&text, width.saturating_sub(text::width(availability) + 1));

    let mut spans = vec![Span::styled(
        left.into_owned(),
        style::style(theme, Role::Fg),
    )];
    if !availability.is_empty() {
        let pad = width.saturating_sub(text::width(&spans[0].content) + text::width(availability));
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(availability, style::fg(theme, Role::Warning)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// `pub(crate)`: `10-02`'s own Zen view reuses this verbatim for its own progress row (elapsed
/// time, bar, remaining time) — it already guards on `state.player.current.is_none()` internally,
/// which is harmless to call again from a context that has already confirmed a current entry.
pub(crate) fn render_line2(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
) {
    if state.player.current.is_none() {
        return;
    }
    let pos = state.player.position;
    let dur = state.player.duration;
    let pos_str = if dur.is_zero() {
        "--:--".to_string()
    } else {
        format_time(pos)
    };
    let dur_str = if dur.is_zero() {
        "--:--".to_string()
    } else {
        format_time(dur)
    };

    // Transport controls live here — in the always-visible player bar rather than inside the Now
    // Playing view, so they are reachable from every tab (`docs/12-decisions.md`). Dropped entirely
    // if the row is too narrow to hold them *and* a usable seek bar.
    let transport_w = transport_width(state);
    let controls_w = if area.width >= transport_w + MIN_SEEK_ROW_WIDTH {
        render_transport(
            f,
            Rect::new(area.x, area.y, transport_w, 1),
            state,
            theme,
            hits,
        );
        transport_w
    } else {
        0
    };
    let area = Rect::new(
        area.x + controls_w,
        area.y,
        area.width.saturating_sub(controls_w),
        1,
    );

    let left_w = text::width(&pos_str) as u16;
    let right_w = text::width(&dur_str) as u16;
    let gap = 1u16;
    if area.width < left_w + right_w + gap * 2 {
        return;
    }
    let bar_w = area.width - left_w - right_w - gap * 2;

    let left_area = Rect::new(area.x, area.y, left_w, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pos_str,
            style::style(theme, Role::Fg),
        ))),
        left_area,
    );

    let bar_area = Rect::new(area.x + left_w + gap, area.y, bar_w, 1);
    // No state field carries a buffered fraction yet (no download/streaming-progress tracking
    // exists on `PlayerState`) — always `None` here; `widgets::progress`'s own tests exercise the
    // buffered-region rendering directly. See `docs/12-decisions.md`.
    render_progress(f, bar_area, pos, dur, None, theme, hits);

    let right_area = Rect::new(area.x + left_w + gap + bar_w + gap, area.y, right_w, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            dur_str,
            style::style(theme, Role::Fg),
        ))),
        right_area,
    );
}

/// The narrowest a seek row can be and still be worth drawing — below this the transport controls
/// are dropped so the bar itself keeps its space.
const MIN_SEEK_ROW_WIDTH: u16 = 24;

/// The blank columns after each glyph. Part of that button's own click target, so the whole strip
/// is live and there is nothing between two buttons that swallows a click.
const TRANSPORT_GAP: u16 = 2;

/// `⏮ ⏯ ⏹ ⏭ 🔀 🔁`, each a click target, with the shuffle/repeat glyphs reflecting current state.
///
/// **Every glyph carries U+FE0F.** `U+23EE`..`U+23F9` are ambiguous-width by default: `unicode-width`
/// calls them 1 cell while terminals draw them as 2-cell emoji. The hit map is built from our
/// width, so it drifted a column further left with each button — clicking Stop landed on Next, and
/// clicking Play landed in the gap between two regions and did nothing (`docs/12-decisions.md`).
/// The variation selector pins emoji presentation, which makes `unicode-width` and the terminal
/// agree on 2. `transport_glyphs_are_unambiguously_two_cells` guards this.
fn transport_segments(state: &AppState) -> [(&'static str, Role, TransportButton); 6] {
    let shuffle_role = if state.queue.shuffled {
        Role::Accent
    } else {
        Role::Dim
    };
    let (repeat_glyph, repeat_role) = match state.queue.repeat {
        RepeatMode::Off => ("\u{1f501}\u{fe0f}", Role::Dim),
        RepeatMode::All => ("\u{1f501}\u{fe0f}", Role::Accent),
        RepeatMode::One => ("\u{1f502}\u{fe0f}", Role::Accent),
    };
    [
        ("\u{23ee}\u{fe0f}", Role::Fg, TransportButton::Prev),
        ("\u{23ef}\u{fe0f}", Role::Fg, TransportButton::PlayPause),
        ("\u{23f9}\u{fe0f}", Role::Fg, TransportButton::Stop),
        ("\u{23ed}\u{fe0f}", Role::Fg, TransportButton::Next),
        ("\u{1f500}\u{fe0f}", shuffle_role, TransportButton::Shuffle),
        (repeat_glyph, repeat_role, TransportButton::Repeat),
    ]
}

fn transport_width(state: &AppState) -> u16 {
    transport_segments(state)
        .iter()
        .map(|(t, _, _)| text::width(t) as u16 + TRANSPORT_GAP)
        .sum()
}

fn render_transport(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    let mut x = area.x;
    let mut spans = Vec::new();
    for (glyph, role, button) in transport_segments(state) {
        let w = text::width(glyph) as u16;
        if x + w > area.x + area.width {
            break;
        }
        // The trailing gap belongs to the button too: a click a column right of the glyph is
        // plainly aimed at it, and leaving those columns unclaimed is what made the strip feel
        // unreliable even where the geometry lined up.
        let slot = (w + TRANSPORT_GAP).min(area.x + area.width - x);
        hits.push(Rect::new(x, area.y, slot, 1), HitTarget::Transport(button));
        spans.push(Span::styled(glyph.to_string(), style::fg(theme, role)));
        spans.push(Span::raw("  "));
        x += w + TRANSPORT_GAP;
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn eq_state_text(eq: &loxia_core::state::player::EqState) -> String {
    if eq.bypassed {
        "Bypassed".to_string()
    } else if eq.enabled {
        format!("Active [{}]", eq.preset_name)
    } else {
        "Off".to_string()
    }
}

/// Collapse order, right to left: `EQ` -> `Output` -> `Bitrate` -> the depth/rate portion. The
/// codec name itself is never dropped. `pub(crate)`: `10-02`'s own Zen view reuses this verbatim
/// for its own format line, rather than re-deriving the same collapse logic a second time.
pub(crate) fn render_line3(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    if area.width == 0 {
        return;
    }
    // Only rendered once mpv has reported a decoded format — i.e. while something is actually
    // playing, which is the only time a codec/bitrate readout means anything.
    let Some(format) = &state.player.format else {
        return;
    };

    let codec_only = format!("\u{1f39a} {}", format.codec.label());
    let depth_rate = Some(match format.bit_depth {
        Some(b) => format!(
            "{b}-bit/{:.1}kHz",
            f64::from(format.sample_rate_hz) / 1000.0
        ),
        None => format!("{:.1}kHz", f64::from(format.sample_rate_hz) / 1000.0),
    });
    // Right-aligned in a fixed four-character field. mpv re-reports the bitrate as a track plays,
    // and on a VBR stream the digit count changes with it (`999` -> `1012` -> `987`), which shifted
    // every field after it sideways once or twice a second — the readouts appeared to dance
    // (`docs/12-decisions.md`). Four digits covers everything up to 9999 kbps, i.e. every lossless
    // stereo rate; a wider value simply pushes out as before rather than being truncated.
    let bitrate = format
        .bitrate_bps
        .map(|b| format!("Bitrate: {:>4} kbps", b / 1000));
    // Where the bytes are coming from. Only shown once the cache lookup has answered — during the
    // brief window before that, claiming either would be a guess.
    let source = state
        .player
        .playback_source
        .map(|s| match s {
            loxia_core::state::player::PlaybackSource::Cache => "Source: cache",
            loxia_core::state::player::PlaybackSource::Caching => "Source: caching",
            loxia_core::state::player::PlaybackSource::Streaming => "Source: stream",
        })
        .map(str::to_string);

    // Right-aligned in a three-character field for the same reason the bitrate is: `9%` -> `100%`
    // would shift the EQ readout beside it every time the volume crossed a digit boundary. Muting
    // reads as `muted` rather than a number, since the number is still whatever it was.
    let volume = if state.player.muted {
        "Vol: muted".to_string()
    } else {
        format!("Vol: {:>3}%", state.player.volume)
    };
    let last_field = format!("EQ: {}", eq_state_text(&state.player.eq));

    // Priority order, most important first; dropped from the end when too wide to fit.
    let mut segments = vec![codec_only];
    if let Some(d) = depth_rate {
        segments.push(d);
    }
    if let Some(b) = bitrate {
        segments.push(b);
    }
    if let Some(src) = source {
        segments.push(src);
    }
    segments.push(volume);
    segments.push(last_field);

    let width = area.width as usize;
    while segments.len() > 1 {
        let joined_width: usize =
            segments.iter().map(|s| text::width(s)).sum::<usize>() + (segments.len() - 1) * 3;
        if joined_width <= width {
            break;
        }
        segments.pop();
    }

    let mut spans = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", style::fg(theme, Role::Border)));
        }
        spans.push(Span::styled(seg.clone(), style::style(theme, Role::Fg)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Transport controls moved out of the Now Playing view and into the always-visible player bar,
    /// so they work from every tab (`docs/12-decisions.md`).
    #[test]
    fn transport_registers_hit_targets_in_the_player_bar() {
        use crate::hit::{HitTarget, TransportButton};
        let state = playing_state();
        let backend = ratatui::backend::TestBackend::new(120, 4);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| render(f, f.area(), &state, &theme, &mut hits))
            .unwrap();

        for button in [
            TransportButton::Prev,
            TransportButton::PlayPause,
            TransportButton::Stop,
            TransportButton::Next,
            TransportButton::Shuffle,
            TransportButton::Repeat,
        ] {
            assert!(
                (0..120).any(|x| (0..4).any(|y| matches!(
                    hits.hit(x, y),
                    Some(HitTarget::Transport(b)) if *b == button
                ))),
                "missing hit target for {button:?}"
            );
        }
    }
    use loxia_core::model::{AudioFormat, Codec};
    use loxia_core::state::player::EqState;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, state: &AppState) -> String {
        // 4 rows: the bar's own top border plus its three content lines.
        let backend = TestBackend::new(w, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme, &mut hits)
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    fn playing_state() -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        state.player.format = Some(AudioFormat {
            codec: Codec::Flac,
            sample_rate_hz: 44_100,
            bit_depth: Some(16),
            channels: 2,
            bitrate_bps: Some(1_000_000),
        });
        state.player.eq = EqState {
            enabled: true,
            preset_name: "flat".to_string(),
            gains: [0.0; 10],
            selected_band: 0,
            bypassed: false,
        };
        state.player.duration = Duration::from_secs(180);
        state.player.position = Duration::from_secs(90);
        state
    }

    #[test]
    fn player_bar_snapshot_playing() {
        let state = playing_state();
        insta::assert_snapshot!(render_at(100, &state));
    }

    #[test]
    fn player_bar_snapshot_paused() {
        let mut state = playing_state();
        state.player.status = PlayStatus::Paused;
        insta::assert_snapshot!(render_at(100, &state));
    }

    #[test]
    fn player_bar_snapshot_stopped() {
        let mut state = playing_state();
        state.player.status = PlayStatus::Stopped;
        insta::assert_snapshot!(render_at(100, &state));
    }

    #[test]
    fn player_bar_snapshot_buffering() {
        let mut state = playing_state();
        state.player.status = PlayStatus::Buffering;
        insta::assert_snapshot!(render_at(100, &state));
    }

    #[test]
    fn line3_collapse_order() {
        let state = playing_state();
        for width in [200, 140, 110, 90, 80] {
            let rendered = render_at(width, &state);
            // The codec name must survive at every one of these widths.
            assert!(rendered.contains("FLAC"), "width {width}: codec dropped");
        }
        let wide = render_at(200, &state);
        assert!(wide.contains("EQ:"));
        assert!(wide.contains("Vol:"));
        assert!(wide.contains("Bitrate:"));

        // Narrow enough that the lowest-priority segment has to go. The threshold moved down when
        // the never-populated `Output` field was removed — the line is simply shorter now.
        let narrowest = render_at(60, &state);
        assert!(
            !narrowest.contains("EQ:"),
            "EQ should be dropped by width 60: {narrowest}"
        );
    }

    /// mpv re-reports the bitrate as a VBR track plays. When the digit count changed, everything
    /// after it slid sideways — the readouts beside it visibly danced. The field is padded to a
    /// fixed width so its neighbours hold still (`docs/12-decisions.md`).
    #[test]
    fn bitrate_field_width_is_stable_across_values() {
        let column_of = |kbps: u32| {
            let mut state = playing_state();
            if let Some(f) = state.player.format.as_mut() {
                f.bitrate_bps = Some(kbps * 1000);
            }
            let rendered = render_at(200, &state);
            rendered
                .lines()
                .find_map(|l| l.find("Vol:"))
                .expect("the volume field is shown at this width")
        };

        let three = column_of(987);
        for kbps in [12, 128, 1012] {
            assert_eq!(
                column_of(kbps),
                three,
                "a {kbps} kbps readout moved the fields after it"
            );
        }
    }

    /// The volume readout sits immediately before the EQ segment, and holds a fixed width so the
    /// EQ text beside it doesn't shift as the volume crosses a digit boundary.
    #[test]
    fn volume_is_shown_before_the_eq_segment() {
        let mut state = playing_state();
        state.player.volume = 80;
        let rendered = render_at(200, &state);

        let volume_at = rendered.find("Vol: ").expect("volume shown");
        let eq_at = rendered.find("EQ:").expect("eq shown");
        assert!(volume_at < eq_at, "volume belongs before the EQ profile");

        let column_of = |v: u8| {
            let mut state = playing_state();
            state.player.volume = v;
            render_at(200, &state)
                .find("EQ:")
                .expect("eq shown at this width")
        };
        assert_eq!(
            column_of(9),
            column_of(100),
            "a changing volume must not shift the EQ readout beside it"
        );
    }

    /// Muted shows as a word: the numeric volume is still whatever it was, so printing it would
    /// say nothing about the fact that no sound is coming out.
    #[test]
    fn muting_is_named_rather_than_shown_as_a_number() {
        let mut state = playing_state();
        state.player.volume = 80;
        state.player.muted = true;
        let rendered = render_at(200, &state);
        assert!(rendered.contains("Vol: muted"), "{rendered}");
        assert!(!rendered.contains("80%"));
    }

    /// The one assertion that isn't circular. Every other check here measures the layout with the
    /// same `text::width` the layout is built from, so it agrees with itself whether or not it
    /// agrees with the *terminal*. `U+23EE`..`U+23F9` bare are the trap: `unicode-width` calls them
    /// 1 cell and terminals draw them as 2, so the hit map drifted a column per button and clicking
    /// Stop hit Next (`docs/12-decisions.md`). A glyph that measures 2 is one whose presentation is
    /// pinned, and therefore one both sides agree on.
    #[test]
    fn transport_glyphs_are_unambiguously_two_cells() {
        let state = playing_state();
        for (glyph, _, button) in transport_segments(&state) {
            assert_eq!(
                text::width(glyph),
                2,
                "{button:?}'s glyph {glyph:?} is ambiguous-width — pin it with U+FE0F"
            );
            assert!(
                glyph.contains('\u{fe0f}'),
                "{button:?}'s glyph must carry the emoji variation selector"
            );
        }
    }

    /// No dead columns: every column of the strip belongs to a button, so a click anywhere in it
    /// does something rather than silently missing between two regions.
    #[test]
    fn the_whole_transport_strip_is_clickable() {
        let state = playing_state();
        let width = 200;
        let backend = TestBackend::new(width, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| render(f, f.area(), &state, &theme, &mut hits))
            .unwrap();

        // The strip starts one column in (the bar's own side padding) and runs `transport_width`.
        let start = 1;
        let end = start + transport_width(&state);
        let row = 2; // border, line1, line2
        let mut seen = Vec::new();
        for col in start..end {
            match hits.hit(col, row) {
                Some(HitTarget::Transport(button)) => {
                    if seen.last() != Some(button) {
                        seen.push(*button);
                    }
                }
                other => panic!("column {col} of the transport strip is dead: {other:?}"),
            }
        }

        assert_eq!(
            seen,
            vec![
                TransportButton::Prev,
                TransportButton::PlayPause,
                TransportButton::Stop,
                TransportButton::Next,
                TransportButton::Shuffle,
                TransportButton::Repeat,
            ],
            "buttons must be contiguous and in order"
        );
    }

    /// Whether a track is coming off the disk or the network, which is otherwise invisible.
    #[test]
    fn playback_source_is_reported_once_it_is_known() {
        use loxia_core::state::player::PlaybackSource;

        let mut state = playing_state();
        state.player.playback_source = Some(PlaybackSource::Cache);
        assert!(render_at(200, &state).contains("Source: cache"));

        state.player.playback_source = Some(PlaybackSource::Streaming);
        assert!(render_at(200, &state).contains("Source: stream"));
    }

    /// Before the cache lookup answers, either label would be a guess — so neither is shown.
    #[test]
    fn playback_source_is_omitted_while_unknown() {
        let mut state = playing_state();
        state.player.playback_source = None;
        assert!(!render_at(200, &state).contains("Source:"));
    }

    #[test]
    fn codec_never_dropped() {
        let state = playing_state();
        let rendered = render_at(20, &state);
        assert!(rendered.contains("FLAC"));
    }

    #[test]
    fn time_format_switches_at_one_hour() {
        assert_eq!(format_time(Duration::from_secs(59 * 60 + 59)), "59:59");
        assert_eq!(format_time(Duration::from_secs(3600)), "1:00:00");
        assert_eq!(format_time(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn resumed_hint_shown_after_session_restore() {
        // `11-06`: the hint keys off `restored_unloaded` now, not `Stopped`-plus-nonzero-position
        // — a real session restore leaves `status == Paused` (`⏸`), never `Stopped`.
        let mut state = playing_state();
        state.player.status = PlayStatus::Paused;
        state.player.restored_unloaded = true;
        state.player.position = Duration::from_secs(42);
        let rendered = render_at(100, &state);
        assert!(rendered.contains("resumed at 0:42"));
        assert!(rendered.contains('‖'));

        let mut fresh = playing_state();
        fresh.player.status = PlayStatus::Paused;
        fresh.player.restored_unloaded = true;
        fresh.player.position = Duration::ZERO;
        let rendered_fresh = render_at(100, &fresh);
        assert!(!rendered_fresh.contains("resumed at"));

        // A plain `Stopped` player (queue ended, not a restore) must never show the hint either,
        // even with a leftover nonzero position — the old guess's own false-positive case.
        let mut stopped = playing_state();
        stopped.player.status = PlayStatus::Stopped;
        stopped.player.position = Duration::from_secs(42);
        let rendered_stopped = render_at(100, &stopped);
        assert!(!rendered_stopped.contains("resumed at"));
    }
}
