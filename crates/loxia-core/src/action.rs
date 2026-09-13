//! Action — the only way state changes. An intent, not an effect: applying one never performs
//! I/O itself, it only mutates `AppState` and optionally returns `Effect`s for the runtime to
//! carry out (`docs/04-state-and-input.md` §§1-2).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::config::{Config, EqPreset, QualityProfile};
use crate::keymap::KeyChord;
use crate::model::{
    Album, AudioDevice, AudioFormat, ItemId, Lyrics, MediaItem, PlaylistEntryId, PlaylistId,
    QueueEntryId, Track,
};
use crate::state::Connectivity;
use crate::state::modal::{ModalKind, SaveSource};
use crate::state::nav::{ColumnKind, Tab};
use crate::state::player::{PlayStatus, SeekTarget};
use crate::state::queue::QueueSource;
use crate::state::toast::ToastLevel;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NavAction {
    MoveUp {
        n: usize,
    },
    MoveDown {
        n: usize,
    },
    NavLeft,
    NavRight,
    /// `n` is `viewport_rows / 2` — the reducer doesn't know the render geometry, so the input
    /// layer (`03-09`) supplies it from the last render, the same pattern `MoveUp`/`MoveDown` use.
    HalfPageUp {
        n: usize,
    },
    HalfPageDown {
        n: usize,
    },
    GoToTop,
    GoToBottom,
    PopColumn,
    SetTab(Tab),
    NextTab,
    PrevTab,
    GoToArtist,
    GoToAlbum,
    OpenFilter,
    SetFilter(String),
    /// One character typed into the inline filter — mirrors `ModalAction::FieldInput`.
    FilterInput(char),
    /// `Backspace` while the inline filter is being typed — mirrors `ModalAction::FieldBackspace`.
    FilterBackspace,
    /// `Enter` while the inline filter is being typed: keeps `filter` as-is but leaves
    /// `filter_editing`, so ordinary navigation resumes over the narrowed list (`04-11`).
    CommitFilter,
    Cancel,
    /// `10-04`: a mouse click on `HitTarget::ColumnItem { column, index }` — focuses that column
    /// (regardless of which one previously had focus) and sets its cursor directly to `index`,
    /// unlike `MoveUp`/`MoveDown`'s relative deltas. No existing action did both at once.
    FocusColumnAt {
        column: usize,
        index: usize,
    },
    /// `10-04`: `ScrollUp`/`ScrollDown` over a `ColumnItem` — moves *that* column's own cursor by
    /// `delta`, without touching `nav.focus` at all. Deliberately distinct from `MoveUp`/
    /// `MoveDown` (which always act on the *focused* column): "scroll targets the column under
    /// the pointer, not the focused one" (this task's own spec) means the two must stay
    /// independent — scrolling a column you haven't clicked into must not steal focus from
    /// wherever the keyboard was already working.
    ScrollColumn {
        column: usize,
        delta: i32,
    },
    /// A click on a row of the shared three-section list (`Search`/`Favourites`). Those rows had
    /// no click behaviour at all: neither tab has a Miller column, so `FocusColumnAt` cannot reach
    /// them, and the mouse simply did nothing on either — half of a live "I can't select anything
    /// from favourites" report (`docs/12-decisions.md`).
    FocusSectionAt {
        section: crate::state::search::SearchSection,
        index: usize,
    },
    /// `10-04`: a single (non-double) click on `HitTarget::QueueEntry` — moves the Now Playing
    /// view's own cursor to that entry without jumping playback there (`QueueAction::JumpTo` is
    /// the existing, separate action a *double*-click uses instead).
    FocusQueueEntry(QueueEntryId),
    /// `ScrollUp`/`ScrollDown` over the Now Playing tab's queue/history pane. That pane is not a
    /// Miller column, so `ScrollColumn` cannot reach it and the wheel did nothing there at all
    /// (`docs/12-decisions.md`). Like `ScrollColumn` this targets the pane under the pointer
    /// rather than whatever holds focus — in particular it must still scroll the list when focus
    /// is parked on the tab sidebar, where a plain `MoveDown` would change tabs instead.
    ScrollNowPlaying {
        delta: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectAction {
    ToggleVisualMode,
    ToggleItem,
    SelectAll,
    ClearSelection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueueAction {
    QueueSelection {
        full_context: bool,
    },
    /// `Enter` — **replace** the queue with the selection and play it now, as opposed to
    /// `QueueSelection`'s append. Implemented as "clear, then queue": the cleared queue is empty, so
    /// the queued tracks (or the fetch reply, for a container) start playing from the top the same
    /// way appending to an empty queue already does (`docs/12-decisions.md`).
    PlaySelection {
        full_context: bool,
    },
    InsertNext,
    RemoveEntry,
    MoveEntry {
        from: usize,
        to: usize,
    },
    Clear,
    /// `seed` drives `queue::shuffle::shuffle`'s `ChaCha8Rng` — supplied by whoever builds this
    /// action from `state.clock` (`06-03`; `docs/12-decisions.md`), never read from a clock inside
    /// the reducer itself, or every shuffle test becomes flaky.
    ToggleShuffle {
        seed: u64,
    },
    CycleRepeat,
    ApplySortProfile(String),
    /// Back to the order the tracks were queued in, clearing any applied sort profile — the
    /// counterpart of `ApplySortProfile`, and the same operation `unshuffle` performs.
    RestoreDefaultOrder,
    InstantMix,
    JumpTo(QueueEntryId),
    /// `a` on a History row (`07-06`) — appends the track back onto the queue as
    /// `QueueSource::Manual`. Boxed: `Track` is a ~450-byte struct (`docs/12-decisions.md`'s own
    /// `MediaItem`/`large_enum_variant` note), and unlike that case this is a brand new variant
    /// with no pre-existing call sites to disrupt, so there's no reason not to box it from the
    /// start.
    RequeueTrack(Box<Track>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlayerAction {
    PlayPause,
    Stop,
    Next,
    Prev,
    Seek(SeekTarget),
    SetVolume(u8),
    VolumeDelta(i8),
    ToggleMute,
    /// `10-12`: Emby's remote-control `Mute`/`Unmute` `GeneralCommand`s each set an absolute
    /// state, unlike every keyboard/mouse path (`M`, the player bar's mute button), which only
    /// ever has `ToggleMute` to reach for — a remote `Mute` while already muted must not
    /// accidentally unmute, which dispatching `ToggleMute` for it would risk.
    SetMute(bool),
    CycleQuality,
    CycleReplayGain,
    SetDevice(String),
    SetEqGain {
        band: usize,
        db: f32,
    },
    SetEqPreset(String),
    ToggleEqBypass,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModalAction {
    Open(ModalKind),
    Close,
    Submit,
    FieldNext,
    FieldPrev,
    FieldInput(char),
    FieldBackspace,
    /// `10-05`: a click on `HitTarget::ModalField(index)` — sets the modal's own field cursor
    /// directly to `index`, unlike `FieldNext`/`FieldPrev`'s relative wraparound steps. The same
    /// "absolute set from a click, relative step from a key" split `NavAction::FocusColumnAt`
    /// already established for Miller columns (`10-04`); a double-click reuses this to set the
    /// cursor before immediately following with `Submit`, exactly as a `ColumnItem` double-click
    /// relies on its own preceding single click having already moved the cursor first.
    FieldSet(usize),
    /// `10-03`: the help modal's own `j`/`k`/`Ctrl+U`/`Ctrl+D` scroll — positive scrolls down,
    /// negative up. Not a generic "every modal can scroll" action: `keymap::resolve`'s own
    /// `Modal(_)` branch only ever produces this for `ModalKind::Help` specifically (see that
    /// module's own doc comment, `docs/12-decisions.md`).
    Scroll(i32),
    /// `10-06`: `↑`/`↓` in the equalizer modal — adjusts the *currently selected* band
    /// (`Modal::Equalizer.band`, moved by `FieldNext`/`FieldPrev`) of `draft_gains` by `delta` dB,
    /// clamped to ±12dB, and re-emits a live-preview `Effect::Audio(SetEq)`. Deliberately distinct
    /// from `PlayerAction::SetEqGain`, which commits directly to the already-active
    /// `player.eq.gains` — editing a draft must never touch the committed curve until `Submit`, so
    /// `Esc` can still restore exactly `gains_at_open`.
    AdjustGain(f32),
    /// `10-06`: a click or drag on `HitTarget::EqBand(band)` — sets that specific band's
    /// `draft_gains` entry to the absolute `db` computed from the pointer's y position, and also
    /// selects it (mirroring `NavAction::FocusColumnAt`'s "a click both moves the cursor *and* the
    /// underlying value in one step" shape, `10-04`). Absolute counterpart to `AdjustGain`'s
    /// relative step, the same split `FieldSet`/`FieldNext` already have.
    SetGainAt {
        band: usize,
        db: f32,
    },
    /// `10-06`: `p` — cycles `Modal::Equalizer.draft_gains`/`preset_idx` through
    /// `player.known_presets` (factory then custom, already in that order), wrapping. Not folded
    /// into `PlayerAction::SetEqPreset`, which commits directly to `player.eq` — same reasoning as
    /// `AdjustGain` above.
    CyclePreset,
    /// `10-06`: `b` — flips `Modal::Equalizer.bypassed` (the modal's own draft flag, distinct from
    /// `PlayerAction::ToggleEqBypass`, which flips the already-committed `player.eq.bypassed`) and
    /// re-emits a live-preview effect reflecting it.
    ToggleBypass,
    /// `t` in the equalizer modal — turns the whole equalizer off/on, not merely bypassed.
    ///
    /// Bypass keeps the filter chain installed with a flat curve, for A/B-ing a setting. "Off" is
    /// the thing a user actually looks for, and until now the only control for it was a row buried
    /// in Settings — the modal always switched the equalizer *on* when submitted and offered no way
    /// back (`docs/12-decisions.md`).
    ToggleEqEnabled,
    /// `10-07`: `Space` in the sleep timer modal — selects the focused radio trigger or toggles
    /// the focused checkbox, whichever `Modal::SleepTimer.field_cursor` currently names. Not
    /// `Enter`, which (every modal, `10-05`) always submits — this modal's own footer literally
    /// names only `[ Enter ] Start`, never "toggle", so `Space` is the dedicated per-row activate
    /// key. Generic by name (like `FieldInput`/`FieldBackspace`), handled only for
    /// `Modal::SleepTimer` today; a later modal could give it its own meaning the same way
    /// `field_input` already varies by modal kind.
    ActivateField,
    /// `10-07`: `d` — disarms an already-armed `player.sleep_timer` and closes the modal, without
    /// stopping playback (unlike `fire_sleep_timer`, which the timer's own natural expiry
    /// triggers). A no-op if nothing is currently armed.
    DisarmSleepTimer,
    /// `10-08`: `P`/`Ctrl+P` — opens the save-playlist modal with an explicit source hint, rather
    /// than going through the generic `Open(ModalKind)` (which carries no way to say "queue" vs
    /// "selection") and having `build_save_playlist_modal` guess from whether a selection happens
    /// to be non-empty. That guess was the pre-existing behaviour (`03-07`) and conflated the two
    /// keys: `P` must always mean the queue, even with an active selection; `Ctrl+P` must mean the
    /// selection, or the single focused item when nothing is multi-selected — two genuinely
    /// different sources no shared auto-detection can express. Also runs the "nothing to save"/
    /// "needs a connection" open-time refusals this task's own spec requires, closing a
    /// previously-flagged gap (`docs/12-decisions.md`).
    OpenSavePlaylist(SaveSource),
    /// `10-08`: `↑`/`↓` while the save-playlist modal's target dropdown has focus (`field == 0`) —
    /// moves `target_cursor` through `Create New Playlist…` (`0`) then the loaded existing
    /// playlists, wrapping. Not folded into `FieldNext`/`FieldPrev`, which move *between* the
    /// modal's four top-level controls, not *within* the dropdown's own option list.
    CycleSaveTarget(i32),
    /// `10-09`: `Tab` in the sort profile modal — flips `Modal::SortProfile.target` between
    /// `Queue`/`Column`. Hardcoded (like `Space`/`d` are for the sleep timer) rather than routed
    /// through `keymap::resolve`, since this modal has no other field to move *between* — `Tab`'s
    /// usual meaning elsewhere.
    ToggleSortTarget,
    /// `10-09`: `e` — "closes this modal and opens Settings → Sorting... editing profiles is a
    /// configuration activity and does not belong in a quick-apply modal." Settings itself has no
    /// real section-navigation model yet (`11-04` builds it) — this switches to `Tab::Settings`,
    /// the closest faithful behaviour achievable before that task exists.
    OpenSettingsSorting,
    /// `11-02`: `Enter` on a `Modal::KeymapEditor` row while nothing is already captured — begins
    /// capture mode (`capturing = true`), distinct from `Submit`, which this same `Enter` means
    /// once a capture has finished (`captured` is `Some`) or a conflict is already being shown.
    StartCapture,
    /// `11-02`: every raw key event while `Modal::KeymapEditor.capturing` is true, captured
    /// verbatim rather than resolved through `keymap::resolve` at all — `input.rs`'s own dedicated
    /// capture-mode check produces this ahead of its normal per-context dispatch, the same way
    /// `Ctrl+C`'s always-quits check runs ahead of everything. The first chord captured arms
    /// `capture_deadline`; a second one arriving before it lapses extends the same `captured`
    /// binding into a 2-chord sequence and ends capture immediately.
    CaptureChord(KeyChord),
    /// `11-02`: `Esc` while `capturing` — abandons the in-progress capture entirely (clears
    /// `captured`/`conflict`/`capture_deadline`, returns to plain row browsing), never closes the
    /// modal itself. Distinct from `Nav::Cancel`'s usual "close this modal" meaning, the same way
    /// Settings' own `CancelTextEdit` needed a dedicated action rather than reusing that ladder
    /// (`11-01`, `docs/12-decisions.md`).
    CancelCapture,
    /// `11-02`: `d` on a `Modal::KeymapEditor` row — rebinds the focused action to whatever
    /// `KeyMap::defaults()` gives it, unconditionally (a restore-to-factory action, not gated
    /// behind the conflict-refusal flow a fresh capture goes through — `docs/12-decisions.md`).
    ResetRowToDefault,
    /// `11-02`: `x` on a `Modal::KeymapEditor` row — removes every binding the focused action
    /// currently holds; its row then shows `—` (`docs/04-state-and-input.md` §7).
    UnbindRow,
    /// `11-02`: `R` on a `Modal::KeymapEditor` row — opens the `Confirm` modal this task's own
    /// spec requires ("a footer action resets all bindings to defaults, behind a `Confirm`"); the
    /// actual reset only happens if that `Confirm` is accepted, via `ResetAllKeybindings` below as
    /// its `on_confirm` payload.
    ConfirmResetAllKeybindings,
    /// `11-02`: replaces the live keymap with `KeyMap::defaults()` and clears every
    /// `config.keybindings` override — only ever dispatched as a `Confirm` modal's `on_confirm`,
    /// never directly from a keypress (`ConfirmResetAllKeybindings` is what a keypress produces).
    ResetAllKeybindings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViewAction {
    ToggleZen,
    ToggleLyrics,
    /// Scroll the unsynced lyrics pane by `delta` source lines (negative = up). A no-op for timed
    /// lyrics, which scroll themselves.
    ScrollLyrics(i32),
    ToggleHelp,
    ToggleHistory,
    SetTheme(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemAction {
    ToggleFavorite,
    ToggleDownload,
    AddToPlaylist {
        playlist: PlaylistId,
    },
    /// `07-03`: `entries` plural — multi-select removes every selected row in one request
    /// (`multiselect_remove_sends_one_request`), not one `Effect` per row. Carries `playlist`
    /// too (the original single-`entry` shape had no way to know which playlist without it).
    RemoveFromPlaylist {
        playlist: PlaylistId,
        entries: Vec<PlaylistEntryId>,
    },
    /// `07-03`: `Ctrl+Up`/`Ctrl+Down` — reorders one row within its playlist by one position.
    MoveInPlaylist {
        playlist: PlaylistId,
        entry: PlaylistEntryId,
        new_index: usize,
    },
    /// `X` — always opens a `Confirm` modal naming the playlist and its track count first
    /// (`docs/12-decisions.md`); never deletes directly.
    DeletePlaylist(PlaylistId),
    /// The `Confirm` modal's own `on_confirm` target for `DeletePlaylist` — never bound to a key
    /// itself, so confirming can't loop back into opening another confirm.
    DeletePlaylistConfirmed(PlaylistId),
    SaveQueueAsPlaylist {
        name: String,
        overview: Option<String>,
    },
}

/// Where a failed fetch was headed, so `LoadFailed` can put the error in the right place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadTarget {
    Column {
        tab: Tab,
        depth: usize,
    },
    Discography {
        tab: Tab,
        depth: usize,
    },
    Search,
    Devices,
    /// A queue-directed fetch (`FetchAlbumTracksForQueue`/`FetchArtistTracksForQueue`, `06-02`)
    /// failed — no column/tab/depth to mark `Error` on, since nothing was ever populated for it.
    QueueFetch,
    /// `07-02`: `Effect::Net(FetchFavourites)` failed.
    Favourites,
    /// `07-02`: `Effect::Net(SetFavorite)` failed for this item — carries the id so
    /// `reducer::queue::favorite_toggle_failed` knows which pending optimistic update to roll
    /// back (`AppState::pending_favorite_toggles` is keyed by it).
    FavoriteToggle(ItemId),
    /// `07-03`: any of `PlaylistRemove`/`PlaylistMove`/`PlaylistDelete` failed for this playlist —
    /// `reducer::queue::playlist_mutation_failed` looks up which one from
    /// `AppState.pending_playlist_mutations` (keyed by the same id) to know what to roll back.
    PlaylistMutation(PlaylistId),
    /// `10-08`: `Effect::Net(PlaylistCreate)`/`PlaylistAdd` failed — no id to roll an optimistic
    /// update back against (unlike `PlaylistMutation`: this is a brand-new save, nothing was ever
    /// optimistically applied to any already-visible state), so this only replaces the pending
    /// `saving <n> tracks…` toast with `could not save: <reason>`.
    PlaylistSave,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataAction {
    ItemsLoaded {
        tab: Tab,
        depth: usize,
        kind: ColumnKind,
        items: Vec<MediaItem>,
        total: usize,
        page: usize,
    },
    /// A background cache fetch finished cleanly (`08-03`). Carries the manifest's new total so
    /// the About view's cache size stops being a startup-only reading, and lets the player bar stop
    /// claiming a track is merely streaming once its local copy exists (`docs/12-decisions.md`).
    CacheFetched {
        track: crate::model::ItemId,
        profile: crate::config::QualityProfile,
        total_bytes: u64,
    },
    /// A background cache fetch started for this track — the player bar shows `caching` rather than
    /// `stream` while it runs.
    CacheFetchStarted {
        track: crate::model::ItemId,
        profile: crate::config::QualityProfile,
    },
    TracksLoaded {
        tracks: Vec<Track>,
        source: QueueSource,
        full_context: bool,
    },
    DiscographyLoaded {
        tab: Tab,
        depth: usize,
        primary: Vec<Album>,
        appears_on: Vec<Album>,
    },
    /// `07-01`: `query` is echoed back from the `Search` effect that triggered this, so a stale
    /// reply for an old query (the user kept typing, a second search fired before the first
    /// replied) can be told apart from the current one — the reducer discards it if
    /// `query != state.search.query.trim()` at the moment it arrives, the same
    /// discard-if-stale pattern `04-10`'s `(tab, depth)` echo on `ItemsLoaded` already uses.
    SearchResultsLoaded {
        query: String,
        results: crate::state::search::SearchResults,
    },
    /// `07-02`: the reply to `Effect::Net(FetchFavourites)` — no query to echo back (unlike
    /// `SearchResultsLoaded`, there's only ever one favourites list, never a stale-vs-current
    /// ambiguity).
    FavouritesLoaded {
        results: crate::state::search::SearchResults,
    },
    LyricsLoaded {
        track: ItemId,
        lyrics: Lyrics,
    },
    ImageLoaded {
        id: ItemId,
        tag: String,
    },
    DevicesLoaded {
        devices: Vec<AudioDevice>,
    },
    /// `09-03`: fired once at startup by the `loxia` binary, which merges `loxia_audio::eq`'s
    /// embedded factory presets with `config.equalizer.custom_presets` — the same
    /// precompute-in-the-platform-crate-carry-as-data shape `DevicesLoaded` already uses
    /// (`docs/12-decisions.md`), since `loxia-core` cannot depend on `loxia-audio` to parse
    /// `assets/eq_presets.toml` itself.
    PresetsLoaded {
        presets: Vec<EqPreset>,
    },
    /// `08-03`: the reply to `Effect::Cache(EnsureCached { track, profile })` — `track`/`profile`
    /// are echoed back so the reducer can tell a stale reply (the user has since moved to a
    /// different track, or cycled quality) from the one that's still relevant, the same
    /// discard-if-stale shape every other correlated reply in this enum already uses.
    /// `path: Some(_)` means the rolling cache already has a complete, playable local copy, and a
    /// corrective `Load` points at it directly; `None` means it doesn't (yet), and the corrective
    /// `Load` uses `stream_url` instead — the real Emby network stream (`stream_url`'s own
    /// `RedactedUrl`, api_key and all). This used to be treated as a no-op ("playback continues
    /// from whatever URL `Load` already started with"), but what `Load` "already started with" is
    /// always just `placeholder_url`'s inert `emby-track:{id}` scheme — mpv cannot open that, so a
    /// track with no local cache yet never actually played at all. A real, confirmed defect found
    /// live: see `docs/12-decisions.md`.
    CacheResolved {
        track: ItemId,
        profile: QualityProfile,
        path: Option<std::path::PathBuf>,
        stream_url: crate::effect::RedactedUrl,
    },
    /// `offline` (`08-06`): whether the underlying `EmbyError` was specifically `Offline` — a
    /// transport-level failure, never `Unauthorized`/`NotFound`/`Transient` (`docs/12-decisions.md`,
    /// `docs/06-cache-and-offline.md` §6: "a 404 on one album is not a network outage"). This is
    /// the only signal `reducer::connectivity`'s consecutive-failure counter reads; `message`
    /// stays a plain string (`e.to_string()`, computed by the network worker, the only place that
    /// still has the real `EmbyError`) since `loxia-core` cannot depend on `loxia-emby` to carry
    /// the typed error itself.
    LoadFailed {
        target: LoadTarget,
        message: String,
        offline: bool,
    },
    /// `10-08`: `Effect::Net(PlaylistCreate)`/`PlaylistAdd` succeeded — `name` is simply echoed
    /// back from whichever effect the worker was given (both now carry their own display name),
    /// used to replace the pending `saving <n> tracks…` toast with `saved to <name>`.
    PlaylistSaved {
        name: String,
    },
    /// `08-04`: progress from `loxia_cache::downloads::Downloads::pin` for one track of a
    /// permanent-download batch — `docs/06-cache-and-offline.md` §5's own wording
    /// (`Event::DownloadProgress { id, done_bytes, total_bytes }`) reads as shorthand for this
    /// variant, not a new top-level `Event` bucket: every other cross-crate reply in this enum
    /// (`CacheResolved`, `ImageLoaded`, ...) already arrives as `Event::Data(DataAction::_)`, and a
    /// download's progress is exactly that kind of reply, just repeated many times per item.
    DownloadProgress {
        id: ItemId,
        done_bytes: u64,
        total_bytes: u64,
    },
    /// A pin finished, or was undone. Carries the download index's own new total so the About
    /// view's "Downloads" figure keeps up without a second round trip.
    DownloadsChanged {
        pinned: Vec<ItemId>,
        total_bytes: u64,
        count: usize,
    },
    /// `10-12`: the WebSocket's `LibraryChanged` message — carries no payload since every column
    /// is marked `Idle` regardless of exactly which items changed, the same "not a refetch storm"
    /// simplification `reducer::connectivity::advance_reconnecting` already makes for the
    /// analogous "something changed server-side" case (`docs/12-decisions.md`): this app's
    /// column-kind model has no cheap way to map Emby's own changed-item-id lists onto
    /// "affected" columns specifically.
    LibraryChanged,
    /// `10-12`: one per entry in the WebSocket's `UserDataChanged.UserDataList` — updates
    /// favourite/play-count in place wherever the item currently appears
    /// (`AppState::update_item_user_data`).
    UserDataChanged {
        id: ItemId,
        is_favorite: bool,
        play_count: u32,
    },
    /// `11-03`: the reply to `Effect::Net(TestServerConnection)` on success — carries
    /// `user_id`/`access_token` too (not just the name/version a plain "Test connection" needs to
    /// show), since the exact same reply also completes a "Save" (`reducer::settings::
    /// server_editor_test_result`), which needs them to actually populate the profile being
    /// persisted.
    ServerTestSucceeded {
        user_id: String,
        access_token: String,
        server_name: String,
        version: String,
    },
    ServerTestFailed {
        message: String,
    },
}

/// Mirrors what the audio engine reports, 1:1 with `Event::Audio` (`docs/04-state-and-input.md`
/// §1) — the runtime converts one directly into the other with no translation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AudioEvent {
    StatusChanged(PlayStatus),
    PositionChanged {
        position: Duration,
        duration: Duration,
    },
    TrackEnded {
        natural: bool,
    },
    FormatDetected(AudioFormat),
    /// Added in `05-06`, mirroring `loxia_audio::backend::AudioEvent::VolumeChanged` (itself
    /// added in `05-04` — see `docs/12-decisions.md` for why neither `docs/05-audio-engine.md`
    /// §2's original table nor this enum's own first draft had anywhere to put a volume/mute
    /// confirmation from the engine).
    VolumeChanged {
        volume: u8,
        muted: bool,
    },
    EngineError(String),
    /// `10-05`: a specific case of the engine's own generic error, carrying the device `id` that
    /// failed to swap in — `AudioError::DeviceUnavailable`'s own `Display` is a fixed, generic
    /// sentence (`docs/12-decisions.md`'s "never interpolate a field's actual content" rule for
    /// engine errors), so folding this into `EngineError(String)` like every other engine error
    /// would lose the one piece of information the device picker's own "could not switch to
    /// `<name>`" toast needs. `workers::audio`'s translation layer is what special-cases this one
    /// `AudioError` variant instead of `.to_string()`-ing it away like the rest.
    DeviceUnavailable {
        id: String,
    },
}

/// Mirrors the runtime/workers, 1:1 with `Event::System`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SystemEvent {
    Tick(Timestamp),
    Resize {
        w: u16,
        h: u16,
    },
    Refresh,
    Quit,
    /// `10-13`: bare `message`/`level`, not a whole pre-built `Toast` — `id`/`created_at` are
    /// `AppState::toast`'s own job (the sole assigner, so its deduplication-by-message logic
    /// always runs), matching every other `SystemEvent` variant's own "plain data, no derived
    /// bookkeeping baked in" shape. Previously carried a full `Toast` (with the sender expected to
    /// stamp its own timestamp) — the WebSocket's `DisplayMessage` handler
    /// (`crates/loxia-player/src/workers/network.rs`) was the only real caller, and no longer needs to
    /// touch `Timestamp`/`Toast` construction at all now that this is a plain, two-field request.
    Toast {
        message: String,
        level: ToastLevel,
    },
    ConnectivityChanged(Connectivity),
    SessionRestored(Box<crate::state::SessionSnapshot>),
    ConfigChanged(Box<Config>),
    /// A `g`-style two-chord prefix was just typed (`keymap::resolve` returned
    /// `Resolution::Pending`) — not in `docs/04-state-and-input.md` §1's original `System` list,
    /// added while implementing `03-09`'s `to_action`, whose own resolution rule 3
    /// ("`Resolution::Pending` → `Action::System(SetPendingChord(chord))`") names it but no such
    /// variant existed. There is no corresponding "clear" variant: an unresolved second chord
    /// (rule 4) resolves to no action at all, relying on `Tick`'s already-built 1-second expiry
    /// (`03-08`) to clear it, exactly as `AppState::pending_chord`'s own doc comment describes.
    SetPendingChord(KeyChord),
    /// `10-10`: crossterm's `FocusGained`/`FocusLost`, forwarded so the reducer can suppress a
    /// desktop notification when the terminal already has focus (the player bar already shows the
    /// same information there). `input.rs` previously discarded both events outright — this is the
    /// only reason they now reach the reducer at all.
    TerminalFocusChanged(bool),
}

/// `11-01`: the Settings tab's own row navigation and editing — distinct from `ModalAction`
/// (settings is a tab, not a modal) and from `NavAction` (rows are a flat, per-section list with
/// no Miller-column/selection concepts at all).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SettingsAction {
    SetSection(crate::state::settings::SettingsSection),
    NextSection,
    PrevSection,
    /// `Esc` from the section's own rows — moves focus to the left-hand section (group) list, where
    /// `↑`/`↓` pick a group and `Enter`/`→` step back into its rows (`docs/12-decisions.md`).
    FocusSectionList,
    /// `Enter`/`→` from the section list — steps focus back into the focused section's rows.
    FocusRows,
    /// `Esc`/`←` from the section list — steps focus out of Settings entirely, onto the main tab
    /// sidebar (`NavState.sidebar_focused`), so `↑`/`↓` switch tabs.
    LeaveToTabSidebar,
    MoveRow(i32),
    /// `11-01`: a mouse click on a row — sets the cursor directly to `index`, mirroring
    /// `ModalAction::FieldSet`'s identical "absolute set from a click" shape.
    SetRow(usize),
    /// `←`/`→` or `Space` — relative: `+1`/`-1` flips a `Toggle`, cycles a `Select` forward/back
    /// through its own `options`, or steps a `Slider`/`Number` by its own `step`/`1`.
    AdjustValue(i32),
    StartTextEdit,
    TextInput(char),
    TextBackspace,
    /// `Delete` — removes the character *at* the cursor, as opposed to `TextBackspace`'s *before*
    /// it. Found missing in the field alongside the cursor-movement variants below: without them,
    /// every Settings text buffer could only ever be edited by erasing from the end.
    TextDeleteForward,
    TextCursorLeft,
    TextCursorRight,
    /// `Home` — jumps to the start of the buffer.
    TextCursorHome,
    /// `End` — jumps to the end of the buffer.
    TextCursorEnd,
    CommitTextEdit,
    CancelTextEdit,
    /// `Ctrl+E` — the focused row's own secret `Text` field only (`access_token`), or (`11-03`)
    /// the server-profile editor's own password/header-value fields while it is open — the same
    /// action, branched by `reducer::settings::apply` on whether `state.settings.server_editor` is
    /// currently open, rather than a second, near-duplicate action name.
    ToggleRevealSecret,
    /// `Enter` on an `Action`-type row.
    ActivateRow,
    /// `11-03`: opens the add-profile form (a blank `ServerDraft`, `id: None`) from the server list
    /// sub-view. `device_id` is stamped immediately, not left for a later `validate()` pass to
    /// fill in lazily — "generated once per profile," and there is no more natural "once" than the
    /// moment the profile is conceived (`docs/12-decisions.md`).
    ServerEditorAddNew,
    /// Opens the edit form for whichever profile the list sub-view's own cursor currently names.
    ServerEditorEdit,
    /// `[←→]`/`Enter` on the `Protocol` field — cycles `http`/`https`. A no-op (returned as-is,
    /// not refused with an error) if a different field is focused, matching every other
    /// field-specific action's own "wrong field, nothing happens" shape in this editor.
    ServerEditorCycleProtocol(i32),
    /// `x`/`Backspace` on the focused profile in the list sub-view — refused (with a reason set on
    /// `state.settings.error`) if it names the *active* server; otherwise removed from
    /// `config.servers` immediately, deleting its cached data too if `delete_data_on_remove` is
    /// set (defaulting to `false`, per this task's own "offers... defaulting to no").
    ServerEditorRemove,
    /// Flips the list sub-view's own "also delete cached files" flag, read at the moment
    /// `ServerEditorRemove` actually runs.
    ServerEditorToggleDeleteData,
    /// Opens the `Confirm` this task's own spec requires before switching the active server
    /// ("switching is disruptive and must be explicit"). A no-op if the focused profile is already
    /// active — there is nothing to switch to.
    ServerEditorSwitch,
    /// The `Confirm`'s own `on_confirm` payload — stops playback, clears the queue and history,
    /// persists the outgoing session under its own server id, sets `config.active_server`, and
    /// emits `Effect::Sys(ReconnectServer)` for the runtime to actually reconnect and reseed the
    /// columns with (`docs/12-decisions.md`: this reducer cannot do the reconnect itself, since it
    /// has no access to `&mut Workers`).
    ServerEditorSwitchConfirmed(crate::model::ServerId),
    /// Validates the new-header name/value pair the draft's own two input fields hold — refuses a
    /// reserved name (`authorization`/`x-emby-authorization`/`host`/`content-length`, case
    /// insensitive) inline rather than adding it, per this task's own spec.
    /// `+` on the endpoint row — another address for the same server.
    ServerEditorAddEndpoint,
    /// `x` on the endpoint row — drops the selected fallback (never the primary).
    ServerEditorRemoveEndpoint,
    ServerEditorAddHeader,
    /// Removes whichever existing header row the draft's own field cursor currently focuses; a
    /// no-op if it isn't currently focused on one.
    ServerEditorRemoveFocusedHeader,
    /// Dispatches `Effect::Net(TestServerConnection)` against the draft's own (unsaved)
    /// `url`/`headers`/`username`/`password` — "must be possible before saving" (this task's own
    /// spec), so this reads only from the draft, never from `config.servers`.
    ServerEditorTestConnection,
    /// Like `ServerEditorTestConnection`, but the reply (`reducer::settings::
    /// server_editor_test_result`) additionally commits the draft into `config.servers` on
    /// success — "user_id/access_token are derived, not typed," so saving a profile always
    /// (re-)authenticates rather than ever accepting a pasted token.
    ServerEditorSave,
    /// `Esc` while the editor is open — from the add/edit form, discards the draft and returns to
    /// the profile list; from the list itself, closes the editor and returns to the Servers
    /// section's own normal row list (`docs/12-decisions.md`: distinct from `Nav::Cancel`'s usual
    /// "close the modal" meaning, the same reason `11-01`'s `CancelTextEdit` and `11-02`'s
    /// `CancelCapture` each needed their own dedicated action instead of reusing that ladder).
    ServerEditorClose,
    /// `11-04`: `n` — begins creating a brand-new sort profile (an empty name buffer,
    /// `creating: true`); `CommitTextEdit` validates and appends it once a name is entered.
    SortEditorNew,
    /// `r` — begins renaming the focused profile (the buffer starts pre-filled with its current
    /// name, `creating: false`). Committing updates `config.sorting.default_queue_profile` too if
    /// it named the profile being renamed (this task's own spec).
    SortEditorRename,
    /// `x` — opens a `Confirm` modal naming the focused profile (a live user deleted their profiles
    /// by accident and asked for a guard, `docs/12-decisions.md`). Confirming dispatches
    /// `SortEditorDeleteProfileConfirmed`.
    SortEditorDeleteProfile,
    /// The `Confirm` modal's own payload — the actual deletion. Clears `default_queue_profile` if it
    /// named the profile just deleted; deleting the very last profile is a legitimate, fully
    /// supported end state.
    SortEditorDeleteProfileConfirmed,
    /// `R` — re-adds the two built-in profiles (`chronological_discog`/`release_chronology`) from
    /// `SortingConfig::default()`, skipping any whose name already exists so a user's own edits are
    /// never clobbered. Recovery for the "deleted them all by accident" case (`docs/12-decisions.md`).
    SortEditorRestoreDefaults,
    /// `a` — appends a default rule (`SortField::Name`, `Direction::Asc`) to the focused profile;
    /// refused with an inline reason once it already has 4 (`config::MAX_SORT_RULES`), rather than
    /// silently doing nothing.
    SortEditorAddRule,
    /// `d` — removes the focused rule from the focused profile.
    SortEditorDeleteRule,
    /// `Ctrl+Up`/`Ctrl+Down` — moves the focused rule by `delta` within the focused profile's own
    /// rule list, clamped at both ends.
    SortEditorReorderRule(i32),
    /// `←`/`→` on a rule row — cycles its `SortField` forward/back through the fixed declared
    /// order, wrapping.
    SortEditorCycleField(i32),
    /// `Space`/`Enter` on a rule row — flips `Asc`/`Desc`.
    SortEditorToggleDirection,
    /// `Enter` on a profile's own header row (not one of its rules) — applies it to the queue,
    /// via the same `QueueAction::ApplySortProfile` the quick-apply `Modal::SortProfile` (`10-09`)
    /// already uses. "Editing a profile does not reorder the user's queue underneath them" (this
    /// task's own spec) is what makes this its own explicit, separate action rather than
    /// happening automatically on every rule edit.
    SortEditorApply,
    /// `Esc` — closes the sort-profile editor, the same "back, not `Nav::Cancel`" shape
    /// `ServerEditorClose` already established.
    SortEditorClose,
    /// `11-05`: `s` — captures `player.eq.gains` *right now* into `EqPresetEditorState::
    /// captured_gains` and begins naming a new custom preset from it.
    EqEditorSaveCurrent,
    /// `r` — begins renaming the focused *custom* preset; refused (with a reason set on the
    /// editor's own `error`) if the focused row is a factory preset.
    EqEditorRename,
    /// `x` — deletes the focused *custom* preset, same factory refusal as `EqEditorRename`.
    /// Falls back to `flat` (and applies it) if the deleted preset was the active one — "the
    /// audio does not keep applying a curve the user just deleted" (this task's own spec).
    EqEditorDelete,
    /// `e` — opens the equalizer modal (`10-06`) prefilled with the focused preset's own gains,
    /// not necessarily whatever is currently playing.
    EqEditorOpenEqualizer,
    /// `Esc` — same "back, not `Nav::Cancel`" shape every other Settings sub-editor's own `Esc`
    /// already has.
    EqEditorClose,
    /// `11-06`: the `Confirm`'s own `on_confirm` payload for "clear saved session" — empties the
    /// queue (`reducer::queue::clear`) and deletes `session.json` from disk.
    ClearSavedSessionConfirmed,
    /// `11-07`: `[l]` in the About section — opens or closes the third-party licences pane in
    /// place of that section's own (empty) row list.
    AboutToggleLicences,
    /// `↑`/`↓`/`j`/`k` while the licences pane is open.
    AboutLicencesScroll(i32),
    /// `11-07`: `[d]` in the About section — copies the About block plus the keybinding conflict
    /// count and the active config's non-secret values to the clipboard, for bug reports.
    AboutCopyDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Nav(NavAction),
    Select(SelectAction),
    Queue(QueueAction),
    Player(PlayerAction),
    Modal(ModalAction),
    View(ViewAction),
    Item(ItemAction),
    Data(DataAction),
    Audio(AudioEvent),
    System(SystemEvent),
    Settings(SettingsAction),
}
