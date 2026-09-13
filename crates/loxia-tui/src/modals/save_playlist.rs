//! Save queue/selection to playlist modal (`10-08`, `design_overview` §3.4): `P` saves the active
//! queue, `Ctrl+P` the current selection (or the focused item), to a new or existing playlist.

use loxia_core::model::{MediaItem, PlaylistId};
use loxia_core::state::AppState;
use loxia_core::state::modal::{Modal, PlaylistTarget, SaveSource};
use loxia_core::state::nav::Tab;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::HitMap;
use crate::style;

const MODAL_WIDTH: u16 = 78;

/// Mirrors `reducer::modal::save_playlist_tracks`'s own counting logic (never its full sort/track
/// collection — the widget only ever needs *how many*) — duplicated, not shared, since the
/// reducer's own version is crate-private and `loxia-tui` cannot reach into `loxia-core`'s
/// reducer internals; the same cross-crate-duplication shape `clamp_eq_gain` already established
/// (`09-03`). `SaveSource::Selection`'s "or the focused item" fallback is mirrored too, so the
/// summary line's count always matches what will actually be saved.
fn track_count_for_save(state: &AppState, source: SaveSource) -> usize {
    match source {
        SaveSource::Queue => state.queue.entries.len(),
        SaveSource::Selection => {
            let Some(column) = state.active_column() else {
                return 0;
            };
            if !column.selection.selected.is_empty() {
                column
                    .items
                    .iter()
                    .filter(|item| {
                        matches!(item, MediaItem::Track(t) if column.selection.selected.contains(&t.id))
                    })
                    .count()
            } else {
                usize::from(matches!(
                    column.items.get(column.cursor),
                    Some(MediaItem::Track(_))
                ))
            }
        }
    }
}

fn source_label(source: SaveSource) -> &'static str {
    match source {
        SaveSource::Queue => "Active Queue",
        SaveSource::Selection => "Selection",
    }
}

fn target_label(state: &AppState, target: &PlaylistTarget) -> String {
    match target {
        PlaylistTarget::New => "Create New Playlist...".to_string(),
        PlaylistTarget::Existing(id) => {
            existing_playlist_name(state, id).unwrap_or_else(|| id.as_str().to_string())
        }
    }
}

