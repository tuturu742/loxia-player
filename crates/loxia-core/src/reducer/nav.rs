//! Reducer: navigation. Handles `Action::Nav`, `Action::Select`, and the column-loading half of
//! `Action::Data` (`docs/04-state-and-input.md` §4).

use crate::action::{DataAction, LoadTarget, NavAction, SelectAction};
use crate::effect::{Effect, NetEffect};
use crate::model::{Album, ItemId, MediaItem, SectionHeader, SectionKind};
use crate::reducer::queue;
use crate::state::AppState;
use crate::state::modal::Modal;
use crate::state::nav::{Column, ColumnKind, LoadState, NavFocus, Tab};
use crate::state::search::{SearchResults, SearchSection};
use crate::state::toast::ToastLevel;

/// Rows kept visible above/below the cursor when recomputing `scroll_offset`.
const SCROLL_MARGIN: usize = 2;

/// `docs/07-ui-spec.md` §9: typing resets the debounce deadline to `state.clock + 250ms` —
/// `state.clock` (the last `Tick`'s timestamp), never a live clock read, per the reducer's own
/// no-time-reads rule.
const SEARCH_DEBOUNCE_MS: i64 = 250;
/// A query shorter than this (after trimming) issues nothing and clears any existing results.
const SEARCH_MIN_QUERY_LEN: usize = 2;
/// Matches `02-07`'s own test usage; this task's spec names no other number.
const SEARCH_LIMIT: usize = 50;

/// Placeholder pending real viewport-height communication from the render layer (`03-09`) — see
/// `docs/12-decisions.md`. `HalfPageUp`/`HalfPageDown` already carry a real `n` from the caller;
/// plain `scroll_offset` margin-keeping for `MoveUp`/`MoveDown` has no such input yet.
pub(crate) const ASSUMED_VIEWPORT_ROWS: usize = 20;

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

pub fn apply_nav(state: &mut AppState, action: NavAction) -> Vec<Effect> {
    match action {
        NavAction::MoveUp { n } => move_focused(state, -(n as isize)),
        NavAction::MoveDown { n } => move_focused(state, n as isize),
        NavAction::HalfPageUp { n } => move_focused(state, -(n as isize)),
        NavAction::HalfPageDown { n } => move_focused(state, n as isize),
        NavAction::NavLeft | NavAction::PopColumn => {
            nav_left(state);
            Vec::new()
        }
        NavAction::NavRight => nav_right(state),
        NavAction::GoToTop => go_to_top(state),
        NavAction::GoToBottom => go_to_bottom(state),
        NavAction::SetTab(tab) => set_tab(state, tab),
        NavAction::NextTab => next_tab(state),
        NavAction::PrevTab => prev_tab(state),
        NavAction::GoToArtist => go_to_artist(state),
        NavAction::GoToAlbum => go_to_album(state),
        NavAction::OpenFilter => open_filter(state),
        NavAction::SetFilter(text) => set_filter(state, text),
        NavAction::FilterInput(c) => filter_input(state, c),
        NavAction::FilterBackspace => filter_backspace(state),
        NavAction::CommitFilter => commit_filter(state),
        NavAction::Cancel => cancel(state),
        NavAction::FocusColumnAt { column, index } => focus_column_at(state, column, index),
        NavAction::ScrollColumn { column, delta } => scroll_column(state, column, delta),
        NavAction::FocusSectionAt { section, index } => focus_section_at(state, section, index),
        NavAction::FocusQueueEntry(id) => focus_queue_entry(state, id),
        NavAction::ScrollNowPlaying { delta } => move_now_playing_cursor(state, delta as isize),
    }
}

/// `10-04`: a click on `HitTarget::ColumnItem { column, index }` — unlike `MoveUp`/`MoveDown`,
/// this both changes which column is focused *and* sets its cursor to an absolute index in one
/// step, since a click names exactly where it landed rather than a relative direction.
fn focus_column_at(state: &mut AppState, column: usize, index: usize) -> Vec<Effect> {
    let Some(col) = state
        .nav
        .per_tab_stacks
        .get_mut(&state.nav.active_tab)
        .and_then(|stack| stack.get_mut(column))
    else {
        return Vec::new();
    };
    let clamped = index.min(col.items.len().saturating_sub(1));
    col.cursor = nearest_selectable(col, clamped);
    state.nav.focus = NavFocus::Column(column);
    state.touch();
    Vec::new()
}

/// Snaps `target` to the nearest selectable index at-or-before it, falling back to the first
/// selectable index if none precede it (e.g. `target` itself is a leading header). A click names
/// whatever row is on screen, header included, but `nav_right`'s own `unreachable!("section
/// headers are never selectable")` means the cursor must never actually land on one — every other
/// cursor-setter (`move_cursor`) already enforces this by construction; this is `FocusColumnAt`'s
/// own version of the same invariant. See `docs/12-decisions.md`.
fn nearest_selectable(column: &Column, target: usize) -> usize {
    let selectable: Vec<usize> = column.selectable_indices().collect();
    selectable
        .iter()
        .rev()
        .find(|&&i| i <= target)
        .or_else(|| selectable.first())
        .copied()
        .unwrap_or(0)
}

/// `10-04`: `ScrollUp`/`ScrollDown` over a `ColumnItem` — moves *that* column's own cursor,
/// deliberately never touching `nav.focus`: "scroll targets the column under the pointer, not
/// the focused one" (this task's own spec) means hovering and scrolling a column must not steal
/// focus from wherever the keyboard was already working.
fn scroll_column(state: &mut AppState, column: usize, delta: i32) -> Vec<Effect> {
    let active_tab = state.nav.active_tab;
    let Some(col) = state
        .nav
        .per_tab_stacks
        .get_mut(&active_tab)
        .and_then(|stack| stack.get_mut(column))
    else {
        return Vec::new();
    };
    move_cursor(col, delta as isize);
    state.touch();
    // The wheel has to pull pages exactly as `j`/`↓` do. Without this the cursor walked to the end
    // of the loaded page and stopped there for good, so a long list "wouldn't scroll past the
    // first pagination" by mouse while the keyboard went all the way (`docs/12-decisions.md`).
    // Keyed to the scrolled column, not the focused one — scrolling still never moves focus.
    pagination_trigger_at(state, column)
}

/// `10-04`: a single (non-double) click on `HitTarget::QueueEntry` — moves the Now Playing view's
/// own cursor to that entry, mirroring `j`/`k`'s existing `move_now_playing_cursor` behaviour,
/// but jumping straight to the clicked row rather than stepping by one.
fn focus_queue_entry(state: &mut AppState, id: crate::model::QueueEntryId) -> Vec<Effect> {
    let Some(pos) = state.queue.play_order.iter().position(|&idx| {
        state
            .queue
            .entries
            .get(idx)
            .is_some_and(|e| e.entry_id == id)
    }) else {
        return Vec::new();
    };
    state.now_playing_cursor = pos;
    state.now_playing_user_scrolled = true;
    // `now_playing_scroll` is deliberately left alone: the row was under the pointer, so it is
    // already on screen, and moving the view would slide it out from under the second click of a
    // double-click (`docs/12-decisions.md`).
    state.touch();
    Vec::new()
}

pub fn apply_select(state: &mut AppState, action: SelectAction) -> Vec<Effect> {
    match action {
        SelectAction::ToggleVisualMode => toggle_visual_mode(state),
        SelectAction::ToggleItem => toggle_item(state),
        SelectAction::SelectAll => select_all(state),
        SelectAction::ClearSelection => clear_selection(state),
    }
}

/// `DownloadProgress` is not this task's concern (per-item progress state doesn't exist yet on
/// `AppState`) — a no-op here rather than panicking, per whichever later task actually owns it.
/// `TracksLoaded` (the reply to a queue-directed fetch, `06-02`) is routed to `reducer::queue`,
/// the module that owns everything else about the queue.
pub fn apply_data(state: &mut AppState, action: DataAction) -> Vec<Effect> {
    match action {
        DataAction::ItemsLoaded {
            tab,
            depth,
            kind,
            items,
            total,
            page,
        } => items_loaded(state, tab, depth, &kind, items, total, page),
        DataAction::DiscographyLoaded {
            tab,
            depth,
            primary,
            appears_on,
        } => discography_loaded(state, tab, depth, primary, appears_on),
        DataAction::LoadFailed {
            target, message, ..
        } => load_failed(state, target, message),
        DataAction::PlaylistSaved { name } => {
            state.toasts.retain(|t| !t.message.starts_with("saving "));
            state.toast(format!("saved to {name}"), ToastLevel::Success);
            state.touch();
            Vec::new()
        }
        DataAction::TracksLoaded {
            tracks,
            source,
            full_context,
        } => queue::apply_tracks_loaded(state, tracks, source, full_context),
        DataAction::SearchResultsLoaded { query, results } => {
            search_results_loaded(state, query, results)
        }
        DataAction::FavouritesLoaded { results } => favourites_loaded(state, results),
        DataAction::LyricsLoaded { track, lyrics } => lyrics_loaded(state, track, lyrics),
        DataAction::CacheResolved {
            track,
            profile,
            path,
            stream_url,
        } => queue::cache_resolved(state, track, profile, path, stream_url),
        DataAction::CacheFetchStarted { track, profile } => {
            queue::cache_fetch_started(state, track, profile)
        }
        DataAction::CacheFetched {
            track,
            profile,
            total_bytes,
        } => queue::cache_fetched(state, track, profile, total_bytes),
        // `08-04`: no reducer path constructs `Effect::Cache(PinDownload { .. })` yet (the
        // wire-level `d`-keybinding wiring is out of this task's own scope, `docs/12-decisions.md`),
        // so this reply has nowhere real to land either — a no-op here until that future task adds
        // `AppState::downloads_active`/per-item progress handling.
        // `downloads_active` is what the header badge counts; the id set is what makes `d` a real
        // toggle. A zero-total progress report is the "started" signal `EmbyFetcher` sends before
        // any bytes land.
        DataAction::DownloadProgress {
            id,
            done_bytes,
            total_bytes,
        } => {
            if total_bytes > 0 && done_bytes >= total_bytes {
                state.downloads.insert(id);
                state.downloads_active = state.downloads_active.saturating_sub(1);
            } else {
                state.downloads_active += 1;
            }
            state.touch();
            Vec::new()
        }
        DataAction::DownloadsChanged {
            pinned,
            total_bytes,
            count,
        } => {
            state.downloads = pinned.into_iter().collect();
            state.cache_stats.download_bytes = total_bytes;
            state.cache_stats.pinned_count = count;
            state.downloads_active = 0;
            state.touch();
            Vec::new()
        }
        // `10-01`: there is still no decoded-image state on `AppState` to update (the decoded
        // image itself lives entirely on the `loxia-tui`/worker side, never in `loxia-core` —
        // `docs/12-decisions.md`), but a redraw is genuinely needed now that one is ready, so this
        // is no longer a bare no-op.
        DataAction::ImageLoaded { .. } => {
            state.touch();
            Vec::new()
        }
        // Previously a no-op alongside `ImageLoaded`: the picker (`09-01`) fired
        // `EnumerateDevices` on open but had nowhere for the reply to land, so it would have shown
        // nothing, forever. Also backs the Settings tab's own output-device dropdown
        // (`docs/12-decisions.md`).
        DataAction::DevicesLoaded { devices } => {
            state.player.known_devices = devices.clone();
            if let Some(Modal::DevicePicker {
                devices: modal_devices,
                load,
                ..
            }) = &mut state.modal
            {
                *load = LoadState::Loaded {
                    total: devices.len(),
                };
                *modal_devices = devices;
            }
            state.touch();
            Vec::new()
        }
        // `09-03`: the one-time startup load — `reducer::player`'s `SetEqPreset` reads this list.
        DataAction::PresetsLoaded { presets } => {
            state.player.known_presets = presets;
            Vec::new()
        }
        // `10-12`: the WebSocket's `LibraryChanged` message.
        DataAction::LibraryChanged => {
            super::connectivity::mark_all_columns_idle(state);
            state.touch();
            Vec::new()
        }
        // `10-12`: the WebSocket's `UserDataChanged` message, one `Action::Data` per entry in its
        // own `UserDataList` (`workers::network`'s own translation layer).
        DataAction::UserDataChanged {
            id,
            is_favorite,
            play_count,
        } => {
            state.update_item_user_data(&id, is_favorite, play_count);
            state.touch();
            Vec::new()
        }
        // `11-03`: the reply to `Effect::Net(TestServerConnection)` — routed to `reducer::settings`
        // since it only ever means anything while the server-profile editor's own draft is open.
        DataAction::ServerTestSucceeded {
            user_id,
            access_token,
            server_name,
            version,
        } => super::settings::server_editor_test_succeeded(
            state,
            user_id,
            access_token,
            server_name,
            version,
        ),
        DataAction::ServerTestFailed { message } => {
            super::settings::server_editor_test_failed(state, message)
        }
    }
}

/// `↑`/`↓` (and `j`/`k`) either switch sidebar tabs or move within the focused content, depending
/// on where focus is (`docs/12-decisions.md`):
///
/// - On the tab sidebar — `focus == NavFocus::Sidebar` for a Miller tab, or `nav.sidebar_focused`
///   for `Search`/`NowPlaying` (which have no column stack to hang `NavFocus::Sidebar` off) — they
///   switch tabs. A user coming from almost any other TUI reasonably expects the arrow keys to move
///   over "the tab list."
/// - Otherwise they move within the content: the focused Miller column, the flat search results, or
///   the Now Playing queue/history.
fn move_focused(state: &mut AppState, delta: isize) -> Vec<Effect> {
    if state.nav.active_tab == Tab::Search {
        return if state.nav.sidebar_focused {
            switch_tab(state, delta)
        } else {
            move_search_cursor(state, delta)
        };
    }
    if state.nav.active_tab == Tab::NowPlaying {
        return if state.nav.sidebar_focused {
            switch_tab(state, delta)
        } else {
            move_now_playing_cursor(state, delta)
        };
    }
    // Favourites is a sectioned tab like Search, not a Miller tab — but it was never given a branch
    // here, so it fell through to the `NavFocus::Sidebar` case below (its stack is empty, so that
    // is always what `nav.focus` holds) and `↑`/`↓` switched sidebar tabs instead of moving through
    // the favourites. There was no way to put the cursor on a row at all, which is what a live user
    // reported as "I can't select anything from favourites" (`docs/12-decisions.md`).
    if state.nav.active_tab == Tab::Favourites {
        return if state.nav.sidebar_focused {
            switch_tab(state, delta)
        } else {
            move_favourites_cursor(state, delta)
        };
    }
    if state.nav.focus == NavFocus::Sidebar {
        return switch_tab(state, delta);
    }
    if let Some(column) = state.active_column_mut() {
        move_cursor(column, delta);
        state.touch();
    }
    pagination_trigger(state)
}

/// `↑`/`↓` while parked on the tab sidebar: move to the adjacent tab and stay parked on *its*
/// sidebar, so a run of presses walks the whole tab bar rather than diving into the first tab that
/// has content. Distinct from `next_tab`/`prev_tab` (the `Tab`-key path), which on the Search tab
/// cycle result *sections* instead — that's never wanted while the sidebar itself is focused.
fn switch_tab(state: &mut AppState, delta: isize) -> Vec<Effect> {
    // The sidebar is a cycle of `TAB_ORDER.len()` tabs plus one extra virtual position at the end:
    // the "Quit" row. `↓` past the last tab lands on Quit; `↓` again wraps to the first tab; `↑`
    // moves back the same way.
    let quit_pos = TAB_ORDER.len() as isize;
    let cycle = quit_pos + 1;
    let current = if state.nav.sidebar_quit_focused {
        quit_pos
    } else {
        TAB_ORDER
            .iter()
            .position(|&t| t == state.nav.active_tab)
            .unwrap_or(0) as isize
    };
    let next = (current + delta).rem_euclid(cycle);
    if next == quit_pos {
        state.nav.sidebar_quit_focused = true;
        park_on_sidebar(state);
        return Vec::new();
    }
    let effects = set_tab(state, TAB_ORDER[next as usize]);
    state.nav.sidebar_quit_focused = false;
    park_on_sidebar(state);
    effects
}

/// Forces focus onto the destination tab's sidebar (the tab list), overriding `set_tab`'s default
/// of focusing content — used by `switch_tab` so arrow-cycling stays on the sidebar across tabs of
/// every kind (Miller, Search, Now Playing) uniformly.
fn park_on_sidebar(state: &mut AppState) {
    state.nav.focus = NavFocus::Sidebar;
    state.nav.window_start = 0;
    state.nav.sidebar_focused = true;
    if state.nav.active_tab == Tab::Search {
        state.search.query_focused = false;
    }
    state.touch();
}

/// `j`/`k` over the Now Playing tab's own Queue/History pane (`07-06`) — neither is a Miller
/// column, so this moves `AppState.now_playing_cursor` directly instead of `move_cursor`'s
/// `Column`-based logic, and marks the pane as manually scrolled (suppressing auto-follow until
/// the next track change clears it, `reducer::queue::load_current`).
fn move_now_playing_cursor(state: &mut AppState, delta: isize) -> Vec<Effect> {
    let count = match state.now_playing_subview {
        crate::state::NowPlayingSub::Queue => state.queue.play_order.len(),
        crate::state::NowPlayingSub::History => state.history.len(),
    };
    if count == 0 {
        return Vec::new();
    }
    let cursor = state.now_playing_cursor as isize;
    state.now_playing_cursor = (cursor + delta).clamp(0, count as isize - 1) as usize;
    state.now_playing_user_scrolled = true;
    keep_now_playing_cursor_visible(state, count);
    state.touch();
    Vec::new()
}

/// Nudges [`AppState::now_playing_scroll`] the *minimum* needed to keep the cursor on screen,
/// margin included — never recentres. Same shape (and same `ASSUMED_VIEWPORT_ROWS` assumption) as
/// `recompute_scroll` for a Miller column.
fn keep_now_playing_cursor_visible(state: &mut AppState, count: usize) {
    let cursor = state.now_playing_cursor;
    if cursor < state.now_playing_scroll + SCROLL_MARGIN {
        state.now_playing_scroll = cursor.saturating_sub(SCROLL_MARGIN);
    }
    let bottom_edge = state.now_playing_scroll + ASSUMED_VIEWPORT_ROWS;
    if cursor + SCROLL_MARGIN >= bottom_edge {
        state.now_playing_scroll =
            (cursor + SCROLL_MARGIN + 1).saturating_sub(ASSUMED_VIEWPORT_ROWS);
    }
    state.now_playing_scroll = state
        .now_playing_scroll
        .min(count.saturating_sub(ASSUMED_VIEWPORT_ROWS));
}

