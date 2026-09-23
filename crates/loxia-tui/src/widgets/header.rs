//! Header bar: title, server, active tab, status badges, help hint, and clock
//! (`docs/07-ui-spec.md` §3, task `04-05`).
//!
//! ```text
//! loxia │ <server> │ <active tab> │ [OFFLINE] [↓N] [⇄] [↻] [⏱Nm] │ [?] │ <clock>
//! ```
//!
//! Right-aligned badges appear only when their underlying condition is active
//! (`docs/07-ui-spec.md` §3). As the available width shrinks, the clock is dropped first, then
//! the `[?]` help hint, then finally the server name — in that order — rather than truncating
//! mid-word. Never renders a token or stream URL: only bare counts, glyphs, and the clock.

use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::hit::{HitMap, HitTarget};
use crate::style;
use crate::text;

/// Everything the header needs to render a single frame. Deliberately plain data rather than a
/// borrowed `&AppState` — the header does not know about the reducer, only about the handful of
/// fields it displays.
#[derive(Debug, Clone, Copy)]
pub struct HeaderState<'a> {
    pub server_name: &'a str,
    pub active_tab: &'a str,
    pub clock: (u8, u8),
    pub offline: bool,
    pub downloads_active: usize,
    pub shuffle: bool,
    pub repeat: bool,
    pub sleep_timer_mins: Option<u32>,
    pub config_warning: bool,
}

/// The active status badges, in the fixed priority order the spec lists them
/// (`docs/07-ui-spec.md` §3), each paired with the theme role it renders in.
fn badges(state: &HeaderState<'_>) -> Vec<(String, Role)> {
    let mut out = Vec::new();
    if state.offline {
        out.push(("OFFLINE".to_string(), Role::Error));
    }
    if state.downloads_active > 0 {
        out.push((format!("↓{}", state.downloads_active), Role::Accent));
    }
    if state.shuffle {
        out.push(("⇄".to_string(), Role::Accent));
    }
    if state.repeat {
        out.push(("↻".to_string(), Role::Accent));
    }
    if let Some(mins) = state.sleep_timer_mins {
        out.push((format!("⏱{mins}m"), Role::Accent));
    }
    if state.config_warning {
        out.push(("⚠".to_string(), Role::Warning));
    }
    out
}

fn format_clock(clock: (u8, u8)) -> String {
    format!("{:02}:{:02}", clock.0, clock.1)
}

/// `loxia │ <server> │ <active tab>` — or, once the server name has been elided for width, just
/// `loxia │ <active tab>`. A single unstyled run; the dim separator that follows it is drawn
/// separately and is not part of this string.
fn left_segment(state: &HeaderState<'_>, include_server: bool) -> String {
    if include_server {
        format!("loxia │ {} │ {}", state.server_name, state.active_tab)
    } else {
        format!("loxia │ {}", state.active_tab)
    }
}

/// The right-aligned run: active badges, the `[?]` help hint, and the clock, each as a
/// `(text, role)` pair in the exact order they are drawn. A badge-to-badge join is a plain space
/// in the default role; every other join is a dim ` │ `, and is only emitted between two
/// segments that are both present.
fn right_spans(
    state: &HeaderState<'_>,
    include_help: bool,
    include_clock: bool,
) -> Vec<(String, Role)> {
    let mut segs: Vec<(String, Role)> = Vec::new();
    for (i, (badge_text, role)) in badges(state).into_iter().enumerate() {
        if i > 0 {
            segs.push((" ".to_string(), Role::Fg));
        }
        segs.push((badge_text, role));
    }
    if include_help {
        if !segs.is_empty() {
            segs.push((" │ ".to_string(), Role::Dim));
        }
        segs.push(("[?]".to_string(), Role::Accent));
    }
    if include_clock {
        if !segs.is_empty() {
            segs.push((" │ ".to_string(), Role::Dim));
        }
        segs.push((format_clock(state.clock), Role::Fg));
    }
    segs
}

fn right_width(spans: &[(String, Role)]) -> u16 {
    spans.iter().map(|(t, _)| text::width(t) as u16).sum()
}

