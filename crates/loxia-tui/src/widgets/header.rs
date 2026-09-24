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

use loxia_core::state::AppState;
use loxia_core::state::player::SleepTrigger;
use loxia_core::state::queue::RepeatMode;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
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

/// Reads `state.clock` (set from `Tick`'s own timestamp, `04-05`) — never `Timestamp::now()`.
/// Two renders of the same state must be byte-identical, which a direct system-clock read would
/// break.
fn format_clock(state: &AppState) -> String {
    let (hour, minute) = loxia_core::local_hour_minute(state.clock);
    format!("{hour:02}:{minute:02}")
}

/// The `loxia │ server │ tab` segment, honouring the elision flags — a dropped segment removes
/// both its text and its separator.
fn left_segment(state: &AppState, show_server: bool, show_tab: bool) -> String {
    let mut parts = vec!["loxia".to_string()];
    if show_server {
        let server = state
            .server
            .server_name
            .clone()
            .unwrap_or_else(|| "no server".to_string());
        parts.push(server);
    }
    if show_tab {
        parts.push(tab_label(state.nav.active_tab).to_string());
    }
    parts.join(" │ ")
}

/// The mouse-reachable help button, sitting immediately left of the clock. Labelled with the key
/// that does the same thing from the keyboard, so the button teaches the binding.
const HELP_LABEL: &str = "[?]";

/// The right-hand cluster's spans, in order — badges (joined by a plain space, not a `│`), then
/// the help button, then the clock, each of those three groups separated by ` │ `. Also returns
/// the cluster's total display width (so the caller knows how much room the never-elided cluster
/// needs before deciding whether the left-hand segment must drop its server or tab text) and the
/// help button's own offset within the cluster, for the mouse hit-test map.
fn right_cluster(state: &AppState, theme: &Theme) -> (Vec<Span<'static>>, usize, Option<usize>) {
    let dim = style::fg(theme, Role::Dim);
    let sep = " │ ";
    let sep_w = text::width(sep);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut width = 0usize;

    let badge_list = badges(state);
    if !badge_list.is_empty() {
        for (i, (label, role)) in badge_list.into_iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
                width += 1;
            }
            width += text::width(&label);
            spans.push(Span::styled(label, style::fg(theme, role)));
        }
        spans.push(Span::styled(sep, dim));
        width += sep_w;
    }

    let help_offset = Some(width);
    width += text::width(HELP_LABEL);
    spans.push(Span::styled(HELP_LABEL, style::fg(theme, Role::Accent)));

    spans.push(Span::styled(sep, dim));
    width += sep_w;

    let clock = format_clock(state);
    width += text::width(&clock);
    spans.push(Span::raw(clock));

    (spans, width, help_offset)
}

/// Renders the whole header row into `area`'s first line. The right-hand cluster (badges, help
/// button, clock) is never elided for width; the left `loxia │ server │ tab` segment drops its
/// server name first, then its active-tab label, if the terminal is too narrow for both.
pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = area.width as usize;
    let dim = style::fg(theme, Role::Dim);
    let sep = " │ ";
    let sep_w = text::width(sep);

    let (right_spans, right_width, _help_offset) = right_cluster(state, theme);

    let fits = |show_server: bool, show_tab: bool| -> bool {
        text::width(&left_segment(state, show_server, show_tab)) + sep_w + right_width <= width
    };

    let mut show_server = true;
    let mut show_tab = true;
    if !fits(show_server, show_tab) {
        show_server = false;
        if !fits(show_server, show_tab) {
            show_tab = false;
        }
    }

    let left_text = left_segment(state, show_server, show_tab);
    let left_w = text::width(&left_text);

    let mut spans: Vec<Span<'static>> = vec![Span::raw(left_text), Span::styled(sep, dim)];

    let used = left_w + sep_w + right_width;
    let filler = width.saturating_sub(used);
    if filler > 0 {
        spans.push(Span::raw(" ".repeat(filler)));
    }
    spans.extend(right_spans);

    let row_area = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(Paragraph::new(Line::from(spans)), row_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::state::Connectivity;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn render_at(width: u16, state: &AppState) -> Buffer {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme)
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn header_snapshot_offline_with_downloads() {
        let mut state = fixtures::app_state();
        state.connectivity = Connectivity::Offline;
        state.downloads_active = 2;
        state.server.server_name = Some("My Server".to_string());
        state.clock = "2024-01-01T23:13:00Z".parse().expect("valid timestamp");

        insta::assert_debug_snapshot!(render_at(100, &state));
    }
}
