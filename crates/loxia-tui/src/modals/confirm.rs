//! Generic confirmation modal (`docs/02-data-model.md` §6's `Modal::Confirm`). `reducer::modal::
//! open_confirm` existed since `03-07` but was never actually dispatched by anything until `11-02`
//! (the keymap editor's "reset all bindings" needed a real yes/no gate) — this is the first render
//! for it too; the generic placeholder box every other still-unbuilt modal falls back to
//! (`render::render_modal_placeholder`) only ever showed the bare title `"Confirm"`, with no room
//! for the caller-supplied `prompt` at all.

use loxia_core::state::AppState;
use loxia_core::state::modal::Modal;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::style;

const TITLE: &str = "CONFIRM";

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let Some(Modal::Confirm { prompt, .. }) = &state.modal else {
        return;
    };
    if area.width < 3 || area.height < 3 {
        return;
    }

    let width = 60.min(area.width).max(1);
    let height = 5.min(area.height).max(1);
    let modal_area = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .style(style::modal_surface(theme))
        .border_style(style::fg(theme, Role::Warning))
        .title(TITLE);
    let inner = block.inner(modal_area);
    // Wipe whatever the view underneath drew before painting the modal — a `Block` only paints its
    // border, so without this the canvas text showed *through* the modal body (seen in the field
    // with the sort menu over Now Playing). `docs/12-decisions.md`.
    f.render_widget(ratatui::widgets::Clear, modal_area);
    f.render_widget(block, modal_area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let footer_height = 1.min(inner.height);
    let prompt_height = inner.height - footer_height;
    let prompt_area = Rect::new(inner.x, inner.y, inner.width, prompt_height);
    let footer_area = Rect::new(inner.x, inner.y + prompt_height, inner.width, footer_height);

    if prompt_height > 0 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                prompt.clone(),
                style::style(theme, Role::Fg),
            )))
            .wrap(Wrap { trim: true }),
            prompt_area,
        );
    }
    if footer_height > 0 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "[Enter] confirm    [Esc] cancel",
                style::fg(theme, Role::Dim),
            )))
            .alignment(Alignment::Center),
            footer_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use loxia_core::action::{Action, ModalAction};
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn state_with(modal: Modal) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(modal);
        state
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

    fn confirm(prompt: &str) -> Modal {
        Modal::Confirm {
            prompt: prompt.to_string(),
            on_confirm: Box::new(Action::Modal(ModalAction::Close)),
        }
    }

    #[test]
    fn renders_the_prompt() {
        let state = state_with(confirm("reset every keybinding to its default?"));
        let rendered = draw_at(80, 20, &state);
        assert!(rendered.contains("reset every keybinding to its default?"));
    }

    #[test]
    fn footer_shows_both_keys() {
        let state = state_with(confirm("delete this playlist?"));
        let rendered = draw_at(80, 20, &state);
        assert!(rendered.contains("Enter"));
        assert!(rendered.contains("confirm"));
        assert!(rendered.contains("Esc"));
        assert!(rendered.contains("cancel"));
    }

    #[test]
    fn does_not_render_for_other_modal_kinds() {
        let state = state_with(Modal::SleepTimer {
            trigger: loxia_core::state::player::SleepTrigger::EndOfTrack,
            fade_out: false,
            quit_after: false,
            field_cursor: 0,
        });
        let rendered = draw_at(80, 20, &state);
        assert!(!rendered.contains(TITLE));
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let state = state_with(confirm("test"));
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn confirm_snapshot() {
        let state = state_with(confirm(
            "reset every keybinding to its default? this cannot be undone.",
        ));
        insta::assert_snapshot!(draw_at(80, 20, &state));
    }
}
