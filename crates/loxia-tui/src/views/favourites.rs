//! Favourites tab: the sectioned layout Search uses (`07-01`), via the shared
//! `widgets::sectioned_list`, over `Filters=IsFavorite` data (`07-02`, `docs/07-ui-spec.md` §9).

use loxia_core::keymap::ActionId;
use loxia_core::state::AppState;
use loxia_core::state::Connectivity;
use loxia_core::state::nav::LoadState;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::hit::HitMap;
use crate::style;
use crate::text;
use crate::widgets::sectioned_list;

pub fn render(f: &mut Frame, canvas: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    if canvas.width == 0 || canvas.height == 0 {
        return;
    }

    let results = &state.favourites.results;
    let total = results.artists.len()
        + results.albums.len()
        + results.tracks.len()
        + results.playlists.len();

    if total == 0 {
        // "The tab renders from cached data when available and otherwise shows the offline
        // empty state" — cached data is just whatever's already in `results` from a previous
        // successful fetch; this branch only ever runs when there genuinely is none.
        if state.connectivity == Connectivity::Offline {
            render_empty_message(f, canvas, "offline — favourites need a connection", theme);
            return;
        }
        if matches!(state.favourites.load, LoadState::Loaded { total: 0 }) {
            let hint = state.keymap.hint_for(ActionId::ToggleFavorite);
            let message = format!("no favourites yet — press {hint} on anything to add it");
            render_empty_message(f, canvas, &message, theme);
            return;
        }
        // Still loading, or errored with nothing to show yet — fall through so
        // `sectioned_list` renders each section's own loading/error line.
    }

    // Highlighted only while the favourites themselves are being browsed — not while focus has
    // stepped out to the tab sidebar, where `↑`/`↓` switch tabs rather than moving over rows. The
    // same rule `views::search` applies, now that Favourites has a cursor to move at all.
    let focused_section = (!state.nav.sidebar_focused).then_some(state.favourites.focused_section);
    sectioned_list::render(
        f,
        canvas,
        results,
        &state.favourites.load,
        focused_section,
        state.favourites.cursors,
        // Everything Emby will favourite, playlists included.
        &loxia_core::state::search::SearchSection::ALL,
        theme,
        hits,
    );
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
    use loxia_core::state::nav::Tab;
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
                render(f, area, state, &theme, &mut hits)
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    fn favourites_state() -> AppState {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Favourites;
        state.keymap = KeyMap::defaults();
        state
    }

    #[test]
    fn favourites_snapshot() {
        let mut state = favourites_state();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        state.favourites.results = loxia_core::state::search::SearchResults {
            artists: vec![a.clone()],
            albums: vec![alb.clone()],
            tracks: vec![fixtures::track("Motion", 1, &alb, &[&a])],
            artists_error: None,
            albums_error: None,
            tracks_error: None,
            playlists: Vec::new(),
            playlists_error: None,
        };
        state.favourites.load = LoadState::Loaded { total: 3 };
        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn favourites_snapshot_empty() {
        let mut state = favourites_state();
        state.favourites.load = LoadState::Loaded { total: 0 };
        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn empty_state_shows_keymap_hint() {
        let mut state = favourites_state();
        state.favourites.load = LoadState::Loaded { total: 0 };
        let rendered = render_at(100, 24, &state);
        assert!(rendered.contains("no favourites yet"));
        assert!(
            rendered.contains('f'),
            "the keymap hint for ToggleFavorite ('f') must appear"
        );
    }

    #[test]
    fn favourites_splits_into_three_sections() {
        let mut state = favourites_state();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        state.favourites.results = loxia_core::state::search::SearchResults {
            artists: vec![a.clone()],
            albums: vec![alb.clone()],
            tracks: vec![fixtures::track("Motion", 1, &alb, &[&a])],
            artists_error: None,
            albums_error: None,
            tracks_error: None,
            playlists: Vec::new(),
            playlists_error: None,
        };
        state.favourites.load = LoadState::Loaded { total: 3 };
        let rendered = render_at(100, 24, &state);
        assert!(rendered.contains("ARTISTS"));
        assert!(rendered.contains("ALBUMS"));
        assert!(rendered.contains("TRACKS"));
    }
}