/// `j`/`k` (and half-page) over the Search tab's results, treated as **one flat list** across all
/// three sections in `Artists`/`Albums`/`Tracks` order (`docs/12-decisions.md`). Previously this
/// moved only *within* the one focused section and did nothing at all while the query line was
/// focused — so a search returning a single artist plus some albums/songs left the user stuck on
/// that lone artist with no way to reach the rest except `Tab`, which read as "arrows don't work."
///
/// From the query line, `↓` steps into the first result and `↑` steps back out to the sidebar (the
/// tab list); once in the results, `↑` past the very first result returns to the query line.
fn move_search_cursor(state: &mut AppState, delta: isize) -> Vec<Effect> {
    let sections: Vec<SearchSection> = SearchSection::ALL
        .into_iter()
        .filter(|&s| state.search.results.count(s) > 0)
        .collect();

    if state.search.query_focused {
        if delta > 0 {
            // `↓` enters the results at the first item of the first non-empty section.
            if let Some(&first) = sections.first() {
                state.search.query_focused = false;
                state.search.focused_section = first;
                state.search.cursors[first.index()] = 0;
                state.touch();
            }
        } else if delta < 0 {
            // `↑` from the query line steps out to the tab sidebar.
            state.nav.sidebar_focused = true;
            state.touch();
        }
        return Vec::new();
    }

    match step_sectioned_cursor(
        &state.search.results,
        state.search.focused_section,
        &state.search.cursors,
        delta,
    ) {
        SectionStep::Moved(section, index) => {
            state.search.focused_section = section;
            state.search.cursors[section.index()] = index;
            state.touch();
        }
        // `↑` past the first result returns focus to the query line.
        SectionStep::PastTop => {
            state.search.query_focused = true;
            state.touch();
        }
        SectionStep::Unchanged => {}
    }
    Vec::new()
}

/// `↑`/`↓` on the Favourites tab — the same flat traversal Search uses, minus the query line: there
/// is nothing above the first favourite, so `↑` there steps out to the tab sidebar (the same escape
/// `nav_left` offers).
fn move_favourites_cursor(state: &mut AppState, delta: isize) -> Vec<Effect> {
    match step_sectioned_cursor(
        &state.favourites.results,
        state.favourites.focused_section,
        &state.favourites.cursors,
        delta,
    ) {
        SectionStep::Moved(section, index) => {
            state.favourites.focused_section = section;
            state.favourites.cursors[section.index()] = index;
            state.touch();
        }
        SectionStep::PastTop => {
            state.nav.sidebar_focused = true;
            state.touch();
        }
        SectionStep::Unchanged => {}
    }
    Vec::new()
}

/// A click on a Search/Favourites row: focuses that section and puts its cursor on the clicked
/// index, and takes focus off the sidebar (and, on Search, off the query line) so the keyboard
/// carries on from where the mouse left it.
fn focus_section_at(state: &mut AppState, section: SearchSection, index: usize) -> Vec<Effect> {
    let (results, cursors) = match state.nav.active_tab {
        Tab::Search => (&state.search.results, &mut state.search.cursors),
        Tab::Favourites => (&state.favourites.results, &mut state.favourites.cursors),
        _ => return Vec::new(),
    };
    if index >= results.count(section) {
        return Vec::new();
    }
    cursors[section.index()] = index;
    match state.nav.active_tab {
        Tab::Search => {
            state.search.focused_section = section;
            state.search.query_focused = false;
        }
        _ => state.favourites.focused_section = section,
    }
    state.nav.sidebar_focused = false;
    state.touch();
    Vec::new()
}

enum SectionStep {
    Moved(SearchSection, usize),
    /// The move ran off the top of the first section. What that means is the caller's to decide.
    PastTop,
    /// Nothing to move over.
    Unchanged,
}

/// Treats the three sections of a `SearchResults` as one flat list in display order and steps
/// `delta` rows through it, skipping empty sections. Shared by the two tabs built on that shape so
/// they cannot drift apart (`docs/12-decisions.md`).
///
/// `saturating_sub` on the focused section's own count, not `- 1`: a `focused_section` holding no
/// results is not in `sections` at all, and the plain subtraction underflowed — a panic the reducer
/// is not allowed to have (`docs/04-state-and-input.md` §4 rule 1) even though nothing currently
/// reaches it.
fn step_sectioned_cursor(
    results: &SearchResults,
    section: SearchSection,
    cursors: &[usize; 4],
    delta: isize,
) -> SectionStep {
    let sections: Vec<SearchSection> = SearchSection::ALL
        .into_iter()
        .filter(|&s| results.count(s) > 0)
        .collect();
    if sections.is_empty() {
        return SectionStep::Unchanged;
    }
    let total: usize = sections.iter().map(|&s| results.count(s)).sum();

    // The current flat position: items before the focused section, plus its own cursor.
    let before: usize = sections
        .iter()
        .take_while(|&&s| s != section)
        .map(|&s| results.count(s))
        .sum();
    let within = cursors[section.index()].min(results.count(section).saturating_sub(1));
    let target = (before + within) as isize + delta;
    if target < 0 {
        return SectionStep::PastTop;
    }

    // Map the flat target back onto a (section, index).
    let mut remaining = (target as usize).min(total - 1);
    for &s in &sections {
        let c = results.count(s);
        if remaining < c {
            return SectionStep::Moved(s, remaining);
        }
        remaining -= c;
    }
    SectionStep::Unchanged
}

/// Emby's own page size (`loxia-emby::query::PAGE_SIZE`, locked by that crate's own
/// `page_size_is_200` test, `02-05`) — duplicated here since `loxia-core` cannot depend on
/// `loxia-emby`.
const PAGE_SIZE: usize = 200;
/// How close to the end of the currently-loaded items the cursor must be before the next page is
/// requested.
const PAGINATION_LOOKAHEAD: usize = 50;

/// The only `ColumnKind`s the network worker's `FetchColumn` arm actually paginates — every other
/// kind (`Playlists`/`PlaylistTracks`, `Albums { of_artist: Some(_) }`/`Tracks`/`ArtistTracks`,
/// `SearchResults`) is loaded in one single-shot request (either by a dedicated effect like
/// `FetchDiscography`, or because the whole list is always small, `07-03`'s own reasoning for
/// `Playlists`) and has no next-page concept for `FetchColumn` to request at all.
///
/// This was a real, confirmed bug found live: `pagination_trigger` used to fire for *every* kind
/// once its `total` items didn't fill a full page (true for effectively any small Playlists list,
/// or any discography-sized Albums/Tracks column), sending a `FetchColumn { page: 1 }` for a kind
/// with no such handling. For the two kinds the worker silently drops (`Albums { of_artist: Some }`
/// /`Tracks`), this just logged noise. For `Playlists` — which *does* have a `FetchColumn` handler,
/// but one that always re-fetches the *entire* list regardless of the requested page — the reply
/// came back reporting `page: 1`, and `items_loaded`'s "page 0 replaces, page > 0 appends" rule then
/// appended that same full list a second time onto the column already showing it, doubling every
/// playlist on screen. See `docs/12-decisions.md`.
fn column_kind_paginates(kind: &ColumnKind) -> bool {
    matches!(
        kind,
        ColumnKind::Artists
            | ColumnKind::AlbumArtists
            | ColumnKind::Albums { of_artist: None }
            | ColumnKind::Genres
            | ColumnKind::GenreArtists { .. }
            // Only a real folder's children paginate; the Folders *root* is the (single-shot,
            // never-paginated) list of music libraries — treating it as paginated would double it
            // the same way the Playlists column once doubled (`docs/12-decisions.md`).
            | ColumnKind::Folders { of_parent: Some(_) }
    )
}

/// `docs`'s own trigger condition, `column.page_loaded * PAGE_SIZE < total`, is followed exactly
/// as given even though it re-fires once more than strictly necessary at the very end of a fully
/// loaded list (e.g. once `page_loaded * 200 >= total` never becomes false-then-true again, a
/// server that returns zero items for the extra request is harmless, just slightly wasteful) —
/// see `docs/12-decisions.md`.
fn pagination_trigger(state: &mut AppState) -> Vec<Effect> {
    let NavFocus::Column(depth) = state.nav.focus else {
        return Vec::new();
    };
    pagination_trigger_at(state, depth)
}

/// [`pagination_trigger`] for an explicitly named column rather than the focused one — what the
/// scroll wheel needs, since it moves the cursor of the column under the pointer without ever
/// taking focus.
fn pagination_trigger_at(state: &mut AppState, depth: usize) -> Vec<Effect> {
    let tab = state.nav.active_tab;
    let Some(column) = state
        .nav
        .per_tab_stacks
        .get(&tab)
        .and_then(|stack| stack.get(depth))
    else {
        return Vec::new();
    };
    if !column_kind_paginates(&column.kind) {
        return Vec::new();
    }
    let LoadState::Loaded { total } = column.load else {
        return Vec::new();
    };
    if column.page_loaded * PAGE_SIZE >= total {
        return Vec::new();
    }
    let remaining = column.items.len().saturating_sub(column.cursor);
    if remaining > PAGINATION_LOOKAHEAD {
        return Vec::new();
    }
    vec![Effect::Net(NetEffect::FetchColumn {
        tab,
        depth,
        kind: column.kind.clone(),
        page: column.page_loaded + 1,
    })]
}

/// Moves `delta` steps along `selectable_indices()` (section headers are never landed on),
/// clamped at both ends — it does not wrap.
fn move_cursor(column: &mut Column, delta: isize) {
    let selectable: Vec<usize> = column.selectable_indices().collect();
    if selectable.is_empty() {
        return;
    }
    let current_pos = selectable
        .iter()
        .position(|&i| i >= column.cursor)
        .unwrap_or(selectable.len() - 1);
    let new_pos = (current_pos as isize + delta).clamp(0, selectable.len() as isize - 1) as usize;
    column.cursor = selectable[new_pos];
    recompute_scroll(column);
}

fn recompute_scroll(column: &mut Column) {
    if column.cursor < column.scroll_offset + SCROLL_MARGIN {
        column.scroll_offset = column.cursor.saturating_sub(SCROLL_MARGIN);
    }
    let bottom_edge = column.scroll_offset + ASSUMED_VIEWPORT_ROWS;
    if column.cursor + SCROLL_MARGIN >= bottom_edge {
        column.scroll_offset =
            (column.cursor + SCROLL_MARGIN + 1).saturating_sub(ASSUMED_VIEWPORT_ROWS);
    }
}

/// After any push or pop, the focused column is always the rightmost visible one, with at most 3
/// data columns visible.
fn recompute_window(state: &mut AppState) {
    if let NavFocus::Column(depth) = state.nav.focus {
        state.nav.window_start = depth.saturating_sub(2);
    }
}

/// `←`/`h`/`Backspace`. On a Miller tab it pops one column (down to the sidebar). On
/// `Search`/`NowPlaying` — which have no column stack — it steps focus out to the tab sidebar
/// instead, so `↑`/`↓` there switch tabs (the mirror of `nav_right` stepping back in). Idempotent
/// once already on the sidebar.
fn nav_left(state: &mut AppState) -> Vec<Effect> {
    let tab = state.nav.active_tab;
    if (tab == Tab::Search || tab == Tab::NowPlaying || tab == Tab::Favourites)
        && !state.nav.sidebar_focused
    {
        state.nav.sidebar_focused = true;
        if tab == Tab::Search {
            // Leaving the query line for the sidebar — otherwise `context_for` would keep routing
            // keystrokes into the query while the sidebar is what's visibly focused.
            state.search.query_focused = false;
        }
        state.touch();
        return Vec::new();
    }
    pop_column(state);
    Vec::new()
}

fn nav_right(state: &mut AppState) -> Vec<Effect> {
    let tab = state.nav.active_tab;
    if tab == Tab::Search {
        if state.nav.sidebar_focused {
            // Step back into the search content from the sidebar, landing on the query line ready
            // to type — the mirror of `nav_left` stepping out.
            state.nav.sidebar_focused = false;
            state.search.query_focused = true;
            state.touch();
            return Vec::new();
        }
        return drill_from_search(state);
    }
    if tab == Tab::NowPlaying || tab == Tab::Favourites {
        if state.nav.sidebar_focused {
            state.nav.sidebar_focused = false;
            state.touch();
        }
        return Vec::new();
    }
    match state.nav.focus {
        NavFocus::Sidebar => {
            if state
                .nav
                .per_tab_stacks
                .get(&state.nav.active_tab)
                .is_some_and(|s| !s.is_empty())
            {
                state.nav.focus = NavFocus::Column(0);
                recompute_window(state);
                state.touch();
            }
            Vec::new()
        }
        NavFocus::Inspector => Vec::new(),
        NavFocus::Column(_) => drill_right(state),
    }
}

/// `NavRight` on a Search result hands off to the Miller stack. Search results are entry points,
/// not a hierarchy, so this builds the destination tab a fresh two-level stack: the tab's own **root
/// list** (Artists / Albums) at depth 0, and the drilled result's column on top of it. A live user
/// found the old behaviour — a single-column stack with just the drilled result — a dead end: after
/// drilling into a search artist they were on the Artists tab showing only that one artist's albums,
/// with no way left to browse any other artist (`docs/12-decisions.md`). Seeding the root beneath it
/// means `←` steps back to a normal, browsable list. Drilling a `Track` result is a no-op: a track
/// has no discography to hand off to, exactly like drilling a `Track` row in a Miller column already
/// is (`drill_into_track_is_a_noop`).
fn drill_from_search(state: &mut AppState) -> Vec<Effect> {
    if state.search.query_focused {
        return Vec::new();
    }
    match state.search.focused_section {
        SearchSection::Artists => {
            let idx = state.search.cursors[SearchSection::Artists.index()];
            let Some(artist) = state.search.results.artists.get(idx).cloned() else {
                return Vec::new();
            };
            open_artist_stack(state, &artist.id, &artist.name)
        }
        SearchSection::Albums => {
            let idx = state.search.cursors[SearchSection::Albums.index()];
            let Some(album) = state.search.results.albums.get(idx).cloned() else {
                return Vec::new();
            };
            open_album_stack(state, &album.id, &album.name)
        }
        // A track result has nothing to drill into, and Search never queries playlists — its own
        // playlists section is always empty.
        SearchSection::Tracks | SearchSection::Playlists => Vec::new(),
    }
}

/// Opens the Artists tab on `artist`'s own albums, with the browsable root list seeded beneath it
/// so `←` steps back into a normal artist list rather than a dead end. Shared by search drill-down
/// and `g a`: both are "I am somewhere else entirely and want to land on *this* artist", and a
/// jump that only switched tabs left the user to find the artist by hand.
fn open_artist_stack(state: &mut AppState, artist: &ItemId, name: &str) -> Vec<Effect> {
    state.nav.active_tab = Tab::Artists;
    let mut root = Column::new(ColumnKind::Artists, "Artists");
    root.load = LoadState::Loading;
    let mut albums = Column::new(
        ColumnKind::Albums {
            of_artist: Some(artist.clone()),
        },
        format!("{name} — Albums"),
    );
    albums.load = LoadState::Loading;
    state
        .nav
        .per_tab_stacks
        .insert(Tab::Artists, vec![root, albums]);
    state.nav.focus = NavFocus::Column(1);
    state.nav.sidebar_focused = false;
    state.nav.sidebar_quit_focused = false;
    recompute_window(state);
    state.touch();
    vec![
        Effect::Net(NetEffect::FetchColumn {
            tab: Tab::Artists,
            depth: 0,
            kind: ColumnKind::Artists,
            page: 0,
        }),
        Effect::Net(NetEffect::FetchDiscography {
            tab: Tab::Artists,
            depth: 1,
            artist: artist.clone(),
        }),
    ]
}

/// The album counterpart of [`open_artist_stack`]: the Albums tab showing `album`'s own tracks,
/// over a seeded root album list.
fn open_album_stack(state: &mut AppState, album: &ItemId, name: &str) -> Vec<Effect> {
    state.nav.active_tab = Tab::Albums;
    let mut root = Column::new(ColumnKind::Albums { of_artist: None }, "Albums");
    root.load = LoadState::Loading;
    let mut tracks = Column::new(
        ColumnKind::Tracks {
            of_album: album.clone(),
        },
        format!("{name} — Tracks"),
    );
    tracks.load = LoadState::Loading;
    state
        .nav
        .per_tab_stacks
        .insert(Tab::Albums, vec![root, tracks]);
    state.nav.focus = NavFocus::Column(1);
    state.nav.sidebar_focused = false;
    state.nav.sidebar_quit_focused = false;
    recompute_window(state);
    state.touch();
    vec![
        Effect::Net(NetEffect::FetchColumn {
            tab: Tab::Albums,
            depth: 0,
            kind: ColumnKind::Albums { of_artist: None },
            page: 0,
        }),
        Effect::Net(NetEffect::FetchAlbumTracks {
            tab: Tab::Albums,
            depth: 1,
            album: album.clone(),
            filter_artist: None,
        }),
    ]
}

