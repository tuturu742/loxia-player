# 02 — Data Model

All types live in `loxia-core` unless stated otherwise. Described as field lists, not code.
Derive `Debug, Clone, PartialEq` everywhere; add `Serialize, Deserialize` where marked **[persist]**.

## 1. Identifiers (`model/ids.rs`)

Newtype wrappers over `String` — never pass a bare `String` id across a function boundary.

| Type | Notes |
| :-- | :-- |
| `ItemId` | Emby item GUID. Used for artists, albums, tracks, genres, folders alike. |
| `ServerId` | Config-assigned local id (`"remote_proxy"`). |
| `UserId` | Emby user GUID. |
| `PlaylistId` | Semantically an `ItemId`; a distinct type for safety. |
| `PlaylistEntryId` | Emby's per-playlist-row id. **Required for removal** — an `ItemId` is not enough. |
| `MediaSourceId` | Identifies a media source within an item; needed for lyric-stream fetching. |
| `QueueEntryId` | Monotonic `u64`, session-local. The same track can appear twice in a queue. |

## 2. Media items (`model/item.rs`)

### `MediaItem` (enum)
The unit a navigation column holds. Variants: `Artist`, `Album`, `Track`, `Genre`, `Folder`,
`Playlist`, `SectionHeader(SectionHeader)`.

`SectionHeader { label: String, count: usize, kind: SectionKind }` where `SectionKind` is
`Albums | AppearsOn | Custom`. **Headers are never selectable** — cursor movement skips over them.

### `Artist`
`id, name, sort_name, album_count, track_count, genres: Vec<String>, is_favorite: bool,
image_tag: Option<String>, overview: Option<String>`

### `Album`
`id, name, sort_name, album_artist_names: Vec<String>, album_artist_ids: Vec<ItemId>,
year: Option<u16>, track_count: u32, total_duration: Duration, genres: Vec<String>,
is_favorite: bool, image_tag: Option<String>, relation: AlbumRelation`

`AlbumRelation` = `Primary` | `AppearsOn { context_artist: ItemId }`.
This drives both the `── ALBUMS ──` / `── APPEARS ON ──` split and the queueing rules.
It is assigned by `loxia-emby::endpoints::discography` (see `03-emby-api.md` §4), never guessed
in the UI.

### `Track`
`id, name, album_id: Option<ItemId>, album_name, album_artist_names: Vec<String>,
artist_ids: Vec<ItemId>, artist_names: Vec<String>, track_number: Option<u32>,
disc_number: Option<u32>, year: Option<u16>, duration: Duration, genres: Vec<String>,
is_favorite: bool, play_count: u32, format: AudioFormat, replay_gain: Option<ReplayGainInfo>,
image_tag: Option<String>, media_source_id: Option<MediaSourceId>,
lyric_stream: Option<LyricStreamRef>, date_created: Option<Timestamp>`

`artist_ids` is what the Appears-On queue filter tests against. It must be populated from Emby's
`ArtistItems` field, not parsed out of a display string.

### `LyricStreamRef`
`{ media_source_id: MediaSourceId, stream_index: u32, format: LyricFormat }`
`LyricFormat` = `Lrc | Srt | Txt`. Discovered from `MediaSources[].MediaStreams` — see
`03-emby-api.md` §7. `None` means the track has no lyrics and the pane stays hidden.

### `AudioFormat` (`model/audio_meta.rs`)
`codec: Codec, sample_rate_hz: u32, bit_depth: Option<u8>, channels: u8, bitrate_bps: Option<u32>`
`Codec` = `Flac | Alac | Mp3 | Aac | Opus | Vorbis | Wav | Other(String)`.
Rendered verbatim in the player bar's `🎚 FLAC 16-bit / 44.1 kHz │ Bitrate: 1012 kbps` line.

### `ReplayGainInfo`
`track_gain_db, album_gain_db, track_peak, album_peak` — all `Option<f32>`.

### `Genre`
`id, name`

Deliberately minimal — every consuming task (`02-05`'s `genres`/`genre_artists`, `07-04`'s Genres
tab) only ever needs the id for the fetch and the name for the row label and the
`ColumnKind::GenreArtists { of_genre }` filter. Emby's genre-filtering endpoint takes the genre
**name**, not its id (`03-emby-api.md` §3) — `id` is kept anyway because a `MediaItem::Genre` still
needs one for `MediaItem::id()` and for stable `BTreeSet<ItemId>` selection.

### `Folder`
`id, name`

Equally minimal: `07-05`'s Folders tab fetches non-recursively by parent id and renders the child's
own name; nothing downstream needs a child count, a path, or a parent back-reference — the Miller
column stack already tracks ancestry as `ColumnKind::Folders { of_parent }` per level.

### `Playlist`
`id, name, overview: Option<String>, track_count: u32, total_duration: Duration, can_edit: bool`

