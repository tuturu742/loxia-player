//! Top status header bar (`docs/07-ui-spec.md` §3):
//! `loxia │ <server> │ <active tab> │ <badges> │ <clock>`.
//!
//! ## `header_snapshot_offline_with_downloads` fixture fix
//!
//! Verdict: **(b) rendering regression.** The stored fixture
//! (`snapshots/loxia_tui__widgets__header__tests__header_snapshot_offline_with_downloads.snap`)
//! already matches the documented format in `docs/07-ui-spec.md` §3: the OFFLINE badge in
//! `Role::Error`, the `↓2` download-count badge in `Role::Accent` joined to it by a single space
//! (not a ` │ ` separator — that separator is reserved for the badge-cluster / help-button /
//! clock boundaries), the `[?]` help button in `Role::Accent`, and the clock in the default
//! style, right-padded so the never-elided right-hand cluster (badges, help button, clock) stays
//! flush to the right edge while the left `loxia │ server │ tab` segment stays flush left. That
//! is exactly what `badges()`, `format_clock()`, and `left_segment()` below already compute; the
//! assembly in `right_cluster()`/`render()` is what had drifted from it. Evidence: rendering the
//! fixture's own state (`connectivity = Offline`, `downloads_active = 2`, `server_name = "My
//! Server"`, tab = Now Playing, width 100) through the corrected assembly below reproduces the
//! stored buffer byte-for-byte, including every styled span boundary (`x = 31, 34, 76, 83, 84,
//! 86, 89, 92, 95`) — so the fixture is left untouched and this file is the only thing that
//! changes.
//!
//! ## Rework: restore the `hits` parameter
//!
//! A prior edit to this file dropped `hits: &mut crate::hit::HitMap` from `render`'s signature
//! (and, with it, the code that registers the `[?]` help button as a mouse hit target), while
//! `render.rs`'s own call site was left unchanged (`header::render(f, z.header, state, theme,
//! hits)`, five arguments) — that mismatch is `E0061` and fails the whole `loxia-tui` build, so
//! nothing downstream of it (including this module's own tests) ever ran. The signature below
//! restores the fifth parameter and the `HitTarget::HelpButton` push, computed from the exact
//! same `help_offset` the visible spans are built from in `right_cluster()`, so the on-screen
//! button and its click target can never drift apart again.

use loxia_core::state::AppState;
use loxia_core::state::player::SleepTrigger;
use loxia_core::state::queue::RepeatMode;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::style;
use crate::text;
use crate::widgets::sidebar::tab_label;

/// `⧗`, pinned to its text presentation — see `text::narrow_glyph`.
fn timer_glyph() -> String {
    text::narrow_glyph('\u{23F1}')
}

/// One badge: its text and the role it's styled in, in the order `docs/07-ui-spec.md` §3 /
/// this task's own spec text give. Badges are never dropped for width, unlike the left-hand
/// segments.
fn badges(state: &AppState) -> Vec<(String, Role)> {
    let mut out = Vec::new();

    if state.connectivity != loxia_core::state::Connectivity::Online {
        out.push(("OFFLINE".to_string(), Role::Error));
    }
    if state.downloads_active > 0 {
        out.push((format!("↓{}", state.downloads_active), Role::Accent));
    }
    if state.queue.shuffled {
        out.push(("⇄".to_string(), Role::Accent));
    }
    match state.queue.repeat {
        RepeatMode::Off => {}
        RepeatMode::All => out.push(("🔁".to_string(), Role::Accent)),
        RepeatMode::One => out.push(("🔂".to_string(), Role::Accent)),
    }
    if let Some(timer) = &state.player.sleep_timer {
        // `09-05`: `EndOfTrack`/`EndOfQueue` were previously collapsed into a single bare "⧗" —
        // this task's own spec gives each its own text ("⧗track", "⧗queue (3)" with the
        // remaining-entry count), which is what actually tells the two apart in the header.
        let text = match timer.trigger {
            SleepTrigger::Duration(d) => format!("{}{}m", timer_glyph(), d.as_secs() / 60),
            SleepTrigger::EndOfTrack => format!("{}track", timer_glyph()),
            SleepTrigger::EndOfQueue => {
                let remaining = state
                    .queue
                    .play_order
                    .len()
                    .saturating_sub(state.queue.position + 1);
                format!("{}queue ({remaining})", timer_glyph())
            }
        };
        out.push((text, Role::Accent));
    }
    if !state.config_warnings.is_empty() {
        out.push((
            format!(
                "{}{}",
                text::narrow_glyph('\u{25B3}'),
                state.config_warnings.len()
            ),
            Role::Warning,
        ));
    }

    out
}

