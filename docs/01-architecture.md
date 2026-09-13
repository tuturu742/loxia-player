# 01 — Architecture

## 1. Why a workspace

A single crate would work, but splitting into a workspace enforces the dependency rules
mechanically — the compiler rejects violations — and lets multiple agents work without collisions.
`loxia-core` being unable to *see* `tokio` or `ratatui` is what keeps the state machine testable.

## 2. Workspace layout

```
loxia-player/
├── Cargo.toml                  # [workspace] members + [workspace.dependencies]
├── rust-toolchain.toml         # channel = "1.97.1"
├── deny.toml                   # cargo-deny licence + advisory policy
├── dist-workspace.toml         # cargo-dist config (phase 12)
├── design_overview             # product spec
├── docs/                       # this blueprint
├── tasks/                      # ordered task library — the thing you execute
├── assets/
│   ├── logo.svg
│   ├── BRANDING.md
│   ├── banner.txt              # ASCII boot banner
│   ├── themes/*.toml           # 7 built-in themes, embedded via include_str!
│   └── eq_presets.toml         # factory equalizer presets
├── crates/
│   ├── loxia-core/             # pure domain + state machine. NO I/O.
│   ├── loxia-emby/             # Emby REST + WebSocket client
│   ├── loxia-audio/            # libmpv2 wrapper, devices, DSP
│   ├── loxia-cache/            # LRU cache, downloads, scrobble buffer, session
│   ├── loxia-tui/              # ratatui rendering + widgets
│   └── loxia/                  # binary: wiring, runtime, event loop
└── packaging/                  # WiX, Homebrew, AUR, Scoop sources
```

### Dependency graph (strictly acyclic)

```
                        loxia-core
                            ▲
            ┌───────────────┼───────────────┬───────────────┐
            │               │               │               │
      loxia-emby      loxia-audio     loxia-cache      loxia-tui
            ▲               ▲               ▲               ▲
            └───────────────┴───────┬───────┴───────────────┘
                                    │
                                  loxia
```

**Rule:** the four middle crates may depend on `loxia-core` and on **nothing else in the
workspace**. Any cross-talk between them goes through the binary as Effects and Events.

## 3. Crate responsibilities & module trees

### 3.1 `loxia-core` — domain & state machine (no I/O)

```
src/
├── lib.rs
├── model/
│   ├── mod.rs
│   ├── ids.rs              # ItemId, ServerId, UserId, PlaylistId, QueueEntryId
│   ├── item.rs             # Artist, Album, Track, Genre, Folder, Playlist, MediaItem
│   ├── audio_meta.rs       # Codec, AudioFormat, ReplayGainInfo
│   └── lyrics.rs           # Lyrics, LyricLine, LRC parser
├── state/
│   ├── mod.rs              # AppState root
│   ├── nav.rs              # NavState, Column, sliding window
│   ├── queue.rs            # QueueState: entries, play_order, history
│   ├── player.rs           # PlayerState mirror
│   ├── modal.rs            # Modal enum
│   ├── search.rs
│   └── toast.rs
├── action.rs               # Action — the ONLY way state changes
├── effect.rs               # Effect — side-effect requests emitted by the reducer
├── event.rs                # Event — results coming back from workers
├── reducer/
│   ├── mod.rs              # apply(&mut AppState, Action) -> Vec<Effect>
│   ├── nav.rs  queue.rs  player.rs  modal.rs  settings.rs
├── queue/
│   ├── sort.rs             # multi-criteria sort profiles
│   ├── shuffle.rs          # non-destructive shuffle
│   └── appears_on.rs       # collaboration grouping + queue filtering rules
├── keymap/
│   ├── mod.rs              # KeyChord, KeyBinding, KeyMap, ActionId
│   ├── parse.rs            # binding-string parser / renderer
│   ├── resolve.rs          # (context, chord) -> Action, pending-prefix support
│   ├── validate.rs         # conflict detection
│   └── defaults.rs         # the default binding table (04 §6)
├── config/
│   ├── mod.rs              # load/merge/validate — parsing only, caller does I/O
│   ├── schema.rs           # serde structs
│   └── migrate.rs          # schema_version migrations
├── theme.rs                # Theme, semantic roles, style lookup
├── paths.rs                # per-OS path computation (pure fn of an injected provider)
├── test_support/           # AppState fixtures shared by every other crate (feature-gated)
└── error.rs
```