### `Lyrics` (`model/lyrics.rs`)
`Unsynced(Vec<String>)` | `Synced(Vec<LyricLine>)`, with `LyricLine { at: Duration, text: String }`.
The active line index is **derived at render time** from the player position — never stored.
The crate owns the LRC parser: it must handle `[mm:ss.xx]`, `[mm:ss.xxx]`, multiple timestamps on
one line, `[ar:]`/`[ti:]`/`[offset:]` metadata tags, and blank lines. An unparseable file
degrades to `Unsynced`, never to an error.

## 3. Navigation state (`state/nav.rs`)

### `NavState`
- `active_tab: Tab` — the 9 sidebar tabs **[persist]**
- `per_tab_stacks: HashMap<Tab, Vec<Column>>` — each tab remembers its own drill state
- `window_start: usize` — index of the leftmost visible column (max 3 visible)
- `focus: NavFocus` — `Sidebar | Column(usize) | Inspector`

### `Column`
- `kind: ColumnKind` — `Artists | Albums { of_artist } | Tracks { of_album } | ArtistTracks { of_artist } | Genres | GenreArtists { of_genre } | Folders { of_parent } | Playlists | PlaylistTracks { of_playlist } | SearchResults`
- `items: Vec<MediaItem>`
- `cursor: usize`, `scroll_offset: usize`
- `selection: SelectionState`
- `filter: Option<String>` — the inline `/` search
- `load: LoadState` — `Idle | Loading | Loaded { total: usize } | Error(String)`
- `page_loaded: usize` — for incremental paging
- `title: String`

### `SelectionState`
- `visual_mode: bool` (toggled by `v`)
- `selected: BTreeSet<ItemId>` — **keyed by id, not index**, so a filter change cannot corrupt it
- `anchor: Option<usize>` for range extension

Selected count and total duration are **derived** in the inspector widget, never stored.

## 4. Queue (`state/queue.rs`)

### `QueueState` **[persist]**
- `entries: Vec<QueueEntry>` — the canonical, unshuffled order. **Never reordered by shuffle.**
- `play_order: Vec<usize>` — indices into `entries`; the identity permutation when unshuffled
- `position: usize` — index into `play_order`
- `shuffled: bool`
- `repeat: RepeatMode` — `Off | All | One`
- `sort_profile: Option<String>`
- `next_entry_id: u64` — the `QueueEntryId` counter

### `QueueEntry`
`{ entry_id: QueueEntryId, track: Track, source: QueueSource, availability: Availability }`
`QueueSource` = `Album{id} | Artist{id} | Playlist{id} | Search | InstantMix{seed} | Manual`
`Availability` = `Remote | Cached | Downloaded | Unavailable`

### `HistoryEntry` **[persist: history.json]**
`{ track: Track, played_at: Timestamp, completed: bool }` — a `VecDeque` capped at 50, newest first.
Stored on `AppState`, not in `QueueState`, so clearing the queue does not clear history.

**Non-destructive shuffle contract (`queue/shuffle.rs`):**
- `shuffle()` permutes **only** `play_order[position+1..]`. The currently playing entry stays at
  `play_order[position]`. `entries` is untouched.
- `unshuffle()` restores `play_order` to the identity permutation and recomputes `position` so the
  same track remains current.
- The round-trip is exact and is property-tested. The RNG is `rand_chacha` seeded from an explicit
  value carried on the action, so tests are deterministic.

## 5. Player mirror (`state/player.rs`)

Reflects the audio engine. **Written only in response to `Event::Audio(..)`** — never speculatively,
because the reducer must not claim playback that the engine has not confirmed.

- `status: PlayStatus` — `Stopped | Loading | Playing | Paused | Buffering`
- `current: Option<QueueEntryId>`
- `position: Duration`, `duration: Duration`
- `volume: u8` (0–100), `muted: bool`
- `format: Option<AudioFormat>` — the **actual decoded** format reported by mpv, not the metadata
- `output: OutputInfo { driver: String, device_id: String, device_name: String }`
- `eq: EqState { enabled, preset_name, gains: [f32; 10], selected_band: usize, bypassed: bool }`
- `replay_gain: ReplayGainMode` — `Album | Track | Off`
- `applied_gain_db: Option<f32>` — what was actually applied, shown in the inspector
- `quality_profile: QualityProfile` — `Direct | High320 | Med192 | Low96`
- `sleep_timer: Option<SleepTimer>`

`SleepTimer { trigger: SleepTrigger, fade_out: bool, quit_after: bool, armed_at: Timestamp }`
`SleepTrigger` = `Duration(Duration) | EndOfTrack | EndOfQueue`

## 6. Modals (`state/modal.rs`)

`AppState.modal` is `Option<Modal>`. **Only one modal at a time** — opening a second replaces it.

