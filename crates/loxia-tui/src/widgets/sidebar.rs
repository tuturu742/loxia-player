//! Nine-tab sidebar navigation widget (`docs/07-ui-spec.md` §4).

use loxia_core::keymap::ActionId;
use loxia_core::state::AppState;
use loxia_core::state::nav::{NavFocus, Tab};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::hit::{HitMap, HitTarget};
use crate::style::{self, selection};
use crate::text;

/// Sidebar tabs, in `Alt+1`..`9`/`Alt+0` order (matches `reducer::nav::TAB_ORDER`,
/// `docs/04-state-and-input.md` §6).
const TAB_ORDER: [Tab; 10] = [
    Tab::NowPlaying,
    Tab::Favourites,
    Tab::Search,
    Tab::Playlists,
    Tab::Artists,
    Tab::AlbumArtists,
    Tab::Albums,
    Tab::Genres,
    Tab::Folders,
    Tab::Settings,
];

pub(crate) fn tab_label(tab: Tab) -> &'static str {
    match tab {
        Tab::NowPlaying => "Now Playing",
        Tab::Favourites => "Favourites",
        Tab::Search => "Search",
        Tab::Playlists => "Playlists",
        Tab::Artists => "Artists",
        Tab::AlbumArtists => "Album Artists",
        Tab::Albums => "Albums",
        Tab::Genres => "Genres",
        Tab::Folders => "Folders",
        Tab::Settings => "Settings",
    }
}

/// Neither `docs/07-ui-spec.md` §4 nor `Theme::glyphs()` (`04-01`) names a per-tab icon set — only
/// the two rows shown in the mock-up (`♪` Now Playing, `♡` Favourites) are given. The rest are this
/// task's own reasonable choices; the `ascii_only` column keeps every glyph a single ASCII byte for
/// vintage-terminal themes, the same intent as `Theme::glyphs()`. See `docs/12-decisions.md`.
fn tab_glyph(tab: Tab, ascii_only: bool) -> char {
    match (tab, ascii_only) {
        (Tab::NowPlaying, false) => '♪',
        (Tab::NowPlaying, true) => '>',
        (Tab::Favourites, false) => '♡',
        (Tab::Favourites, true) => '+',
        (Tab::Search, false) => '⚲',
        (Tab::Search, true) => '?',
        // `≡` (U+2261), not `☰` (U+2630): the trigram is East Asian **Wide**, a genuine two
        // cells, so the Playlists label started a column right of every other tab's
        // (`docs/12-decisions.md`). Only the ascii_only themes escaped it, since they never draw
        // it at all. `sidebar_glyphs_are_one_cell_wide` guards the whole set.
        (Tab::Playlists, false) => '≡',
        (Tab::Playlists, true) => '=',
        (Tab::Artists, false) => '☻',
        (Tab::Artists, true) => 'A',
        // Not a second face: `☺` (U+263A), the obvious pair for `☻`, is emoji-capable and so
        // banned from the chrome (`text::CHROME_GLYPHS`). A filled ring from the same geometric
        // family as Albums' `◫` reads as "the credited artist behind the albums" instead.
        (Tab::AlbumArtists, false) => '◉',
        (Tab::AlbumArtists, true) => 'B',
        (Tab::Albums, false) => '◫',
        (Tab::Albums, true) => 'L',
        (Tab::Genres, false) => '#',
        (Tab::Genres, true) => '#',
        (Tab::Folders, false) => '⌂',
        (Tab::Folders, true) => 'F',
        (Tab::Settings, false) => '⛭',
        (Tab::Settings, true) => '*',
    }
}

fn action_id_for(tab: Tab) -> ActionId {
    match tab {
        Tab::NowPlaying => ActionId::JumpTab1,
        Tab::Favourites => ActionId::JumpTab2,
        Tab::Search => ActionId::JumpTab3,
        Tab::Playlists => ActionId::JumpTab4,
        Tab::Artists => ActionId::JumpTab5,
        Tab::AlbumArtists => ActionId::JumpTab6,
        Tab::Albums => ActionId::JumpTab7,
        Tab::Genres => ActionId::JumpTab8,
        Tab::Folders => ActionId::JumpTab9,
        Tab::Settings => ActionId::JumpTab10,
    }
}

