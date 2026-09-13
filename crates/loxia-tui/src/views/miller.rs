//! Sliding-window Miller column composition — the view behind five of the nine sidebar tabs
//! (`docs/07-ui-spec.md` §5, `docs/02-data-model.md` §3).

use loxia_core::state::AppState;
use loxia_core::state::nav::NavFocus;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::hit::HitMap;
use crate::layout::plan_canvas;
use crate::style;
use crate::text;
use crate::widgets::column::render_column;
use crate::widgets::sidebar::tab_label;

pub fn render(
    f: &mut Frame,
    canvas: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
    art: &mut crate::widgets::album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    let Some(stack) = state.nav.per_tab_stacks.get(&state.nav.active_tab) else {
        render_placeholder(f, canvas, state, theme);
        return Vec::new();
    };
    if stack.is_empty() {
        render_placeholder(f, canvas, state, theme);
        return Vec::new();
    }

    let window_start = state.nav.window_start.min(stack.len() - 1);
    // The reducer guarantees at most 3 columns from `window_start` onward (`03-06`); the width
    // ladder (`plan_canvas`) may narrow that further. When it does, the *rightmost* (focused)
    // columns must survive, not the leftmost — a 1-wide degraded view should still show the
    // column the user is actually looking at.
    let available = &stack[window_start..];
    let plan = plan_canvas(canvas, available.len());
    let visible_count = plan.columns.len();
    let start_within = available.len() - visible_count;
    let visible = &available[start_within..];
    let absolute_start = window_start + start_within;

    let focus_depth = match state.nav.focus {
        NavFocus::Column(depth) => Some(depth),
        _ => None,
    };

    for (i, (col, &area)) in visible.iter().zip(plan.columns.iter()).enumerate() {
        let absolute_index = absolute_start + i;
        let focused = focus_depth == Some(absolute_index);

        if i == 0 && absolute_start > 0 {
            let ancestor_titles: Vec<&str> = stack[..absolute_start]
                .iter()
                .map(|c| c.title.as_str())
                .collect();
            let path = format!("{}/", ancestor_titles.join("/"));
            let elided = text::ellipsize_start(&path, 40);
            render_column_with_title(
                f,
                area,
                col,
                absolute_index,
                focused,
                state,
                theme,
                hits,
                &elided,
            );
        } else {
            render_column(f, area, col, absolute_index, focused, state, theme, hits);
        }
    }

    match plan.inspector {
        Some(inspector_area) => {
            crate::widgets::inspector::render(f, inspector_area, state, theme, hits, art)
        }
        None => Vec::new(),
    }
}

/// `render_column`'s own signature has no room for an overridden title (it reads `col.title`
/// directly) — the parent-path indicator needs one. Rather than change that signature (used
/// exactly as given by `04-06`), render a column whose `title` field is temporarily swapped for
/// the elided ancestor path; everything else about the column (items, cursor, selection, load
/// state) is untouched.
#[allow(clippy::too_many_arguments)]
fn render_column_with_title(
    f: &mut Frame,
    area: Rect,
    col: &loxia_core::state::nav::Column,
    idx: usize,
    focused: bool,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
    title: &str,
) {
    let mut with_title = col.clone();
    with_title.title = title.to_string();
    render_column(f, area, &with_title, idx, focused, state, theme, hits);
}

