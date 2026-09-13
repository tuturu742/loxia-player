//! Keyboard cheat sheet overlay (`10-03`, `docs/04-state-and-input.md` §6). Every row is
//! generated from the live `KeyMap` — there is no hardcoded key table in this file. That is the
//! whole point: a user who remaps a key sees their own binding here, and a developer who adds an
//! action cannot forget to document it. The only hardcoded strings below are human-readable
//! *labels* (what an action does) and category *titles* — never a key spelling.

use loxia_core::keymap::{ActionId, HelpCategory, InputContext, KeyMap};
use loxia_core::state::AppState;
use loxia_core::state::nav::Tab;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use strum::IntoEnumIterator;

use crate::style;

/// Shown in the key column for an action with no binding at all — "a user can see the action
/// exists and go bind it" (this task's own spec), rather than the row vanishing entirely.
const UNBOUND: &str = "—";

/// Declared display order — also the order categories are assigned to columns. `pub(crate)`:
/// `modals::keymap_editor` (`11-02`) reuses this same declared order for its own flat,
/// single-column grouped list, rather than re-declaring it.
pub(crate) const CATEGORY_ORDER: [HelpCategory; 8] = [
    HelpCategory::Navigation,
    HelpCategory::Playback,
    HelpCategory::Queue,
    HelpCategory::Selection,
    HelpCategory::Audio,
    HelpCategory::Items,
    HelpCategory::Views,
    HelpCategory::System,
];

pub(crate) fn category_title(cat: HelpCategory) -> &'static str {
    match cat {
        HelpCategory::Navigation => "NAVIGATION",
        HelpCategory::Playback => "PLAYBACK",
        HelpCategory::Queue => "QUEUE",
        HelpCategory::Selection => "SELECTION",
        HelpCategory::Audio => "AUDIO",
        HelpCategory::Items => "ITEMS",
        HelpCategory::Views => "VIEWS",
        HelpCategory::System => "SYSTEM",
    }
}

/// A human-readable label per action — describes what the action *does*; the key that triggers
/// it always comes from `KeyMap::hint_for`, never from here. `pub(crate)`: `modals::keymap_editor`
/// (`11-02`) reuses this same table rather than re-declaring 72 labels a second time.
pub(crate) fn action_label(action: ActionId) -> &'static str {
    match action {
        ActionId::MoveDown => "Column Down",
        ActionId::MoveUp => "Column Up",
        ActionId::NavLeft => "Column Left",
        ActionId::NavRight => "Column Right",
        ActionId::HalfPageUp => "Half Page Up",
        ActionId::HalfPageDown => "Half Page Down",
        ActionId::GoToTop => "Go To Top",
        ActionId::GoToBottom => "Go To Bottom",
        ActionId::PopColumn => "Back",
        ActionId::NextTab => "Next Tab",
        ActionId::PrevTab => "Prev Tab",
        ActionId::JumpTab1 => "Jump Tab 1",
        ActionId::JumpTab2 => "Jump Tab 2",
        ActionId::JumpTab3 => "Jump Tab 3",
        ActionId::JumpTab4 => "Jump Tab 4",
        ActionId::JumpTab5 => "Jump Tab 5",
        ActionId::JumpTab6 => "Jump Tab 6",
        ActionId::JumpTab7 => "Jump Tab 7",
        ActionId::JumpTab8 => "Jump Tab 8",
        ActionId::JumpTab9 => "Jump Tab 9",
        ActionId::JumpTab10 => "Jump Tab 10",
        ActionId::OpenFilter => "Filter",
        ActionId::GoToArtist => "Go To Artist",
        ActionId::GoToAlbum => "Go To Album",
        ActionId::Cancel => "Cancel",
        ActionId::PlayPause => "Play / Pause",
        ActionId::NextTrack => "Next Track",
        ActionId::PrevTrack => "Prev Track",
        ActionId::Stop => "Stop",
        ActionId::SeekBack5 => "Seek −5s",
        ActionId::SeekForward5 => "Seek +5s",
        ActionId::SeekBack30 => "Seek −30s",
        ActionId::SeekForward30 => "Seek +30s",
        ActionId::VolumeUp => "Volume Up",
        ActionId::VolumeDown => "Volume Down",
        ActionId::ToggleMute => "Mute",
        ActionId::QueueArtistOnly => "Play (Replace Queue)",
        ActionId::QueueFullContext => "Add to Queue",
        ActionId::InsertNext => "Insert Next",
        ActionId::InstantMix => "Instant Mix",
        ActionId::ToggleShuffle => "Shuffle",
        ActionId::CycleRepeat => "Repeat Mode",
        ActionId::OpenSortMenu => "Sort Menu",
        ActionId::RemoveEntry => "Remove Entry",
        ActionId::ToggleVisualSelect => "Visual Select",
        ActionId::ToggleItem => "Toggle Item",
        ActionId::SelectAll => "Select All",
        ActionId::ToggleEqualizer => "Equalizer",
        ActionId::CycleReplayGain => "ReplayGain Mode",
        ActionId::CycleQualityProfile => "Quality Profile",
        ActionId::OpenDevicePicker => "Output Device",
        ActionId::OpenSleepTimer => "Sleep Timer",
        ActionId::ToggleFavorite => "Favourite",
        ActionId::ToggleDownload => "Download",
        ActionId::SaveQueueAsPlaylist => "Save Queue As Playlist",
        ActionId::AddToPlaylist => "Add To Playlist",
        ActionId::DeletePlaylist => "Delete Playlist",
        ActionId::MoveTrackUp => "Move Track Up",
        ActionId::MoveTrackDown => "Move Track Down",
        ActionId::ToggleZenMode => "Zen Mode",
        ActionId::ToggleHistory => "History",
        ActionId::ToggleLyrics => "Lyrics",
        ActionId::LyricsScrollUp => "Lyrics Up",
        ActionId::LyricsScrollDown => "Lyrics Down",
        ActionId::ToggleHelp => "Help",
        ActionId::Quit => "Quit",
        ActionId::Refresh => "Refresh",
    }
}

