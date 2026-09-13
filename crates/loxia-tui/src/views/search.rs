//! Search tab: debounced query, three result sections (`07-01`, `docs/07-ui-spec.md` §9).
//!
//! The section list itself is `widgets::sectioned_list`, shared with the Favourites tab
//! (`07-02`) — this module only owns the query line on top of it.

use loxia_core::state::AppState;
use loxia_core::state::nav::LoadState;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget};
use crate::style;
use crate::text;
use crate::widgets::sectioned_list;

/// The query line's own bordered row.
const QUERY_HEIGHT: u16 = 3;

pub fn render(f: &mut Frame, canvas: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    if canvas.width == 0 || canvas.height == 0 {
        return;
    }

    let query_height = QUERY_HEIGHT.min(canvas.height);
    let query_area = Rect::new(canvas.x, canvas.y, canvas.width, query_height);
    render_query(f, query_area, state, theme, hits);

    let sections_area = Rect::new(
        canvas.x,
        canvas.y + query_height,
        canvas.width,
        canvas.height.saturating_sub(query_height),
    );

    let query = state.search.query.trim();
    if query.is_empty() {
        render_empty_message(f, sections_area, "type to search", theme);
        return;
    }
    let total = state.search.results.artists.len()
        + state.search.results.albums.len()
        + state.search.results.tracks.len();
    if total == 0 && matches!(state.search.load, LoadState::Loaded { total: 0 }) {
        render_empty_message(
            f,
            sections_area,
            &format!("no results for \"{query}\""),
            theme,
        );
        return;
    }

    // A result section is highlighted only while the results themselves are being browsed — not
    // while the query line is focused, nor while focus has stepped out to the tab sidebar
    // (`nav.sidebar_focused`), where `↑`/`↓` switch tabs rather than moving over results.
    let focused_section = if state.search.query_focused || state.nav.sidebar_focused {
        None
    } else {
        Some(state.search.focused_section)
    };
    sectioned_list::render(
        f,
        sections_area,
        &state.search.results,
        &state.search.load,
        focused_section,
        state.search.cursors,
        // Search queries artists, albums and tracks — never playlists, so it never shows that
        // section (`widgets::sectioned_list::render`).
        &[
            loxia_core::state::search::SearchSection::Artists,
            loxia_core::state::search::SearchSection::Albums,
            loxia_core::state::search::SearchSection::Tracks,
        ],
        theme,
        hits,
    );
}

/// `10-13`: "`type to search` before a query; `no results for \"<q>\"` after" (this task's own
/// table) — wrapped and vertically centered, the same shape every other view's own empty-state
/// helper already uses.
fn render_empty_message(f: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let lines = text::wrap(message, area.width as usize);
    let total = lines.len() as u16;
    let start_y = area.y + area.height.saturating_sub(total) / 2;
    for (i, line) in lines.iter().enumerate() {
        let y = start_y + i as u16;
        if y >= area.y + area.height {
            break;
        }
        let row = Rect::new(area.x, y, area.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                line.clone(),
                style::fg(theme, Role::Dim),
            )))
            .alignment(Alignment::Center),
            row,
        );
    }
}

fn render_query(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    let focused = state.search.query_focused;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::border(theme, focused))
        .title("SEARCH");
    let inner = block.inner(area);
    f.render_widget(block, area);
    hits.push(area, HitTarget::SearchQuery);

    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut text = state.search.query.clone();
    if focused {
        text.push('█');
    }
    let truncated = text::truncate(&text, inner.width as usize).into_owned();
    let line = Line::from(Span::raw(truncated));
    f.render_widget(
        Paragraph::new(line),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::state::AppState;
    use loxia_core::state::nav::Tab;
    use loxia_core::state::search::SearchResults;
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

    fn search_state() -> AppState {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Search;
        state
    }

    #[test]
    fn search_snapshot_results() {
        let mut state = search_state();
        state.search.query = "boy harsher".to_string();
        state.search.query_focused = false;
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        state.search.results = SearchResults {
            artists: vec![a.clone()],
            albums: vec![alb.clone()],
            tracks: vec![
                fixtures::track("Motion", 1, &alb, &[&a]),
                fixtures::track("Fate", 2, &alb, &[&a]),
            ],
            artists_error: None,
            albums_error: None,
            tracks_error: None,
            playlists: Vec::new(),
            playlists_error: None,
        };
        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn search_snapshot_empty() {
        let mut state = search_state();
        state.search.query = "xyz".to_string();
        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn search_snapshot_loading() {
        let mut state = search_state();
        state.search.query = "boy harsher".to_string();
        state.search.load = loxia_core::state::nav::LoadState::Loading;
        let rendered = render_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    /// `10-13`: "`type to search` before a query".
    #[test]
    fn type_to_search_shown_before_a_query() {
        let state = search_state();
        assert!(state.search.query.is_empty());
        let rendered = render_at(100, 24, &state);
        assert!(rendered.contains("type to search"));
    }

    /// `10-13`: "`no results for \"<q>\"` after" a query that finished loading empty.
    #[test]
    fn no_results_shown_after_a_completed_empty_search() {
        let mut state = search_state();
        state.search.query = "xyz123".to_string();
        state.search.load = loxia_core::state::nav::LoadState::Loaded { total: 0 };
        let rendered = render_at(100, 24, &state);
        assert!(rendered.contains("no results for \"xyz123\""));
    }
}