### 3.2 `loxia-emby` — server client

```
src/
├── lib.rs
├── client.rs               # EmbyClient: reqwest + base url + auth + custom headers
├── auth.rs                 # AuthenticateByName, token validation, X-Emby-Authorization
├── query.rs                # shared item-query builder
├── endpoints/
│   ├── mod.rs
│   ├── items.rs            # /Users/{u}/Items, /Artists, /MusicGenres, folders
│   ├── discography.rs      # the one-query appears-on grouping (03 §4)
│   ├── search.rs
│   ├── playlists.rs
│   ├── favorites.rs
│   ├── playback.rs         # PlaybackInfo + Sessions/Playing reporting
│   ├── instant_mix.rs
│   ├── lyrics.rs           # subtitle-stream discovery + fetch (03 §7)
│   └── images.rs
├── stream.rs               # audio stream URL builder
├── dto/                    # raw serde DTOs + `impl From<Dto> for loxia_core::model::*`
├── ws.rs                   # WebSocket session
├── retry.rs                # backoff, offline detection
└── error.rs
```

### 3.3 `loxia-audio` — playback engine

```
src/
├── lib.rs
├── backend.rs              # trait AudioBackend + AudioCommand + AudioEvent
├── mock.rs                 # MockEngine: deterministic, virtual clock
├── mpv/
│   ├── handle.rs           # libmpv2 init, property observation, event pump thread
│   ├── props.rs            # typed property constants
│   └── filters.rs          # audio filter graph construction
├── device/
│   ├── mod.rs              # AudioDevice, enumeration, hot-swap
│   ├── linux.rs  windows.rs  macos.rs
├── eq.rs                   # 10-band ISO curve -> anequalizer filter
├── replaygain.rs
├── gapless.rs              # next-track preloading via the mpv playlist
└── error.rs
```

### 3.4 `loxia-cache` — storage tiers

```
src/
├── lib.rs
├── layout.rs               # directory scheme, cache keys, path sanitiser
├── manifest.rs             # JSON index with atomic write-rename
├── lru.rs                  # rolling cache + eviction
├── downloads.rs            # permanent pinned items, sidecars, resume
├── offline_index.rs        # browse tree built from download sidecars
├── scrobble.rs             # offline playback-event queue
├── session.rs              # session.json + history.json
└── error.rs
```

### 3.5 `loxia-tui` — rendering

```
src/
├── lib.rs
├── render.rs               # draw(frame, &AppState, &Theme, &mut HitMap) — single entry point
├── layout.rs               # zone computation + responsive degradation
├── views/
│   ├── miller.rs  now_playing.rs  search.rs  favourites.rs
│   ├── playlists.rs  genres.rs  folders.rs  settings.rs  zen.rs
├── widgets/
│   ├── sidebar.rs  header.rs  column.rs  inspector.rs  player_bar.rs
│   ├── progress.rs  section_header.rs  lyrics.rs  album_art.rs  toast.rs
├── modals/
│   ├── help.rs  equalizer.rs  device_picker.rs  sleep_timer.rs
│   ├── save_playlist.rs  sort_profile.rs  keymap_editor.rs  confirm.rs
├── hit.rs                  # HitMap, HitTarget
├── text.rs                 # grapheme-safe truncation and width measurement
└── style.rs                # Theme -> ratatui::Style resolution
```

**Rendering rule:** `draw()` takes `&AppState` immutably and returns `()`. It may write hit-test
rectangles into the `&mut HitMap` and nothing else. No state mutation during render.

### 3.6 `loxia-player` — the binary

```
src/
├── main.rs                 # CLI args, logging init, panic hook, terminal guard
├── bootstrap.rs            # load config, restore session, connect, spawn workers
├── runtime.rs              # THE EVENT LOOP
├── dispatch.rs             # Effect -> worker routing
├── input.rs                # crossterm events -> Action via keymap + HitMap
├── terminal.rs             # raw mode, alt screen, mouse capture, restore-on-panic
├── doctor.rs               # `loxia-player --doctor` diagnostics
└── workers/
    ├── mod.rs  network.rs  audio.rs  cache.rs  mpris.rs  notify.rs
```

