//! In-UI keybinding remapper (`11-02`, `docs/04-state-and-input.md` §7). Every row is generated
//! from the live `KeyMap`, grouped by `HelpCategory` exactly like the help modal's own cheat
//! sheet — this file reuses that module's per-action label table and declared category order
//! directly (`crate::modals::help::{action_label, category_title, CATEGORY_ORDER}`) rather than
//! re-declaring 72 labels a second time.

use loxia_core::keymap::parse::render_binding;
use loxia_core::keymap::{ActionId, HelpCategory, KeyMap};
use loxia_core::state::AppState;
use loxia_core::state::modal::Modal;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use strum::IntoEnumIterator;

use crate::hit::{HitMap, HitTarget};
use crate::modals::help::{CATEGORY_ORDER, action_label, category_title};
use crate::style;

const TITLE: &str = "KEYBINDINGS";
const UNBOUND: &str = "—";
const MODAL_WIDTH: u16 = 100;

/// One flat row this modal draws — either a category header (not selectable) or a real action row
/// (selectable; one per `ActionId`, `ActionId::iter`'s own order, re-grouped by `CATEGORY_ORDER`
/// the same way `modals::help` groups its own columns).
enum Row {
    Header(HelpCategory),
    Action(ActionId),
}

fn flat_rows() -> Vec<Row> {
    let mut rows = Vec::new();
    for &cat in &CATEGORY_ORDER {
        rows.push(Row::Header(cat));
        for action in ActionId::iter().filter(|a| a.help_category() == cat) {
            rows.push(Row::Action(action));
        }
    }
    rows
}

/// Position within `flat_rows()` of the `action_cursor`-th `Row::Action` — the two indices differ
/// because header rows are interspersed between groups, so this walks counting only action rows.
fn list_index_for_cursor(rows: &[Row], action_cursor: usize) -> Option<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Row::Action(_)))
        .nth(action_cursor)
        .map(|(i, _)| i)
}

/// Every other action currently sharing at least one of `action`'s own live bindings, per
/// `KeyMap::validate`'s insertion-history conflict list (the same source `views::settings`'s own
/// per-section badge uses, `11-01`) — a conflict is reported against *every* binding in the
/// pairing, so this simply collects whichever entries name `action` and returns the others.
fn conflicting_with(keymap: &KeyMap, action: ActionId) -> Vec<ActionId> {
    let mut others = Vec::new();
    for conflict in keymap.validate() {
        if conflict.actions.contains(&action) {
            for &other in &conflict.actions {
                if other != action && !others.contains(&other) {
                    others.push(other);
                }
            }
        }
    }
    others
}

fn any_conflict_in(keymap: &KeyMap, cat: HelpCategory) -> bool {
    keymap
        .validate()
        .iter()
        .any(|c| c.actions.iter().any(|a| a.help_category() == cat))
}

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    let Some(Modal::KeymapEditor {
        action_cursor,
        capturing,
        conflict,
        captured,
        ..
    }) = &state.modal
    else {
        return;
    };
    if area.width < 3 || area.height < 3 {
        return;
    }

    let width = MODAL_WIDTH.min(area.width).max(1);
    let height = area.height.saturating_sub(2).clamp(1, area.height);
    let modal_area = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .style(style::modal_surface(theme))
        .border_style(style::border(theme, true))
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

    let status_height = 1.min(inner.height);
    let footer_height = 1.min(inner.height.saturating_sub(status_height));
    let list_height = inner.height - status_height - footer_height;

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height);
    let status_area = Rect::new(inner.x, inner.y + list_height, inner.width, status_height);
    let footer_area = Rect::new(
        inner.x,
        inner.y + list_height + status_height,
        inner.width,
        footer_height,
    );

    render_list(f, list_area, state, *action_cursor, theme, hits);
    render_status(
        f,
        status_area,
        *capturing,
        *conflict,
        captured.as_ref(),
        theme,
    );
    render_footer(f, footer_area, *conflict, theme);
}