/// Pushes a new column for the selected item, or reuses an already-loaded one at that depth
/// (drilling in, then out, then back in) with its cursor intact and no effect.
fn drill_right(state: &mut AppState) -> Vec<Effect> {
    let Some(item) = state.selected_item().cloned() else {
        return Vec::new();
    };

    let tab = state.nav.active_tab;
    let current_depth = match state.nav.focus {
        NavFocus::Column(d) => d,
        _ => 0,
    };
    let next_depth = current_depth + 1;

    let (kind, title, effect): (ColumnKind, String, Option<Effect>) = match &item {
        MediaItem::Artist(a) => (
            ColumnKind::Albums {
                of_artist: Some(a.id.clone()),
            },
            format!("{} — Albums", a.name),
            Some(Effect::Net(NetEffect::FetchDiscography {
                tab,
                depth: next_depth,
                artist: a.id.clone(),
            })),
        ),
        MediaItem::Album(a) => {
            let kind = ColumnKind::Tracks {
                of_album: a.id.clone(),
            };
            let effect = NetEffect::FetchAlbumTracks {
                tab,
                depth: next_depth,
                album: a.id.clone(),
                filter_artist: None,
            };
            (
                kind,
                format!("{} — Tracks", a.name),
                Some(Effect::Net(effect)),
            )
        }
        MediaItem::Genre(g) => {
            let kind = ColumnKind::GenreArtists {
                of_genre: g.name.clone(),
            };
            let effect = NetEffect::FetchColumn {
                tab,
                depth: next_depth,
                kind: kind.clone(),
                page: 0,
            };
            (
                kind,
                format!("{} — Artists", g.name),
                Some(Effect::Net(effect)),
            )
        }
        MediaItem::Folder(f) => {
            let kind = ColumnKind::Folders {
                of_parent: Some(f.id.clone()),
            };
            let effect = NetEffect::FetchColumn {
                tab,
                depth: next_depth,
                kind: kind.clone(),
                page: 0,
            };
            (kind, f.name.clone(), Some(Effect::Net(effect)))
        }
        MediaItem::Playlist(p) => {
            let kind = ColumnKind::PlaylistTracks {
                of_playlist: p.id.clone(),
            };
            let effect = NetEffect::FetchColumn {
                tab,
                depth: next_depth,
                kind: kind.clone(),
                page: 0,
            };
            (
                kind,
                format!("{} — Tracks", p.name),
                Some(Effect::Net(effect)),
            )
        }
        MediaItem::Track(_) => return Vec::new(),
        // Was `unreachable!("section headers are never selectable")` — a real crash found in the
        // field: `items_loaded`/`discography_loaded` both had a cursor-clamping bug that could
        // land the cursor on a header despite this invariant (fixed at the source, see their own
        // `nearest_selectable` calls), but the reducer must never crash the whole process over an
        // input-shape assumption elsewhere turning out wrong — nothing to drill into, same as a
        // `Track`, is the correct behaviour regardless (`docs/12-decisions.md`).
        MediaItem::SectionHeader(_) => return Vec::new(),
    };

    let stack = state.nav.per_tab_stacks.entry(tab).or_default();

    let reuse = stack
        .get(next_depth)
        .is_some_and(|c| c.kind == kind && matches!(c.load, LoadState::Loaded { .. }));
    if reuse {
        // Nothing to discard — this is exactly the already-present data being reused, cursor and
        // any columns drilled even deeper than it left untouched.
        state.nav.focus = NavFocus::Column(next_depth);
        recompute_window(state);
        state.touch();
        return Vec::new();
    }

    stack.truncate(next_depth);
    let mut column = Column::new(kind, title);
    column.load = LoadState::Loading;
    stack.push(column);
    state.nav.focus = NavFocus::Column(next_depth);
    recompute_window(state);
    state.touch();

    effect.into_iter().collect()
}

/// Pops the rightmost column, restoring focus to its parent with the parent's cursor unchanged.
/// Popping the last remaining column is a no-op on the stack; focus moves to the sidebar instead.
fn pop_column(state: &mut AppState) {
    if !matches!(state.nav.focus, NavFocus::Column(_)) {
        return;
    }
    let tab = state.nav.active_tab;
    let Some(stack) = state.nav.per_tab_stacks.get_mut(&tab) else {
        return;
    };
    if stack.len() > 1 {
        stack.pop();
        state.nav.focus = NavFocus::Column(stack.len() - 1);
    } else {
        state.nav.focus = NavFocus::Sidebar;
    }
    recompute_window(state);
    state.touch();
}

fn go_to_top(state: &mut AppState) -> Vec<Effect> {
    if let Some(column) = state.active_column_mut() {
        let first = column.selectable_indices().next();
        if let Some(first) = first {
            column.cursor = first;
            recompute_scroll(column);
            state.touch();
        }
    }
    Vec::new()
}

fn go_to_bottom(state: &mut AppState) -> Vec<Effect> {
    if let Some(column) = state.active_column_mut() {
        let last = column.selectable_indices().last();
        if let Some(last) = last {
            column.cursor = last;
            recompute_scroll(column);
            state.touch();
        }
    }
    pagination_trigger(state)
}

fn seed_column_for_tab(tab: Tab) -> Option<(ColumnKind, String)> {
    match tab {
        Tab::Artists => Some((ColumnKind::Artists, "Artists".to_string())),
        Tab::AlbumArtists => Some((ColumnKind::AlbumArtists, "Album Artists".to_string())),
        Tab::Albums => Some((ColumnKind::Albums { of_artist: None }, "Albums".to_string())),
        Tab::Genres => Some((ColumnKind::Genres, "Genres".to_string())),
        Tab::Folders => Some((
            ColumnKind::Folders { of_parent: None },
            "Folders".to_string(),
        )),
        Tab::Playlists => Some((ColumnKind::Playlists, "Playlists".to_string())),
        // Dedicated views, not generic Miller-column browsers (docs/01-architecture.md §3.5).
        Tab::NowPlaying | Tab::Favourites | Tab::Search | Tab::Settings => None,
    }
}

/// A tab visited for the first time gets a seed column and its fetch effect; a previously-visited
/// tab's stack (and every column's cursor within it) is preserved untouched.
/// `10-09`: `pub(crate)` (was module-private) so `reducer::modal::open_settings_sorting` can
/// reuse the real tab-switch logic (focus/window-start recompute, seed-column fetch) rather than
/// hand-rolling a smaller, subtly-different version of it.
pub(crate) fn set_tab(state: &mut AppState, tab: Tab) -> Vec<Effect> {
    if state.nav.active_tab == tab {
        return Vec::new();
    }
    // A selection does not survive leaving its tab. It is a transient mode, not a property of the
    // list, and one left behind reappeared — count and all — on returning, with no memory of having
    // made it (`docs/12-decisions.md`).
    if let Some(column) = state.active_column_mut() {
        column.selection.visual_mode = false;
        column.selection.selected.clear();
    }
    state.nav.active_tab = tab;

    let mut effects = Vec::new();
    if let std::collections::hash_map::Entry::Vacant(entry) = state.nav.per_tab_stacks.entry(tab) {
        match seed_column_for_tab(tab) {
            Some((kind, title)) => {
                let mut column = Column::new(kind.clone(), title);
                column.load = LoadState::Loading;
                entry.insert(vec![column]);
                effects.push(Effect::Net(NetEffect::FetchColumn {
                    tab,
                    depth: 0,
                    kind,
                    page: 0,
                }));
            }
            None => {
                entry.insert(Vec::new());
            }
        }
    }

    let depth = state.nav.per_tab_stacks.get(&tab).map_or(0, Vec::len);
    state.nav.focus = if depth == 0 {
        NavFocus::Sidebar
    } else {
        NavFocus::Column(depth - 1)
    };
    // An explicit jump onto a tab (`Alt+N`, a click, `SetTab`) focuses its *content*; `switch_tab`
    // overrides this back to the sidebar afterwards for the arrow-cycling case only.
    state.nav.sidebar_focused = false;
    // Any tab jump also leaves the sidebar's Quit row (a landing spot, never a destination).
    state.nav.sidebar_quit_focused = false;
    // Settings' own inner "section list vs rows" focus resets to rows on every (re-)entry, matching
    // every other tab's "content focused on entry" default.
    state.settings.section_list_focused = false;
    recompute_window(state);

    // "The query line is focused on tab entry" (`docs/07-ui-spec.md` §9) — every switch *onto*
    // Search re-focuses it, even on a return visit whose results/cursors are otherwise preserved
    // untouched (matching every other tab's own "stack preserved" convention above).
    if tab == Tab::Search {
        state.search.query_focused = true;
    }

    // Loaded on first tab entry only (`docs/07-ui-spec.md` §9) — `LoadState::Idle` is the same
    // "never fetched yet" signal `seed_column_for_tab`'s own vacant-entry check uses for Miller
    // columns; Favourites has no column of its own to key that off, so this checks the load
    // state directly instead. A return visit leaves whatever's already loaded untouched; `Ctrl+R`
    // (`refresh_favourites`) is the only other way to re-fetch.
    if tab == Tab::Favourites && state.favourites.load == LoadState::Idle {
        state.favourites.load = LoadState::Loading;
        effects.push(Effect::Net(NetEffect::FetchFavourites));
    }

    state.touch();
    effects
}

/// On the Search tab, once the query line has been left (`Enter`), `Tab`/`Shift+Tab` cycle result
/// sections instead of switching sidebar tabs — `docs/07-ui-spec.md` §9's own text for `search.rs`
/// ("`Tab` cycles sections") overrides the global `NextTab`/`PrevTab` binding in that one context.
fn next_tab(state: &mut AppState) -> Vec<Effect> {
    if state.nav.active_tab == Tab::Search
        && !state.search.query_focused
        && !state.nav.sidebar_focused
    {
        cycle_search_section(state, 1);
        return Vec::new();
    }
    let idx = TAB_ORDER
        .iter()
        .position(|&t| t == state.nav.active_tab)
        .unwrap_or(0);
    set_tab(state, TAB_ORDER[(idx + 1) % TAB_ORDER.len()])
}

fn prev_tab(state: &mut AppState) -> Vec<Effect> {
    if state.nav.active_tab == Tab::Search
        && !state.search.query_focused
        && !state.nav.sidebar_focused
    {
        cycle_search_section(state, -1);
        return Vec::new();
    }
    let idx = TAB_ORDER
        .iter()
        .position(|&t| t == state.nav.active_tab)
        .unwrap_or(0);
    set_tab(
        state,
        TAB_ORDER[(idx + TAB_ORDER.len() - 1) % TAB_ORDER.len()],
    )
}

/// `Tab`/`Shift+Tab` over the Search tab's result sections, **skipping empty ones**.
///
/// Landing on a section with nothing in it is a dead stop, and Search gained a permanently empty
/// one when `SearchSection::Playlists` was added for the Favourites tab (Search does not query
/// playlists). Skipping is the honest rule for both: a section with no results is not somewhere a
/// user can be (`docs/12-decisions.md`).
///
/// With *every* section empty there is nowhere to go, so focus stays put rather than cycling
/// through four equally empty headings.
fn cycle_search_section(state: &mut AppState, delta: isize) {
    let sections = SearchSection::ALL;
    let start = state.search.focused_section.index() as isize;
    for step in 1..=sections.len() as isize {
        let i = (start + delta * step).rem_euclid(sections.len() as isize) as usize;
        let candidate = sections[i];
        if state.search.results.count(candidate) > 0 {
            state.search.focused_section = candidate;
            state.touch();
            return;
        }
    }
}

/// `g a`: open the playing track's own artist, not merely the Artists tab. It used to do the
/// latter — land on the tab's plain artist list and leave the user to find the artist by hand,
/// which is not what "go to artist" means (`docs/12-decisions.md`). Reuses the same two-level
/// stack a search drill-down builds, so `←` still steps back into a browsable list.
///
/// The track's *first* artist is the destination when it credits several: it is the one the UI
/// leads with everywhere else, and a single keystroke has to pick one.
fn go_to_artist(state: &mut AppState) -> Vec<Effect> {
    let Some(entry) = state.current_entry() else {
        return Vec::new();
    };
    let Some(artist) = entry.track.artist_ids.first().cloned() else {
        return Vec::new();
    };
    let name = entry
        .track
        .artist_names
        .first()
        .cloned()
        .unwrap_or_default();
    open_artist_stack(state, &artist, &name)
}

/// `g l`, the album counterpart of [`go_to_artist`] — opens the playing track's own album.
fn go_to_album(state: &mut AppState) -> Vec<Effect> {
    let Some(entry) = state.current_entry() else {
        return Vec::new();
    };
    let Some(album) = entry.track.album_id.clone() else {
        return Vec::new();
    };
    let name = entry.track.album_name.clone();
    open_album_stack(state, &album, &name)
}

/// `/` always resets to an empty needle and (re-)enters editing, even if a filter was already
/// committed — `docs/07-ui-spec.md` §5 / task `04-11`'s own spec literally says "sets
/// `column.filter = Some(String::new())`", not "resume the previous query".
/// `/` — for a Miller column, opens the inline filter; for the Search tab, returns focus to the
/// query line (`docs/07-ui-spec.md` §9: "`/` returns to the query line").
fn open_filter(state: &mut AppState) -> Vec<Effect> {
    if state.nav.active_tab == Tab::Search {
        state.search.query_focused = true;
        state.touch();
        return Vec::new();
    }
    if let Some(column) = state.active_column_mut() {
        column.filter = Some(String::new());
        column.filter_editing = true;
        let first = column.selectable_indices().next().unwrap_or(0);
        column.cursor = first;
        recompute_scroll(column);
        state.touch();
    }
    Vec::new()
}

fn set_filter(state: &mut AppState, text: String) -> Vec<Effect> {
    if state.nav.active_tab == Tab::Search {
        return search_set_query(state, text);
    }
    if let Some(column) = state.active_column_mut() {
        column.filter = Some(text);
        let first = column.selectable_indices().next().unwrap_or(0);
        column.cursor = first;
        recompute_scroll(column);
        state.touch();
    }
    // The inline filter only ever matched against rows *already paginated in*, so a band near the
    // end of a long Artists list simply never appeared (`docs/12-decisions.md`). Applying a filter
    // now pulls the next page too; `items_loaded` keeps pulling further pages while the filter
    // stays active, so the whole list is progressively searched.
    load_next_page_for_filter(state)
}

/// The next-page fetch to keep an active inline filter's search complete, or empty if the focused
/// column isn't a paginated one, has no active (non-empty) filter, or is already fully loaded.
/// Deliberately independent of `pagination_trigger` (which fires off cursor proximity, not the
/// filter) — a filtered column must keep loading even though the cursor never moves near the end.
fn load_next_page_for_filter(state: &AppState) -> Vec<Effect> {
    let NavFocus::Column(depth) = state.nav.focus else {
        return Vec::new();
    };
    let tab = state.nav.active_tab;
    let Some(column) = state.active_column() else {
        return Vec::new();
    };
    if !column.filter.as_deref().is_some_and(|f| !f.is_empty()) {
        return Vec::new();
    }
    if !column_kind_paginates(&column.kind) {
        return Vec::new();
    }
    let LoadState::Loaded { total } = column.load else {
        return Vec::new();
    };
    // Everything already loaded — `(page_loaded + 1) * PAGE_SIZE` is the count fetched so far. Note
    // this is the *exact* completion test, not `pagination_trigger`'s deliberately looser
    // `page_loaded * PAGE_SIZE >= total` (which tolerates one redundant end-of-list fetch because a
    // cursor-proximity gate keeps it from mattering) — here there is no such gate, so an off-by-one
    // would fetch a guaranteed-empty extra page every time.
    if (column.page_loaded + 1) * PAGE_SIZE >= total {
        return Vec::new();
    }
    vec![Effect::Net(NetEffect::FetchColumn {
        tab,
        depth,
        kind: column.kind.clone(),
        page: column.page_loaded + 1,
    })]
}

/// Typing into the Search tab's query line resets the debounce deadline (`docs/07-ui-spec.md`
/// §9) — `state.clock` (the last `Tick`'s timestamp), never a live clock read.
fn search_set_query(state: &mut AppState, text: String) -> Vec<Effect> {
    state.search.query = text;
    let deadline = state
        .clock
        .checked_add(jiff::SignedDuration::from_millis(SEARCH_DEBOUNCE_MS))
        .unwrap_or(state.clock);
    state.search.debounce_until = Some(deadline);
    state.touch();
    Vec::new()
}

fn filter_input(state: &mut AppState, c: char) -> Vec<Effect> {
    if state.nav.active_tab == Tab::Search {
        let mut text = state.search.query.clone();
        text.push(c);
        return search_set_query(state, text);
    }
    let Some(column) = state.active_column() else {
        return Vec::new();
    };
    let mut text = column.filter.clone().unwrap_or_default();
    text.push(c);
    set_filter(state, text)
}

/// `String::pop` removes one `char` (a full Unicode scalar value), never a lone byte — the same
/// convention `reducer::modal::field_backspace` already uses for its own text fields.
fn filter_backspace(state: &mut AppState) -> Vec<Effect> {
    if state.nav.active_tab == Tab::Search {
        let mut text = state.search.query.clone();
        text.pop();
        return search_set_query(state, text);
    }
    let Some(column) = state.active_column() else {
        return Vec::new();
    };
    let mut text = column.filter.clone().unwrap_or_default();
    text.pop();
    set_filter(state, text)
}

/// `Enter` while editing the inline filter: keeps `filter`'s text but leaves `filter_editing`, so
/// `InputContext` reverts to `Normal` and ordinary navigation resumes over the still-narrowed
/// list. On the Search tab: leaves the query line and focuses the first non-empty section
/// (`docs/07-ui-spec.md` §9).
fn commit_filter(state: &mut AppState) -> Vec<Effect> {
    if state.nav.active_tab == Tab::Search {
        if state.search.query_focused {
            state.search.query_focused = false;
            if let Some(section) = state.search.results.first_nonempty() {
                state.search.focused_section = section;
            }
            state.touch();
        }
        return Vec::new();
    }
    if let Some(column) = state.active_column_mut()
        && column.filter_editing
    {
        column.filter_editing = false;
        state.touch();
    }
    Vec::new()
}

/// Precedence ladder, first rung that applies: close a modal → clear an active filter → leave
/// visual mode → clear selection → do nothing.
fn cancel(state: &mut AppState) -> Vec<Effect> {
    if state.modal.is_some() {
        return super::modal::close_modal(state);
    }
    if let Some(column) = state.active_column_mut() {
        if column.filter.is_some() {
            column.filter = None;
            column.filter_editing = false;
            state.touch();
            return Vec::new();
        }
        // Leaving visual mode takes the selection with it, in the same press. It used to clear
        // only the *mode*: the checkboxes go with it (`widgets::column` draws them only in visual
        // mode), so the rows stayed selected while nothing on screen said so — the inspector kept
        // counting them and `Enter`/`a`/`i` kept acting on all of them. Reported as "when you close
        // selection it still says how many items were selected" (`docs/12-decisions.md`).
        if column.selection.visual_mode || !column.selection.selected.is_empty() {
            column.selection.visual_mode = false;
            column.selection.selected.clear();
            state.touch();
            return Vec::new();
        }
    }
    Vec::new()
}

fn toggle_visual_mode(state: &mut AppState) -> Vec<Effect> {
    if let Some(column) = state.active_column_mut() {
        column.selection.visual_mode = !column.selection.visual_mode;
        if !column.selection.visual_mode {
            column.selection.selected.clear();
        }
        state.touch();
    }
    Vec::new()
}

/// A visual selection holds **one kind of row at a time**. A column can mix kinds (a Folders
/// column lists folders and tracks together), and a selection spanning both behaved unpredictably
/// enough that a user asked for it to be disallowed outright rather than made to work
/// (`docs/12-decisions.md`). The first selected row fixes the kind; a row of any other kind is
/// refused with a toast rather than silently ignored or silently clearing what is already selected.
fn toggle_item(state: &mut AppState) -> Vec<Effect> {
    let Some(item) = state.selected_item().cloned() else {
        return Vec::new();
    };
    let Some(id) = item.id().cloned() else {
        return Vec::new();
    };
    let Some(column) = state.active_column() else {
        return Vec::new();
    };
    if column.selection.selected.contains(&id) {
        state
            .active_column_mut()
            .unwrap()
            .selection
            .selected
            .remove(&id);
        state.touch();
        return Vec::new();
    }
    if let Some(kind) = selection_kind(column)
        && kind != std::mem::discriminant(&item)
    {
        let noun = item_kind_noun(&item);
        state.toast(
            format!("selection already holds other items — Esc first to select {noun}"),
            ToastLevel::Warning,
        );
        return Vec::new();
    }
    let column = state.active_column_mut().unwrap();
    // Selecting anything *is* being in visual mode — otherwise the row is selected with no
    // checkbox drawn beside it (`widgets::column` draws them only in visual mode), an invisible
    // selection that every queue key still acts on.
    column.selection.visual_mode = true;
    column.selection.selected.insert(id);
    state.touch();
    Vec::new()
}

