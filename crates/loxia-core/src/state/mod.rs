//! AppState root and its sub-state modules.

pub mod favourites;
pub mod modal;
pub mod nav;
pub mod player;
pub mod queue;
pub mod search;
pub mod settings;
pub mod toast;

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::config::{Config, ConfigWarning, QualityProfile};
use crate::keymap::{KeyChord, KeyMap};
use crate::model::{ItemId, Lyrics, MediaItem, PlaylistId, ServerId, UserId};
use crate::theme::Theme;

use favourites::FavouritesState;
use modal::Modal;
use nav::{Column, NavFocus, NavState, Tab};
use player::{EqState, PlayerState};
use queue::{HistoryEntry, QueueEntry, QueueState};
use search::SearchState;
use settings::SettingsState;
use toast::{Toast, ToastLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Connectivity {
    #[default]
    Online,
    Offline,
    Reconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NowPlayingSub {
    #[default]
    Queue,
    History,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ServerSession {
    pub user_id: Option<UserId>,
    pub server_name: Option<String>,
    pub connected_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub image_cache_bytes: u64,
    pub audio_cache_bytes: u64,
    pub pinned_count: usize,
    /// `11-07`: total on-disk size of permanent downloads (`loxia_cache::downloads`) — distinct
    /// from `pinned_count`, which is only ever the number of entries. Read once at startup for the
    /// About view's "Downloads … (14.7 GB, 312 tracks)" line; this crate has no I/O of its own to
    /// keep it live thereafter (`docs/12-decisions.md`).
    pub download_bytes: u64,
}

/// `11-07`: environment facts the About view shows that `AppState` has no other way to know —
/// gathered once, in the binary, before the render loop starts (the same "static boot info, one
/// direct field write, no full `Action` round-trip" pattern `main.rs` already uses for
/// `state.history`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AboutInfo {
    pub libmpv: LibmpvStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LibmpvStatus {
    /// `reason` is `"--no-audio"` when the mock backend was explicitly requested, or the real
    /// engine's own init-failure message otherwise (`main::select_audio_backend`'s existing
    /// "falls back to the mock engine with a warning" path) — distinct strings, since only the
    /// first is the literal `"not loaded (--no-audio)"` this task's own spec names.
    NotLoaded(String),
    Loaded {
        version: String,
        path: String,
    },
}

impl Default for LibmpvStatus {
    fn default() -> Self {
        LibmpvStatus::NotLoaded("not started".to_string())
    }
}

/// Bumped whenever `SessionSnapshot`'s own shape changes in a way that makes an old snapshot
/// unsafe to deserialise as the new one. The canonical value — `loxia_cache::session::
/// SCHEMA_VERSION` is a re-export of this, not a second copy, so the two can never drift apart —
/// lives here rather than in `loxia-cache` because `11-03`'s server-switch reducer code needs to
/// stamp a fresh `SessionSnapshot` itself (a pure `AppState` -> `SessionSnapshot` copy, no I/O),
/// and `loxia-core` cannot depend on `loxia-cache` to borrow its constant.
pub const SESSION_SCHEMA_VERSION: u32 = 1;

/// `session.json` (`docs/02-data-model.md` §9). Defined here because
/// `Action::System::SessionRestored` needs a concrete payload type, even though reading/writing
/// the file is `loxia-cache`'s job, a later phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub schema_version: u32,
    pub server_id: ServerId,
    pub queue: QueueState,
    pub position_secs: f64,
    pub active_tab: Tab,
    pub zen_mode: bool,
    pub volume: u8,
    pub quality_profile: QualityProfile,
    pub eq: EqState,
    pub saved_at: Timestamp,
}

/// Deliberately **not** `Serialize`/`Deserialize` — nothing persists the whole struct at once.
/// `NavState`/`QueueState`/`HistoryEntry` carry their own derives for `session.json`/
/// `history.json`, and `SessionSnapshot` copies out just the fields that round-trip.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppState {
    pub config: Config,
    pub theme: Theme,
    pub keymap: KeyMap,
    pub nav: NavState,
    pub queue: QueueState,
    /// Correlates a `FetchAlbumTracksForQueue` reply back to the `a`/`A` press that triggered it
    /// (`06-02`) — the effect itself is identical either way (`docs/03-emby-api.md` §4: filtering
    /// happens client-side on the reply, never before the fetch), so the filter decision has
    /// nowhere else to survive the round trip. Keyed by album id so two rapid presses on different
    /// albums don't clobber each other; `None` means "queue everything" (Primary album, or `A` on
    /// an AppearsOn one), `Some(artist)` means "keep only that artist's tracks" (`a` on an
    /// AppearsOn album). Consumed (removed) when the matching reply arrives.
    pub pending_album_queue_fetch: HashMap<ItemId, Option<ItemId>>,
    /// The multi-row / insert-next queue request currently awaiting its fetches, if any — see
    /// [`crate::state::queue::QueueBatch`] for why one is needed at all. At most one is in flight:
    /// starting another replaces it, so a fetch that never replies (deduplicated away as already
    /// in-flight, say) can delay a batch but can never wedge queueing permanently. Replies to a
    /// replaced batch are not lost — they fall through to the ordinary append path.
    pub pending_queue_batch: Option<crate::state::queue::QueueBatch>,
    pub history: VecDeque<HistoryEntry>,
    /// Whether a history entry has already been recorded for the play currently loaded at
    /// `queue.current()` (`06-05`) — reset to `false` every time `queue::load_current` emits a
    /// fresh `Load`, so `Repeat::One` replaying the same entry records once per play, but a
    /// duplicate completion signal for the *same* play (a stray second `TrackEnded`, or
    /// `TrackEnded` arriving after the position-crossing threshold already recorded it) does not
    /// double up. Deliberately not part of `HistoryEntry`/`QueueState` — this is bookkeeping about
    /// the current play, not persisted state.
    pub history_recorded_this_play: bool,
    /// Correlates an in-flight `Effect::Net(InstantMix)` reply back to the seed's display name
    /// (`06-08`) — `DataAction::TracksLoaded`'s `source: QueueSource::InstantMix { seed }` carries
    /// only the `ItemId`, not a name, and the toast on reply ("instant mix: N tracks from
    /// `<name>`" / "no instant mix available for `<name>`") needs one. `None` once no mix is
    /// in flight; consumed (taken) when the reply lands.
    pub pending_instant_mix: Option<String>,
    pub player: PlayerState,
    /// Only one modal at a time, by construction — opening a second replaces it.
    pub modal: Option<Modal>,
    pub search: SearchState,
    pub favourites: FavouritesState,
    pub settings: SettingsState,
    /// `11-01`: "persistence is debounced 1 second" — set to `now + 1s` on every settings edit,
    /// mirroring `SearchState::debounce_until`'s own shape; `reducer::tick` fires the actual
    /// `Effect::Sys(WriteConfig)` once this deadline passes. `None` while nothing is pending.
    /// Every *other* pre-existing config-writing path in this codebase writes immediately — this
    /// debounce is deliberately scoped to the settings view's own edits only (`docs/12-decisions.md`).
    pub settings_write_debounce_until: Option<Timestamp>,
    /// One in-flight optimistic `ToggleFavorite` per item (`07-02`), keyed by id — consumed
    /// (removed) once the matching `SetFavorite` reply (success, or `LoadFailed` triggering a
    /// rollback) arrives.
    pub pending_favorite_toggles: HashMap<ItemId, PendingFavoriteToggle>,
    /// One in-flight optimistic playlist mutation per playlist (`07-03`) — consumed (removed)
    /// only on failure (a success needs no rollback, so nothing removes it then; see
    /// `docs/12-decisions.md` for the same bounded-leak reasoning `07-02`'s own
    /// `pending_favorite_toggles` already accepted).
    pub pending_playlist_mutations: HashMap<PlaylistId, PendingPlaylistMutation>,
    pub toasts: Vec<Toast>,
    /// `10-13`: backs each new `Toast`'s own `id` (`AppState::toast`) — a stable identity
    /// distinct from `message` equality, which the same method's own deduplication uses instead.
    pub next_toast_id: u64,
    pub connectivity: Connectivity,
    /// Consecutive `EmbyError::Offline` failures (`08-06`, `docs/06-cache-and-offline.md` §6) —
    /// reset by any successful reply, incremented only by `DataAction::LoadFailed { offline:
    /// true, .. }`. Reaching `reducer::connectivity`'s threshold (2) enters `Offline`.
    pub offline_failures: u32,
    /// When the next connectivity probe (`Effect::Net(NetEffect::Reconnect)`) is due — `None`
    /// while `connectivity != Offline`, or immediately after entering `Offline` (due on the very
    /// next `Tick`). Advanced by `reducer::connectivity::schedule_next_probe`.
    pub next_probe_at: Option<Timestamp>,
    /// The current probe backoff, in seconds — starts at 5, doubles on every scheduled probe, capped
    /// at 30 (`docs/06-cache-and-offline.md` §6).
    pub probe_backoff_secs: i64,
    /// Probes made since going offline, reset on every return to `Online`.
    ///
    /// Counted separately from `probe_backoff_secs` rather than inferred from it: the backoff is
    /// clamped at the probe backoff's own cap, so past the cap it no
    /// longer says how many attempts have been made — and "have the cheap attempts been spent yet"
    /// is exactly the question deciding when to try a profile's other addresses
    /// (`docs/12-decisions.md`).
    pub probes_since_offline: u32,
    pub zen_mode: bool,
    /// `Some((track_id, lyrics))` for at most one track at a time (`07-07`) — a new
    /// `DataAction::LyricsLoaded` replaces it outright, never merges. Pane *visibility* is
    /// `config.ui.show_lyrics` itself (`ActionId::ToggleLyrics` flips and persists that field
    /// directly, `docs/12-decisions.md`); there is deliberately no separate session-only flag.
    pub lyrics: Option<(ItemId, Lyrics)>,
    /// How many lyric lines are scrolled off the top of the pane, for **unsynced** lyrics only —
    /// timed ones follow playback and have nothing to scroll manually. Counted in source lines
    /// rather than rendered rows, so the reducer can clamp it exactly without knowing the pane's
    /// width or wrapping (`docs/12-decisions.md`). Reset whenever the lyrics themselves change.
    pub lyrics_scroll: usize,
    pub now_playing_subview: NowPlayingSub,
    /// The Now Playing tab's own cursor (`07-06`) — indexes into `queue.play_order` when
    /// `now_playing_subview == Queue`, or into `history_sorted()`'s newest-first order when
    /// `History`. Neither pane is a Miller column (`seed_column_for_tab` gives `NowPlaying` no
    /// column stack at all), so this is dedicated state rather than reusing `Column::cursor`.
    /// Auto-follows `queue.position` (reset alongside `now_playing_user_scrolled` whenever
    /// `reducer::queue::load_current` runs) until the user moves it manually; reset to `0` when
    /// the sub-view toggles, since a queue-row index means nothing in the history list.
    pub now_playing_cursor: usize,
    /// Set whenever the user moves `now_playing_cursor` manually — suppresses auto-scroll-to-
    /// current until the next track change clears it.
    pub now_playing_user_scrolled: bool,
    /// First visible row of the Now Playing pane — a real stored offset, like `Column::scroll_
    /// offset`, rather than something derived from the cursor at render time.
    ///
    /// It used to be derived, by *centring* the cursor. Every cursor move therefore re-centred the
    /// list, so clicking a row scrolled it out from under the pointer and the second click of a
    /// double-click landed on a different track — "in a long play queue the view jumps"
    /// (`docs/12-decisions.md`). With the offset stored, a click that lands on an already-visible
    /// row changes nothing about what is on screen.
    pub now_playing_scroll: usize,
    /// A partial chord sequence (e.g. after typing `g`, waiting for `g`/`a`/`l`), cleared on
    /// `Tick` after a 1-second timeout.
    pub pending_chord: Option<(KeyChord, Timestamp)>,
    /// The last `Tick`'s timestamp — added by task `04-05` for the header clock. A widget must
    /// read this, never call `Timestamp::now()` itself, or two renders of the same state stop
    /// being byte-identical (`header_uses_state_clock_not_system_clock`). Reducer rule 2 ("no
    /// time... in the reducer") is upheld: this is *written* only from `Tick`'s own carried
    /// timestamp, never read back to make a branching decision by the reducer itself.
    pub clock: Timestamp,
    /// Counts every `Tick` since launch (`06-07`) — the 100th (10s of 100ms ticks) drives the
    /// periodic playback-progress report (`docs/04-state-and-input.md` §9 rule 5). Ephemeral
    /// bookkeeping, not a time *value* itself, so it doesn't conflict with reducer rule 2 ("no
    /// time... in the reducer") the way reading a live clock would.
    pub tick_count: u64,
    pub downloads_active: usize,
    /// Ids currently pinned as permanent downloads — seeded from `downloads_index.json` at startup
    /// and kept current by `DataAction::DownloadProgress`/`DownloadRemoved`.
    ///
    /// `d` is a *toggle*, so it has to know which way it is going, and this is the only thing in
    /// the app that knows. The whole download feature was unreachable before this existed: the
    /// reducer's `ToggleDownload` arm returned no effects at all (`docs/12-decisions.md`).
    pub downloads: std::collections::BTreeSet<ItemId>,
    /// The address currently connected on — the primary, or whichever fallback answered.
    ///
    /// Worth showing: with more than one endpoint configured, "which way in am I using?" decides
    /// whether a slow library is the LAN or the round trip through a proxy, and nothing else on
    /// screen would say (`docs/12-decisions.md`).
    pub active_endpoint: String,
    pub cache_stats: CacheStats,
    /// `11-07`: `None` until `main` resolves it at startup — a real `Paths` needs actual base
    /// directories to construct (`Paths::resolve`'s own `dirs: &impl BaseDirs`), which nothing
    /// before that point has. Construction itself performs no I/O (`paths.rs`'s own doc comment),
    /// so holding the result here afterward is no different from any other plain-data field; the
    /// About view is its only reader today (config/cache/download paths).
    pub paths: Option<crate::paths::Paths>,
    pub about: AboutInfo,
    pub server: ServerSession,
    /// Surfaced at startup and in Settings.
    pub config_warnings: Vec<ConfigWarning>,
    pub should_quit: bool,
    pub dirty: bool,
    /// Whether the terminal itself currently has input focus (`10-10`) — `None` until the first
    /// `SystemEvent::TerminalFocusChanged` arrives, meaning "unknown", not "unfocused". A desktop
    /// notification on track change is suppressed only when this is `Some(true)`; an unknown state
    /// notifies normally (`docs/12-decisions.md`: some terminals never send focus events at all,
    /// and refusing to notify just because none has arrived yet would silently break notifications
    /// there forever).
    pub terminal_focused: Option<bool>,
}

/// A row removed from `AppState.favourites.results` when unfavouriting from the Favourites tab
/// itself (`07-02`) — restored at the same index, not appended, if the underlying `SetFavorite`
/// request fails; appending would make an accidental unfavourite look like the row jumped to the
/// bottom instead of simply coming back.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // see `MediaItem`'s own rationale, crates/loxia-core/src/model/item.rs
pub enum RemovedFavourite {
    Artist(usize, crate::model::Artist),
    Album(usize, crate::model::Album),
    Track(usize, crate::model::Track),
    Playlist(usize, crate::model::Playlist),
}

/// One in-flight optimistic favourite toggle (`07-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct PendingFavoriteToggle {
    /// The value it was flipped *to* — restoring on failure means setting it back to `!on`.
    pub on: bool,
    pub removed_from_favourites: Option<RemovedFavourite>,
}