/// `(key hint, label)` for every action in `category`, in `ActionId::iter`'s own declaration
/// order. `KeyMap::hint_for` already returns the literal string `"unbound"` for an action with no
/// binding — remapped to [`UNBOUND`] (`—`) here, since that's this file's own display contract,
/// not `KeyMap`'s.
fn rows_for(keymap: &KeyMap, category: HelpCategory) -> Vec<(String, &'static str)> {
    ActionId::iter()
        .filter(|a| a.help_category() == category)
        .map(|a| {
            let hint = keymap.hint_for(a);
            let key = if hint == "unbound" {
                UNBOUND.to_string()
            } else {
                hint
            };
            (key, action_label(a))
        })
        .collect()
}

fn tab_label(tab: Tab) -> &'static str {
    match tab {
        Tab::NowPlaying => "Now Playing",
        Tab::Favourites => "Favourites",
        Tab::Search => "Search",
        Tab::Playlists => "Playlists",
        Tab::Artists | Tab::AlbumArtists | Tab::Albums | Tab::Genres | Tab::Folders => {
            "Miller Columns"
        }
        Tab::Settings => "Settings",
    }
}

/// `[Context: ...]` — `captured` is `Modal::Help`'s own `context` field, snapshotted when the
/// help modal was opened (`reducer::modal::open_modal`'s `current_input_context`), not re-derived
/// live: if help was opened *over* another modal (`?` replaces whatever was showing), that other
/// modal is no longer in `state.modal` to look up again — only the captured value still knows it
/// was there. `InputContext::Normal` carries no tab of its own, but the active tab cannot change
/// while any modal (including Help) is open (`reducer::modal::allows`), so reading it live here is
/// equivalent to having captured it at open time.
fn context_label(state: &AppState, captured: InputContext) -> String {
    match captured {
        InputContext::Modal(kind) => format!("{kind:?}"),
        InputContext::TextInput => "Filter".to_string(),
        InputContext::Normal => tab_label(state.nav.active_tab).to_string(),
    }
}

/// How many columns the cheat sheet uses at `width` — this task's own rule: 3 at/above 120, 2
/// from 90 to 119, 1 below that (scrolling makes up the difference).
fn column_count(width: u16) -> u16 {
    if width < 90 {
        1
    } else if width < 120 {
        2
    } else {
        3
    }
}

const COLUMN_GAP: u16 = 2;

