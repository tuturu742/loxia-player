//! `draw(frame, &AppState, &Theme, &mut HitMap)` — the single render entry point every later
//! widget hangs off (`docs/07-ui-spec.md` §§1-2).

use loxia_core::state::AppState;
use loxia_core::state::modal::Modal;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::hit::HitMap;
use crate::layout::zones;
use crate::style::style;
use crate::widgets::{header, player_bar, sidebar, toast};

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

/// Renders the whole frame in order: header, sidebar, canvas, player bar, toasts, then the modal
/// — modals last so they overlay everything else. Takes `&AppState` and never mutates it; the
/// only out-parameter is `hits`, cleared and repopulated every call.
///
/// In this task the canvas is a placeholder block with correct borders and title for the four
/// tabs without a dedicated view yet (phase 07); the header, sidebar, and player bar are real
/// widgets as of `04-05`/`04-09`.
pub fn draw(
    f: &mut Frame,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
    art: &mut crate::widgets::album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    hits.clear();
    let area = f.area();

    // Paint the theme's own background across the whole frame first. Widgets only ever colour the
    // cells they actually write, and many use `style::fg` (no background component at all), so
    // without this base coat every gap between them kept the terminal's default background — which
    // is why a dark theme looked patchy rather than applied (`docs/12-decisions.md`). A theme whose
    // `bg` is `Reset` (the default) paints nothing, deliberately inheriting the terminal's own.
    f.render_widget(
        ratatui::widgets::Block::default().style(crate::style::style(theme, Role::Fg)),
        area,
    );

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(f, area, theme);
        return Vec::new();
    }

    let z = zones(area, state);

    header::render(f, z.header, state, theme, hits);
    if let Some(sidebar_area) = z.sidebar {
        sidebar::render(f, sidebar_area, state, theme, hits);
    }
    let effects = render_canvas(f, z.canvas, hits, theme, state, art);
    // Zen mode draws its own two-line footer inside the canvas (`views::zen`), so the global player
    // bar would sit directly beneath it as a second, duplicate status bar — a real doubling seen in
    // the field (`docs/12-decisions.md`).
    if !state.zen_mode {
        player_bar::render(f, z.player, state, theme, hits);
    }
    toast::render(f, area, state, theme);
    render_modal_placeholder(f, area, theme, state, hits);
    effects
}

/// Below `MIN_WIDTH`x`MIN_HEIGHT`, `Rect` arithmetic on the normal layout produces zero-width
/// areas and panicking widgets — render only a centred message and return before attempting it.
fn render_too_small(f: &mut Frame, area: Rect, theme: &Theme) {
    let message = format!("terminal too small (needs {MIN_WIDTH}x{MIN_HEIGHT})");
    let line = Paragraph::new(Line::from(message))
        .alignment(Alignment::Center)
        .style(style(theme, Role::Error));
    let row = area.height / 2;
    let center = Rect::new(area.x, area.y + row, area.width, 1.min(area.height));
    f.render_widget(line, center);
}