## 4. Runtime & threading model

```
 ┌──────────────────────────── main thread (async, tokio) ────────────────────────────┐
 │                                                                                    │
 │   crossterm EventStream ──┐                                                        │
 │   tick timer (100ms)    ──┼──► select! ──► Action ──► reducer ──► Vec<Effect>       │
 │   Event channel (rx)    ──┘        │                    │              │            │
 │                                    │              &mut AppState        ▼            │
 │                                    ▼                    │        dispatch(effect)   │
 │                             if dirty { draw(&AppState) }│              │            │
 └────────────────────────────────────────────────────────────────────────┼───────────┘
                                                                          │
        ┌─────────────────┬──────────────────┬──────────────────┬─────────┴────────┐
        ▼                 ▼                  ▼                  ▼                  ▼
   network worker    audio worker      cache worker       mpris worker      notify worker
   (tokio tasks)     (mpv thread)      (blocking pool)    (souvlaki)        (notify-rust)
        │                 │                  │                  │                  │
        └─────────────────┴──────────────────┴──────────────────┴──────────────────┘
                                             │
                                    Event channel (tx) ──► back to the main loop
```

**Key decisions:**

- `AppState` lives on the main thread only, behind a plain `&mut`. **No `Arc<Mutex<AppState>>`.**
  This eliminates a whole class of deadlocks and makes the reducer trivially testable.
- Workers own their resources (HTTP client, mpv handle, cache index) and never see `AppState`.
  They consume `Effect`s and emit `Event`s. Both are plain data: `Clone + Send + 'static`.
- Channels: `tokio::sync::mpsc::unbounded_channel`, one per worker for Effects and one shared
  many-producer channel for Events. Unbounded is deliberate — volume is low, and a bounded channel
  can deadlock the render loop.
- libmpv2's handle is owned by one dedicated OS thread that blocks in `wait_event` and forwards
  results into the Event channel. It is shut down explicitly on quit or the process will not exit.
- **Render throttling:** the reducer sets `state.dirty` on any visible change; the loop redraws at
  most once per 16 ms. The 100 ms tick drives progress and lyrics. Never redraw per event.

## 5. Data flow example — pressing `a` on an "Appears On" album

1. `input.rs` reads `KeyEvent(Char('a'))`.
2. `keymap::resolve(InputContext::MillerColumns, chord)` → `Action::Queue(QueueSelection { full_context: false })`.
3. `reducer::queue` reads the selected item, sees an `Album` with
   `relation: AppearsOn { context_artist }`, and emits
   `Effect::Net(FetchAlbumTracks { album_id, filter: ArtistOnly(context_artist) })`.
   State gains a loading marker. The queue does not change yet.
4. `dispatch` routes to the network worker, which calls `endpoints::items::album_tracks`.
5. The worker emits `Event::AlbumTracksLoaded { album_id, tracks }`.
6. The loop converts it to `Action::Data(TracksLoaded { .. })`. The reducer applies
   `appears_on::filter_for_artist`, applies the active sort profile, appends to the queue, clears
   the loading marker, and — if nothing is playing — emits `Effect::Audio(Load { .. })`.
7. `state.dirty` is set; the next frame renders.

Every feature decomposes exactly this way. **When in doubt, follow this shape.**

## 6. Error & offline strategy

- Each library crate defines a `thiserror` enum. The binary aggregates with `anyhow` at the edges.
- Network errors classify as `Transient` (retry with backoff), `Unauthorized`, `NotFound`, or
  `Offline`. `Offline` flips `AppState.connectivity`, which makes the UI:
  - serve browse data from the offline index built from download sidecars,
  - refuse to queue items that are neither cached nor downloaded, rendering them dimmed with `⚠`,
  - route playback events to the scrobble buffer instead of the server.
- A reconnect probe runs on a 5 s → 30 s jittered backoff while offline. Success drains the buffer.

## 7. Logging

`tracing` with a `LOXIA_LOG` env filter. **File sink only** — `~/.local/state/loxia-player/loxia-player.log`,
rolled daily, 5 files retained. Writing to stdout would corrupt the TUI. Spans: `net`, `audio`,
`cache`, `ui`, `state`. Access tokens and stream URLs are redacted by the `Display` impls before
they can reach a log line.