fn existing_playlist_name(state: &AppState, id: &PlaylistId) -> Option<String> {
    state
        .nav
        .per_tab_stacks
        .get(&Tab::Playlists)
        .and_then(|stack| stack.first())
        .and_then(|column| {
            column.items.iter().find_map(|item| match item {
                MediaItem::Playlist(p) if p.id.as_str() == id.as_str() => Some(p.name.clone()),
                _ => None,
            })
        })
}

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, _hits: &mut HitMap) {
    let Some(Modal::SavePlaylist {
        target,
        name,
        overview,
        autosort,
        field,
        source,
        error,
        ..
    }) = &state.modal
    else {
        return;
    };
    if area.width < 3 || area.height < 3 {
        return;
    }

    let is_new = *target == PlaylistTarget::New;
    // summary, blank, target row, (name + description rows, only for New), blank, sort checkbox,
    // (error row, only when set), blank, footer — 7 rows always present, plus the conditional ones.
    let content_rows = 7 + if is_new { 2 } else { 0 } + usize::from(error.is_some());
    let width = MODAL_WIDTH.min(area.width).max(1);
    let height = ((content_rows + 2) as u16).min(area.height).max(1);
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
        .title("SAVE TO PLAYLIST");
    let inner = block.inner(modal_area);
    // Wipe whatever the view underneath drew before painting the modal — a `Block` only paints its
    // border, so without this the canvas text showed *through* the modal body (seen in the field
    // with the sort menu over Now Playing). `docs/12-decisions.md`.
    f.render_widget(ratatui::widgets::Clear, modal_area);
    f.render_widget(block, modal_area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let end_y = inner.y + inner.height;
    let mut y = inner.y;
    let fg = style::style(theme, Role::Fg);
    let dim = style::fg(theme, Role::Dim);
    let accent = style::fg(theme, Role::Accent);
    let row_style = |focused: bool| if focused { accent } else { fg };

    let count = track_count_for_save(state, *source);
    y = draw_line(
        f,
        inner,
        y,
        end_y,
        &format!("Tracks to save: {count} tracks ({})", source_label(*source)),
        dim,
    );
    y = draw_line(f, inner, y, end_y, "", fg);

    y = draw_line(
        f,
        inner,
        y,
        end_y,
        &format!(
            "Target Playlist : [ {} \u{25bc} ]",
            target_label(state, target)
        ),
        row_style(*field == 0),
    );

    if is_new {
        y = draw_line(
            f,
            inner,
            y,
            end_y,
            &format!("Playlist Name   : [ {name} ]"),
            row_style(*field == 1),
        );
        y = draw_line(
            f,
            inner,
            y,
            end_y,
            &format!("Description     : [ {overview} ]"),
            row_style(*field == 2),
        );
    }

    y = draw_line(f, inner, y, end_y, "", fg);
    y = draw_line(
        f,
        inner,
        y,
        end_y,
        &format!(
            "[{}] Sort tracks with the active sort profile before saving",
            if *autosort { "X" } else { " " }
        ),
        row_style(*field == 3),
    );

    if let Some(message) = error {
        y = draw_line(
            f,
            inner,
            y,
            end_y,
            message,
            style::style(theme, Role::Error),
        );
    }

    y = draw_line(f, inner, y, end_y, "", fg);
    let verb = if is_new { "Save" } else { "Add" };
    draw_line(
        f,
        inner,
        y,
        end_y,
        &format!("[ Enter ] {verb}    [ Esc ] Cancel"),
        dim,
    );
}

fn draw_line(
    f: &mut Frame,
    inner: Rect,
    y: u16,
    end_y: u16,
    text: &str,
    style: ratatui::style::Style,
) -> u16 {
    if y < end_y {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text.to_string(), style))),
            Rect::new(inner.x, y, inner.width, 1),
        );
    }
    y + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn modal(
        target: PlaylistTarget,
        name: &str,
        overview: &str,
        autosort: bool,
        field: usize,
        error: Option<&str>,
    ) -> Modal {
        Modal::SavePlaylist {
            target,
            target_cursor: 0,
            name: name.to_string(),
            overview: overview.to_string(),
            autosort,
            field,
            source: SaveSource::Queue,
            error: error.map(str::to_string),
        }
    }

    fn state_with(m: Modal) -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        state.modal = Some(m);
        state
    }

    fn draw_at(w: u16, h: u16, state: &AppState) -> String {
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
        format!("{:?}", terminal.backend().buffer())
    }

    #[test]
    fn summary_shows_source_and_count() {
        let state = state_with(modal(PlaylistTarget::New, "", "", false, 0, None));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("Tracks to save: 10 tracks (Active Queue)"));
    }

    #[test]
    fn new_target_shows_name_and_description() {
        let state = state_with(modal(
            PlaylistTarget::New,
            "Late Night Darkwave",
            "",
            false,
            0,
            None,
        ));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("Playlist Name"));
        assert!(rendered.contains("Late Night Darkwave"));
        assert!(rendered.contains("Description"));
        assert!(rendered.contains("[ Enter ] Save"));
    }

    #[test]
    fn existing_target_hides_name_and_description() {
        let mut state = state_with(modal(
            PlaylistTarget::Existing(PlaylistId::from("pl-1")),
            "",
            "",
            false,
            0,
            None,
        ));
        state.nav.per_tab_stacks.insert(
            Tab::Playlists,
            vec![playlists_column(vec![("pl-1", "My Mix")])],
        );
        let rendered = draw_at(90, 20, &state);
        assert!(!rendered.contains("Playlist Name"));
        assert!(!rendered.contains("Description"));
        assert!(rendered.contains("My Mix"));
    }

    #[test]
    fn existing_target_button_says_add() {
        let state = state_with(modal(
            PlaylistTarget::Existing(PlaylistId::from("pl-1")),
            "",
            "",
            false,
            0,
            None,
        ));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("[ Enter ] Add"));
        assert!(!rendered.contains("[ Enter ] Save"));
    }

    fn playlists_column(playlists: Vec<(&str, &str)>) -> loxia_core::state::nav::Column {
        let mut column = loxia_core::state::nav::Column::new(
            loxia_core::state::nav::ColumnKind::Playlists,
            "Playlists",
        );
        column.items = playlists
            .into_iter()
            .map(|(id, name)| {
                MediaItem::Playlist(loxia_core::model::Playlist {
                    id: loxia_core::model::ItemId::from(id),
                    name: name.to_string(),
                    overview: None,
                    track_count: 0,
                    total_duration: std::time::Duration::ZERO,
                    can_edit: true,
                    is_favorite: false,
                })
            })
            .collect();
        column
    }

    #[test]
    fn inline_error_shown_when_set() {
        let state = state_with(modal(
            PlaylistTarget::New,
            "",
            "",
            false,
            0,
            Some("name is required"),
        ));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("name is required"));
    }

    #[test]
    fn does_not_render_for_other_modal_kinds() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::Help {
            context: loxia_core::keymap::InputContext::Normal,
            scroll: 0,
        });
        let rendered = draw_at(90, 20, &state);
        assert!(!rendered.contains("SAVE TO PLAYLIST"));
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let state = state_with(modal(PlaylistTarget::New, "x", "", false, 0, None));
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn save_playlist_snapshot_new() {
        let state = state_with(modal(
            PlaylistTarget::New,
            "Late Night Darkwave",
            "",
            false,
            1,
            None,
        ));
        let rendered = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn save_playlist_snapshot_existing() {
        let mut state = state_with(modal(
            PlaylistTarget::Existing(PlaylistId::from("pl-1")),
            "",
            "",
            true,
            3,
            None,
        ));
        state.nav.per_tab_stacks.insert(
            Tab::Playlists,
            vec![playlists_column(vec![("pl-1", "My Mix")])],
        );
        let rendered = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn save_playlist_snapshot_error() {
        let state = state_with(modal(
            PlaylistTarget::New,
            "",
            "",
            false,
            1,
            Some("name is required"),
        ));
        let rendered = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }
}