/// Dispatches on `state.nav.active_tab`: the three generic Miller-backed tabs (Artists, Albums,
/// Genres — `reducer::nav::seed_column_for_tab`'s own split) go through the real
/// `views::miller::render` (`04-07`); Search (`07-01`), Favourites (`07-02`), Playlists (`07-03`),
/// and Folders (`07-05`) have their own dedicated views — each is a Miller stack too, but needs its
/// own empty/offline state, so none of them share `render_canvas`'s generic-Miller branch even
/// though each still calls `views::miller::render` internally. NowPlaying (`07-06`) is not a
/// Miller tab at all — its own two-pane `views::now_playing::render`. Settings (`11-01`) isn't a
/// Miller tab either — its own two-pane section-list/controls `views::settings::render`. Zen mode
/// (`10-02`) replaces the whole canvas regardless of tab, checked first.
fn render_canvas(
    f: &mut Frame,
    area: Rect,
    hits: &mut HitMap,
    theme: &Theme,
    state: &AppState,
    art: &mut crate::widgets::album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    use loxia_core::state::nav::Tab;

    if state.zen_mode {
        return crate::views::zen::render(f, area, state, theme, hits, art);
    }
    if state.nav.active_tab == Tab::NowPlaying {
        return crate::views::now_playing::render(f, area, state, theme, hits, art);
    }

    match state.nav.active_tab {
        // The Miller-backed tabs return the inspector's own art effects; the rest draw no art.
        Tab::Artists | Tab::AlbumArtists | Tab::Albums | Tab::Genres => {
            crate::views::miller::render(f, area, state, theme, hits, art)
        }
        Tab::Playlists => crate::views::playlists::render(f, area, state, theme, hits, art),
        Tab::Folders => crate::views::folders::render(f, area, state, theme, hits, art),
        Tab::Search => {
            crate::views::search::render(f, area, state, theme, hits);
            Vec::new()
        }
        Tab::Favourites => {
            crate::views::favourites::render(f, area, state, theme, hits);
            Vec::new()
        }
        Tab::Settings => {
            crate::views::settings::render(f, area, state, theme, hits);
            Vec::new()
        }
        Tab::NowPlaying => unreachable!("handled above"),
    }
}

