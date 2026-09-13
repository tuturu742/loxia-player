//! Toast notification stack (`10-13`, `docs/07-ui-spec.md` §13): bottom-right, stacked upward,
//! at most 3 visible with a `+n more` summary line, overlaid on top of the player bar without
//! resizing the layout — `render::draw` calls this last (before the modal), over an area computed
//! from the already-final `zones()` layout, so its own presence never changes where anything else
//! sits.

use loxia_core::state::AppState;
use loxia_core::state::toast::{Toast, ToastLevel};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::style;
use crate::text;

const MAX_VISIBLE: usize = 3;
/// The narrowest a toast box ever gets. It grows past this to fit its own longest message (bounded
/// by the available area) rather than truncating — a fixed 40 silently ate the last character of any
/// 39-character message, e.g. "no bit-perfect capable output available" reading as "…availabl"
/// (`docs/12-decisions.md`).
const TOAST_MIN_WIDTH: u16 = 40;
/// One bordered line per toast: top border, the message itself, bottom border.
const TOAST_HEIGHT: u16 = 3;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    if state.toasts.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }

    let overflow = state.toasts.len().saturating_sub(MAX_VISIBLE);
    let visible_count = state.toasts.len().min(MAX_VISIBLE);
    // Newest last (`state.toasts` is append-ordered) — rendered at the bottom, closest to the
    // player bar it overlays, with the stack growing upward as older-of-the-visible-set toasts
    // sit above it.
    let visible = &state.toasts[state.toasts.len() - visible_count..];

    let overflow_rows: u16 = u16::from(overflow > 0);
    let content_height = (visible_count as u16) * TOAST_HEIGHT + overflow_rows;

    // Wide enough for the longest visible message plus its own two border cells, never below
    // `TOAST_MIN_WIDTH` and never past the area itself.
    let widest = visible
        .iter()
        .map(|t| text::width(&t.message))
        .max()
        .unwrap_or(0);
    let needed = u16::try_from(widest.saturating_add(2)).unwrap_or(u16::MAX);
    let width = needed.max(TOAST_MIN_WIDTH).min(area.width);
    let height = content_height.min(area.height);
    if width == 0 || height == 0 {
        return;
    }

    let stack_area = Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y + area.height.saturating_sub(height),
        width,
        height,
    );

    let mut y = stack_area.y;
    let end_y = stack_area.y + stack_area.height;
    if overflow > 0 {
        let line = format!("+{overflow} more");
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(line, style::fg(theme, Role::Dim)))),
            Rect::new(stack_area.x, y, stack_area.width, 1),
        );
        y += 1;
    }
    for toast in visible {
        if y + TOAST_HEIGHT > end_y {
            break;
        }
        render_one(
            f,
            Rect::new(stack_area.x, y, stack_area.width, TOAST_HEIGHT),
            toast,
            theme,
        );
        y += TOAST_HEIGHT;
    }
}

fn render_one(f: &mut Frame, area: Rect, toast: &Toast, theme: &Theme) {
    let role = role_for_level(toast.level);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::fg(theme, role));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    // `ellipsize`, not `truncate`: the box is already sized to fit, so this only ever fires on a
    // terminal too narrow for the message — and then it must *look* cut off rather than silently
    // dropping the tail.
    let message = text::ellipsize(&toast.message, inner.width as usize).into_owned();
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(message, style::fg(theme, role)))),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
}