#[allow(clippy::too_many_arguments)]
fn render_list(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    action_cursor: usize,
    theme: &Theme,
    hits: &mut HitMap,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = flat_rows();
    let cursor_index = list_index_for_cursor(&rows, action_cursor).unwrap_or(0);

    // Auto-scrolled to keep the cursor row visible — no persisted scroll offset of its own (unlike
    // `Modal::Help.scroll`), since `action_cursor` alone is always enough to recompute the same
    // window on every render.
    let height = area.height as usize;
    let start = if rows.len() <= height {
        0
    } else {
        cursor_index
            .saturating_sub(height / 2)
            .min(rows.len() - height)
    };

    // A real bug found in the field: this used to start at `0` unconditionally, so once the list
    // had actually scrolled (`start > 0`) it undercounted every action row skipped before the
    // visible window — `is_cursor` (below) then never matched `action_cursor` again for the rest
    // of the session, and `HitTarget::ModalField(this_index)` pointed a click at the wrong action
    // entirely. Seeding it with however many `Row::Action` entries precede `start` keeps it a true
    // *global* action index throughout, matching `action_cursor`'s own numbering
    // (`docs/12-decisions.md`).
    let mut flat_action_index = rows[..start]
        .iter()
        .filter(|r| matches!(r, Row::Action(_)))
        .count();
    for (i, row) in rows.iter().enumerate().skip(start).take(height) {
        let y = area.y + (i - start) as u16;
        let row_area = Rect::new(area.x, y, area.width, 1);
        match row {
            Row::Header(cat) => {
                let warn = if any_conflict_in(&state.keymap, *cat) {
                    "⚠ "
                } else {
                    ""
                };
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!("{warn}{}", category_title(*cat)),
                        style::fg(theme, Role::Accent).add_modifier(Modifier::BOLD),
                    ))),
                    row_area,
                );
            }
            Row::Action(action) => {
                let this_index = flat_action_index;
                flat_action_index += 1;
                let is_cursor = this_index == action_cursor;
                let hint = state.keymap.hint_for(*action);
                let key = if hint == "unbound" {
                    UNBOUND.to_string()
                } else {
                    hint
                };
                let others = conflicting_with(&state.keymap, *action);
                let mut text = format!("  {:<24} {key}", action_label(*action));
                if !others.is_empty() {
                    let names = others
                        .iter()
                        .map(|a| action_label(*a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    text.push_str(&format!("          ⚠ also bound to: {names}"));
                }
                let row_style = if is_cursor {
                    style::selection(theme, true)
                } else if !others.is_empty() {
                    style::style(theme, Role::Warning)
                } else {
                    style::style(theme, Role::Fg)
                };
                hits.push(row_area, HitTarget::ModalField(this_index));
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(text, row_style))),
                    row_area,
                );
            }
        }
    }
}

fn render_status(
    f: &mut Frame,
    area: Rect,
    capturing: bool,
    conflict: Option<ActionId>,
    captured: Option<&loxia_core::keymap::KeyBinding>,
    theme: &Theme,
) {
    if area.height == 0 {
        return;
    }
    let (text, role) = if let Some(incumbent) = conflict {
        let chord = captured
            .map(render_binding)
            .unwrap_or_else(|| "?".to_string());
        (
            format!("{chord} is already bound to {}", action_label(incumbent)),
            Role::Error,
        )
    } else if capturing {
        ("press a key…".to_string(), Role::Accent)
    } else {
        (String::new(), Role::Fg)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(text, style::style(theme, role)))),
        area,
    );
}