/// `hint_for` renders the whole binding (`"alt+3"`, `"f3"`, `"unbound"`) — only the trailing digit
/// is shown here, so a remapped `JumpTabN` still displays correctly without hardcoding the number
/// (`docs/07-ui-spec.md` §4: "do not hardcode").
fn digit_hint(hint: &str) -> String {
    hint.chars()
        .last()
        .filter(char::is_ascii_digit)
        .map(String::from)
        .unwrap_or_default()
}

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    let sidebar_focused = state.nav.focus == NavFocus::Sidebar;

    for (i, &tab) in TAB_ORDER.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }
        let row_area = Rect::new(area.x, area.y + i as u16, area.width, 1);
        hits.push(row_area, HitTarget::SidebarTab(tab));

        let active = state.nav.active_tab == tab;
        let marker = if active && sidebar_focused && !state.nav.sidebar_quit_focused {
            "▸ "
        } else {
            "  "
        };
        let glyph = crate::text::narrow_glyph(tab_glyph(tab, theme.ascii_only));
        let hint = digit_hint(&state.keymap.hint_for(action_id_for(tab)));

        let width = row_area.width as usize;
        let hint_w = text::width(&hint);
        let left_budget = width.saturating_sub(hint_w + 1);
        let left = format!("{marker}{glyph} {}", tab_label(tab));
        let left = text::ellipsize(&left, left_budget);
        let left_padded = text::pad_to(&left, width.saturating_sub(hint_w));

        let row_style = if active {
            selection(theme, true)
        } else {
            style::style(theme, Role::Fg)
        };
        let hint_style = row_style.fg(style::to_color(theme.color(Role::Dim)));

        let line = Line::from(vec![
            Span::styled(left_padded, row_style),
            Span::styled(hint, hint_style),
        ]);
        f.render_widget(Paragraph::new(line), row_area);
    }

    // A clickable "Quit" row directly beneath the tabs — not a tab (it switches nothing), so it's
    // rendered here rather than joining `TAB_ORDER`, and it carries its own `QuitButton` hit target.
    let quit_i = TAB_ORDER.len() as u16;
    if quit_i < area.height {
        let row_area = Rect::new(area.x, area.y + quit_i, area.width, 1);
        hits.push(row_area, HitTarget::QuitButton);
        let focused = state.nav.sidebar_quit_focused;
        let glyph = if theme.ascii_only { 'x' } else { '\u{23fb}' };
        let marker = if focused { "▸ " } else { "  " };
        let text = format!("{marker}{glyph} Quit");
        let label = text::ellipsize(&text, row_area.width as usize).into_owned();
        let quit_style = if focused {
            selection(theme, true)
        } else {
            style::style(theme, Role::Dim)
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(label, quit_style))),
            row_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::keymap::KeyMap;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(state: &AppState, theme: &Theme) -> (String, HitMap) {
        let backend = TestBackend::new(16, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, theme, &mut hits)
            })
            .unwrap();
        (format!("{:?}", terminal.backend().buffer()), hits)
    }

    fn state_with_keymap() -> AppState {
        AppState {
            keymap: KeyMap::defaults(),
            ..AppState::default()
        }
    }

    #[test]
    fn sidebar_snapshot_default() {
        let state = state_with_keymap();
        let theme = Theme::default();
        let (rendered, _) = render_at(&state, &theme);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn sidebar_snapshot_focused() {
        let mut state = state_with_keymap();
        state.nav.focus = NavFocus::Sidebar;
        let theme = Theme::default();
        let (rendered, _) = render_at(&state, &theme);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn sidebar_marks_active_tab() {
        let mut state = state_with_keymap();
        state.nav.focus = NavFocus::Sidebar;
        state.nav.active_tab = Tab::Favourites;
        let theme = Theme::default();
        let (rendered, _) = render_at(&state, &theme);
        assert!(rendered.contains("Favourites"));
        assert!(rendered.contains('▸'));
    }

    #[test]
    fn sidebar_hint_reflects_remapped_key() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("jump_tab3".to_string(), "alt+9".to_string());
        let (keymap, _warnings) = KeyMap::from_config(&overrides);
        let state = AppState {
            keymap,
            ..AppState::default()
        };
        let theme = Theme::default();
        let (rendered, _) = render_at(&state, &theme);
        let search_line = rendered.lines().find(|l| l.contains("Search")).unwrap();
        assert!(search_line.contains('9'));
        assert!(!search_line.contains('3'));
    }

    #[test]
    fn quit_row_renders_and_registers_a_hit_target_below_the_tabs() {
        // A taller sidebar than `render_at`'s own, so the Quit row (beneath the last tab) fits.
        let backend = TestBackend::new(16, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut hits = HitMap::default();
        let state = state_with_keymap();
        let theme = Theme::default();
        terminal
            .draw(|f| render(f, f.area(), &state, &theme, &mut hits))
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Quit"), "{rendered}");
        assert_eq!(
            hits.hit(1, TAB_ORDER.len() as u16),
            Some(&HitTarget::QuitButton),
            "the Quit row sits directly beneath the tabs and is clickable"
        );
    }

    /// The sidebar is a fixed 16 columns and every row is `"<marker> <glyph> <label> <digit>"`, so
    /// a glyph the terminal draws as two cells shifts that row's label right of all the others.
    /// `☰` (U+2630) is East Asian Wide and did exactly that to Playlists — visible in every theme
    /// except the two `ascii_only` ones, which never draw it (`docs/12-decisions.md`).
    #[test]
    fn sidebar_glyphs_are_one_cell_wide() {
        for &tab in TAB_ORDER.iter() {
            for ascii_only in [false, true] {
                let glyph = tab_glyph(tab, ascii_only);
                assert_eq!(
                    crate::text::width(&glyph.to_string()),
                    1,
                    "{tab:?}'s glyph {glyph:?} (ascii_only={ascii_only}) is not one cell wide"
                );
            }
        }
    }

    #[test]
    fn sidebar_registers_a_hit_target_for_every_tab() {
        let state = state_with_keymap();
        let theme = Theme::default();
        let (_, hits) = render_at(&state, &theme);
        for (i, &tab) in TAB_ORDER.iter().enumerate() {
            assert_eq!(
                hits.hit(1, i as u16),
                Some(&HitTarget::SidebarTab(tab)),
                "missing hit target for {tab:?}"
            );
        }
    }
}