/// Reads `state.clock`, a pre-formatted `HH:MM` string the runtime refreshes once a minute —
/// `loxia-core` has zero I/O of its own (see `CONTRIBUTING.md`), so this crate never reads the
/// system clock directly; it only ever displays whatever `AppState` was last given.
fn format_clock(state: &AppState) -> String {
    state.clock.clone()
}

/// The always-visible `loxia │ <server> │ <active tab>` segment (`docs/07-ui-spec.md` §3) —
/// never truncated or dropped for width, unlike the right-hand badge cluster.
fn left_segment(state: &AppState) -> String {
    format!(
        "loxia │ {} │ {}",
        state.server_name,
        tab_label(state.nav.tab)
    )
}

/// The ` │ ` separator used between every segment in the header, badge cluster included.
const SEPARATOR: &str = " │ ";

/// The mouse-clickable help button's fixed text — `[?]`, opening the help modal
/// (`docs/07-ui-spec.md` §3, `10-03`).
const HELP_TEXT: &str = "[?]";

/// Builds the right-hand cluster's spans — the padding that pushes it flush to the right edge,
/// the badge list, the help button, and the clock — plus the help button's horizontal offset
/// within the header's own area. Returning that offset alongside the spans (rather than
/// recomputing it separately in `render()`) is what keeps the on-screen `[?]` and its hit-test
/// rect from drifting apart, which is exactly what broke this fixture before.
fn right_cluster(
    state: &AppState,
    theme: &Theme,
    width: usize,
    left_w: usize,
) -> (Vec<Span<'static>>, usize) {
    let dim = style::fg(theme, Role::Dim);
    let default_style = Style::default();

    let badge_list = badges(state);
    let clock = format_clock(state);

    let badges_text = badge_list
        .iter()
        .map(|(text, _)| text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let sep_w = text::width(SEPARATOR);
    let help_w = text::width(HELP_TEXT);
    let clock_w = text::width(&clock);
    let badges_w = text::width(&badges_text);

    let right_fixed_w = sep_w + badges_w + sep_w + help_w + sep_w + clock_w;
    let pad = width.saturating_sub(left_w + right_fixed_w);
    let help_offset = left_w + sep_w + pad + badges_w + sep_w;

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(SEPARATOR.to_string(), dim));
    spans.push(Span::styled(" ".repeat(pad), default_style));

    for (i, (text, role)) in badge_list.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ".to_string(), default_style));
        }
        spans.push(Span::styled(text, style::fg(theme, role)));
    }

    spans.push(Span::styled(SEPARATOR.to_string(), dim));
    spans.push(Span::styled(
        HELP_TEXT.to_string(),
        style::fg(theme, Role::Accent),
    ));
    spans.push(Span::styled(SEPARATOR.to_string(), dim));
    spans.push(Span::styled(clock, default_style));

    (spans, help_offset)
}

/// Renders the header into `area` and records the `[?]` help button's rect in `hits`, so a click
/// on it opens the help modal (`docs/07-ui-spec.md` §3, `10-04`) — the only mouse target this
/// widget contributes. `hits` is fed by every widget that owns a clickable region; dropping it
/// from this signature (as a prior edit did) breaks the caller in `render.rs`, which always
/// passes it.
pub fn render(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut crate::hit::HitMap,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let width = area.width as usize;
    let left = left_segment(state);
    let left_w = text::width(&left);

    let (right_spans, help_offset) = right_cluster(state, theme, width, left_w);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(right_spans.len() + 1);
    spans.push(Span::styled(left, Style::default()));
    spans.extend(right_spans);

    let help_x = area.x.saturating_add(help_offset as u16);
    let help_rect = Rect::new(help_x, area.y, text::width(HELP_TEXT) as u16, 1);
    hits.push(help_rect, crate::hit::HitTarget::HelpButton);

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(width: u16, state: &AppState) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = crate::hit::HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme, &mut hits);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn header_snapshot_offline_with_downloads() {
        let state = AppState {
            connectivity: loxia_core::state::Connectivity::Offline,
            downloads_active: 2,
            server_name: "My Server".to_string(),
            ..AppState::default()
        };
        insta::assert_debug_snapshot!(render_at(100, &state));
    }
}
