//! Top status header bar (`docs/07-ui-spec.md` §3):
//! `loxia │ <server> │ <active tab> │ <badges> │ <clock>`.

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

/// The right-hand cluster's spans, in order — badges, then the help button, then the clock, each
/// pair separated by ` │ `. Also returns the help button's own offset within the cluster (`None`
/// when it was elided), which is all the caller needs to register the mouse hit target at the
/// correct absolute column.
fn right_cluster(state: &AppState, theme: &Theme) -> (Vec<Span<'static>>, String) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (text, role) in badges(state) {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(text, style::style(theme, role)));
    }
    if !spans.is_empty() {
        spans.push(Span::raw(" │ "));
    }
    spans.push(Span::styled(
        HELP_LABEL.to_string(),
        style::style(theme, Role::Accent),
    ));
    spans.push(Span::raw(" │ "));
    let clock = format_clock(state);
    spans.push(Span::styled(clock.clone(), style::style(theme, Role::Fg)));
    (spans, clock)
}

fn right_cluster_width(state: &AppState) -> u16 {
    let mut width: usize = 0;
    for (text, _) in badges(state) {
        if width > 0 {
            width += 1;
        }
        width += unicode_width::UnicodeWidthStr::width(text.as_str());
    }
    if width > 0 {
        width += 3;
    }
    width += unicode_width::UnicodeWidthStr::width(HELP_LABEL);
    width += 3;
    width += unicode_width::UnicodeWidthStr::width(format_clock(state).as_str());
    width as u16
}

/// Renders the header bar into `area` (always the topmost single row). Progressively elides the
/// server name, then the active tab label, then drops down to a bare `loxia` when the terminal is
/// too narrow to fit everything at once — the right-hand cluster (badges, help button, clock) is
/// never dropped.
pub fn render(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut crate::hit::HitMap,
) {
    if area.height == 0 {
        return;
    }
    let row = Rect::new(area.x, area.y, area.width, 1);

    let right_width = right_cluster_width(state);
    let available_left: i64 = i64::from(row.width) - i64::from(right_width) - 1;

    let mut show_server = true;
    let mut show_tab = true;
    let mut left = left_segment(state, show_server, show_tab);
    while unicode_width::UnicodeWidthStr::width(left.as_str()) as i64 > available_left.max(0)
        && (show_server || show_tab)
    {
        if show_tab {
            show_tab = false;
        } else {
            show_server = false;
        }
        left = left_segment(state, show_server, show_tab);
    }

    let (right_spans, _clock) = right_cluster(state, theme);
    let left_width = unicode_width::UnicodeWidthStr::width(left.as_str()) as u16;
    let pad = row
        .width
        .saturating_sub(left_width)
        .saturating_sub(right_width);

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(left, style::style(theme, Role::Fg)));
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad as usize)));
    }
    spans.extend(right_spans);

    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), row);

    // Register the `[?]` help button's mouse hit target at its actual rendered column.
    let help_offset = row
        .width
        .saturating_sub(right_width)
        .saturating_add(0);
    let _ = help_offset;
    let clock_len = unicode_width::UnicodeWidthStr::width(format_clock(state).as_str()) as u16;
    let help_x_end = row.width.saturating_sub(clock_len).saturating_sub(3);
    let help_x_start = help_x_end.saturating_sub(HELP_LABEL.len() as u16);
    if help_x_start < row.width {
        hits.push(
            Rect::new(
                row.x + help_x_start,
                row.y,
                (help_x_end - help_x_start).min(row.width - help_x_start),
                1,
            ),
            crate::hit::HitTarget::HelpButton,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::state::Connectivity;

    fn render_at(width: u16, state: &AppState) -> ratatui::buffer::Buffer {
        let backend = ratatui::backend::TestBackend::new(width, 1);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        let theme = Theme::default();
        let mut hits = crate::hit::HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme, &mut hits);
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    #[test]
    fn header_snapshot_offline_with_downloads() {
        let mut state = AppState::default();
        state.server.server_name = Some("My Server".to_string());
        state.connectivity = Connectivity::Offline;
        state.downloads_active = 2;
        state.clock = loxia_core::test_support::fixtures::fixed_clock_hm(23, 13);
        insta::assert_debug_snapshot!(render_at(100, &state));
    }
}
