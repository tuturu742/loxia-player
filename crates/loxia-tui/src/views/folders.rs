//! Folders tab: raw directory-tree browsing (`07-05`), a Miller stack of `Folders { of_parent }`
//! columns rendered by `views::miller`/`widgets::column` unchanged.
//!
//! The only thing this view adds is the offline gate — unlike Favourites/Playlists, which render
//! whatever's cached and only show a connection-needed message once genuinely empty, folders are
//! not represented in the download sidecars at all (`docs/12-decisions.md`), so any cached column
//! contents would show a tree that's misleadingly partial (some subfolders explored, most not).
//! Offline always replaces the whole tab with the explanatory message, regardless of what's cached.

use loxia_core::state::AppState;
use loxia_core::state::Connectivity;
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

    if state.connectivity == Connectivity::Offline {
        render_empty_message(f, canvas, "folder browsing needs a connection", theme);
        return Vec::new();
    }

    super::miller::render(f, canvas, state, theme, hits, art)
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
    use loxia_core::state::nav::{Column, ColumnKind, NavFocus, Tab};
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

    fn folders_state() -> AppState {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Folders;
        let mut col = Column::new(ColumnKind::Folders { of_parent: None }, "Folders");
        col.items = vec![loxia_core::model::MediaItem::Folder(
            loxia_core::model::Folder {
                id: loxia_core::model::ItemId::from("folder-1"),
                name: "Albums".to_string(),
            },
        )];
        state.nav.per_tab_stacks.insert(Tab::Folders, vec![col]);
        state.nav.focus = NavFocus::Column(0);
        state
    }

    #[test]
    fn offline_shows_explanatory_state() {
        let mut state = folders_state();
        state.connectivity = Connectivity::Offline;
        let rendered = render_at(100, 24, &state);
        assert!(rendered.contains("folder browsing needs a connection"));
        assert!(
            !rendered.contains("Albums"),
            "cached tree must not show through while offline"
        );
    }

    #[test]
    fn online_renders_the_miller_stack() {
        let state = folders_state();
        let rendered = render_at(100, 24, &state);
        assert!(rendered.contains("Albums"));
    }

    #[test]
    fn folders_snapshot() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Folders;
        let a = fixtures::artist("Loose Files");
        let alb = fixtures::album("Untagged", 2020, &a);
        let mut col = Column::new(ColumnKind::Folders { of_parent: None }, "Folders");
        col.items = vec![
            loxia_core::model::MediaItem::Folder(loxia_core::model::Folder {
                id: loxia_core::model::ItemId::from("folder-albums"),
                name: "Albums".to_string(),
            }),
            loxia_core::model::MediaItem::Track(fixtures::track("track2", 2, &alb, &[&a])),
            loxia_core::model::MediaItem::Track(fixtures::track("track10", 10, &alb, &[&a])),
        ];
        col.load = loxia_core::state::nav::LoadState::Loaded { total: 3 };
        state.nav.per_tab_stacks.insert(Tab::Folders, vec![col]);
        state.nav.focus = NavFocus::Column(0);

        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
        assert!(rendered.contains('📁'));
        assert!(rendered.contains('♪'));
    }

    #[test]
    fn empty_folder_snapshot() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Folders;
        let mut col = Column::new(ColumnKind::Folders { of_parent: None }, "Folders");
        col.load = loxia_core::state::nav::LoadState::Loaded { total: 0 };
        state.nav.per_tab_stacks.insert(Tab::Folders, vec![col]);
        state.nav.focus = NavFocus::Column(0);

        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
        assert!(rendered.contains("nothing here — no audio files in this folder"));
    }
}