fn render_footer(f: &mut Frame, area: Rect, conflict: Option<ActionId>, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    let text = if let Some(incumbent) = conflict {
        format!(
            "[Enter] rebind anyway (unbinds {})    [Esc] cancel",
            action_label(incumbent)
        )
    } else {
        "[Enter] remap    [d] reset    [x] unbind    [R] reset all    [Esc] close".to_string()
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(text, style::fg(theme, Role::Dim))))
            .alignment(Alignment::Center),
        area,
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn state_with(modal: Modal) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.keymap = KeyMap::defaults();
        state.modal = Some(modal);
        state
    }

    fn editor(action_cursor: usize) -> Modal {
        Modal::KeymapEditor {
            action_cursor,
            capturing: false,
            conflict: None,
            captured: None,
            capture_deadline: None,
        }
    }

    fn draw_at(w: u16, h: u16, state: &AppState) -> (String, HitMap) {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme, &mut hits)
            })
            .unwrap();
        (format!("{:?}", terminal.backend().buffer()), hits)
    }

    #[test]
    fn renders_category_headers_and_action_labels() {
        let state = state_with(editor(0));
        let (rendered, _) = draw_at(100, 60, &state);
        assert!(rendered.contains("PLAYBACK"));
        assert!(rendered.contains("Play / Pause"));
    }

    #[test]
    fn unbound_action_shows_dash() {
        let mut raw: BTreeMap<String, String> = BTreeMap::new();
        raw.insert("play_pause".to_string(), "none".to_string());
        let (keymap, _) = KeyMap::from_config(&raw);
        let mut state = state_with(editor(0));
        state.keymap = keymap;
        let (rendered, _) = draw_at(100, 60, &state);
        assert!(rendered.contains(UNBOUND));
    }

    #[test]
    fn capturing_shows_press_a_key_prompt() {
        let state = state_with(Modal::KeymapEditor {
            action_cursor: 0,
            capturing: true,
            conflict: None,
            captured: None,
            capture_deadline: None,
        });
        let (rendered, _) = draw_at(100, 60, &state);
        assert!(rendered.contains("press a key"));
    }

    #[test]
    fn conflict_shows_incumbent_and_rebind_footer() {
        let mut raw: BTreeMap<String, String> = BTreeMap::new();
        raw.insert("stop".to_string(), "space".to_string());
        let (keymap, _) = KeyMap::from_config(&raw);
        let action_cursor = ActionId::iter()
            .position(|a| a == ActionId::PlayPause)
            .unwrap();
        let mut state = state_with(Modal::KeymapEditor {
            action_cursor,
            capturing: false,
            conflict: Some(ActionId::Stop),
            captured: Some(loxia_core::keymap::parse::parse_binding("space").unwrap()),
            capture_deadline: None,
        });
        state.keymap = keymap;
        let (rendered, _) = draw_at(100, 60, &state);
        assert!(rendered.contains("already bound to Stop"));
        assert!(rendered.contains("rebind anyway"));
    }

    #[test]
    fn conflicting_rows_show_warning_badge() {
        let mut raw: BTreeMap<String, String> = BTreeMap::new();
        raw.insert("stop".to_string(), "space".to_string());
        let (keymap, _) = KeyMap::from_config(&raw);
        let mut state = state_with(editor(0));
        state.keymap = keymap;
        let (rendered, _) = draw_at(100, 60, &state);
        assert!(rendered.contains("also bound to"));
        // The category header for whichever group `PlayPause`/`Stop` conflict in also carries the
        // badge.
        assert!(rendered.contains("⚠ PLAYBACK"));
    }

    #[test]
    fn cursor_row_registers_a_modal_field_hit() {
        let state = state_with(editor(0));
        let (_, hits) = draw_at(100, 60, &state);
        assert!(
            (0..60u16)
                .filter_map(|row| hits.hit(4, row))
                .any(|t| matches!(t, HitTarget::ModalField(0)))
        );
    }

    #[test]
    fn does_not_render_for_other_modal_kinds() {
        let state = state_with(Modal::Confirm {
            prompt: "test".to_string(),
            on_confirm: Box::new(loxia_core::action::Action::Modal(
                loxia_core::action::ModalAction::Close,
            )),
        });
        let (rendered, _) = draw_at(100, 60, &state);
        assert!(!rendered.contains(TITLE));
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let state = state_with(editor(0));
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    /// A real bug found in the field: once the list had scrolled past its first page, the visible
    /// window's own per-row action-index counter restarted at `0` instead of continuing from
    /// however many `Row::Action` entries preceded it — so neither the "is this the cursor" row
    /// highlight nor `HitTarget::ModalField`'s own click index ever matched `action_cursor` again
    /// for the rest of the session (small, wrong numbers instead of the real one). The user's own
    /// report: "when i scroll through keyboard shortcut editor it scrolls further down then on the
    /// screen." Proven here via the hit map: a large `action_cursor` well past one screen's worth
    /// of rows must still register `HitTarget::ModalField(action_cursor)` — under the old bug the
    /// visible window's own indices never got anywhere near that value.
    #[test]
    fn hit_target_index_stays_correct_after_scrolling() {
        let cursor = 40; // `ActionId` has 72 variants; well past a 20-row viewport
        let state = state_with(editor(cursor));
        let (_, hits) = draw_at(100, 20, &state);
        let found = (0..20u16).any(|row| {
            hits.hit(4, row)
                .is_some_and(|t| matches!(t, HitTarget::ModalField(i) if *i == cursor))
        });
        assert!(found, "no row registered HitTarget::ModalField({cursor})");
    }

    #[test]
    fn scrolls_to_keep_cursor_visible() {
        // `ActionId::iter()` has 72 variants — a cursor near the end must still be visible in a
        // short viewport, proving the auto-scroll window actually follows it.
        let last = ActionId::iter().count() - 1;
        let state = state_with(editor(last));
        let (rendered, _) = draw_at(100, 20, &state);
        assert!(rendered.contains(action_label(ActionId::iter().next_back().unwrap())));
    }

    #[test]
    fn keymap_editor_snapshot() {
        let state = state_with(editor(0));
        let (rendered, _) = draw_at(100, 40, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn keymap_editor_snapshot_capturing() {
        let state = state_with(Modal::KeymapEditor {
            action_cursor: 0,
            capturing: true,
            conflict: None,
            captured: None,
            capture_deadline: None,
        });
        let (rendered, _) = draw_at(100, 40, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn keymap_editor_snapshot_conflict() {
        let mut raw: BTreeMap<String, String> = BTreeMap::new();
        raw.insert("stop".to_string(), "space".to_string());
        let (keymap, _) = KeyMap::from_config(&raw);
        let action_cursor = ActionId::iter()
            .position(|a| a == ActionId::PlayPause)
            .unwrap();
        let mut state = state_with(Modal::KeymapEditor {
            action_cursor,
            capturing: false,
            conflict: Some(ActionId::Stop),
            captured: Some(loxia_core::keymap::parse::parse_binding("space").unwrap()),
            capture_deadline: None,
        });
        state.keymap = keymap;
        let (rendered, _) = draw_at(100, 40, &state);
        insta::assert_snapshot!(rendered);
    }
}