fn role_for_level(level: ToastLevel) -> Role {
    match level {
        ToastLevel::Info => Role::Fg,
        ToastLevel::Success => Role::Success,
        ToastLevel::Warning => Role::Warning,
        ToastLevel::Error => Role::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::Timestamp;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn toast(id: u64, message: &str, level: ToastLevel) -> Toast {
        Toast {
            id,
            message: message.to_string(),
            level,
            created_at: Timestamp::now(),
        }
    }

    fn draw_at(w: u16, h: u16, state: &AppState) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme)
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    #[test]
    fn empty_toasts_render_nothing() {
        let state = AppState::default();
        let rendered = draw_at(80, 24, &state);
        assert!(!rendered.contains('┌'), "no border should be drawn at all");
    }

    /// `10-13`: `at_most_three_visible_with_overflow_count`.
    #[test]
    fn at_most_three_visible_with_overflow_count() {
        let mut state = AppState::default();
        for i in 0..5 {
            state
                .toasts
                .push(toast(i, &format!("toast {i}"), ToastLevel::Info));
        }
        let rendered = draw_at(80, 24, &state);
        assert!(rendered.contains("+2 more"));
        // The three most recent (2, 3, 4) are visible; the two oldest are not.
        assert!(rendered.contains("toast 2"));
        assert!(rendered.contains("toast 3"));
        assert!(rendered.contains("toast 4"));
        assert!(!rendered.contains("toast 0"));
        assert!(!rendered.contains("toast 1"));
    }

    #[test]
    fn no_overflow_line_at_exactly_three() {
        let mut state = AppState::default();
        for i in 0..3 {
            state
                .toasts
                .push(toast(i, &format!("toast {i}"), ToastLevel::Info));
        }
        let rendered = draw_at(80, 24, &state);
        assert!(!rendered.contains("more"));
    }

    /// `10-13`: `toast_style_per_level` — each level renders with a visibly distinct role;
    /// checked structurally (four different `Style`s used) rather than pinning to specific theme
    /// colours, which is `theme`'s own concern.
    #[test]
    fn toast_style_per_level() {
        let theme = Theme::default();
        let roles: Vec<Role> = [
            ToastLevel::Info,
            ToastLevel::Success,
            ToastLevel::Warning,
            ToastLevel::Error,
        ]
        .into_iter()
        .map(role_for_level)
        .collect();
        let styles: std::collections::HashSet<_> =
            roles.iter().map(|&r| style::fg(&theme, r)).collect();
        assert_eq!(
            styles.len(),
            4,
            "all four levels must resolve to distinct styles"
        );
    }

    /// `10-13`: `toasts_overlay_without_resizing_layout` — rendering with zero, one, and five
    /// toasts must never change `layout::zones`'s own output; this widget only ever draws *over*
    /// an already-final area, never asked to influence it.
    #[test]
    fn toasts_overlay_without_resizing_layout() {
        let area = Rect::new(0, 0, 100, 30);
        let empty_state = AppState::default();
        let mut full_state = AppState::default();
        for i in 0..5 {
            full_state
                .toasts
                .push(toast(i, &format!("toast {i}"), ToastLevel::Info));
        }
        let zones_empty = crate::layout::zones(area, &empty_state);
        let zones_full = crate::layout::zones(area, &full_state);
        assert_eq!(zones_empty.header, zones_full.header);
        assert_eq!(zones_empty.sidebar, zones_full.sidebar);
        assert_eq!(zones_empty.canvas, zones_full.canvas);
        assert_eq!(zones_empty.player, zones_full.player);
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let mut state = AppState::default();
        state.toasts.push(toast(0, "hi", ToastLevel::Info));
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn toast_snapshot_stacked() {
        let mut state = AppState::default();
        state
            .toasts
            .push(toast(0, "working offline", ToastLevel::Warning));
        state
            .toasts
            .push(toast(1, "saved to Late Night Drives", ToastLevel::Success));
        state.toasts.push(toast(
            2,
            "could not save: server returned 500",
            ToastLevel::Error,
        ));
        let rendered = draw_at(80, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    /// A real bug: the box was a fixed 40 cells (38 inner), so this exact 39-character message
    /// rendered as "...availabl" — the last letter silently eaten (`docs/12-decisions.md`).
    #[test]
    fn a_message_longer_than_the_minimum_width_is_not_cut_off() {
        let message = "no bit-perfect capable output available";
        assert_eq!(message.len(), 39, "the reported message's own length");
        let mut state = AppState::default();
        state.toasts.push(toast(1, message, ToastLevel::Warning));

        let rendered = draw_at(80, 24, &state);
        assert!(rendered.contains(message), "{rendered}");
    }

    #[test]
    fn a_message_too_wide_for_the_terminal_is_visibly_ellipsized() {
        let mut state = AppState::default();
        state.toasts.push(toast(
            1,
            "a message far wider than this narrow terminal could ever show",
            ToastLevel::Info,
        ));
        let rendered = draw_at(24, 8, &state);
        assert!(rendered.contains('\u{2026}'), "{rendered}");
    }

    #[test]
    fn toast_snapshot_overflow() {
        let mut state = AppState::default();
        for i in 0..6 {
            state
                .toasts
                .push(toast(i, &format!("event {i}"), ToastLevel::Info));
        }
        let rendered = draw_at(80, 24, &state);
        insta::assert_snapshot!(rendered);
    }
}