fn render_placeholder(f: &mut Frame, canvas: Rect, state: &AppState, theme: &Theme) {
    if canvas.width == 0 || canvas.height == 0 {
        return;
    }
    let label = tab_label(state.nav.active_tab);
    let text = format!("{label} will load here");
    let row = Rect::new(canvas.x, canvas.y + canvas.height / 2, canvas.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(text))
            .alignment(Alignment::Center)
            .style(style::fg(theme, Role::Dim)),
        row,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::model::{Genre, ItemId, MediaItem};
    use loxia_core::state::nav::{Column, ColumnKind, NavFocus, Tab};
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16, state: &AppState) -> (String, HitMap) {
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
        (format!("{:?}", terminal.backend().buffer()), hits)
    }

    #[test]
    fn miller_snapshot_three_columns() {
        let state = fixtures::fixture_miller_3col();
        let (rendered, _) = render_at(180, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn miller_snapshot_sliding_window() {
        let state = fixtures::fixture_miller_5col();
        let (rendered, _) = render_at(180, 20, &state);
        insta::assert_snapshot!(rendered);
        // Columns 3-5 (0-indexed 2..5) are visible: window_start = 2 in the fixture. Column 2's
        // own title is replaced by the parent-path indicator (checked separately below), so this
        // looks at each visible column's item content instead.
        assert!(rendered.contains("Child 2"));
        assert!(rendered.contains("Child 3"));
        assert!(rendered.contains("Child 4"));
        assert!(rendered.contains("Folder 3"));
        assert!(rendered.contains("Folder 4"));
    }

    #[test]
    fn parent_path_indicator_absent_at_window_start_zero() {
        let state = fixtures::fixture_miller_3col();
        assert_eq!(state.nav.window_start, 0);
        let (rendered, _) = render_at(180, 20, &state);
        assert!(!rendered.contains('…'));
    }

    /// Gives every column in a 5-deep stack a long, folder-path-like title, so the combined
    /// ancestor path is guaranteed to exceed the indicator's budget regardless of column width —
    /// `fixture_miller_5col`'s own short `"Folder N"` titles are too short to ever need eliding.
    fn lengthen_titles(state: &mut AppState, tab: Tab) {
        if let Some(stack) = state.nav.per_tab_stacks.get_mut(&tab) {
            for (i, col) in stack.iter_mut().enumerate() {
                col.title = format!("Very Long Nested Folder Name Number {i}");
            }
        }
    }

    #[test]
    fn parent_path_is_start_elided() {
        let mut state = fixtures::fixture_miller_5col();
        let tab = state.nav.active_tab;
        lengthen_titles(&mut state, tab);
        let (rendered, _) = render_at(180, 20, &state);
        assert!(rendered.contains('…'), "expected an elided ancestor path");
        let title_line = rendered.lines().find(|l| l.contains('…')).unwrap();
        // The tail (closer ancestor, column 1) survives; the head (column 0) is cut.
        assert!(title_line.contains("Number 1"));
        assert!(!title_line.contains("Number 0"));
    }

    #[test]
    fn focused_column_gets_focus_border() {
        let state = fixtures::fixture_miller_3col();
        let (rendered, _) = render_at(180, 20, &state);
        // Just confirm it renders without panicking and includes the focused column's content;
        // exact border colour is covered by `widgets::column`'s own tests.
        assert!(rendered.contains("Tracks"));
    }

    #[test]
    fn empty_stack_renders_placeholder() {
        let state = fixtures::fixture_empty();
        assert_eq!(state.nav.active_tab, Tab::NowPlaying);
        let (rendered, _) = render_at(80, 24, &state);
        assert!(rendered.contains("will load here"));
    }

    #[test]
    fn view_does_not_mutate_window_start() {
        let state = fixtures::fixture_miller_5col();
        let before = state.nav.window_start;
        let _ = render_at(180, 20, &state);
        assert_eq!(state.nav.window_start, before);
    }

    #[test]
    fn miller_snapshot_80x24_single_column() {
        let state = fixtures::fixture_miller_3col();
        let (rendered, _) = render_at(80, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    /// `07-04`: a genre-drilled `Genres → GenreArtists → Albums → Tracks` stack, focus at the
    /// deepest level — the first tab to ever reach 4 Miller levels, exercising the sliding window
    /// one column further than any prior tab.
    fn fixture_genres_4col() -> AppState {
        let mut state = fixtures::fixture_empty();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);

        let mut genres_col = Column::new(ColumnKind::Genres, "Genres");
        genres_col.items = vec![MediaItem::Genre(Genre {
            id: ItemId::from("genre-darkwave"),
            name: "Darkwave".to_string(),
        })];

        let mut artists_col = Column::new(
            ColumnKind::GenreArtists {
                of_genre: "Darkwave".to_string(),
            },
            "Darkwave — Artists",
        );
        artists_col.items = vec![MediaItem::Artist(a.clone())];

        let mut albums_col = Column::new(
            ColumnKind::Albums {
                of_artist: Some(a.id.clone()),
            },
            format!("{} — Albums", a.name),
        );
        albums_col.items = vec![MediaItem::Album(alb.clone())];

        let mut tracks_col = Column::new(
            ColumnKind::Tracks {
                of_album: alb.id.clone(),
            },
            format!("{} — Tracks", alb.name),
        );
        tracks_col.items = vec![MediaItem::Track(t)];

        state.nav.active_tab = Tab::Genres;
        state.nav.per_tab_stacks.insert(
            Tab::Genres,
            vec![genres_col, artists_col, albums_col, tracks_col],
        );
        state.nav.focus = NavFocus::Column(3);
        state.nav.window_start = 1;
        state
    }

    #[test]
    fn genres_snapshot_four_columns() {
        let state = fixture_genres_4col();
        let (rendered, _) = render_at(180, 20, &state);
        insta::assert_snapshot!(rendered);
        assert!(rendered.contains("Boy Harsher"));
        assert!(rendered.contains("Care"));
        assert!(rendered.contains("Motion"));
    }

    #[test]
    fn parent_path_shows_genre_at_level_four() {
        let state = fixture_genres_4col();
        assert_eq!(
            state.nav.window_start, 1,
            "levels 2-4 (depths 1-3) visible, level 1 (Genres) slid out of view"
        );
        let (rendered, _) = render_at(180, 20, &state);
        // The shared sliding-window breadcrumb (`04-07`) shows the *hidden* column's own title —
        // "Genres" here — the same mechanism `parent_path_is_start_elided` already covers for
        // Folders, where each hidden column happens to carry a real per-instance name. The Genres
        // tab's level-1 column is a flat, generic list ("Genres" always, regardless of which genre
        // was drilled into), so unlike Folders this never surfaces the *specific* genre name in
        // the breadcrumb itself — an accepted limitation of reusing the mechanism unchanged
        // (`docs/12-decisions.md`).
        assert!(
            rendered.contains("Genres/"),
            "breadcrumb shows the hidden root column's own title"
        );
    }

    #[test]
    fn miller_snapshot_appears_on_split() {
        let state = fixtures::fixture_appears_on();
        let (rendered, _) = render_at(180, 20, &state);
        insta::assert_snapshot!(rendered);
        assert!(rendered.contains("APPEARS ON"));
    }
}