/// Every row of the kind already being selected — or, with nothing selected yet, of the kind under
/// the cursor. Same one-kind-at-a-time rule as [`toggle_item`]: in a Folders column `V` selects the
/// folders *or* the tracks, never both at once.
fn select_all(state: &mut AppState) -> Vec<Effect> {
    let Some(column) = state.active_column() else {
        return Vec::new();
    };
    let Some(kind) = selection_kind(column)
        .or_else(|| column.items.get(column.cursor).map(std::mem::discriminant))
    else {
        return Vec::new();
    };
    let ids: Vec<ItemId> = column
        .selectable_indices()
        .filter(|&i| std::mem::discriminant(&column.items[i]) == kind)
        .filter_map(|i| column.items[i].id().cloned())
        .collect();
    if let Some(column) = state.active_column_mut() {
        column.selection.visual_mode = true;
        column.selection.selected = ids.into_iter().collect();
        state.touch();
    }
    Vec::new()
}

/// The `MediaItem` variant a column's selection currently holds, or `None` when nothing is
/// selected. Rows whose ids have since left the column are ignored.
fn selection_kind(column: &Column) -> Option<std::mem::Discriminant<MediaItem>> {
    column
        .items
        .iter()
        .find(|item| {
            item.id()
                .is_some_and(|id| column.selection.selected.contains(id))
        })
        .map(std::mem::discriminant)
}

fn item_kind_noun(item: &MediaItem) -> &'static str {
    match item {
        MediaItem::Track(_) => "tracks",
        MediaItem::Album(_) => "albums",
        MediaItem::Artist(_) => "artists",
        MediaItem::Playlist(_) => "playlists",
        MediaItem::Folder(_) => "folders",
        MediaItem::Genre(_) => "genres",
        MediaItem::SectionHeader(_) => "items",
    }
}

fn clear_selection(state: &mut AppState) -> Vec<Effect> {
    if let Some(column) = state.active_column_mut() {
        column.selection.selected.clear();
        state.touch();
    }
    Vec::new()
}

/// `page == 0` replaces the target column's items (a fresh load); `page > 0` appends (the next
/// page arriving from `pagination_trigger`). Either way, sets `Loaded { total }` and
/// `page_loaded = page`, clamps the cursor into range, and preserves selection by id. A `kind`
/// mismatch means the user has since navigated away from or replaced this column — the stale
/// response is dropped.
fn items_loaded(
    state: &mut AppState,
    tab: Tab,
    depth: usize,
    kind: &ColumnKind,
    items: Vec<MediaItem>,
    total: usize,
    page: usize,
) -> Vec<Effect> {
    let Some(column) = state
        .nav
        .per_tab_stacks
        .get_mut(&tab)
        .and_then(|s| s.get_mut(depth))
    else {
        return Vec::new();
    };
    if &column.kind != kind {
        return Vec::new();
    }
    // A page already applied is a **duplicate reply**, and appending it again would show the same
    // rows twice. Two independent paths request `page_loaded + 1` — `pagination_trigger` off cursor
    // proximity and `load_next_page_for_filter` off an active filter — and the worker's in-flight
    // guard deregisters a key *before* its reply reaches the reducer, so a second request issued in
    // that window escapes coalescing and fetches the same page again. A user filtering for an
    // artist saw them listed twice, and the duplicate vanished as soon as a later page-0 reply
    // rebuilt the list (`docs/12-decisions.md`).
    //
    // Only ever `page_loaded + 1` is requested, so any `page` at or below it has been applied
    // already. Page 0 is exempt: it *replaces* rather than appends, and is how a refresh works.
    if page > 0 && page <= column.page_loaded {
        return Vec::new();
    }
    if page == 0 {
        column.items = items;
    } else {
        column.items.extend(items);
    }
    // `07-05`: folders sort first, files after, natural-order (`06-04`'s own comparator) within
    // each group — re-sorting the *whole* accumulated list on every page rather than just the new
    // page keeps this correct regardless of where a page boundary falls, at a cost this task's own
    // per-directory page cap (200) makes negligible.
    if matches!(kind, ColumnKind::Folders { .. }) {
        sort_folder_items(&mut column.items);
    }
    column.page_loaded = page;
    column.load = LoadState::Loaded { total };
    let present_ids: std::collections::BTreeSet<ItemId> = column
        .items
        .iter()
        .filter_map(|i| i.id().cloned())
        .collect();
    column
        .selection
        .selected
        .retain(|id| present_ids.contains(id));
    // A real crash found in the field: a bare `.min(len - 1)` only keeps the cursor in bounds, not
    // off a `SectionHeader` — `nearest_selectable` (already relied on by `focus_column_at` for the
    // identical reason) is what every cursor-setter actually needs, not just the ones a click can
    // reach (`docs/12-decisions.md`).
    column.cursor = nearest_selectable(
        column,
        column.cursor.min(column.items.len().saturating_sub(1)),
    );
    state.touch();
    // If an inline filter is active on this column, keep pulling pages until the whole list has
    // been loaded (and thus fully searched) — this is what walks a filter past the first page.
    load_next_page_for_filter(state)
}

/// `07-05`: `📁` folders before `♪` audio files; natural order by name within each group.
fn sort_folder_items(items: &mut [MediaItem]) {
    items.sort_by(|a, b| {
        let a_is_folder = matches!(a, MediaItem::Folder(_));
        let b_is_folder = matches!(b, MediaItem::Folder(_));
        match (a_is_folder, b_is_folder) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => crate::queue::sort::natural_cmp(a.display_name(), b.display_name()),
        }
    });
}

fn discography_loaded(
    state: &mut AppState,
    tab: Tab,
    depth: usize,
    primary: Vec<Album>,
    appears_on: Vec<Album>,
) -> Vec<Effect> {
    let Some(column) = state
        .nav
        .per_tab_stacks
        .get_mut(&tab)
        .and_then(|s| s.get_mut(depth))
    else {
        return Vec::new();
    };
    let total = primary.len() + appears_on.len();
    let mut items: Vec<MediaItem> = Vec::with_capacity(total + 2);
    // `docs/03-emby-api.md` §4: both sections get a header; either is omitted entirely (not
    // rendered as an empty header) when its own count is zero.
    if !primary.is_empty() {
        items.push(MediaItem::SectionHeader(SectionHeader {
            label: "ALBUMS".to_string(),
            count: primary.len(),
            kind: SectionKind::Albums,
        }));
        items.extend(primary.into_iter().map(MediaItem::Album));
    }
    if !appears_on.is_empty() {
        items.push(MediaItem::SectionHeader(SectionHeader {
            label: "APPEARS ON".to_string(),
            count: appears_on.len(),
            kind: SectionKind::AppearsOn,
        }));
        items.extend(appears_on.into_iter().map(MediaItem::Album));
    }
    column.items = items;
    column.load = LoadState::Loaded { total };
    // Same real crash as `items_loaded`'s identical fix: this column's own first row is a
    // `SectionHeader` ("ALBUMS") whenever there's at least one primary album — a bare
    // `.min(len - 1)` left `cursor` (still `0` from `Column::new`, untouched by any real
    // navigation yet) resting squarely on it (`docs/12-decisions.md`).
    column.cursor = nearest_selectable(
        column,
        column.cursor.min(column.items.len().saturating_sub(1)),
    );
    state.touch();
    Vec::new()
}

/// The reply to `Effect::Net(Search)` — discarded if `query` is stale (the user kept typing and a
/// fresher search is already pending/in flight; comparing against the *live* trimmed query, not
/// `last_searched`, so a reply for what's currently on screen is never dropped even if a newer
/// debounce hasn't fired yet).
fn search_results_loaded(
    state: &mut AppState,
    query: String,
    results: SearchResults,
) -> Vec<Effect> {
    if query != state.search.query.trim() {
        return Vec::new();
    }
    let total = results.artists.len() + results.albums.len() + results.tracks.len();
    state.search.results = results;
    state.search.load = LoadState::Loaded { total };
    for section in SearchSection::ALL {
        let count = state.search.results.count(section);
        let idx = &mut state.search.cursors[section.index()];
        *idx = if count == 0 { 0 } else { (*idx).min(count - 1) };
    }
    state.touch();
    Vec::new()
}

/// The reply to `Effect::Net(FetchFavourites)` — no staleness check needed (unlike Search):
/// there is only ever one favourites list, never a second in-flight request for different text
/// to disambiguate from.
fn favourites_loaded(state: &mut AppState, results: SearchResults) -> Vec<Effect> {
    let total = results.artists.len() + results.albums.len() + results.tracks.len();
    state.favourites.results = results;
    state.favourites.load = LoadState::Loaded { total };
    for section in SearchSection::ALL {
        let count = state.favourites.results.count(section);
        let idx = &mut state.favourites.cursors[section.index()];
        *idx = if count == 0 { 0 } else { (*idx).min(count - 1) };
    }
    state.touch();
    Vec::new()
}

/// `Ctrl+R` on the Favourites tab (`07-02`) — re-fetches unconditionally, regardless of current
/// `load` state (unlike `set_tab`'s first-visit check, an explicit refresh request always fires).
pub(crate) fn refresh_favourites(state: &mut AppState) -> Vec<Effect> {
    state.favourites.load = LoadState::Loading;
    state.touch();
    vec![Effect::Net(NetEffect::FetchFavourites)]
}

/// The reply to `Effect::Net(FetchLyrics)` (`07-07`) — discarded if the track that requested it is
/// no longer the one currently playing (the fetch can easily outlive a fast skip past it), the
/// same shape `stale_search_reply_is_discarded` already established for Search. `lyrics.rs`'s own
/// `fetch` never actually returns an `Err` (a fetch failure degrades to `Lyrics::Unsynced(vec![])`
/// — lyrics are cosmetic and never worth a user-visible error, `docs/03-emby-api.md` §7), so
/// there is no failure branch to handle here at all; the widget itself treats an empty result
/// exactly like "no lyrics" and hides the pane.
/// Stores the reply **unconditionally**, keyed by the track it is for.
///
/// This used to discard any reply whose track wasn't the current entry, as a staleness guard — but
/// that guard is both redundant and a silent failure mode: `widgets::lyrics` already filters on the
/// stored id (`state.lyrics.filter(|(id, _)| *id == entry.track.id)`), so a reply for a track the
/// user has skipped past is ignored at render time anyway, while *any* mismatch here — however it
/// arises — threw away a perfectly good fetch and left the pane on "loading lyrics…" for good, with
/// a successful fetch in the logs and nothing on screen to match it (`docs/12-decisions.md`).
fn lyrics_loaded(state: &mut AppState, track: ItemId, lyrics: crate::model::Lyrics) -> Vec<Effect> {
    // A new set of lyrics starts at the top; keeping the previous track's offset would open
    // mid-song, or past the end of a shorter lyric.
    state.lyrics_scroll = 0;
    state.lyrics = Some((track, lyrics));
    state.touch();
    Vec::new()
}

/// `Tick`'s half of the debounce (`docs/07-ui-spec.md` §9, `docs/04-state-and-input.md` §9 rule
/// 5-adjacent — search shares the same "periodic reducer-driven work" home as the progress report
/// and toast expiry, called from `reducer::mod`'s own `tick` alongside them).
pub(crate) fn maybe_fire_search(state: &mut AppState) -> Vec<Effect> {
    let Some(deadline) = state.search.debounce_until else {
        return Vec::new();
    };
    if state.clock < deadline {
        return Vec::new();
    }
    state.search.debounce_until = None;

    let query = state.search.query.trim().to_string();
    if query.chars().count() < SEARCH_MIN_QUERY_LEN {
        if state.search.results != SearchResults::default() || state.search.load != LoadState::Idle
        {
            state.search.results = SearchResults::default();
            state.search.load = LoadState::Idle;
            state.touch();
        }
        state.search.last_searched = None;
        return Vec::new();
    }

    if state.search.last_searched.as_deref() == Some(query.as_str()) {
        return Vec::new();
    }
    state.search.last_searched = Some(query.clone());
    state.search.load = LoadState::Loading;
    state.touch();
    vec![Effect::Net(NetEffect::Search {
        query,
        limit: SEARCH_LIMIT,
    })]
}