/// One in-flight optimistic playlist mutation (`07-03`), keyed by playlist id in
/// `AppState.pending_playlist_mutations` — a second mutation on the same playlist before the
/// first resolves overwrites the pending entry (last request wins), the same simplification
/// `06-02`'s `pending_album_queue_fetch` already accepted.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // see `MediaItem`'s own rationale, crates/loxia-core/src/model/item.rs
pub enum PendingPlaylistMutation {
    /// The removed rows, as `(original absolute index, item)` pairs sorted by index ascending —
    /// reinserted in that same order on failure so a multi-row removal rolls back cleanly.
    Remove(Vec<(usize, MediaItem)>),
    /// The entry moved, and where it was before the move — restoring means moving it back.
    Move {
        entry: crate::model::PlaylistEntryId,
        from_index: usize,
    },
    /// The removed `MediaItem::Playlist` row (from the `Playlists` list column itself) and its
    /// original index.
    Delete(usize, MediaItem),
}

impl AppState {
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// The optimistic-update reference implementation (`07-02`) — flips `is_favorite` wherever
    /// `id` currently appears: every Miller column across every tab, the queue, history, the
    /// Search tab's results, and the Favourites tab's own results. Never removes a row itself
    /// (`Tab::Favourites`'s own "unfavouriting removes the row" behaviour is handled separately,
    /// since only that one view drops rows that no longer belong in it) — this only ever updates
    /// the flag, everywhere it's stored.
    pub fn update_item_favorite(&mut self, id: &ItemId, on: bool) {
        fn set_if_match(item: &mut MediaItem, id: &ItemId, on: bool) {
            match item {
                MediaItem::Artist(a) if &a.id == id => a.is_favorite = on,
                MediaItem::Album(a) if &a.id == id => a.is_favorite = on,
                MediaItem::Track(t) if &t.id == id => t.is_favorite = on,
                MediaItem::Playlist(p) if &p.id == id => p.is_favorite = on,
                _ => {}
            }
        }

        for columns in self.nav.per_tab_stacks.values_mut() {
            for column in columns.iter_mut() {
                for item in column.items.iter_mut() {
                    set_if_match(item, id, on);
                }
            }
        }
        for entry in self.queue.entries.iter_mut() {
            if &entry.track.id == id {
                entry.track.is_favorite = on;
            }
        }
        for entry in self.history.iter_mut() {
            if &entry.track.id == id {
                entry.track.is_favorite = on;
            }
        }
        for results in [&mut self.search.results, &mut self.favourites.results] {
            for a in results.artists.iter_mut() {
                if &a.id == id {
                    a.is_favorite = on;
                }
            }
            for a in results.albums.iter_mut() {
                if &a.id == id {
                    a.is_favorite = on;
                }
            }
            for t in results.tracks.iter_mut() {
                if &t.id == id {
                    t.is_favorite = on;
                }
            }
            for p in results.playlists.iter_mut() {
                if &p.id == id {
                    p.is_favorite = on;
                }
            }
        }
    }