/// Draws the header into `area`'s first row and registers the `[?]` help button's hit rect.
///
/// `area.height` is expected to be exactly `1` (the root layout always gives the header
/// `Constraint::Length(1)`, `docs/07-ui-spec.md` §2), but only the first row is ever touched, so
/// a taller area is harmless.
pub fn render(f: &mut Frame, area: Rect, state: &HeaderState<'_>, theme: &Theme, hits: &mut HitMap) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let total_width = area.width;

    let mut include_server = true;
    let mut include_help = true;
    let mut include_clock = true;

    let (left, right) = loop {
        let left = left_segment(state, include_server);
        let right = right_spans(state, include_help, include_clock);
        let left_w = text::width(&left) as u16;
        let sep_w: u16 = if right.is_empty() { 0 } else { 3 };
        let right_w = right_width(&right);
        let fits = left_w + sep_w + right_w <= total_width;
        let can_shrink_more = include_clock || include_help || include_server;
        if fits || !can_shrink_more {
            break (left, right);
        }
        if include_clock {
            include_clock = false;
        } else if include_help {
            include_help = false;
        } else {
            include_server = false;
        }
    };

    let left = text::truncate(&left, total_width as usize).into_owned();
    let left_w = text::width(&left) as u16;
    let right_w = right_width(&right);
    let sep_w: u16 = if right.is_empty() { 0 } else { 3 };
    let pad_w = total_width.saturating_sub(left_w + sep_w + right_w);

    let mut spans = vec![Span::styled(left, style::fg(theme, Role::Fg))];
    let mut cursor = area.x + left_w;

    if !right.is_empty() {
        spans.push(Span::styled(" │ ", style::fg(theme, Role::Dim)));
        cursor += 3;
        if pad_w > 0 {
            spans.push(Span::raw(" ".repeat(pad_w as usize)));
            cursor += pad_w;
        }
        for (seg_text, role) in &right {
            if seg_text.as_str() == "[?]" {
                let w = text::width(seg_text) as u16;
                hits.register(HitTarget::HelpButton, Rect::new(cursor, area.y, w, 1));
            }
            spans.push(Span::styled(seg_text.clone(), style::fg(theme, *role)));
            cursor += text::width(seg_text) as u16;
        }
    }

    let row_area = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(Paragraph::new(Line::from(spans)), row_area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    use super::*;

    fn base_state() -> HeaderState<'static> {
        HeaderState {
            server_name: "My Server",
            active_tab: "Now Playing",
            clock: (23, 13),
            offline: false,
            downloads_active: 0,
            shuffle: false,
            repeat: false,
            sleep_timer_mins: None,
            config_warning: false,
        }
    }

    fn render_at(width: u16, state: &HeaderState<'_>) -> Buffer {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let theme = Theme::default();
        let mut hits = HitMap::default();
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
        let mut state = base_state();
        state.offline = true;
        state.downloads_active = 2;
        let buffer = render_at(100, &state);
        insta::assert_snapshot!(format!("{buffer:?}"));
    }

    #[test]
    fn header_badges_appear_only_when_active() {
        let state = base_state();
        let rendered = format!("{:?}", render_at(100, &state));
        assert!(!rendered.contains("OFFLINE"));
        assert!(!rendered.contains('↓'));
        assert!(!rendered.contains('⇄'));
        assert!(!rendered.contains('⏱'));

        let mut with_shuffle = state;
        with_shuffle.shuffle = true;
        let rendered = format!("{:?}", render_at(100, &with_shuffle));
        assert!(rendered.contains('⇄'));
    }

    #[test]
    fn badge_text_per_trigger() {
        let mut state = base_state();
        state.offline = true;
        assert_eq!(badges(&state), vec![("OFFLINE".to_string(), Role::Error)]);

        let mut state = base_state();
        state.downloads_active = 3;
        assert_eq!(badges(&state), vec![("↓3".to_string(), Role::Accent)]);

        let mut state = base_state();
        state.shuffle = true;
        assert_eq!(badges(&state), vec![("⇄".to_string(), Role::Accent)]);

        let mut state = base_state();
        state.repeat = true;
        assert_eq!(badges(&state), vec![("↻".to_string(), Role::Accent)]);

        let mut state = base_state();
        state.sleep_timer_mins = Some(30);
        assert_eq!(badges(&state), vec![("⏱30m".to_string(), Role::Accent)]);

        let mut state = base_state();
        state.config_warning = true;
        assert_eq!(badges(&state), vec![("⚠".to_string(), Role::Warning)]);
    }

    #[test]
    fn header_elides_clock_first_then_server() {
        let mut state = base_state();
        state.offline = true;

        let wide = format!("{:?}", render_at(100, &state));
        assert!(wide.contains("My Server"));
        assert!(wide.contains("23:13"));

        // Narrow enough to drop the clock, but not so narrow the server must go too.
        let narrower = format!("{:?}", render_at(45, &state));
        assert!(narrower.contains("My Server"));
        assert!(!narrower.contains("23:13"));

        // Narrower still: the server name is dropped as well.
        let narrowest = format!("{:?}", render_at(20, &state));
        assert!(!narrowest.contains("My Server"));
    }

    #[test]
    fn help_button_hit_rect_matches_where_it_is_drawn() {
        let state = base_state();
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, &state, &theme, &mut hits);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();

        // With no badges active, `[?]` is drawn at x = 89 for a 100-wide, base-state header.
        let expected_x = 89u16;
        let drawn: String = (0..3)
            .map(|dx| buffer[(expected_x + dx, 0)].symbol())
            .collect();
        assert_eq!(drawn, "[?]");
        assert_eq!(
            hits.hit(expected_x, 0),
            Some(HitTarget::HelpButton),
            "clicking where [?] is drawn must resolve to the help button"
        );
    }

    #[test]
    fn help_button_elides_after_the_clock_but_before_the_server() {
        let mut state = base_state();
        state.offline = true;

        // At the width where the clock has just been dropped, the help button is still present.
        let mid = format!("{:?}", render_at(45, &state));
        assert!(!mid.contains("23:13"));
        assert!(mid.contains("[?]"));

        // Narrower again: the help button is dropped too, before the server name is.
        let narrower = format!("{:?}", render_at(38, &state));
        assert!(!narrower.contains("[?]"));
        assert!(narrower.contains("My Server"));
    }

    #[test]
    fn help_button_is_left_of_the_clock() {
        let state = base_state();
        let rendered = format!("{:?}", render_at(100, &state));
        let help_pos = rendered.find("[?]").expect("help button text present");
        let clock_pos = rendered.find("23:13").expect("clock text present");
        assert!(help_pos < clock_pos);
    }

    #[test]
    fn header_uses_state_clock_not_system_clock() {
        let mut state = base_state();
        state.clock = (3, 7);
        let rendered = format!("{:?}", render_at(100, &state));
        assert!(rendered.contains("03:07"));
    }
}