/// Sets `Error(msg)` and leaves existing items intact so the user does not lose their place on a
/// transient failure.
fn load_failed(state: &mut AppState, target: LoadTarget, message: String) -> Vec<Effect> {
    match target {
        LoadTarget::Column { tab, depth } | LoadTarget::Discography { tab, depth } => {
            if let Some(column) = state
                .nav
                .per_tab_stacks
                .get_mut(&tab)
                .and_then(|s| s.get_mut(depth))
            {
                column.load = LoadState::Error(message);
                state.touch();
            }
        }
        LoadTarget::Search | LoadTarget::Devices => {}
        LoadTarget::Favourites => {
            state.favourites.load = LoadState::Error(message);
            state.touch();
        }
        // No column/tab/depth was ever populated for a queue-directed fetch, so there's nothing
        // to mark `Error` — a toast is the only way this failure is visible at all.
        LoadTarget::QueueFetch => {
            state.toast("failed to load tracks", ToastLevel::Error);
            state.touch();
            // A batched multi-row queue request can never complete once one of its fetches has
            // failed — the failure carries no source to blame — so it is flushed with whatever did
            // arrive rather than left waiting forever (`reducer::queue::queue_batch_fetch_failed`).
            return queue::queue_batch_fetch_failed(state);
        }
        LoadTarget::FavoriteToggle(id) => {
            return queue::favorite_toggle_failed(state, id, message);
        }
        LoadTarget::PlaylistMutation(id) => {
            return queue::playlist_mutation_failed(state, id, message);
        }
        // `10-08`: no id to roll an optimistic update back against — this is a brand-new save,
        // nothing was ever applied to any already-visible state — so this only replaces the
        // pending `saving <n> tracks…` toast with the failure reason.
        LoadTarget::PlaylistSave => {
            state.toasts.retain(|t| !t.message.starts_with("saving "));
            state.toast(format!("could not save: {message}"), ToastLevel::Error);
            state.touch();
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::action::Action;
    use crate::model::{Folder, QueueEntryId};
    use crate::state::modal::Modal;
    use crate::test_support::fixtures;
    use crate::test_support::scenario::Scenario;

    fn focus_column(mut state: AppState, tab: Tab, depth: usize) -> AppState {
        state.nav.active_tab = tab;
        state.nav.focus = NavFocus::Column(depth);
        state
    }

    /// `10-01`: no `AppState` field records the decoded image itself (it lives entirely on the
    /// `loxia-tui`/worker side), but a redraw is still owed once one is ready.
    #[test]
    fn image_loaded_marks_state_dirty() {
        let mut state = fixtures::fixture_empty();
        state.dirty = false;
        let effects = apply_data(
            &mut state,
            crate::action::DataAction::ImageLoaded {
                id: crate::model::ItemId::from("t1"),
                tag: "tag-1".to_string(),
            },
        );
        assert!(effects.is_empty());
        assert!(state.dirty);
    }

    /// `10-12`: `library_changed_marks_idle_not_refetch` — asserts zero network effects, and
    /// every already-populated column marked `Idle` rather than refetched immediately.
    #[test]
    fn library_changed_marks_idle_not_refetch() {
        let mut state = fixtures::fixture_miller_3col();
        for stack in state.nav.per_tab_stacks.values_mut() {
            for column in stack.iter_mut() {
                column.load = LoadState::Loaded { total: 3 };
            }
        }
        let effects = apply_data(&mut state, crate::action::DataAction::LibraryChanged);
        assert!(
            effects.iter().all(|e| !matches!(e, Effect::Net(_))),
            "must not trigger a refetch storm"
        );
        for stack in state.nav.per_tab_stacks.values() {
            for column in stack {
                assert_eq!(column.load, LoadState::Idle);
            }
        }
    }

    /// `10-12`.
    #[test]
    fn user_data_changed_updates_in_place() {
        let state = fixtures::fixture_miller_3col();
        let tab = Tab::Artists;
        let id = match &state.nav.per_tab_stacks[&tab][2].items[0] {
            MediaItem::Track(t) => t.id.clone(),
            other => panic!("expected a track, got {other:?}"),
        };
        let mut state = state;
        apply_data(
            &mut state,
            crate::action::DataAction::UserDataChanged {
                id: id.clone(),
                is_favorite: true,
                play_count: 7,
            },
        );
        match &state.nav.per_tab_stacks[&tab][2].items[0] {
            MediaItem::Track(t) => {
                assert!(t.is_favorite);
                assert_eq!(t.play_count, 7);
            }
            other => panic!("expected a track, got {other:?}"),
        }
    }

    #[test]
    fn section_headers_are_skipped_by_cursor_movement() {
        let state = focus_column(fixtures::fixture_appears_on(), Tab::Artists, 0);
        let header_index = state.nav.per_tab_stacks[&Tab::Artists][0]
            .items
            .iter()
            .position(|i| matches!(i, MediaItem::SectionHeader(_)))
            .unwrap();

        let mut scenario = Scenario::new(state);
        for _ in 0..10 {
            scenario = scenario.dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
            let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
            assert_ne!(
                column.cursor, header_index,
                "cursor landed on the section header"
            );
        }
    }

    #[test]
    fn move_down_from_above_header_lands_below_it() {
        let mut state = focus_column(fixtures::fixture_appears_on(), Tab::Artists, 0);
        let header_index = state.nav.per_tab_stacks[&Tab::Artists][0]
            .items
            .iter()
            .position(|i| matches!(i, MediaItem::SectionHeader(_)))
            .unwrap();
        state.nav.per_tab_stacks.get_mut(&Tab::Artists).unwrap()[0].cursor = header_index - 1;

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.cursor, header_index + 1);
    }

    #[test]
    fn movement_clamps_at_list_ends() {
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 2);
        let scenario = Scenario::new(state)
            .dispatch_all((0..20).map(|_| Action::Nav(NavAction::MoveUp { n: 1 })));
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][2].cursor,
            0
        );

        let last = scenario.state().nav.per_tab_stacks[&Tab::Artists][2]
            .items
            .len()
            - 1;
        let scenario =
            scenario.dispatch_all((0..20).map(|_| Action::Nav(NavAction::MoveDown { n: 1 })));
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][2].cursor,
            last
        );
    }

    // --- 10-04: mouse-only nav actions --------------------------------------------------------

    #[test]
    fn focus_column_at_switches_focus_and_sets_cursor() {
        // `fixture_miller_3col` starts focused on column 2 (Tracks); a click on column 0 (Artists)
        // must both move focus there *and* set its cursor directly, unlike `MoveUp`/`MoveDown`'s
        // relative deltas.
        let state = fixtures::fixture_miller_3col();
        assert_eq!(state.nav.focus, NavFocus::Column(2));

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::FocusColumnAt {
            column: 0,
            index: 1,
        }));

        assert_eq!(scenario.state().nav.focus, NavFocus::Column(0));
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][0].cursor,
            1
        );
    }

    #[test]
    fn focus_column_at_clamps_index_to_last_item() {
        // The Artists column has 2 items (indices 0, 1) — a click reporting an out-of-range index
        // (stale `HitMap` entry, resized column, etc.) must clamp rather than panic or store an
        // out-of-bounds cursor.
        let state = fixtures::fixture_miller_3col();

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::FocusColumnAt {
            column: 0,
            index: 999,
        }));

        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][0].cursor,
            1
        );
    }

    #[test]
    fn focus_column_at_missing_column_is_noop() {
        let state = fixtures::fixture_miller_3col();

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::FocusColumnAt {
            column: 9,
            index: 0,
        }));

        assert_eq!(
            scenario.state().nav.focus,
            NavFocus::Column(2),
            "a click on a column that doesn't exist must leave focus untouched"
        );
        assert!(scenario.effects().is_empty());
    }

    #[test]
    fn scroll_column_moves_only_the_targeted_columns_cursor() {
        // Scrolling targets the column under the pointer, never the focused one — column 2
        // (Tracks, focused, cursor already at 1) must stay put while column 0 (Artists, cursor 0)
        // moves.
        let state = fixtures::fixture_miller_3col();
        assert_eq!(state.nav.focus, NavFocus::Column(2));
        assert_eq!(
            state.nav.per_tab_stacks[&Tab::Artists][2].cursor,
            1,
            "precondition: the fixture starts the Tracks column's cursor at 1"
        );

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::ScrollColumn {
            column: 0,
            delta: 1,
        }));

        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][0].cursor,
            1
        );
        assert_eq!(
            scenario.state().nav.focus,
            NavFocus::Column(2),
            "scrolling a non-focused column must not steal focus"
        );
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][2].cursor,
            1,
            "the focused column's own cursor must be untouched by another column's scroll"
        );
    }

    #[test]
    fn scroll_column_missing_column_is_noop() {
        let state = fixtures::fixture_miller_3col();
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::ScrollColumn {
            column: 9,
            delta: 3,
        }));
        assert!(scenario.effects().is_empty());
    }

    #[test]
    fn focus_queue_entry_moves_cursor_without_jumping_playback() {
        let state = fixtures::fixture_playing_queue();
        let target_id = state.queue.entries[7].entry_id;
        let position_before = state.queue.position;

        let scenario =
            Scenario::new(state).dispatch(Action::Nav(NavAction::FocusQueueEntry(target_id)));

        assert_eq!(scenario.state().now_playing_cursor, 7);
        assert!(scenario.state().now_playing_user_scrolled);
        assert_eq!(
            scenario.state().queue.position,
            position_before,
            "focusing a queue entry must not jump playback there (that's `QueueAction::JumpTo`)"
        );
    }

    /// The wheel over the Now Playing pane scrolls the pane itself, and — like `ScrollColumn` —
    /// targets what is under the pointer rather than what holds focus. With focus parked on the
    /// tab sidebar a plain `MoveDown` would switch tabs instead, which is not what a wheel over
    /// the queue means (`docs/12-decisions.md`).
    #[test]
    fn scroll_now_playing_moves_the_pane_cursor_even_from_the_sidebar() {
        let mut state = fixtures::fixture_playing_queue();
        state.nav.active_tab = Tab::NowPlaying;
        state.nav.sidebar_focused = true;
        state.now_playing_cursor = 4;
        let tab_before = state.nav.active_tab;

        let scenario =
            Scenario::new(state).dispatch(Action::Nav(NavAction::ScrollNowPlaying { delta: 3 }));

        assert_eq!(scenario.state().now_playing_cursor, 7);
        assert!(scenario.state().now_playing_user_scrolled);
        assert_eq!(
            scenario.state().nav.active_tab,
            tab_before,
            "scrolling the queue must not change tabs"
        );
    }

    /// Clamped at both ends rather than wrapping or running off the list.
    #[test]
    fn scroll_now_playing_clamps_at_the_ends() {
        let state = fixtures::fixture_playing_queue();
        let last = state.queue.play_order.len() - 1;

        let up = Scenario::new(state.clone())
            .dispatch(Action::Nav(NavAction::ScrollNowPlaying { delta: -999 }));
        assert_eq!(up.state().now_playing_cursor, 0);

        let down =
            Scenario::new(state).dispatch(Action::Nav(NavAction::ScrollNowPlaying { delta: 999 }));
        assert_eq!(down.state().now_playing_cursor, last);
    }

    /// `g a` used to only switch to the Artists tab, dropping the user on a plain alphabetical
    /// list with the playing artist still to be found by hand (`docs/12-decisions.md`). It must
    /// land *on* that artist's albums, with the root list beneath so `←` still browses.
    #[test]
    fn go_to_artist_opens_the_playing_artists_albums() {
        let state = fixtures::fixture_playing_queue();
        let track = state
            .current_entry()
            .expect("something is playing")
            .track
            .clone();
        let artist = track.artist_ids[0].clone();

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::GoToArtist));

        assert_eq!(scenario.state().nav.active_tab, Tab::Artists);
        let stack = &scenario.state().nav.per_tab_stacks[&Tab::Artists];
        assert_eq!(stack.len(), 2, "the root artist list must stay browsable");
        assert_eq!(stack[0].kind, ColumnKind::Artists);
        assert_eq!(
            stack[1].kind,
            ColumnKind::Albums {
                of_artist: Some(artist.clone())
            }
        );
        assert_eq!(scenario.state().nav.focus, NavFocus::Column(1));
        assert!(
            scenario.effects().iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::FetchDiscography { artist: a, .. }) if *a == artist
            )),
            "the artist's own albums must be fetched"
        );
    }

    /// The album counterpart, landing on that album's tracks.
    #[test]
    fn go_to_album_opens_the_playing_albums_tracks() {
        let state = fixtures::fixture_playing_queue();
        let track = state
            .current_entry()
            .expect("something is playing")
            .track
            .clone();
        let album = track
            .album_id
            .clone()
            .expect("the fixture track has an album");

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::GoToAlbum));

        assert_eq!(scenario.state().nav.active_tab, Tab::Albums);
        let stack = &scenario.state().nav.per_tab_stacks[&Tab::Albums];
        assert_eq!(stack.len(), 2);
        assert_eq!(
            stack[1].kind,
            ColumnKind::Tracks {
                of_album: album.clone()
            }
        );
        assert_eq!(scenario.state().nav.focus, NavFocus::Column(1));
        assert!(
            scenario.effects().iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::FetchAlbumTracks { album: a, .. }) if *a == album
            )),
            "the album's own tracks must be fetched"
        );
    }

    /// Nothing playing, nothing to go to — and in particular no tab switch, which would be a
    /// confusing half-action.
    #[test]
    fn go_to_artist_and_album_are_noops_with_nothing_playing() {
        for action in [NavAction::GoToArtist, NavAction::GoToAlbum] {
            let state = fixtures::fixture_empty();
            let tab_before = state.nav.active_tab;
            let scenario = Scenario::new(state).dispatch(Action::Nav(action.clone()));
            assert_eq!(scenario.state().nav.active_tab, tab_before, "{action:?}");
            assert!(scenario.effects().is_empty(), "{action:?}");
        }
    }

    #[test]
    fn focus_queue_entry_missing_id_is_noop() {
        let state = fixtures::fixture_playing_queue();
        let cursor_before = state.now_playing_cursor;

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::FocusQueueEntry(
            QueueEntryId(u64::MAX),
        )));

        assert_eq!(scenario.state().now_playing_cursor, cursor_before);
        assert!(scenario.effects().is_empty());
    }

    #[test]
    fn scroll_offset_keeps_cursor_visible_with_margin() {
        let mut state = fixtures::fixture_empty();
        let a = fixtures::artist("Many Tracks");
        let alb = fixtures::album("Long Album", 2020, &a);
        let mut column = Column::new(
            ColumnKind::Tracks {
                of_album: alb.id.clone(),
            },
            "Tracks",
        );
        column.items = (0..50)
            .map(|n| MediaItem::Track(fixtures::track(&format!("T{n}"), n, &alb, &[&a])))
            .collect();
        state.nav.active_tab = Tab::Artists;
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![column]);
        state.nav.focus = NavFocus::Column(0);

        let scenario = Scenario::new(state)
            .dispatch_all((0..40).map(|_| Action::Nav(NavAction::MoveDown { n: 1 })));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert!(
            column.cursor >= column.scroll_offset + SCROLL_MARGIN
                || column.cursor == column.items.len() - 1
        );
        assert!(column.cursor < column.scroll_offset + ASSUMED_VIEWPORT_ROWS);
    }

    fn loaded_folder_column(id: &str, child_id: &str) -> Column {
        let mut column = Column::new(
            ColumnKind::Folders {
                of_parent: Some(ItemId::from(id)),
            },
            id.to_string(),
        );
        column.items = vec![MediaItem::Folder(Folder {
            id: ItemId::from(child_id),
            name: child_id.to_string(),
        })];
        column.load = LoadState::Loaded { total: 1 };
        column
    }

    #[test]
    fn drilling_past_three_columns_slides_window() {
        let mut state = fixtures::fixture_empty();
        let stack = vec![
            loaded_folder_column("f0", "f1"),
            loaded_folder_column("f1", "f2"),
            loaded_folder_column("f2", "f3"),
            loaded_folder_column("f3", "f4"),
            loaded_folder_column("f4", "f5"),
        ];
        state.nav.active_tab = Tab::Folders;
        state.nav.per_tab_stacks.insert(Tab::Folders, stack);
        state.nav.focus = NavFocus::Column(0);

        let scenario =
            Scenario::new(state).dispatch_all((0..4).map(|_| Action::Nav(NavAction::NavRight)));
        assert_eq!(scenario.state().nav.focus, NavFocus::Column(4));
        assert_eq!(scenario.state().nav.window_start, 2);
    }

    #[test]
    fn pop_column_restores_previous_cursor() {
        let mut state = fixtures::fixture_empty();
        let mut col0 = loaded_folder_column("root", "f0");
        col0.cursor = 0;
        let col1 = loaded_folder_column("f0", "f1");
        state.nav.active_tab = Tab::Folders;
        state
            .nav
            .per_tab_stacks
            .insert(Tab::Folders, vec![col0, col1]);
        state.nav.focus = NavFocus::Column(1);

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::PopColumn));
        assert_eq!(scenario.state().nav.focus, NavFocus::Column(0));
        assert_eq!(scenario.state().nav.per_tab_stacks[&Tab::Folders].len(), 1);
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Folders][0].cursor,
            0
        );
    }

    #[test]
    fn redrilling_reuses_loaded_column_without_effect() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Folders;
        state.nav.per_tab_stacks.insert(
            Tab::Folders,
            vec![
                loaded_folder_column("root", "f0"),
                loaded_folder_column("f0", "f1"),
            ],
        );
        state.nav.focus = NavFocus::Column(0);

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::NavRight));
        assert_eq!(scenario.state().nav.focus, NavFocus::Column(1));
        scenario.assert_no_effects();
    }

    #[test]
    fn drill_into_track_is_a_noop() {
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 2);
        let before = state.nav.per_tab_stacks[&Tab::Artists].len();
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::NavRight));
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists].len(),
            before
        );
        scenario.assert_no_effects();
    }

    #[test]
    fn tab_switch_preserves_per_tab_column_stack() {
        let mut state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 2);
        state.nav.per_tab_stacks.get_mut(&Tab::Artists).unwrap()[2].cursor = 1;
        let before = state.nav.per_tab_stacks[&Tab::Artists].clone();

        let scenario = Scenario::new(state)
            .dispatch(Action::Nav(NavAction::SetTab(Tab::Genres)))
            .dispatch(Action::Nav(NavAction::SetTab(Tab::Artists)));

        assert_eq!(scenario.state().nav.per_tab_stacks[&Tab::Artists], before);
    }

    #[test]
    fn move_down_switches_tabs_while_focus_is_on_the_sidebar() {
        // Real bug: arrow keys did nothing at all while focus was on the sidebar (e.g. right after
        // popping out of the last column, or on any tab with no Miller column of its own) — only
        // `Tab`/`Shift+Tab` could switch tabs, which a user coming from almost any other TUI
        // reasonably didn't expect (`docs/12-decisions.md`).
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Artists;
        state.nav.focus = NavFocus::Sidebar;

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_eq!(scenario.state().nav.active_tab, Tab::AlbumArtists);
    }

    /// The Album Artists tab sits between Artists and Albums, seeds its own column on first visit,
    /// and paginates like every other library-wide list.
    #[test]
    fn album_artists_tab_seeds_and_fetches_its_own_column() {
        let state = fixtures::fixture_empty();
        let scenario =
            Scenario::new(state).dispatch(Action::Nav(NavAction::SetTab(Tab::AlbumArtists)));

        let stack = &scenario.state().nav.per_tab_stacks[&Tab::AlbumArtists];
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].kind, ColumnKind::AlbumArtists);
        assert!(scenario.effects().iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::FetchColumn {
                kind: ColumnKind::AlbumArtists,
                ..
            })
        )));
        assert!(
            super::column_kind_paginates(&ColumnKind::AlbumArtists),
            "a library-wide artist list is long enough to need paging, exactly like `Artists`"
        );
    }

    /// It is its own tab, not a re-skin of Artists: the two keep separate column stacks, so
    /// browsing one never disturbs the other.
    #[test]
    fn album_artists_and_artists_keep_separate_stacks() {
        let state = fixtures::fixture_empty();
        let scenario = Scenario::new(state)
            .dispatch(Action::Nav(NavAction::SetTab(Tab::Artists)))
            .dispatch(Action::Nav(NavAction::SetTab(Tab::AlbumArtists)));

        let stacks = &scenario.state().nav.per_tab_stacks;
        assert_eq!(stacks[&Tab::Artists][0].kind, ColumnKind::Artists);
        assert_eq!(stacks[&Tab::AlbumArtists][0].kind, ColumnKind::AlbumArtists);
    }

    #[test]
    fn move_up_switches_tabs_while_focus_is_on_the_sidebar() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Artists;
        state.nav.focus = NavFocus::Sidebar;

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveUp { n: 1 }));
        assert_eq!(scenario.state().nav.active_tab, Tab::Playlists);
    }

    #[test]
    fn down_past_the_last_tab_lands_on_quit_then_wraps() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Settings; // the last tab
        state.nav.focus = NavFocus::Sidebar;

        // Down past Settings -> the Quit row (no tab change).
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        assert!(scenario.state().nav.sidebar_quit_focused);
        assert_eq!(scenario.state().nav.active_tab, Tab::Settings);

        // Down again wraps to the first tab, leaving Quit.
        let scenario = scenario.dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        assert!(!scenario.state().nav.sidebar_quit_focused);
        assert_eq!(scenario.state().nav.active_tab, Tab::NowPlaying);
    }

    #[test]
    fn up_from_quit_returns_to_the_last_tab() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Settings;
        state.nav.focus = NavFocus::Sidebar;
        state.nav.sidebar_quit_focused = true;

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveUp { n: 1 }));
        assert!(!scenario.state().nav.sidebar_quit_focused);
        assert_eq!(scenario.state().nav.active_tab, Tab::Settings);
    }

    #[test]
    fn up_from_the_first_tab_wraps_onto_quit() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::NowPlaying; // the first tab
        state.nav.focus = NavFocus::Sidebar;
        // NowPlaying is a non-Miller tab: its sidebar is expressed by `sidebar_focused`, not
        // `focus == Sidebar`.
        state.nav.sidebar_focused = true;

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveUp { n: 1 }));
        assert!(scenario.state().nav.sidebar_quit_focused);
    }

    #[test]
    fn a_tab_jump_clears_quit_focus() {
        let mut state = fixtures::fixture_empty();
        state.nav.sidebar_quit_focused = true;
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::SetTab(Tab::Genres)));
        assert!(!scenario.state().nav.sidebar_quit_focused);
    }

    /// The reply is stored whatever the queue is doing — discarding it on a track mismatch threw
    /// away a successful fetch and stranded the pane on "loading lyrics…" (`docs/12-decisions.md`).
    /// `widgets::lyrics` filters on the stored id, so a genuinely stale reply is still never shown.
    #[test]
    fn lyrics_are_stored_even_if_the_current_entry_has_moved_on() {
        let state = fixtures::fixture_playing_queue();
        let other = ItemId::from("some-other-track");
        let scenario = Scenario::new(state).dispatch(Action::Data(DataAction::LyricsLoaded {
            track: other.clone(),
            lyrics: crate::model::Lyrics::Unsynced(vec!["a line".to_string()]),
        }));
        let stored = scenario.state().lyrics.as_ref().expect("reply kept");
        assert_eq!(stored.0, other);
    }

    #[test]
    fn first_visit_to_tab_emits_seed_fetch() {
        let scenario = Scenario::new(fixtures::fixture_empty())
            .dispatch(Action::Nav(NavAction::SetTab(Tab::Genres)));
        assert_eq!(
            scenario.last_effect(),
            Some(&Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Genres,
                depth: 0,
                kind: ColumnKind::Genres,
                page: 0
            }))
        );
    }

    #[test]
    fn visual_selection_survives_filter_change() {
        let mut state = focus_column(fixtures::fixture_visual_select(), Tab::Artists, 0);
        let before = state.nav.per_tab_stacks.get_mut(&Tab::Artists).unwrap()[0]
            .selection
            .selected
            .clone();
        assert!(!before.is_empty());

        let scenario =
            Scenario::new(state).dispatch(Action::Nav(NavAction::SetFilter("tra".to_string())));
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][0]
                .selection
                .selected,
            before
        );
    }

    #[test]
    fn filter_preserves_selection_by_id() {
        // "Boy Harsher" (index 0) is selected, then filtered *out* of visibility entirely — an
        // index-keyed selection would silently point at whatever ends up at index 0 afterwards
        // ("Sync24"); id-keyed selection must still name the original artist.
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        let target_id = state.nav.per_tab_stacks[&Tab::Artists][0].items[0]
            .id()
            .unwrap()
            .clone();

        let scenario = Scenario::new(state)
            .dispatch(Action::Select(SelectAction::ToggleItem))
            .dispatch(Action::Nav(NavAction::SetFilter("sync".to_string())));

        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert!(column.selection.selected.contains(&target_id));
        assert!(
            column
                .visible_items()
                .iter()
                .all(|(_, item)| item.id() != Some(&target_id)),
            "the selected artist must actually be filtered out of visibility for this test to mean anything"
        );
    }

    #[test]
    fn cursor_moves_to_first_visible_after_filter() {
        // Cursor starts on "Boy Harsher" (index 0), which the filter then excludes — the cursor
        // must land on "Sync24" (index 1), the first surviving selectable row.
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        let scenario =
            Scenario::new(state).dispatch(Action::Nav(NavAction::SetFilter("sync".to_string())));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.cursor, 1);
    }

    #[test]
    fn esc_clears_filter() {
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        let scenario = Scenario::new(state)
            .dispatch(Action::Nav(NavAction::OpenFilter))
            .dispatch(Action::Nav(NavAction::FilterInput('s')))
            .dispatch(Action::Nav(NavAction::Cancel));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.filter, None);
        assert!(!column.filter_editing);
    }

    #[test]
    fn enter_keeps_filter_and_exits_text_mode() {
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        let scenario = Scenario::new(state)
            .dispatch(Action::Nav(NavAction::OpenFilter))
            .dispatch(Action::Nav(NavAction::FilterInput('s')))
            .dispatch(Action::Nav(NavAction::FilterInput('y')))
            .dispatch(Action::Nav(NavAction::CommitFilter));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.filter.as_deref(), Some("sy"));
        assert!(!column.filter_editing);
    }

    #[test]
    fn open_filter_always_resets_to_empty() {
        // `/` resets the query even if a filter was already committed from an earlier pass —
        // task `04-11`'s spec is explicit that `OpenFilter` sets `Some(String::new())`, not
        // "resume the previous text".
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        let scenario = Scenario::new(state)
            .dispatch(Action::Nav(NavAction::OpenFilter))
            .dispatch(Action::Nav(NavAction::FilterInput('s')))
            .dispatch(Action::Nav(NavAction::CommitFilter))
            .dispatch(Action::Nav(NavAction::OpenFilter));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.filter.as_deref(), Some(""));
        assert!(column.filter_editing);
    }

    #[test]
    fn backspace_removes_grapheme() {
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        let scenario = Scenario::new(state)
            .dispatch(Action::Nav(NavAction::OpenFilter))
            .dispatch(Action::Nav(NavAction::FilterInput('🎵')))
            .dispatch(Action::Nav(NavAction::FilterBackspace));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.filter.as_deref(), Some(""));
    }

    /// `Esc` used to turn off visual mode and leave the rows selected. The checkboxes go with the
    /// mode (`widgets::column` draws them only in visual mode), so the selection became invisible
    /// while the inspector still counted it and every queue key still acted on it — "when you close
    /// selection it still says how many items were selected" (`docs/12-decisions.md`).
    #[test]
    fn escaping_a_selection_clears_it_in_one_press() {
        let mut state = focus_column(fixtures::fixture_visual_select(), Tab::Artists, 0);
        assert!(
            state.active_column().unwrap().selection.selected.len() > 1,
            "fixture assumption"
        );

        crate::reducer::apply(&mut state, Action::Nav(NavAction::Cancel));
        let column = state.active_column().unwrap();
        assert!(!column.selection.visual_mode);
        assert!(
            column.selection.selected.is_empty(),
            "the rows must not stay selected once the checkboxes are gone"
        );
    }

    /// A selection is a transient mode, not a property of the list: one left behind used to
    /// reappear, count and all, on returning to the tab.
    #[test]
    fn a_selection_does_not_survive_leaving_its_tab() {
        let mut state = focus_column(fixtures::fixture_visual_select(), Tab::Artists, 0);
        assert!(!state.active_column().unwrap().selection.selected.is_empty());

        crate::reducer::apply(&mut state, Action::Nav(NavAction::SetTab(Tab::Albums)));
        crate::reducer::apply(&mut state, Action::Nav(NavAction::SetTab(Tab::Artists)));

        let column = state.active_column().unwrap();
        assert!(column.selection.selected.is_empty());
        assert!(!column.selection.visual_mode);
    }

    /// Selecting a row *is* being in visual mode — otherwise `.` builds a selection with no
    /// checkbox drawn beside it, which every queue key still acts on.
    #[test]
    fn selecting_a_row_enters_visual_mode() {
        let mut state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        assert!(!state.active_column().unwrap().selection.visual_mode);

        crate::reducer::apply(&mut state, Action::Select(SelectAction::ToggleItem));
        assert!(state.active_column().unwrap().selection.visual_mode);

        let mut state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        crate::reducer::apply(&mut state, Action::Select(SelectAction::SelectAll));
        assert!(state.active_column().unwrap().selection.visual_mode);
    }

    /// A column can list more than one kind of row (Folders lists folders and tracks together), and
    /// a selection spanning both was unpredictable enough that a user asked for it to be disallowed
    /// rather than fixed. The first selected row fixes the kind.
    #[test]
    fn a_selection_holds_one_kind_of_row_at_a_time() {
        use crate::model::{Folder, ItemId as Id};
        let a = artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let track = fixtures::track("T", 1, &alb, &[&a]);
        let folder = Folder {
            id: Id::from("f1"),
            name: "Disc 1".to_string(),
        };
        let mut column = Column::new(
            ColumnKind::Folders {
                of_parent: Some(Id::from("root")),
            },
            "Folder",
        );
        column.items = vec![
            MediaItem::Folder(folder.clone()),
            MediaItem::Track(track.clone()),
        ];
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Folders;
        state.nav.per_tab_stacks.insert(Tab::Folders, vec![column]);
        state.nav.focus = NavFocus::Column(0);

        crate::reducer::apply(&mut state, Action::Select(SelectAction::ToggleVisualMode));
        crate::reducer::apply(&mut state, Action::Select(SelectAction::ToggleItem));
        state.active_column_mut().unwrap().cursor = 1;
        crate::reducer::apply(&mut state, Action::Select(SelectAction::ToggleItem));

        let column = state.active_column().unwrap();
        assert_eq!(
            column.selection.selected.len(),
            1,
            "a track must not join a selection of folders"
        );
        assert!(column.selection.selected.contains(&folder.id));
        assert!(state.toasts.iter().any(|t| t.message.contains("Esc")));

        // `V` follows the same rule: it selects the kind already being selected, not everything.
        crate::reducer::apply(&mut state, Action::Select(SelectAction::SelectAll));
        let column = state.active_column().unwrap();
        assert_eq!(column.selection.selected.len(), 1);
        assert!(column.selection.selected.contains(&folder.id));
    }

    #[test]
    fn select_all_excludes_headers() {
        let state = focus_column(fixtures::fixture_appears_on(), Tab::Artists, 0);
        let scenario = Scenario::new(state).dispatch(Action::Select(SelectAction::SelectAll));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.selection.selected.len(), 4);
        for item in &column.items {
            if let Some(id) = item.id() {
                assert!(column.selection.selected.contains(id));
            }
        }
    }

    #[test]
    fn cancel_precedence_ladder() {
        // Rung 1: close a modal.
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::Help {
            context: crate::keymap::InputContext::Normal,
            scroll: 0,
        });
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));
        assert!(scenario.state().modal.is_none());

        // Rung 2: clear an active filter.
        let mut state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 2);
        state.nav.per_tab_stacks.get_mut(&Tab::Artists).unwrap()[2].filter = Some("x".to_string());
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));
        assert!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][2]
                .filter
                .is_none()
        );

        // Rung 3: leave visual mode, taking the selection with it — one press, not two. Splitting
        // them left an invisible selection behind (`escaping_a_selection_clears_it_in_one_press`).
        let state = focus_column(fixtures::fixture_visual_select(), Tab::Artists, 0);
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert!(!column.selection.visual_mode);
        assert!(column.selection.selected.is_empty());

        // The same rung still catches a selection left without visual mode — nothing builds one
        // now, but it must not become unclearable if something ever does.
        let mut state = focus_column(fixtures::fixture_visual_select(), Tab::Artists, 0);
        state.nav.per_tab_stacks.get_mut(&Tab::Artists).unwrap()[0]
            .selection
            .visual_mode = false;
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));
        assert!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][0]
                .selection
                .selected
                .is_empty()
        );

        // Rung 4: nothing to do.
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 2);
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));
        assert!(!scenario.state().dirty);
    }

    #[test]
    fn items_loaded_clamps_out_of_range_cursor() {
        let mut state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        state.nav.per_tab_stacks.get_mut(&Tab::Artists).unwrap()[0].cursor = 5;
        let kind = state.nav.per_tab_stacks[&Tab::Artists][0].kind.clone();

        let a = fixtures::artist("New");
        let items = vec![MediaItem::Artist(a)];
        let action = Action::Data(DataAction::ItemsLoaded {
            tab: Tab::Artists,
            depth: 0,
            kind,
            items,
            total: 1,
            page: 0,
        });
        let scenario = Scenario::new(state).dispatch(action);
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Artists][0].cursor,
            0
        );
    }

    #[test]
    fn discography_loaded_builds_section_headers() {
        let mut state = focus_column(fixtures::fixture_empty(), Tab::Artists, 0);
        state.nav.per_tab_stacks.insert(
            Tab::Artists,
            vec![Column::new(
                ColumnKind::Albums { of_artist: None },
                "Albums",
            )],
        );
        let a = fixtures::artist("Sync24");
        let primary = fixtures::album("Comfortable Void", 2012, &a);
        let comp = fixtures::appears_on_album("Fahrenheit Project", 2005, &a);

        let action = Action::Data(DataAction::DiscographyLoaded {
            tab: Tab::Artists,
            depth: 0,
            primary: vec![primary],
            appears_on: vec![comp],
        });
        let scenario = Scenario::new(state).dispatch(action);
        let items = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0].items;

        assert!(matches!(
            &items[0],
            MediaItem::SectionHeader(h) if h.label == "ALBUMS" && h.count == 1
        ));
        assert!(matches!(&items[1], MediaItem::Album(_)));
        assert!(matches!(
            &items[2],
            MediaItem::SectionHeader(h) if h.label == "APPEARS ON" && h.count == 1
        ));
        assert!(matches!(&items[3], MediaItem::Album(_)));
    }

    /// A real crash found in the field: drilling into any artist with at least one primary album
    /// left the cursor resting on the leading "ALBUMS" `SectionHeader` (`Column::new`'s own
    /// default `cursor: 0`, never moved off it before this reply lands) — the *very next* `Right`/
    /// `Enter` then hit `drill_right`'s "section headers are never selectable" invariant, which
    /// used to be `unreachable!()`. This is close to the most common drill in the whole app
    /// (artist → albums), so the crash was, in practice, close to guaranteed on a real library.
    #[test]
    fn discography_loaded_does_not_leave_the_cursor_on_a_header() {
        let mut state = focus_column(fixtures::fixture_empty(), Tab::Artists, 0);
        state.nav.per_tab_stacks.insert(
            Tab::Artists,
            vec![Column::new(
                ColumnKind::Albums { of_artist: None },
                "Albums",
            )],
        );
        let a = fixtures::artist("Sync24");
        let primary = fixtures::album("Comfortable Void", 2012, &a);

        let action = Action::Data(DataAction::DiscographyLoaded {
            tab: Tab::Artists,
            depth: 0,
            primary: vec![primary],
            appears_on: vec![],
        });
        let mut scenario = Scenario::new(state).dispatch(action);
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert!(
            matches!(column.items[column.cursor], MediaItem::Album(_)),
            "cursor rests on {:?} at index {}, not the album",
            column.items[column.cursor],
            column.cursor
        );

        // The actual reported crash: `Right` (drill) on this exact state must not panic.
        scenario = scenario.dispatch(Action::Nav(NavAction::NavRight));
        let _ = scenario.state();
    }

    #[test]
    fn empty_appears_on_section_is_omitted() {
        let mut state = focus_column(fixtures::fixture_empty(), Tab::Artists, 0);
        state.nav.per_tab_stacks.insert(
            Tab::Artists,
            vec![Column::new(
                ColumnKind::Albums { of_artist: None },
                "Albums",
            )],
        );
        let a = fixtures::artist("Sync24");
        let primary = fixtures::album("Comfortable Void", 2012, &a);

        let action = Action::Data(DataAction::DiscographyLoaded {
            tab: Tab::Artists,
            depth: 0,
            primary: vec![primary],
            appears_on: vec![],
        });
        let scenario = Scenario::new(state).dispatch(action);
        let items = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0].items;

        assert_eq!(items.len(), 2, "no empty APPEARS ON header should appear");
        assert!(
            !items
                .iter()
                .any(|i| matches!(i, MediaItem::SectionHeader(h) if h.label == "APPEARS ON"))
        );
    }

    fn artists_column_loaded(total: usize, page_loaded: usize, cursor: usize) -> AppState {
        let mut state = focus_column(fixtures::fixture_empty(), Tab::Artists, 0);
        let loaded_count = ((page_loaded + 1) * 200).min(total);
        let mut column = Column::new(ColumnKind::Artists, "Artists");
        column.items = (0..loaded_count)
            .map(|i| MediaItem::Artist(fixtures::artist(&format!("Artist {i}"))))
            .collect();
        column.load = LoadState::Loaded { total };
        column.page_loaded = page_loaded;
        column.cursor = cursor;
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![column]);
        state
    }

    #[test]
    fn pagination_triggers_within_last_50() {
        // 500 total, page 0 (200 items) loaded, cursor within the last 50 loaded items.
        let state = artists_column_loaded(500, 0, 160);
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        assert!(scenario.effects().iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Artists,
                depth: 0,
                kind: ColumnKind::Artists,
                page: 1
            })
        )));
    }

    /// Reported in the field: the Artists list wouldn't scroll past the first page. Walks the
    /// cursor down the way a user holding `j` does, delivering each page the reducer asks for, and
    /// asserts it gets all the way to the end of a 2145-artist library rather than sticking at a
    /// page boundary.
    #[test]
    fn scrolling_walks_through_every_page_of_a_long_artist_list() {
        const TOTAL: usize = 2145;
        let mut state = artists_column_loaded(TOTAL, 0, 0);

        for step in 0..TOTAL + 10 {
            let effects =
                super::super::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
            for effect in effects {
                let Effect::Net(NetEffect::FetchColumn {
                    tab,
                    depth,
                    kind,
                    page,
                }) = effect
                else {
                    continue;
                };
                let loaded = ((page + 1) * 200).min(TOTAL);
                let items: Vec<MediaItem> = ((page * 200)..loaded)
                    .map(|i| MediaItem::Artist(fixtures::artist(&format!("Artist {i}"))))
                    .collect();
                super::super::apply(
                    &mut state,
                    Action::Data(DataAction::ItemsLoaded {
                        tab,
                        depth,
                        kind,
                        items,
                        total: TOTAL,
                        page,
                    }),
                );
            }
            let column = &state.nav.per_tab_stacks[&Tab::Artists][0];
            assert!(
                column.cursor >= step.min(TOTAL - 1),
                "cursor stuck at {} on step {step} (loaded {} of {TOTAL}, page_loaded {})",
                column.cursor,
                column.items.len(),
                column.page_loaded
            );
        }

        let column = &state.nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.items.len(), TOTAL, "the whole library should load");
        assert_eq!(
            column.cursor,
            TOTAL - 1,
            "the cursor should reach the last artist"
        );
    }

    /// Reported in the field as "the artists page doesn't scroll past the first pagination". The
    /// keyboard path pulls pages (`move_focused` -> `pagination_trigger`); the wheel returned no
    /// effects at all, so it walked to the end of the loaded page and stopped there permanently
    /// (`docs/12-decisions.md`).
    #[test]
    fn scrolling_with_the_wheel_pulls_the_next_page() {
        let state = artists_column_loaded(500, 0, 160);
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::ScrollColumn {
            column: 0,
            delta: 3,
        }));

        assert!(
            scenario.effects().iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::FetchColumn {
                    kind: ColumnKind::Artists,
                    page: 1,
                    ..
                })
            )),
            "the wheel must pull pages exactly as j/down do: {:?}",
            scenario.effects()
        );
    }

    /// The wheel targets the column under the pointer, so pagination must key off *that* column —
    /// not the focused one, which is a different depth with a different kind and page state.
    #[test]
    fn wheel_pagination_targets_the_scrolled_column_not_the_focused_one() {
        let mut state = artists_column_loaded(500, 0, 160);
        // A second, fully-loaded column on top, focused — the one `pagination_trigger` would see.
        let mut albums = Column::new(ColumnKind::Albums { of_artist: None }, "Albums");
        albums.items = vec![MediaItem::Album(fixtures::album(
            "Care",
            2019,
            &fixtures::artist("Boy Harsher"),
        ))];
        albums.load = LoadState::Loaded { total: 1 };
        state
            .nav
            .per_tab_stacks
            .get_mut(&Tab::Artists)
            .unwrap()
            .push(albums);
        state.nav.focus = NavFocus::Column(1);

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::ScrollColumn {
            column: 0,
            delta: 3,
        }));

        let pages: Vec<(usize, usize)> = scenario
            .effects()
            .iter()
            .filter_map(|e| match e {
                Effect::Net(NetEffect::FetchColumn { depth, page, .. }) => Some((*depth, *page)),
                _ => None,
            })
            .collect();
        assert_eq!(
            pages,
            vec![(0, 1)],
            "the scrolled column (depth 0) is what should page, not the focused one"
        );
    }

    /// Reported in the field: filtering for an artist listed them **twice**, and the duplicate
    /// disappeared once scrolling pulled a further page. Two paths independently request
    /// `page_loaded + 1` (cursor proximity and the active filter), and the worker's in-flight guard
    /// frees a key before its reply is applied — so the same page can be fetched twice and appended
    /// twice (`docs/12-decisions.md`).
    #[test]
    fn a_page_delivered_twice_is_only_applied_once() {
        let mut state = artists_column_loaded(500, 0, 0);
        let page_one: Vec<MediaItem> = (200..400)
            .map(|i| MediaItem::Artist(fixtures::artist(&format!("Artist {i}"))))
            .collect();
        let deliver = |state: &mut AppState, items: Vec<MediaItem>| {
            super::super::apply(
                state,
                Action::Data(DataAction::ItemsLoaded {
                    tab: Tab::Artists,
                    depth: 0,
                    kind: ColumnKind::Artists,
                    items,
                    total: 500,
                    page: 1,
                }),
            );
        };

        deliver(&mut state, page_one.clone());
        let after_first = state.nav.per_tab_stacks[&Tab::Artists][0].items.len();
        assert_eq!(after_first, 400, "page 1 appends onto page 0");

        deliver(&mut state, page_one);

        let column = &state.nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(
            column.items.len(),
            400,
            "the repeated page must not be appended a second time"
        );
        let names: std::collections::BTreeSet<&str> =
            column.items.iter().map(|i| i.display_name()).collect();
        assert_eq!(names.len(), column.items.len(), "no row may appear twice");
    }

    /// Page 0 stays exempt — it *replaces* rather than appends, which is how a refresh works.
    #[test]
    fn page_zero_still_replaces_on_a_refresh() {
        let mut state = artists_column_loaded(500, 1, 0);
        let fresh: Vec<MediaItem> = (0..3)
            .map(|i| MediaItem::Artist(fixtures::artist(&format!("Fresh {i}"))))
            .collect();

        super::super::apply(
            &mut state,
            Action::Data(DataAction::ItemsLoaded {
                tab: Tab::Artists,
                depth: 0,
                kind: ColumnKind::Artists,
                items: fresh,
                total: 3,
                page: 0,
            }),
        );

        let column = &state.nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.items.len(), 3, "a refresh rebuilds the column");
    }

    #[test]
    fn pagination_does_not_trigger_far_from_the_end() {
        let state = artists_column_loaded(500, 0, 10);
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        scenario.assert_no_effects();
    }

    #[test]
    fn applying_a_filter_pulls_the_next_page_even_far_from_the_end() {
        // Real bug: the inline filter only matched already-loaded rows, so a band on page 2+ of a
        // long Artists list never appeared. Setting a filter now pulls the next page (cursor is at
        // the top, nowhere near the end — `pagination_trigger` alone would fire nothing).
        let state = artists_column_loaded(500, 0, 0);
        let scenario =
            Scenario::new(state).dispatch(Action::Nav(NavAction::SetFilter("z".to_string())));
        assert!(scenario.effects().iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::FetchColumn {
                kind: ColumnKind::Artists,
                page: 1,
                ..
            })
        )));
    }

    #[test]
    fn filtered_column_keeps_pulling_pages_as_they_arrive() {
        let mut state = artists_column_loaded(500, 0, 0);
        state.nav.per_tab_stacks.get_mut(&Tab::Artists).unwrap()[0].filter = Some("z".to_string());
        // Page 1 arriving while the filter is active pulls page 2.
        let more: Vec<MediaItem> = (200..400)
            .map(|i| MediaItem::Artist(fixtures::artist(&format!("Artist {i}"))))
            .collect();
        let scenario = Scenario::new(state).dispatch(Action::Data(DataAction::ItemsLoaded {
            tab: Tab::Artists,
            depth: 0,
            kind: ColumnKind::Artists,
            items: more,
            total: 500,
            page: 1,
        }));
        assert!(scenario.effects().iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::FetchColumn {
                kind: ColumnKind::Artists,
                page: 2,
                ..
            })
        )));
    }

    #[test]
    fn a_fully_loaded_filtered_column_pulls_nothing_more() {
        let mut state = artists_column_loaded(150, 0, 0); // 150 < one full page: already complete
        state.nav.per_tab_stacks.get_mut(&Tab::Artists).unwrap()[0].filter = Some("z".to_string());
        let scenario =
            Scenario::new(state).dispatch(Action::Nav(NavAction::SetFilter("zz".to_string())));
        scenario.assert_no_effects();
    }

    #[test]
    fn pagination_never_triggers_for_a_small_playlists_column() {
        // Real bug: a `Playlists` column whose `total` doesn't fill a full 200-item page used to
        // trigger `FetchColumn { page: 1 }` as soon as the cursor was within 50 of the end (true for
        // almost any real playlist count) — the worker's `Playlists` handler always re-fetches the
        // *whole* list regardless of the page it's asked for, so this doubled every playlist on
        // screen once the reply came back reporting `page: 1` (`docs/12-decisions.md`).
        let mut state = focus_column(fixtures::fixture_empty(), Tab::Playlists, 0);
        let mut column = Column::new(ColumnKind::Playlists, "Playlists");
        column.items = (0..5)
            .map(|i| {
                MediaItem::Playlist(crate::model::Playlist {
                    id: ItemId::from(format!("pl-{i}")),
                    name: format!("Playlist {i}"),
                    overview: None,
                    track_count: 3,
                    total_duration: std::time::Duration::from_secs(600),
                    can_edit: true,
                    is_favorite: false,
                })
            })
            .collect();
        column.load = LoadState::Loaded { total: 5 };
        column.page_loaded = 0;
        column.cursor = 0;
        state
            .nav
            .per_tab_stacks
            .insert(Tab::Playlists, vec![column]);

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        scenario.assert_no_effects();
    }

    #[test]
    fn pagination_never_triggers_for_a_discography_albums_column() {
        // Same bug, the other shape: an `Albums { of_artist: Some(_) }` column (loaded via
        // `FetchDiscography`, never `FetchColumn`) has no `page_loaded` tracking of its own — this
        // used to fire a `FetchColumn` the worker just logs and drops (`docs/12-decisions.md`).
        let a = fixtures::artist("Boy Harsher");
        let mut state = focus_column(fixtures::fixture_empty(), Tab::Artists, 0);
        let mut column = Column::new(
            ColumnKind::Albums {
                of_artist: Some(a.id.clone()),
            },
            "Boy Harsher — Albums",
        );
        column.items = (0..3)
            .map(|i| MediaItem::Album(fixtures::album(&format!("Album {i}"), 2020, &a)))
            .collect();
        column.load = LoadState::Loaded { total: 3 };
        column.page_loaded = 0;
        column.cursor = 0;
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![column]);

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        scenario.assert_no_effects();
    }

    #[test]
    fn page_two_appends_not_replaces() {
        let state = artists_column_loaded(500, 0, 0);
        let new_items: Vec<MediaItem> = (0..50)
            .map(|i| MediaItem::Artist(fixtures::artist(&format!("Page2 {i}"))))
            .collect();
        let action = Action::Data(DataAction::ItemsLoaded {
            tab: Tab::Artists,
            depth: 0,
            kind: ColumnKind::Artists,
            items: new_items,
            total: 500,
            page: 1,
        });
        let scenario = Scenario::new(state).dispatch(action);
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.items.len(), 250, "page 2 must append, not replace");
        assert_eq!(column.page_loaded, 1);
    }

    #[test]
    fn load_failed_preserves_existing_items() {
        let state = focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 0);
        let before_items = state.nav.per_tab_stacks[&Tab::Artists][0].items.clone();

        let action = Action::Data(DataAction::LoadFailed {
            target: LoadTarget::Column {
                tab: Tab::Artists,
                depth: 0,
            },
            message: "offline".to_string(),
            offline: false,
        });
        let scenario = Scenario::new(state).dispatch(action);
        let column = &scenario.state().nav.per_tab_stacks[&Tab::Artists][0];
        assert_eq!(column.items, before_items);
        assert_eq!(column.load, LoadState::Error("offline".to_string()));
    }

    fn arb_nav_action() -> impl Strategy<Value = Action> {
        prop_oneof![
            (0usize..3).prop_map(|n| Action::Nav(NavAction::MoveUp { n })),
            (0usize..3).prop_map(|n| Action::Nav(NavAction::MoveDown { n })),
            Just(Action::Nav(NavAction::NavLeft)),
            Just(Action::Nav(NavAction::NavRight)),
            Just(Action::Nav(NavAction::PopColumn)),
            Just(Action::Nav(NavAction::GoToTop)),
            Just(Action::Nav(NavAction::GoToBottom)),
            Just(Action::Nav(NavAction::NextTab)),
            Just(Action::Nav(NavAction::PrevTab)),
            Just(Action::Nav(NavAction::OpenFilter)),
            any::<char>().prop_map(|c| Action::Nav(NavAction::FilterInput(c))),
            Just(Action::Nav(NavAction::FilterBackspace)),
            Just(Action::Nav(NavAction::CommitFilter)),
            Just(Action::Nav(NavAction::Cancel)),
            Just(Action::Select(SelectAction::ToggleVisualMode)),
            Just(Action::Select(SelectAction::ToggleItem)),
            Just(Action::Select(SelectAction::SelectAll)),
            Just(Action::Select(SelectAction::ClearSelection)),
            (0usize..6, 0usize..20).prop_map(|(column, index)| Action::Nav(
                NavAction::FocusColumnAt { column, index }
            )),
            (0usize..6, -10i32..10)
                .prop_map(|(column, delta)| Action::Nav(NavAction::ScrollColumn { column, delta })),
            any::<u64>().prop_map(|id| Action::Nav(NavAction::FocusQueueEntry(QueueEntryId(id)))),
        ]
    }

    // --- 07-01: search tab -------------------------------------------------------------------

    use crate::action::SystemEvent;
    use crate::effect::AudioEffect;
    use crate::model::{Album, AlbumRelation, Artist, Track};
    use crate::state::search::{SearchResults, SearchSection};
    use std::time::Duration as StdDuration;

    fn search_state() -> AppState {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Search;
        state
    }

    fn tick_at(state: &AppState, offset_ms: i64) -> Action {
        Action::System(SystemEvent::Tick(
            state
                .search
                .debounce_until
                .unwrap()
                .checked_add(jiff::SignedDuration::from_millis(offset_ms))
                .unwrap(),
        ))
    }

    #[test]
    fn search_debounces_250ms() {
        let scenario = Scenario::new(search_state()).dispatch_all(
            "hello"
                .chars()
                .map(|c| Action::Nav(NavAction::FilterInput(c))),
        );
        assert!(
            scenario.effects().is_empty(),
            "typing itself must never issue a Search effect"
        );
        assert_eq!(scenario.state().search.query, "hello");

        let fire_at = tick_at(scenario.state(), 0);
        let scenario = scenario.dispatch(fire_at);
        assert!(matches!(
            scenario.effects(),
            [Effect::Net(NetEffect::Search { query, .. })] if query == "hello"
        ));
    }

    #[test]
    fn short_query_issues_no_request() {
        let scenario =
            Scenario::new(search_state()).dispatch(Action::Nav(NavAction::FilterInput('a')));
        let fire_at = tick_at(scenario.state(), 0);
        let scenario = scenario.dispatch(fire_at);
        assert!(scenario.effects().is_empty());
        assert_eq!(scenario.state().search.load, LoadState::Idle);
    }

    #[test]
    fn query_unchanged_after_debounce_issues_nothing() {
        let scenario = Scenario::new(search_state()).dispatch_all(
            "hello"
                .chars()
                .map(|c| Action::Nav(NavAction::FilterInput(c))),
        );
        let fire_at = tick_at(scenario.state(), 0);
        let scenario = scenario.dispatch(fire_at);
        assert_eq!(scenario.effects().len(), 1, "first debounce fires once");

        // Re-arm the debounce (e.g. a stray keystroke resolving to the same text) without
        // changing the query, then let it fire again.
        let mut state = scenario.state().clone();
        state.search.debounce_until = Some(state.clock);
        let now = state.clock;
        let effects = crate::reducer::apply(&mut state, Action::System(SystemEvent::Tick(now)));
        assert!(
            effects.is_empty(),
            "a debounce firing for an unchanged query must issue nothing"
        );
    }

    fn artist(name: &str) -> Artist {
        Artist {
            id: ItemId::from(format!("artist-{name}")),
            name: name.to_string(),
            sort_name: name.to_string(),
            album_count: 0,
            track_count: 0,
            genres: Vec::new(),
            is_favorite: false,
            image: None,
            overview: None,
        }
    }

    fn album(name: &str, a: &Artist) -> Album {
        Album {
            id: ItemId::from(format!("album-{name}")),
            name: name.to_string(),
            sort_name: name.to_string(),
            album_artist_names: vec![a.name.clone()],
            album_artist_ids: vec![a.id.clone()],
            year: Some(2020),
            track_count: 0,
            total_duration: StdDuration::ZERO,
            genres: Vec::new(),
            is_favorite: false,
            image: None,
            relation: AlbumRelation::Primary,
        }
    }

    fn track(name: &str, alb: &Album, a: &Artist) -> Track {
        Track {
            id: ItemId::from(format!("track-{name}")),
            name: name.to_string(),
            album_id: Some(alb.id.clone()),
            album_name: alb.name.clone(),
            album_artist_names: alb.album_artist_names.clone(),
            artist_ids: vec![a.id.clone()],
            artist_names: vec![a.name.clone()],
            track_number: Some(1),
            disc_number: Some(1),
            year: alb.year,
            duration: StdDuration::from_secs(180),
            genres: Vec::new(),
            is_favorite: false,
            play_count: 0,
            format: crate::model::AudioFormat {
                codec: crate::model::Codec::Flac,
                sample_rate_hz: 44_100,
                bit_depth: Some(16),
                channels: 2,
                bitrate_bps: Some(1_000_000),
            },
            replay_gain: None,
            image: None,
            media_source_id: None,
            lyric_stream: None,
            date_created: None,
            playlist_entry_id: None,
        }
    }

    fn results_with(artists: Vec<Artist>, albums: Vec<Album>, tracks: Vec<Track>) -> SearchResults {
        SearchResults {
            artists,
            albums,
            tracks,
            artists_error: None,
            albums_error: None,
            tracks_error: None,
            playlists: Vec::new(),
            playlists_error: None,
        }
    }

    #[test]
    fn tab_cycles_sections() {
        let mut state = search_state();
        state.search.query_focused = false;
        let a = artist("Sync24");
        let alb = fixtures::album("Source", 2007, &a);
        state.search.results = results_with(
            vec![a.clone()],
            vec![alb.clone()],
            vec![fixtures::track("Bloom", 1, &alb, &[&a])],
        );
        assert_eq!(state.search.focused_section, SearchSection::Artists);

        let effects = crate::reducer::apply(&mut state, Action::Nav(NavAction::NextTab));
        assert!(effects.is_empty());
        assert_eq!(state.search.focused_section, SearchSection::Albums);

        crate::reducer::apply(&mut state, Action::Nav(NavAction::NextTab));
        assert_eq!(state.search.focused_section, SearchSection::Tracks);

        // Straight back to Artists: `Playlists` exists for the Favourites tab and Search never
        // populates it, so cycling must not stop on it.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::NextTab));
        assert_eq!(
            state.search.focused_section,
            SearchSection::Artists,
            "cycling wraps past the empty playlists section"
        );

        crate::reducer::apply(&mut state, Action::Nav(NavAction::PrevTab));
        assert_eq!(state.search.focused_section, SearchSection::Tracks);
    }

    /// An empty section is not somewhere a user can be: with nothing loaded at all, `Tab` has
    /// nowhere to go and leaves focus where it is rather than cycling empty headings.
    #[test]
    fn tab_does_not_cycle_through_empty_sections() {
        let mut state = search_state();
        state.search.query_focused = false;
        let a = artist("Sync24");
        let alb = fixtures::album("Source", 2007, &a);
        state.search.results = results_with(vec![], vec![alb], vec![]);

        crate::reducer::apply(&mut state, Action::Nav(NavAction::NextTab));
        assert_eq!(
            state.search.focused_section,
            SearchSection::Albums,
            "the one section with results is the only place to go"
        );

        let mut state = search_state();
        state.search.query_focused = false;
        crate::reducer::apply(&mut state, Action::Nav(NavAction::NextTab));
        assert_eq!(state.search.focused_section, SearchSection::Artists);
    }

    #[test]
    fn enter_focuses_first_nonempty_section() {
        let mut state = search_state();
        let a = artist("Sync24");
        state.search.results = results_with(vec![], vec![album("Void", &a)], vec![]);
        assert!(state.search.query_focused);

        crate::reducer::apply(&mut state, Action::Nav(NavAction::CommitFilter));
        assert!(!state.search.query_focused);
        assert_eq!(state.search.focused_section, SearchSection::Albums);
    }

    #[test]
    fn search_down_from_query_enters_results_then_flows_across_sections() {
        let mut state = search_state();
        let a = artist("Sync24");
        let alb = album("Void", &a);
        let t = track("Motion", &alb, &a);
        // One artist, one album, one track — the exact "single result per section" shape that used
        // to strand the user on the lone artist with no way down.
        state.search.results = results_with(vec![a], vec![alb], vec![t]);
        assert!(state.search.query_focused);

        // Down: query line -> first (and only) artist.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert!(!state.search.query_focused);
        assert_eq!(state.search.focused_section, SearchSection::Artists);
        assert_eq!(state.search.cursors[SearchSection::Artists.index()], 0);

        // Down again crosses into the Albums section (this is what was impossible before).
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_eq!(state.search.focused_section, SearchSection::Albums);

        // And once more into Tracks.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_eq!(state.search.focused_section, SearchSection::Tracks);

        // Down at the very end stays put.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_eq!(state.search.focused_section, SearchSection::Tracks);
    }

    #[test]
    fn search_up_flows_back_across_sections_then_returns_to_the_query_line() {
        let mut state = search_state();
        let a = artist("Sync24");
        let alb = album("Void", &a);
        state.search.results = results_with(vec![a], vec![alb], vec![]);
        state.search.query_focused = false;
        state.search.focused_section = SearchSection::Albums;

        // Up from the first album crosses back into Artists...
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveUp { n: 1 }));
        assert_eq!(state.search.focused_section, SearchSection::Artists);

        // ...and once more returns focus to the query line.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveUp { n: 1 }));
        assert!(state.search.query_focused);
    }

    #[test]
    fn search_up_from_query_parks_on_sidebar_and_arrows_switch_tabs() {
        let mut state = search_state();
        let a = artist("Sync24");
        state.search.results = results_with(vec![a], vec![], vec![]);
        assert!(state.search.query_focused);
        assert!(!state.nav.sidebar_focused);

        // Up from the query line steps out to the tab sidebar.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveUp { n: 1 }));
        assert!(state.nav.sidebar_focused);

        // Now Down switches to the next tab (Playlists follows Search in TAB_ORDER) and stays parked
        // on that tab's sidebar.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_eq!(state.nav.active_tab, Tab::Playlists);
        assert_eq!(state.nav.focus, NavFocus::Sidebar);
    }

    #[test]
    fn nav_left_parks_search_on_the_sidebar_and_nav_right_returns() {
        let mut state = search_state();
        let a = artist("Sync24");
        state.search.results = results_with(vec![a], vec![], vec![]);

        crate::reducer::apply(&mut state, Action::Nav(NavAction::NavLeft));
        assert!(state.nav.sidebar_focused);
        assert!(!state.search.query_focused);

        crate::reducer::apply(&mut state, Action::Nav(NavAction::NavRight));
        assert!(!state.nav.sidebar_focused);
        assert!(
            state.search.query_focused,
            "stepping back into search lands on the query line ready to type"
        );
    }

    #[test]
    fn now_playing_left_right_toggles_sidebar_and_arrows_switch_tabs() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::NowPlaying;
        assert!(!state.nav.sidebar_focused);

        // Left parks on the sidebar; Down then switches to the next tab.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::NavLeft));
        assert!(state.nav.sidebar_focused);
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_eq!(state.nav.active_tab, Tab::Favourites);
    }

    /// The Favourites tab is sectioned like Search, but `move_focused` had no branch for it — so
    /// `nav.focus` (always `Sidebar`, since the tab has no column stack) sent `↑`/`↓` to
    /// `switch_tab` and the cursor could never be put on a favourite at all. A live user reported
    /// it as "I can't select anything from favourites" (`docs/12-decisions.md`).
    #[test]
    fn arrows_move_through_favourites_instead_of_switching_tabs() {
        let artist = artist("Sync24");
        let album = fixtures::album("Source", 2007, &artist);
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Favourites;
        state.favourites.results = results_with(
            vec![artist.clone()],
            vec![album.clone()],
            vec![fixtures::track("Bloom", 1, &album, &[&artist])],
        );

        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_eq!(
            state.nav.active_tab,
            Tab::Favourites,
            "the arrow must move within the tab, not off it"
        );
        assert_eq!(state.favourites.focused_section, SearchSection::Albums);

        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_eq!(state.favourites.focused_section, SearchSection::Tracks);
        assert_eq!(state.favourites.cursors[SearchSection::Tracks.index()], 0);

        // Back up to the top, then one more `↑`: with no query line above it, focus steps out to
        // the tab sidebar, where arrows switch tabs again.
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveUp { n: 1 }));
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveUp { n: 1 }));
        assert!(!state.nav.sidebar_focused);
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveUp { n: 1 }));
        assert!(state.nav.sidebar_focused);
        crate::reducer::apply(&mut state, Action::Nav(NavAction::MoveDown { n: 1 }));
        assert_ne!(state.nav.active_tab, Tab::Favourites);
    }

    /// A click on a Search/Favourites row did nothing at all — neither tab has a Miller column, so
    /// `FocusColumnAt` could not reach them and no other action existed.
    #[test]
    fn clicking_a_favourites_row_moves_the_cursor_to_it() {
        let artist = artist("Sync24");
        let album = fixtures::album("Source", 2007, &artist);
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Favourites;
        state.nav.sidebar_focused = true;
        state.favourites.results = results_with(
            vec![artist.clone()],
            vec![
                album.clone(),
                fixtures::album("Comfortable Void", 2012, &artist),
            ],
            vec![],
        );

        crate::reducer::apply(
            &mut state,
            Action::Nav(NavAction::FocusSectionAt {
                section: SearchSection::Albums,
                index: 1,
            }),
        );
        assert_eq!(state.favourites.focused_section, SearchSection::Albums);
        assert_eq!(state.favourites.cursors[SearchSection::Albums.index()], 1);
        assert!(!state.nav.sidebar_focused);

        // Out of range is ignored rather than parking the cursor past the end.
        crate::reducer::apply(
            &mut state,
            Action::Nav(NavAction::FocusSectionAt {
                section: SearchSection::Tracks,
                index: 0,
            }),
        );
        assert_eq!(state.favourites.focused_section, SearchSection::Albums);
    }

    #[test]
    fn drill_into_artist_switches_to_artists_tab() {
        let mut state = search_state();
        let a = artist("Sync24");
        state.search.results = results_with(vec![a.clone()], vec![], vec![]);
        state.search.query_focused = false;
        state.search.focused_section = SearchSection::Artists;

        let effects = crate::reducer::apply(&mut state, Action::Nav(NavAction::NavRight));
        assert_eq!(state.nav.active_tab, Tab::Artists);
        // Two columns now: the browsable Artists root (so `←` isn't a dead end), then this artist's
        // albums on top of it, focused.
        let stack = &state.nav.per_tab_stacks[&Tab::Artists];
        assert_eq!(stack.len(), 2);
        assert_eq!(stack[0].kind, ColumnKind::Artists);
        assert!(matches!(
            stack[1].kind,
            ColumnKind::Albums { of_artist: Some(ref id) } if *id == a.id
        ));
        assert_eq!(state.nav.focus, NavFocus::Column(1));
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::FetchColumn {
                kind: ColumnKind::Artists,
                depth: 0,
                ..
            })
        )));
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::FetchDiscography { artist, depth: 1, .. }) if *artist == a.id
        )));
    }

    #[test]
    fn queue_from_search_result_works() {
        let mut state = search_state();
        let a = artist("Sync24");
        let alb = album("Void", &a);
        let t = track("Motion", &alb, &a);
        state.search.results = results_with(vec![], vec![], vec![t.clone()]);
        state.search.query_focused = false;
        state.search.focused_section = SearchSection::Tracks;

        let effects = crate::reducer::apply(
            &mut state,
            Action::Queue(crate::action::QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 1);
        assert_eq!(state.queue.entries[0].track.id, t.id);
        // `08-03`: `load_current` now always emits `EnsureCached` immediately before `Load`.
        // `10-10`: a trailing desktop-notify effect, since the fixture's own `terminal_focused`
        // is `None` (unknown) and `ui.desktop_notifications` defaults on. `10-11`: and a trailing
        // MPRIS metadata update, since something is now queued.
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(_),
                Effect::Audio(AudioEffect::Load { .. }),
                Effect::Sys(crate::effect::SysEffect::Notify(_)),
                Effect::Sys(crate::effect::SysEffect::UpdateMpris(_))
            ]
        ));
    }

    #[test]
    fn partial_failure_shows_per_section_error() {
        let mut state = search_state();
        state.search.query = "boy harsher".to_string();
        let results = SearchResults {
            artists: Vec::new(),
            albums: vec![album("Care", &artist("Boy Harsher"))],
            tracks: Vec::new(),
            artists_error: Some("server error".to_string()),
            albums_error: None,
            tracks_error: None,
            playlists: Vec::new(),
            playlists_error: None,
        };
        crate::reducer::apply(
            &mut state,
            Action::Data(DataAction::SearchResultsLoaded {
                query: "boy harsher".to_string(),
                results,
            }),
        );
        assert_eq!(
            state.search.results.error(SearchSection::Artists),
            Some("server error")
        );
        assert!(state.search.results.error(SearchSection::Albums).is_none());
        assert!(!state.search.results.is_empty(SearchSection::Albums));
    }

    #[test]
    fn stale_search_reply_is_discarded() {
        let mut state = search_state();
        state.search.query = "new query".to_string();
        let effects = crate::reducer::apply(
            &mut state,
            Action::Data(DataAction::SearchResultsLoaded {
                query: "old query".to_string(),
                results: results_with(vec![artist("Stale")], vec![], vec![]),
            }),
        );
        assert!(effects.is_empty());
        assert!(state.search.results.artists.is_empty());
    }

    #[test]
    fn query_focused_on_tab_entry() {
        let mut state = fixtures::fixture_empty();
        state.search.query_focused = false;
        crate::reducer::apply(&mut state, Action::Nav(NavAction::SetTab(Tab::Search)));
        assert!(state.search.query_focused);
    }

    // --- 07-04: genres tab -------------------------------------------------------------------

    fn genre_column(name: &str) -> Column {
        let mut column = Column::new(ColumnKind::Genres, "Genres");
        column.items = vec![MediaItem::Genre(crate::model::Genre {
            id: ItemId::from(format!("genre-{name}")),
            name: name.to_string(),
        })];
        column.load = LoadState::Loaded { total: 1 };
        column
    }

    fn genre_artists_column(genre_name: &str, artist: &crate::model::Artist) -> Column {
        let mut column = Column::new(
            ColumnKind::GenreArtists {
                of_genre: genre_name.to_string(),
            },
            format!("{genre_name} — Artists"),
        );
        column.items = vec![MediaItem::Artist(artist.clone())];
        column.load = LoadState::Loaded { total: 1 };
        column
    }

    #[test]
    fn genres_seed_column_loads_on_first_entry() {
        let scenario = Scenario::new(fixtures::fixture_empty())
            .dispatch(Action::Nav(NavAction::SetTab(Tab::Genres)));
        assert_eq!(scenario.state().nav.active_tab, Tab::Genres);
        assert_eq!(
            scenario.state().nav.per_tab_stacks[&Tab::Genres][0].kind,
            ColumnKind::Genres
        );
        assert_eq!(
            scenario.last_effect(),
            Some(&Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Genres,
                depth: 0,
                kind: ColumnKind::Genres,
                page: 0,
            }))
        );
    }

    #[test]
    fn drill_genre_to_artists() {
        let mut state = fixtures::fixture_empty();
        state
            .nav
            .per_tab_stacks
            .insert(Tab::Genres, vec![genre_column("Darkwave")]);
        let mut state = focus_column(state, Tab::Genres, 0);

        let effects = crate::reducer::apply(&mut state, Action::Nav(NavAction::NavRight));
        assert_eq!(state.nav.per_tab_stacks[&Tab::Genres].len(), 2);
        assert_eq!(
            state.nav.per_tab_stacks[&Tab::Genres][1].kind,
            ColumnKind::GenreArtists {
                of_genre: "Darkwave".to_string(),
            }
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::FetchColumn {
                kind: ColumnKind::GenreArtists { of_genre },
                ..
            })] if of_genre == "Darkwave"
        ));
    }

    #[test]
    fn drill_artist_reuses_discography_kind() {
        let a = fixtures::artist("Boy Harsher");
        let mut state = fixtures::fixture_empty();
        state.nav.per_tab_stacks.insert(
            Tab::Genres,
            vec![
                genre_column("Darkwave"),
                genre_artists_column("Darkwave", &a),
            ],
        );
        let mut state = focus_column(state, Tab::Genres, 1);

        let effects = crate::reducer::apply(&mut state, Action::Nav(NavAction::NavRight));
        assert_eq!(state.nav.per_tab_stacks[&Tab::Genres].len(), 3);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::FetchDiscography { artist, .. })] if *artist == a.id
        ));
    }

    #[test]
    fn four_level_drill_slides_window() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);

        let mut albums_col = Column::new(
            ColumnKind::Albums {
                of_artist: Some(a.id.clone()),
            },
            format!("{} — Albums", a.name),
        );
        albums_col.items = vec![MediaItem::Album(alb.clone())];
        albums_col.load = LoadState::Loaded { total: 1 };

        let mut tracks_col = Column::new(
            ColumnKind::Tracks {
                of_album: alb.id.clone(),
            },
            format!("{} — Tracks", alb.name),
        );
        tracks_col.items = vec![MediaItem::Track(t)];
        tracks_col.load = LoadState::Loaded { total: 1 };

        let mut state = fixtures::fixture_empty();
        state.nav.per_tab_stacks.insert(
            Tab::Genres,
            vec![
                genre_column("Darkwave"),
                genre_artists_column("Darkwave", &a),
                albums_col,
                tracks_col,
            ],
        );
        state.nav.active_tab = Tab::Genres;
        state.nav.focus = NavFocus::Column(0);

        let scenario =
            Scenario::new(state).dispatch_all((0..3).map(|_| Action::Nav(NavAction::NavRight)));
        assert_eq!(scenario.state().nav.focus, NavFocus::Column(3));
        assert_eq!(
            scenario.state().nav.window_start,
            1,
            "levels 2-4 (depths 1-3) visible, level 1 (the genre list) slid out of view"
        );
    }

    #[test]
    fn appears_on_split_present_at_level_three() {
        let a = fixtures::artist("Sync24");
        let mut state = fixtures::fixture_empty();
        state.nav.per_tab_stacks.insert(
            Tab::Genres,
            vec![
                genre_column("Darkwave"),
                genre_artists_column("Darkwave", &a),
                Column::new(
                    ColumnKind::Albums {
                        of_artist: Some(a.id.clone()),
                    },
                    format!("{} — Albums", a.name),
                ),
            ],
        );
        let state = focus_column(state, Tab::Genres, 2);

        let primary = fixtures::album("Comfortable Void", 2012, &a);
        let comp = fixtures::appears_on_album("Fahrenheit Project", 2005, &a);
        let scenario = Scenario::new(state).dispatch(Action::Data(DataAction::DiscographyLoaded {
            tab: Tab::Genres,
            depth: 2,
            primary: vec![primary],
            appears_on: vec![comp],
        }));
        let items = &scenario.state().nav.per_tab_stacks[&Tab::Genres][2].items;
        assert!(matches!(
            &items[0],
            MediaItem::SectionHeader(h) if h.label == "ALBUMS" && h.count == 1
        ));
        assert!(matches!(
            &items[2],
            MediaItem::SectionHeader(h) if h.label == "APPEARS ON" && h.count == 1
        ));
    }

    // --- 07-05: folders tab -------------------------------------------------------------------

    fn folder_item(id: &str, name: &str) -> MediaItem {
        MediaItem::Folder(Folder {
            id: ItemId::from(id),
            name: name.to_string(),
        })
    }

    fn file_item(name: &str) -> MediaItem {
        let a = fixtures::artist("Loose Files");
        let alb = fixtures::album("Untagged", 2020, &a);
        MediaItem::Track(fixtures::track(name, 1, &alb, &[&a]))
    }

    fn folders_state() -> AppState {
        let mut state = fixtures::fixture_empty();
        state.nav.per_tab_stacks.insert(
            Tab::Folders,
            vec![Column::new(
                ColumnKind::Folders { of_parent: None },
                "Folders",
            )],
        );
        state
    }

    fn load_folder_items(state: AppState, items: Vec<MediaItem>) -> Scenario {
        let total = items.len();
        Scenario::new(state).dispatch(Action::Data(DataAction::ItemsLoaded {
            tab: Tab::Folders,
            depth: 0,
            kind: ColumnKind::Folders { of_parent: None },
            items,
            total,
            page: 0,
        }))
    }

    #[test]
    fn folders_sort_before_files() {
        let scrambled = vec![
            file_item("Track 1"),
            folder_item("f2", "Zeta"),
            file_item("Track 2"),
            folder_item("f1", "Alpha"),
        ];
        let scenario = load_folder_items(folders_state(), scrambled);
        let items = &scenario.state().nav.per_tab_stacks[&Tab::Folders][0].items;
        assert!(matches!(items[0], MediaItem::Folder(_)));
        assert!(matches!(items[1], MediaItem::Folder(_)));
        assert!(matches!(items[2], MediaItem::Track(_)));
        assert!(matches!(items[3], MediaItem::Track(_)));
    }

    #[test]
    fn natural_order_within_groups() {
        let scrambled = vec![
            file_item("track10"),
            file_item("track2"),
            file_item("track1"),
        ];
        let scenario = load_folder_items(folders_state(), scrambled);
        let names: Vec<&str> = scenario.state().nav.per_tab_stacks[&Tab::Folders][0]
            .items
            .iter()
            .map(MediaItem::display_name)
            .collect();
        assert_eq!(names, vec!["track1", "track2", "track10"]);
    }

    #[test]
    fn deep_stack_slides_window() {
        let mut state = fixtures::fixture_empty();
        let stack: Vec<Column> = (0..7)
            .map(|i| loaded_folder_column(&format!("f{i}"), &format!("f{}", i + 1)))
            .collect();
        state.nav.active_tab = Tab::Folders;
        state.nav.per_tab_stacks.insert(Tab::Folders, stack);
        state.nav.focus = NavFocus::Column(0);

        let scenario =
            Scenario::new(state).dispatch_all((0..6).map(|_| Action::Nav(NavAction::NavRight)));
        assert_eq!(scenario.state().nav.focus, NavFocus::Column(6));
        assert_eq!(
            scenario.state().nav.window_start,
            4,
            "the sliding window keeps working with no depth cap"
        );
    }

    fn folders_column_loaded(total: usize, page_loaded: usize, cursor: usize) -> AppState {
        let mut state = focus_column(fixtures::fixture_empty(), Tab::Folders, 0);
        let loaded_count = ((page_loaded + 1) * 200).min(total);
        // A real *sub-folder* (`of_parent: Some(_)`) — a directory's own children do paginate. The
        // Folders *root* (`of_parent: None`) is the single-shot music-library list and never does.
        let kind = ColumnKind::Folders {
            of_parent: Some(ItemId::from("lib-1")),
        };
        let mut column = Column::new(kind, "Folder");
        column.items = (0..loaded_count)
            .map(|i| folder_item(&format!("f{i}"), &format!("Folder {i}")))
            .collect();
        column.load = LoadState::Loaded { total };
        column.page_loaded = page_loaded;
        column.cursor = cursor;
        state.nav.per_tab_stacks.insert(Tab::Folders, vec![column]);
        state
    }

    #[test]
    fn pagination_at_200_per_directory() {
        // 500 entries, page 0 (200 loaded) loaded, cursor within the last 50 loaded — same
        // pagination-lookahead threshold every other column already uses.
        let state = folders_column_loaded(500, 0, 160);
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        assert!(scenario.effects().iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Folders,
                depth: 0,
                kind: ColumnKind::Folders { of_parent: Some(_) },
                page: 1
            })
        )));
    }

    #[test]
    fn folders_root_is_the_library_list_and_never_paginates() {
        // The Folders root lists music libraries (a handful, one shot) — it must not fire a
        // `page: 1` fetch the way a real directory does, or the libraries would double
        // (`docs/12-decisions.md`).
        let mut state = focus_column(fixtures::fixture_empty(), Tab::Folders, 0);
        let mut column = Column::new(ColumnKind::Folders { of_parent: None }, "Folders");
        column.items = (0..3)
            .map(|i| folder_item(&format!("lib{i}"), &format!("Library {i}")))
            .collect();
        column.load = LoadState::Loaded { total: 3 };
        column.cursor = 0;
        state.nav.per_tab_stacks.insert(Tab::Folders, vec![column]);
        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));
        assert!(
            !scenario
                .effects()
                .iter()
                .any(|e| matches!(e, Effect::Net(NetEffect::FetchColumn { .. })))
        );
    }

    proptest! {
        #[test]
        fn reducer_never_panics(actions in prop::collection::vec(arb_nav_action(), 0..100)) {
            for fixture in [
                fixtures::fixture_empty(),
                fixtures::fixture_miller_3col(),
                fixtures::fixture_miller_5col(),
                fixtures::fixture_appears_on(),
                fixtures::fixture_visual_select(),
                fixtures::fixture_playing_queue(),
                fixtures::fixture_offline(),
                fixtures::fixture_with_history(),
            ] {
                let _ = Scenario::new(fixture).dispatch_all(actions.clone());
            }
        }
    }
}