    /// `10-12`: the WebSocket's `UserDataChanged` reply — like `update_item_favorite`, but also
    /// updates `play_count`. Only `Track` carries that field (`Artist`/`Album` only ever get
    /// `is_favorite` touched, the same reach `update_item_favorite` already has).
    pub fn update_item_user_data(&mut self, id: &ItemId, is_favorite: bool, play_count: u32) {
        fn set_if_match(item: &mut MediaItem, id: &ItemId, is_favorite: bool, play_count: u32) {
            match item {
                MediaItem::Artist(a) if &a.id == id => a.is_favorite = is_favorite,
                MediaItem::Album(a) if &a.id == id => a.is_favorite = is_favorite,
                MediaItem::Playlist(p) if &p.id == id => p.is_favorite = is_favorite,
                MediaItem::Track(t) if &t.id == id => {
                    t.is_favorite = is_favorite;
                    t.play_count = play_count;
                }
                _ => {}
            }
        }

        for columns in self.nav.per_tab_stacks.values_mut() {
            for column in columns.iter_mut() {
                for item in column.items.iter_mut() {
                    set_if_match(item, id, is_favorite, play_count);
                }
            }
        }
        for entry in self.queue.entries.iter_mut() {
            if &entry.track.id == id {
                entry.track.is_favorite = is_favorite;
                entry.track.play_count = play_count;
            }
        }
        for entry in self.history.iter_mut() {
            if &entry.track.id == id {
                entry.track.is_favorite = is_favorite;
                entry.track.play_count = play_count;
            }
        }
        for results in [&mut self.search.results, &mut self.favourites.results] {
            for a in results.artists.iter_mut() {
                if &a.id == id {
                    a.is_favorite = is_favorite;
                }
            }
            for a in results.albums.iter_mut() {
                if &a.id == id {
                    a.is_favorite = is_favorite;
                }
            }
            for t in results.tracks.iter_mut() {
                if &t.id == id {
                    t.is_favorite = is_favorite;
                    t.play_count = play_count;
                }
            }
        }
    }