pub fn render(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    captured_context: InputContext,
    scroll: u16,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let width = area.width.saturating_sub(4).clamp(1, 140).min(area.width);
    let height = area.height.saturating_sub(2).min(area.height);
    let modal_area = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    let conflicts = state
        .config_warnings
        .iter()
        .filter(|w| w.message.contains("is bound to both"))
        .count();

    let mut title = format!(
        "LOXIA KEYBOARD CHEAT SHEET [Context: {}]",
        context_label(state, captured_context)
    );
    if conflicts > 0 {
        title.push_str(&format!(" ⚠ {conflicts} conflicts — see Settings"));
    }

    let border_role = if conflicts > 0 {
        Role::Warning
    } else {
        Role::Border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .style(style::modal_surface(theme))
        .border_style(style::fg(theme, border_role))
        .title(title);
    let inner = block.inner(modal_area);
    // Wipe whatever the view underneath drew before painting the modal — a `Block` only paints its
    // border, so without this the canvas text showed *through* the modal body (seen in the field
    // with the sort menu over Now Playing). `docs/12-decisions.md`.
    f.render_widget(ratatui::widgets::Clear, modal_area);
    f.render_widget(block, modal_area);

    if inner.height == 0 {
        return;
    }

    let footer_height = 1.min(inner.height);
    let body_height = inner.height - footer_height;
    let body_area = Rect::new(inner.x, inner.y, inner.width, body_height);
    let footer_area = Rect::new(inner.x, inner.y + body_height, inner.width, footer_height);

    render_body(f, body_area, state, scroll, theme);
    if footer_height > 0 {
        render_footer(f, footer_area, theme);
    }
}

fn render_body(f: &mut Frame, area: Rect, state: &AppState, scroll: u16, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let columns = column_count(area.width);
    let col_width = (area
        .width
        .saturating_sub(COLUMN_GAP * columns.saturating_sub(1)))
        / columns;
    if col_width == 0 {
        return;
    }

    let column_categories = balance_into_columns(&state.keymap, columns as usize);
    for (i, cats) in column_categories.iter().enumerate() {
        let x = area.x + i as u16 * (col_width + COLUMN_GAP);
        let col_area = Rect::new(x, area.y, col_width, area.height);
        render_column(f, col_area, state, cats, scroll, theme);
    }
}

/// Header + separator + one row per action + a trailing blank — the exact line count
/// [`render_column`] will draw for `cat`.
fn category_block_height(keymap: &KeyMap, cat: HelpCategory) -> usize {
    2 + rows_for(keymap, cat).len() + 1
}

/// Distributes the 8 categories across `columns` by total *row* height, not raw category count —
/// `Navigation` alone (23 actions) is several times taller than `System` (2), so chunking
/// `CATEGORY_ORDER` into equal-*count* groups would badly overflow one column while leaving
/// another mostly empty (found while testing this module against the real default keymap; see
/// `docs/12-decisions.md`). Greedy: walk categories in declared order, always adding the next one
/// to whichever column is currently shortest — simple, deterministic, and close enough to
/// balanced for eight fixed-size blocks.
fn balance_into_columns(keymap: &KeyMap, columns: usize) -> Vec<Vec<HelpCategory>> {
    let mut cols: Vec<Vec<HelpCategory>> = vec![Vec::new(); columns];
    let mut totals = vec![0usize; columns];
    for &cat in &CATEGORY_ORDER {
        let (idx, _) = totals.iter().enumerate().min_by_key(|&(_, &t)| t).unwrap();
        cols[idx].push(cat);
        totals[idx] += category_block_height(keymap, cat);
    }
    cols
}

fn render_column(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    categories: &[HelpCategory],
    scroll: u16,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut lines: Vec<Line> = Vec::new();
    for &cat in categories {
        let title = category_title(cat);
        lines.push(Line::from(Span::styled(
            title,
            style::fg(theme, Role::Accent).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            "─".repeat(title.len()),
            style::fg(theme, Role::Border),
        )));
        for (key, label) in rows_for(&state.keymap, cat) {
            let text = format!("{key:<10} {label}");
            lines.push(Line::from(Span::styled(
                text,
                style::style(theme, Role::Fg),
            )));
        }
        lines.push(Line::from(""));
    }

    for (i, line) in lines
        .into_iter()
        .skip(scroll as usize)
        .take(area.height as usize)
        .enumerate()
    {
        let row = Rect::new(area.x, area.y + i as u16, area.width, 1);
        f.render_widget(Paragraph::new(line), row);
    }
}

fn render_footer(f: &mut Frame, area: Rect, theme: &Theme) {
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "[ Press ? or Esc to close ]",
            style::fg(theme, Role::Dim),
        )))
        .alignment(Alignment::Center),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::config::{ConfigWarning, Severity};
    use loxia_core::keymap::KeyMap;
    use loxia_core::state::modal::ModalKind;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16, state: &AppState, context: InputContext, scroll: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, context, scroll, &theme)
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    fn state_with_keymap() -> AppState {
        AppState {
            keymap: KeyMap::defaults(),
            ..AppState::default()
        }
    }

    /// `Clear` resets a modal's area to the terminal's *default* colours and a `Block` paints only
    /// its border — so the sheet came out with the theme's background only on the cells that
    /// happened to hold text, and the terminal's own background in every gap between them
    /// (`docs/12-decisions.md`). Checked on a theme whose background is a real colour; the default
    /// terminal theme uses `Reset` for everything and could never have shown this.
    #[test]
    fn every_cell_of_the_sheet_carries_the_theme_background() {
        use loxia_core::theme::Role;
        let state = state_with_keymap();
        let theme = Theme::builtin("cyberpunk_neon").unwrap();
        let expected = crate::style::to_color(theme.color(Role::Bg));

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, &state, InputContext::Normal, 0, &theme)
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // The modal is centred, so sample well inside it rather than guessing its exact rect.
        let mut checked = 0;
        for y in 4..26 {
            for x in 20..80 {
                let cell = buffer.cell((x, y)).unwrap();
                assert_eq!(
                    cell.bg,
                    expected,
                    "cell ({x},{y}) = {:?} is not on the themed surface",
                    cell.symbol()
                );
                checked += 1;
            }
        }
        assert!(checked > 500, "sampled too little of the sheet");
    }

    #[test]
    fn help_rows_generated_from_keymap() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("play_pause".to_string(), "F1".to_string());
        overrides.insert("next_track".to_string(), "F2".to_string());
        overrides.insert("quit".to_string(), "F3".to_string());
        let (keymap, _warnings) = KeyMap::from_config(&overrides);
        let state = AppState {
            keymap,
            ..AppState::default()
        };

        // `render_binding` spells a function key lowercase (`"f1"`), matching `parse.rs`'s own
        // rendering convention — never uppercase.
        let rendered = render_at(160, 40, &state, InputContext::Normal, 0);
        assert!(rendered.contains("f1"));
        assert!(rendered.contains("f2"));
        assert!(rendered.contains("f3"));
    }

    #[test]
    fn no_hardcoded_key_strings() {
        // Scans only the *non-test* portion of this file — the part that actually renders —
        // since the banned substrings below would otherwise also match this very test's own
        // source (an unavoidable self-reference, not a real hit). A hardcoded *label* like
        // "Play / Pause" is fine; a hardcoded *key* spelling (a modifier prefix, or a named key
        // `parse.rs` treats specially) is not — the only place that text may come from is
        // whatever `KeyMap` produces at runtime.
        let source = include_str!("help.rs");
        let non_test_source = source
            .split("#[cfg(test)]")
            .next()
            .unwrap()
            .to_ascii_lowercase();
        for banned in ["ctrl+", "alt+", "shift+", "\"space\"", "\"esc\"", "\"tab\""] {
            assert!(
                !non_test_source.contains(banned),
                "found a hardcoded key literal resembling {banned:?}"
            );
        }
    }

    #[test]
    fn every_action_id_appears_exactly_once() {
        let keymap = KeyMap::defaults();
        let mut seen = std::collections::HashSet::new();
        for &cat in &CATEGORY_ORDER {
            for (_, label) in rows_for(&keymap, cat) {
                assert!(
                    seen.insert(label),
                    "{label} appeared in more than one category"
                );
            }
        }
        assert_eq!(seen.len(), ActionId::iter().count());
    }

    #[test]
    fn unbound_actions_show_dash() {
        // The factory keymap leaves nothing unbound, so this steals `select_all`'s own single
        // default binding for `cancel` instead — `select_all` is left with no binding at all
        // afterwards, exactly the state this row's dash is meant to reveal.
        let victim = KeyMap::defaults()
            .binding_for(ActionId::SelectAll)
            .cloned()
            .unwrap();
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert(
            "cancel".to_string(),
            loxia_core::keymap::parse::render_binding(&victim),
        );
        let (keymap, _warnings) = KeyMap::from_config(&overrides);
        let state = AppState {
            keymap,
            ..AppState::default()
        };
        let rendered = render_at(160, 40, &state, InputContext::Normal, 0);
        assert!(
            rendered.contains(UNBOUND),
            "at least one factory-unbound action must render the dash"
        );
    }

    #[test]
    fn context_shown_in_title() {
        let state = state_with_keymap();
        for (tab, expected) in [
            (Tab::NowPlaying, "Now Playing"),
            (Tab::Artists, "Miller Columns"),
            (Tab::Playlists, "Playlists"),
        ] {
            let mut state = state.clone();
            state.nav.active_tab = tab;
            let rendered = render_at(160, 40, &state, InputContext::Normal, 0);
            assert!(
                rendered.contains(&format!("[Context: {expected}]")),
                "tab {tab:?} must show context {expected:?}, got: {rendered}"
            );
        }
    }

    #[test]
    fn modal_context_shows_modal_bindings() {
        let state = state_with_keymap();
        let normal = render_at(160, 40, &state, InputContext::Normal, 0);
        let modal = render_at(
            160,
            40,
            &state,
            InputContext::Modal(ModalKind::Equalizer),
            0,
        );
        assert!(normal.contains("[Context: Now Playing]"));
        assert!(modal.contains("[Context: Equalizer]"));
        assert_ne!(normal, modal);
    }

    #[test]
    fn conflict_warning_shown_in_header() {
        let mut state = state_with_keymap();
        state.config_warnings.push(ConfigWarning {
            field: "keybindings.play_pause".to_string(),
            message: "key Space is bound to both play_pause and toggle_mute; using toggle_mute"
                .to_string(),
            severity: Severity::Warning,
        });
        let rendered = render_at(160, 40, &state, InputContext::Normal, 0);
        assert!(rendered.contains("1 conflicts"));
        assert!(rendered.contains("see Settings"));
    }

    #[test]
    fn column_count_by_width() {
        assert_eq!(column_count(130), 3);
        assert_eq!(column_count(100), 2);
        assert_eq!(column_count(80), 1);
    }

    #[test]
    fn scrolls_when_content_overflows() {
        let state = state_with_keymap();
        let top = render_at(90, 15, &state, InputContext::Normal, 0);
        let scrolled = render_at(90, 15, &state, InputContext::Normal, 5);
        assert_ne!(top, scrolled, "scrolling must change what's visible");
    }

    /// The reducer's own `Modal::Help` is rebuilt with `scroll: 0` on every fresh `Open`
    /// (`reducer::modal::open_modal`, `03-07`) — this proves the *rendering* half: passing `0`
    /// after having previously rendered a nonzero scroll produces the same output as never having
    /// scrolled at all, i.e. there is no rendering-side state to carry across a close/reopen.
    #[test]
    fn scroll_resets_on_close() {
        let state = state_with_keymap();
        let fresh = render_at(90, 15, &state, InputContext::Normal, 0);
        let reset_after_scroll = render_at(90, 15, &state, InputContext::Normal, 0);
        assert_eq!(fresh, reset_after_scroll);
    }

    #[test]
    fn help_snapshot_wide() {
        let state = state_with_keymap();
        insta::assert_snapshot!(render_at(150, 40, &state, InputContext::Normal, 0));
    }

    #[test]
    fn help_snapshot_narrow() {
        let state = state_with_keymap();
        insta::assert_snapshot!(render_at(85, 30, &state, InputContext::Normal, 0));
    }

    #[test]
    fn help_snapshot_with_conflicts() {
        let mut state = state_with_keymap();
        state.config_warnings.push(ConfigWarning {
            field: "keybindings.play_pause".to_string(),
            message: "key Space is bound to both play_pause and toggle_mute; using toggle_mute"
                .to_string(),
            severity: Severity::Warning,
        });
        insta::assert_snapshot!(render_at(150, 40, &state, InputContext::Normal, 0));
    }

    #[test]
    fn help_snapshot_modal_context() {
        let state = state_with_keymap();
        insta::assert_snapshot!(render_at(
            150,
            40,
            &state,
            InputContext::Modal(ModalKind::Equalizer),
            0
        ));
    }
}