| Variant | Local state |
| :-- | :-- |
| `Help` | `context: InputContext`, `scroll: u16` |
| `Equalizer` | `band: usize`, `draft_gains: [f32;10]`, `gains_at_open: [f32;10]`, `preset_idx`, `bypassed` |
| `DevicePicker` | `devices: Vec<AudioDevice>`, `cursor`, `load: LoadState` |
| `SleepTimer` | `trigger`, `fade_out`, `quit_after`, `field_cursor` |
| `SavePlaylist` | `target: PlaylistTarget`, `name: String`, `overview: String`, `autosort: bool`, `field: usize`, `source: SaveSource` |
| `SortProfile` | `profiles: Vec<SortProfile>`, `cursor`, `editing: Option<RuleEditor>` |
| `KeymapEditor` | `action_cursor`, `capturing: bool`, `conflict: Option<ActionId>`, `captured: Option<KeyBinding>`, `capture_deadline: Option<Timestamp>` |
| `Confirm` | `prompt: String`, `on_confirm: Box<Action>` |

## 7. Root (`state/mod.rs`)

```
AppState {
    config: Config
    theme: Theme
    keymap: KeyMap
    nav: NavState
    queue: QueueState
    history: VecDeque<HistoryEntry>
    player: PlayerState
    modal: Option<Modal>
    search: SearchState
    toasts: Vec<Toast>
    connectivity: Connectivity          // Online | Offline | Reconnecting
    zen_mode: bool
    lyrics_visible: bool
    lyrics: Option<(ItemId, Lyrics)>
    now_playing_subview: NowPlayingSub  // Queue | History
    pending_chord: Option<(KeyChord, Timestamp)>
    downloads_active: usize
    cache_stats: CacheStats
    server: ServerSession { user_id, server_name, connected_at }
    config_warnings: Vec<ConfigWarning> // surfaced at startup and in Settings
    should_quit: bool
    dirty: bool
}
```

## 8. Config (`config/schema.rs`) **[persist: config.toml]**

Mirrors `design_overview` §7 with the changes recorded in `12-decisions.md` §5.

```
Config {
    schema_version: u32,
    active_server: String,
    servers: Vec<ServerConfig>,
    audio: AudioConfig,
    cache: CacheConfig,
    transcode: TranscodeConfig,
    ui: UiConfig,
    logging: LoggingConfig,
    sorting: SortingConfig,
    equalizer: EqConfig,
    keybindings: BTreeMap<ActionId, String>,
}
```

Requirements:
- Every field carries `#[serde(default)]` with a documented default. **An empty or missing config
  file must produce a working app.**
- `validate()` returns `Vec<ConfigWarning>` and never fails hard. An unknown theme name falls back
  to `default_terminal` with a warning; it does not abort startup.
- `access_token` is a secret: the file is written `0600` on Unix, and a warning is emitted if it is
  group- or world-readable. The token is redacted from every `Debug`/`Display` impl and log line.

Removed relative to `design_overview` §7: `audio.crossfade_sec` and `ui.show_spectrum_analyzer`
(features cut — `12-decisions.md` §3).
Added: `schema_version`, `servers[].device_id`, `cache.prefetch_on_play`, `cache.image_cache_mb`,
`ui.restore_autoplay`, `ui.enable_websocket`, `ui.ascii_only`, `ui.show_lyrics`,
`audio.replaygain_preamp_db`, and the `[logging]` section.

### `SortProfile`
`{ name: String, rules: Vec<SortRule> }` with `SortRule { field: SortField, direction: Direction }`.
**Maximum 4 rules**, enforced in `validate()`. `SortField` = `Name | Artist | AlbumArtist | Album |
Year | TrackNumber | Genre | DateAdded`.

## 9. Persistence targets

| File | Path (Linux; `dirs` equivalents elsewhere) | Written by | When |
| :-- | :-- | :-- | :-- |
| `config.toml` | `~/.config/loxia-player/` | `loxia-player` | on settings change, debounced 1 s |
| `session.json` | `~/.local/state/loxia-player/` | `loxia-cache::session` | on quit, on track change, every 15 s while playing |
| `history.json` | `~/.local/state/loxia-player/` | `loxia-cache::session` | on track completion |
| `scrobbles.json` | `~/.local/state/loxia-player/` | `loxia-cache::scrobble` | on each offline playback event |
| `cache_index.json` | `~/.cache/loxia-player/` | `loxia-cache::manifest` | on cache mutation, debounced 1 s |
| `downloads_index.json` | `~/.local/share/loxia-player/downloads/` | `loxia-cache::downloads` | on pin/unpin |
| `loxia-player.log` | `~/.local/state/loxia-player/` | `tracing-appender` | always |

**Every write is atomic:** write `<name>.tmp` in the same directory, `fsync`, then rename.

### `SessionSnapshot` (session.json)
`{ schema_version, server_id, queue: QueueState, position_secs: f64, active_tab, zen_mode, volume,
quality_profile, eq: EqState, saved_at }`

On restore: validate `schema_version` and that `server_id` matches `active_server`; drop the
snapshot otherwise. Tracks come from the snapshot itself — **do not refetch at startup**. Entries
are marked `Unavailable` lazily, on first access, if the server no longer resolves them.
Restore is always **paused** (`12-decisions.md` §6).