/// Dispatches to each open modal's own render function. `11-02`: `Modal::Confirm` (the keymap
/// editor's "reset all bindings" gate) was the last kind left on the generic placeholder box this
/// function used to fall back to for whatever `modals/*.rs` hadn't been built yet — with every
/// `ModalKind` now real, that fallback is unreachable, so this matches on `Modal` directly instead
/// of `if matches!`-chaining down to it: a future modal kind added without a render arm here now
/// fails to compile instead of silently drawing an empty box (`docs/12-decisions.md`).
fn render_modal_placeholder(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    state: &AppState,
    hits: &mut HitMap,
) {
    let Some(modal) = &state.modal else {
        return;
    };
    match modal {
        Modal::Help { context, scroll } => {
            crate::modals::help::render(f, area, state, *context, *scroll, theme);
        }
        Modal::DevicePicker { .. } => {
            crate::modals::device_picker::render(f, area, state, theme, hits);
        }
        Modal::Equalizer { .. } => {
            crate::modals::equalizer::render(f, area, state, theme, hits);
        }
        Modal::SleepTimer { .. } => {
            crate::modals::sleep_timer::render(f, area, state, theme, hits);
        }
        Modal::SavePlaylist { .. } => {
            crate::modals::save_playlist::render(f, area, state, theme, hits);
        }
        Modal::SortProfile { .. } => {
            crate::modals::sort_profile::render(f, area, state, theme, hits);
        }
        Modal::KeymapEditor { .. } => {
            crate::modals::keymap_editor::render(f, area, state, theme, hits);
        }
        Modal::Confirm { .. } => {
            crate::modals::confirm::render(f, area, state, theme);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::state::toast::{Toast, ToastLevel};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn draw_at(w: u16, h: u16, state: &AppState) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let mut off = crate::widgets::album_art::ArtOff::default();
                draw(f, state, &theme, &mut hits, &mut off.art());
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    /// `10-13`: `every_view_has_an_empty_state` — a table test rendering each tab's own canvas
    /// against a completely fresh `AppState` (no column stack ever seeded, no results, no
    /// history, no queue) and asserting real, readable text appears — never a blank canvas.
    /// Exercises `render_canvas`'s own dispatch directly, not the whole `draw()` chrome (header/
    /// sidebar/player bar always render something regardless of tab, which would make a
    /// whole-frame blankness check meaningless).
    #[test]
    fn every_view_has_an_empty_state() {
        use loxia_core::state::nav::Tab;
        for tab in [
            Tab::NowPlaying,
            Tab::Favourites,
            Tab::Search,
            Tab::Playlists,
            Tab::Artists,
            Tab::Albums,
            Tab::Genres,
            Tab::Folders,
            Tab::Settings,
        ] {
            let mut state = AppState {
                keymap: loxia_core::keymap::KeyMap::defaults(),
                ..AppState::default()
            };
            state.nav.active_tab = tab;

            let backend = TestBackend::new(80, 20);
            let mut terminal = Terminal::new(backend).unwrap();
            let theme = Theme::default();
            let mut hits = HitMap::default();
            terminal
                .draw(|f| {
                    let area = f.area();
                    let mut off = crate::widgets::album_art::ArtOff::default();
                    render_canvas(f, area, &mut hits, &theme, &state, &mut off.art());
                })
                .unwrap();
            let rendered = format!("{:?}", terminal.backend().buffer());
            let readable_chars = rendered.chars().filter(|c| c.is_alphabetic()).count();
            assert!(
                readable_chars > 5,
                "{tab:?}'s canvas rendered with no readable text at all — a blank empty state"
            );
        }
    }

    #[test]
    fn too_small_renders_message_only() {
        let state = AppState::default();
        let rendered = draw_at(40, 10, &state);
        assert!(rendered.contains("terminal too small (needs 80x24)"));
        assert!(
            !rendered.contains('┌'),
            "must not attempt the normal bordered layout"
        );
    }

    #[test]
    fn too_small_does_not_panic() {
        let state = AppState::default();
        for w in 1..MIN_WIDTH {
            for h in [1, 5, 23] {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    /// Zen draws its own footer inside the canvas; the global player bar underneath it produced a
    /// visibly doubled status bar (`docs/12-decisions.md`).
    #[test]
    fn zen_does_not_double_the_status_bar() {
        use loxia_core::test_support::fixtures;

        let mut state = fixtures::fixture_playing_queue();
        state.keymap = loxia_core::keymap::KeyMap::defaults();
        state.player.status = loxia_core::state::player::PlayStatus::Playing;

        // The transport row is drawn once per status bar, so counting it counts the bars.
        let count = |s: &str| s.matches('\u{23ef}').count();

        assert_eq!(count(&draw_at(120, 30, &state)), 1, "one bar outside Zen");
        state.zen_mode = true;
        let zen = draw_at(120, 30, &state);
        assert_eq!(
            count(&zen),
            1,
            "Zen must show its own footer only, not it *and* the global bar: {zen}"
        );
    }

    /// Zen's footer replaces the global player bar, so it must claim exactly the bar's own zone
    /// height and put its rows in the same places — otherwise the bottom panel visibly resizes and
    /// its contents shift as Zen is toggled (`docs/12-decisions.md`).
    #[test]
    fn the_status_bar_does_not_move_when_zen_is_toggled() {
        use loxia_core::test_support::fixtures;

        let mut state = fixtures::fixture_playing_queue();
        state.keymap = loxia_core::keymap::KeyMap::defaults();
        state.player.status = loxia_core::state::player::PlayStatus::Playing;
        // The technical readout only exists once mpv has reported a decoded format.
        state.player.format = Some(loxia_core::model::AudioFormat {
            codec: loxia_core::model::Codec::Flac,
            sample_rate_hz: 44_100,
            bit_depth: Some(16),
            channels: 2,
            bitrate_bps: Some(1_012_000),
        });

        // The transport row and the technical readout: the two rows whose position would give away
        // a change in the panel's height.
        let row_of = |s: &str, needle: &str| {
            s.lines()
                .position(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("{needle:?} not rendered in:\n{s}"))
        };

        let normal = draw_at(120, 30, &state);
        state.zen_mode = true;
        let zen = draw_at(120, 30, &state);

        for needle in ["\u{23ef}", "Bitrate:"] {
            assert_eq!(
                row_of(&zen, needle),
                row_of(&normal, needle),
                "{needle:?} moved when Zen was toggled"
            );
        }
    }

    /// A `Block` paints only its border, so without an explicit `Clear` the view underneath showed
    /// *through* the modal body — reported with the sort menu over Now Playing
    /// (`docs/12-decisions.md`).
    #[test]
    fn a_modal_body_is_not_bled_through_by_the_view_beneath() {
        use loxia_core::state::modal::{Modal, SortApplyTarget};
        use loxia_core::test_support::fixtures;

        let mut state = fixtures::fixture_playing_queue();
        state.keymap = loxia_core::keymap::KeyMap::defaults();
        state.nav.active_tab = loxia_core::state::nav::Tab::NowPlaying;
        state.modal = Some(Modal::SortProfile {
            profiles: state.config.sorting.profiles.clone(),
            cursor: 0,
            editing: None,
            target: SortApplyTarget::Queue,
        });

        let rendered = draw_at(120, 30, &state);
        // The modal's own rows: from its title border down to its closing border.
        let body: Vec<&str> = rendered
            .lines()
            .skip_while(|l| !l.contains("SORT PROFILE"))
            .take_while(|l| !l.contains("\u{2514}\u{2500}\u{2500}\u{2500}\u{2500}"))
            .collect();
        assert!(!body.is_empty(), "the modal did not render: {rendered}");

        for line in &body {
            // Whatever sits left of the modal's own left edge is the underlying view and is fine;
            // everything from the modal's border rightwards must be the modal's own content.
            let Some(inside) = line.split_once('\u{2502}').map(|(_, rest)| rest) else {
                continue;
            };
            assert!(
                !inside.contains("Boy Harsher") && !inside.contains("[album]"),
                "queue text bled into the modal body: {line}"
            );
        }
    }

    /// `10-02`: end-to-end through the real dispatch this task wired in (`render_canvas`'s own
    /// `state.zen_mode` check) — not just `layout::zones`'s own `zen_hides_sidebar` (which only
    /// proves the *zone* is `None`, not that nothing ends up drawn where it would have been).
    #[test]
    fn zen_hides_sidebar_and_columns() {
        use loxia_core::test_support::fixtures;

        let mut state = fixtures::fixture_playing_queue();
        state.zen_mode = true;
        state.keymap = loxia_core::keymap::KeyMap::defaults();
        let rendered = draw_at(120, 30, &state);

        assert!(
            !rendered.contains("Artists"),
            "the sidebar's own tab labels must not appear in Zen"
        );
        assert!(
            rendered.contains("Track 4"),
            "Zen's own now-playing content must render instead"
        );
    }

    #[test]
    fn modal_renders_on_top() {
        let state = AppState {
            modal: Some(Modal::Help {
                context: loxia_core::keymap::InputContext::Normal,
                scroll: 0,
            }),
            ..AppState::default()
        };
        let rendered = draw_at(100, 30, &state);
        // `10-03`: the help modal is a real widget now, not the generic
        // `format!("{:?}", modal.kind())` placeholder this test originally asserted on — its own
        // title (always in the border, never scrolled off) is the stable thing to check instead.
        assert!(rendered.contains("LOXIA KEYBOARD CHEAT SHEET"));
    }

    #[test]
    fn toasts_render_when_present() {
        let mut state = AppState::default();
        state.toasts.push(Toast {
            id: 0,
            message: "hi".to_string(),
            level: ToastLevel::Info,
            created_at: loxia_core::Timestamp::now(),
        });
        let rendered = draw_at(100, 30, &state);
        assert!(rendered.contains("hi"));
    }

    #[test]
    fn layout_snapshot_80x24() {
        insta::assert_snapshot!(draw_at(80, 24, &AppState::default()));
    }

    #[test]
    fn layout_snapshot_120x30() {
        insta::assert_snapshot!(draw_at(120, 30, &AppState::default()));
    }

    #[test]
    fn layout_snapshot_200x50() {
        insta::assert_snapshot!(draw_at(200, 50, &AppState::default()));
    }
}