    pub fn active_column(&self) -> Option<&Column> {
        let NavFocus::Column(depth) = self.nav.focus else {
            return None;
        };
        self.nav
            .per_tab_stacks
            .get(&self.nav.active_tab)?
            .get(depth)
    }

    pub fn active_column_mut(&mut self) -> Option<&mut Column> {
        let NavFocus::Column(depth) = self.nav.focus else {
            return None;
        };
        self.nav
            .per_tab_stacks
            .get_mut(&self.nav.active_tab)?
            .get_mut(depth)
    }

    pub fn selected_item(&self) -> Option<&MediaItem> {
        let column = self.active_column()?;
        column.items.get(column.cursor)
    }

    pub fn current_entry(&self) -> Option<&QueueEntry> {
        self.queue.current()
    }

    /// `history` newest-first (`07-06`) — sorted explicitly by `played_at` at read time rather
    /// than trusted from the `VecDeque`'s own front-to-back order: `reducer::player::record_
    /// history_now`'s `push_front` already keeps it that way in practice, but nothing about
    /// `VecDeque<HistoryEntry>`'s type enforces it, and a fixture or future change that appended
    /// differently would silently invert the Now Playing history pane. At most 50 entries, so
    /// sorting on every read is not worth avoiding.
    pub fn history_sorted(&self) -> Vec<&crate::state::queue::HistoryEntry> {
        let mut entries: Vec<&crate::state::queue::HistoryEntry> = self.history.iter().collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.played_at));
        entries
    }

    /// Stamps `created_at` with the wall clock — the one deliberate exception to "no time in the
    /// reducer" (`docs/04-state-and-input.md` §4 rule 2): the given signature carries no
    /// timestamp to thread through, and a toast's display time is never branched on by reducer
    /// logic (expiry compares against `Tick`'s own timestamp, not a second `now()` call), so it
    /// does not threaten determinism the way a logic-affecting time read would.
    ///
    /// `10-13`: "an identical message arriving while the same one is displayed refreshes its
    /// timestamp instead of stacking... ten failed favourite toggles produce one toast, not ten" —
    /// deduplicated by `message` equality against whatever is already in `self.toasts`, the sole
    /// entry point every toast in this codebase goes through (including the WebSocket's own
    /// `SystemEvent::Toast`, `reducer::mod`'s own handler for it), so this is the one place that
    /// needs to enforce it.
    pub fn toast(&mut self, msg: impl Into<String>, level: ToastLevel) {
        let message = msg.into();
        if let Some(existing) = self.toasts.iter_mut().find(|t| t.message == message) {
            existing.level = level;
            existing.created_at = Timestamp::now();
        } else {
            let id = self.next_toast_id;
            self.next_toast_id += 1;
            self.toasts.push(Toast {
                id,
                message,
                level,
                created_at: Timestamp::now(),
            });
        }
        self.touch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_appstate_is_coherent() {
        let state = AppState::default();
        assert!(state.modal.is_none());
        assert!(state.queue.entries.is_empty());
        assert!(state.active_column().is_none());
        assert!(state.selected_item().is_none());
        assert_eq!(state.connectivity, Connectivity::Online);
        assert_eq!(state.player.status, player::PlayStatus::Stopped);
    }

    #[test]
    fn touch_sets_dirty() {
        let mut state = AppState::default();
        assert!(!state.dirty);
        state.touch();
        assert!(state.dirty);
    }

    #[test]
    fn state_types_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<AppState>();
    }

    /// `10-13`: `duplicate_message_refreshes_instead_of_stacking` — "ten failed favourite toggles
    /// produce one toast, not ten" (this task's own spec), proven directly against
    /// `AppState::toast` itself.
    #[test]
    fn duplicate_message_refreshes_instead_of_stacking() {
        let mut state = AppState::default();
        for _ in 0..10 {
            state.toast("could not update favourite: offline", ToastLevel::Error);
        }
        assert_eq!(state.toasts.len(), 1);
    }

    #[test]
    fn duplicate_message_keeps_the_same_identity() {
        let mut state = AppState::default();
        state.toast("working offline", ToastLevel::Warning);
        let first_id = state.toasts[0].id;

        state.toast("working offline", ToastLevel::Warning);

        assert_eq!(state.toasts.len(), 1, "still one toast, not two");
        assert_eq!(
            state.toasts[0].id, first_id,
            "refreshing keeps the same identity rather than minting a new one"
        );
    }

    #[test]
    fn distinct_messages_each_get_a_unique_id() {
        let mut state = AppState::default();
        state.toast("first", ToastLevel::Info);
        state.toast("second", ToastLevel::Info);
        assert_ne!(state.toasts[0].id, state.toasts[1].id);
    }
}
