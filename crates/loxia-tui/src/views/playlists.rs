//! Playlists tab: a two-column Miller stack (`Playlists → PlaylistTracks`, `07-03`), rendered
//! entirely by the shared `views::miller`/`widgets::column` machinery already built in phase 04.
//!
//! The only thing this view adds is the "no playlists at all" empty state.
//! `widgets::column::empty_message`'s generic `ColumnKind::Playlists` text
//! ("nothing here — no playlists") has no room for a keymap hint, and every other tab sharing
//! that widget must stay message-only — so the task's `no playlists — press {P} to save the
//! queue as one` copy is special-cased here instead. The empty-*playlist* case
//! ("nothing here — this playlist is empty") is exactly the message `empty_message` already
//! renders for `ColumnKind::PlaylistTracks`, so it needs no override.

use loxia_core::keymap::ActionId;
use loxia_core::state::AppState;
use loxia_core::state::nav::{ColumnKind, LoadState, Tab};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::hit::HitMap;
use crate::style;
use crate::text;

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

    if no_playlists_loaded(state) {
        let hint = state.keymap.hint_for(ActionId::SaveQueueAsPlaylist);
        let message = format!("no playlists — press {hint} to save the queue as one");
        render_empty_message(f, canvas, &message, theme);
        return Vec::new();
    }

    super::miller::render(f, canvas, state, theme, hits, art)
}

/// True only once the `Playlists` list column has actually finished loading with zero rows —
/// still-loading, errored, or not-yet-visited all fall through to `views::miller::render`, which
/// already renders the right loading/error/placeholder state for those (mirrors `07-02`'s
/// favourites empty-state gate).
fn no_playlists_loaded(state: &AppState) -> bool {
    let Some(stack) = state.nav.per_tab_stacks.get(&Tab::Playlists) else {
        return false;
    };
    let Some(column) = stack.iter().find(|c| c.kind == ColumnKind::Playlists) else {
        return false;
    };
    column.items.is_empty() && matches!(column.load, LoadState::Loaded { total: 0 })
}

fn render_empty_message(f: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let width = area.width as usize;
    let lines = text::wrap(message, width);
    let start_row = area.y + area.height / 2;
    for (i, line) in lines.iter().enumerate() {
        let row_y = start_row + i as u16;
        if row_y >= area.y + area.height {
            break;
        }
        let row = Rect::new(area.x, row_y, area.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(line.clone()))
                .alignment(Alignment::Center)
                .style(style::fg(theme, Role::Dim)),
            row,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::keymap::KeyMap;
    use loxia_core::state::nav::{Column, NavFocus};
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16, state: &AppState) -> String {
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
        format!("{:?}", terminal.backend().buffer())
    }

    fn playlist_item(id: &str, name: &str, track_count: u32) -> loxia_core::model::MediaItem {
        loxia_core::model::MediaItem::Playlist(loxia_core::model::Playlist {
            id: loxia_core::model::ItemId::from(id),
            name: name.to_string(),
            overview: None,
            track_count,
            total_duration: std::time::Duration::ZERO,
            can_edit: true,
            is_favorite: false,
        })
    }

    fn playlists_state() -> AppState {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Playlists;
        state.keymap = KeyMap::defaults();
        state
    }

    #[test]
    fn empty_playlist_and_no_playlists_states() {
        // No playlists at all: the dedicated hint-bearing message, not the generic column one.
        let mut state = playlists_state();
        let mut col = Column::new(ColumnKind::Playlists, "Playlists");
        col.load = LoadState::Loaded { total: 0 };
        state.nav.per_tab_stacks.insert(Tab::Playlists, vec![col]);
        let rendered = render_at(100, 24, &state);
        assert!(rendered.contains("no playlists"));
        assert!(
            rendered.contains('P'),
            "the SaveQueueAsPlaylist hint ('P') must appear"
        );
        assert!(!rendered.contains("nothing here — no playlists"));

        // A playlist with no tracks: the generic `PlaylistTracks` empty message, unmodified.
        let mut state = playlists_state();
        let mut playlists_col = Column::new(ColumnKind::Playlists, "Playlists");
        playlists_col.items = vec![playlist_item("playlist-1", "Empty One", 0)];
        playlists_col.load = LoadState::Loaded { total: 1 };
        let mut tracks_col = Column::new(
            ColumnKind::PlaylistTracks {
                of_playlist: loxia_core::model::ItemId::from("playlist-1"),
            },
            "Empty One",
        );
        tracks_col.load = LoadState::Loaded { total: 0 };
        state
            .nav
            .per_tab_stacks
            .insert(Tab::Playlists, vec![playlists_col, tracks_col]);
        state.nav.focus = NavFocus::Column(1);
        let rendered = render_at(100, 24, &state);
        // `10-13`'s wording is checked in two pieces, not as one contiguous substring — this
        // narrow Miller column wraps it across two lines (`render_empty`'s own wrapping,
        // deliberate: `docs/12-decisions.md`).
        assert!(rendered.contains("nothing here"));
        assert!(rendered.contains("is empty"));
    }

    #[test]
    fn playlists_snapshot() {
        let mut state = playlists_state();
        let mut col = Column::new(ColumnKind::Playlists, "Playlists");
        col.items = vec![
            playlist_item("playlist-1", "Late Night Drives", 12),
            playlist_item("playlist-2", "Focus", 30),
        ];
        col.load = LoadState::Loaded { total: 2 };
        state.nav.per_tab_stacks.insert(Tab::Playlists, vec![col]);
        state.nav.focus = NavFocus::Column(0);
        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn playlist_tracks_snapshot() {
        let mut state = playlists_state();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let t2 = fixtures::playlist_track("Fate", 2, &alb, &[&a], "entry-2");

        let mut playlists_col = Column::new(ColumnKind::Playlists, "Playlists");
        playlists_col.items = vec![playlist_item("playlist-1", "Late Night Drives", 2)];
        playlists_col.load = LoadState::Loaded { total: 1 };

        let mut tracks_col = Column::new(
            ColumnKind::PlaylistTracks {
                of_playlist: loxia_core::model::ItemId::from("playlist-1"),
            },
            "Late Night Drives",
        );
        tracks_col.items = vec![
            loxia_core::model::MediaItem::Track(t1),
            loxia_core::model::MediaItem::Track(t2),
        ];
        tracks_col.load = LoadState::Loaded { total: 2 };

        state
            .nav
            .per_tab_stacks
            .insert(Tab::Playlists, vec![playlists_col, tracks_col]);
        state.nav.focus = NavFocus::Column(1);
        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }
}
