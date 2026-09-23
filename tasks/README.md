# loxia task library

This is the index CONTRIBUTING.md refers to. Work happens one task at a time: pick the
lowest-numbered unticked task below whose prerequisites are already ticked, follow its task file
under `tasks/<phase>/`, tick its box in the same PR that completes it.

Ticking a box here is the single source of truth for "done" — a task file existing is not the
same as it being finished.

## Queue-audit summary (added by this change)

Per the queue-audit request, before adding anything new the following were checked:

- **`tasks/README.md` (this file, prior revision).** Searched for `queue`, `insert`, `play next`,
  `QueueBatch`, `preload`. Phase 06 (`tasks/phase-06-queue/`) is the only phase whose task titles
  match: `06-01-queue-state-basics.md`, `06-02-appears-on-queue-rules.md`, `06-03-shuffle.md`,
  `06-04-sort-profiles.md`, `06-05-listening-history.md`, `06-06-gapless-preloading.md`,
  `06-07-playback-reporting-wiring.md`, `06-08-instant-mix.md`. All eight were already ticked —
  none unticked, in progress, or blocked. None of the eight titles or their acceptance sections
  cover "insert next" positioning specifically, retracting an already-issued gapless preload after
  a later queue edit, or auditing every consumer of the queue's ordering field. That is a genuine
  gap, not a duplicate of existing work.
- **`docs/12-decisions.md` §9 (the deviation log).** Read in full. It has rows covering the mpv
  `af`/`anequalizer` routing decision (cited from `crates/loxia-audio/src/eq.rs` and
  `crates/loxia-audio/src/mpv/filters.rs`), the `EqCurve` vs `Effect::Audio::SetEq` split (cited
  from `crates/loxia-audio/src/backend.rs`), the `VolumeChanged` event addition (`05-04`), the
  fixture secret-scan scoping (`.github/workflows/ci.yml`), and the `library_not_found_hint`
  runtime-vs-`cfg` decision (`crates/loxia-audio/src/error.rs`). There is **no existing row** on
  queue insert position, shuffle-and-insert interaction, or retracting a gapless preload. This
  confirms the five tasks below are new work, not a duplicate of a documented deviation.
- Source citations that anchored the phase/ordering choices below: `06-03` (shuffle seed,
  `crates/loxia-core/src/queue/shuffle.rs`), `06-06` (preload, `crates/loxia-audio/src/gapless.rs`
  — "`reducer::queue` ... is what decides *when* to send that `Preload`"), `06-07` (reporting
  wiring), `09-01` (device swap, `crates/loxia-audio/src/device/mod.rs`), `11-06` (session
  restore). None of these five needed re-opening as a task — they're prerequisites of the new work,
  not the same work.

The five new task files below (`06-09` through `06-13`) are added under `tasks/phase-06-queue/`
because they are direct follow-ons to `06-03`/`06-06`/`06-07`, and are listed in that phase's
section with explicit prerequisites.

## Phase 00 — Scaffolding

- [x] `00-01` — Workspace skeleton
- [x] `00-02` — Workspace dependencies
- [x] `00-03` — Dev tooling and licence
- [x] `00-04` — CI workflow
- [x] `00-05` — cargo-deny policy

## Phase 01 — Config

- [x] `01-01` — Config schema
- [x] `01-02` — Config defaults and validation
- [x] `01-03` — Path resolution
- [x] `01-04` — Config file I/O
- [x] `01-05` — Terminal guard
- [x] `01-06` — CLI and logging
- [x] `01-07` — Domain model types
- [x] `01-08` — Lyrics model and LRC parser

## Phase 02 — Emby client

- [x] `02-01` — API audit
- [x] `02-02` — HTTP client and auth
- [x] `02-03` — Errors and retry
- [x] `02-04` — DTOs and conversion
- [x] `02-05` — Item query builder
- [x] `02-06` — Discography / appears-on
- [x] `02-07` — Search, favourites, instant mix
- [x] `02-08` — Playlists
- [x] `02-09` — PlaybackInfo and stream URLs
- [x] `02-10` — Playback reporting
- [x] `02-11` — Lyrics
- [x] `02-12` — Images
- [x] `02-13` — Probe example

## Phase 03 — State machine

- [x] `03-01` — AppState and substates
- [x] `03-02` — Test support fixtures
- [x] `03-03` — Action / Effect / Event
- [x] `03-04` — Key chords and parser
- [x] `03-05` — Default keymap and validation
- [x] `03-06` — Reducer: navigation
- [x] `03-07` — Reducer: modals
- [x] `03-08` — Runtime event loop
- [x] `03-09` — Input mapping

## Phase 04 — Miller UI

- [x] `04-01` — Theme system
- [x] `04-02` — Root layout
- [x] `04-03` — Text helpers
- [x] `04-04` — Hit map
- [x] `04-05` — Sidebar and header
- [x] `04-06` — Column widget
- [x] `04-07` — Miller view
- [x] `04-08` — Inspector
- [x] `04-09` — Player bar
- [x] `04-10` — Network worker and wiring
- [x] `04-11` — Inline filter

## Phase 05 — Audio

- [x] `05-01` — Backend trait and types
- [x] `05-02` — Mock engine
- [x] `05-03` — mpv handle
- [x] `05-04` — mpv event pump
- [x] `05-05` — Custom headers and diagnostics
- [x] `05-06` — Audio worker

## Phase 06 — Queue

- [x] `06-01` — Queue state basics
- [x] `06-02` — Appears-on queue rules
- [x] `06-03` — Shuffle
- [x] `06-04` — Sort profiles
- [x] `06-05` — Listening history
- [x] `06-06` — Gapless preloading
- [x] `06-07` — Playback reporting wiring
- [x] `06-08` — Instant mix
- [ ] `06-09` — Queue-insert characterization tests (prereqs: `06-01`, `06-02`, `06-03`, `06-04`)
- [ ] `06-10` — Insert-next consistency fix (prereqs: `06-09`)
- [ ] `06-11` — Stale-preload audit (prereqs: `06-06`, `06-07`, `06-10`)
- [ ] `06-12` — Stale-preload retraction (prereqs: `06-11`) — **crosses `loxia-audio`,
      `loxia-core`, and `loxia-player`; explicitly authorised in the task file itself**
- [ ] `06-13` — `play_order` consumer audit (prereqs: `06-12`)

## Phase 07 — Views

- [x] `07-01` — Search tab
- [x] `07-02` — Favourites tab
- [x] `07-03` — Playlists tab
- [x] `07-04` — Genres tab
- [x] `07-05` — Folders tab
- [x] `07-06` — Now Playing view
- [x] `07-07` — Lyrics pane

## Phase 08 — Cache & offline

- [x] `08-01` — Cache paths and sanitiser
- [x] `08-02` — Manifest and LRU
- [x] `08-03` — Cache write-through
- [x] `08-04` — Permanent downloads
- [x] `08-05` — Offline browse index
- [x] `08-06` — Connectivity state machine
- [x] `08-07` — Scrobble buffer
- [x] `08-08` — Session and history persistence

## Phase 09 — Advanced audio

- [x] `09-01` — Device enumeration and swap
- [x] `09-02` — Bit-perfect mode *(shipped, then its per-OS detection code was removed together
      with `loxia-audio::device`'s platform children — see `docs/12-decisions.md`; tracked as done,
      not reopened, since removal was itself a recorded deviation, not an incomplete task)*
- [x] `09-03` — Equalizer engine
- [x] `09-04` — Replay gain
- [x] `09-05` — Sleep timer
- [x] `09-06` — Quality profiles

## Phase 10 — Polish

- [x] `10-01` — Album art
- [x] `10-02` — Zen mode
- [x] `10-03` — Help modal
- [x] `10-04` — Mouse support
- [x] `10-05` — Device picker modal
- [x] `10-06` — Equalizer modal
- [x] `10-07` — Sleep timer modal
- [x] `10-08` — Save playlist modal
- [x] `10-09` — Sort profile modal
- [x] `10-10` — Desktop notifications
- [x] `10-11` — Media keys
- [x] `10-12` — WebSocket remote control
- [x] `10-13` — Toasts and empty states

## Phase 11 — Settings

- [x] `11-01` — Settings view
- [x] `11-02` — Keymap editor
- [x] `11-03` — Server profiles
- [x] `11-04` — Sort profile editor
- [x] `11-05` — EQ preset manager
- [x] `11-06` — Session restore wiring
- [x] `11-07` — About view

## Phase 12 — Packaging

- [x] `12-01` — cargo-dist setup
- [x] `12-02` — Windows packaging
- [x] `12-03` — macOS packaging
- [x] `12-04` — Linux packaging
- [x] `12-05` — Licence compliance checks
- [x] `12-06` — Branding assets
- [ ] `12-07` — README and user docs *(no top-level `README.md` exists yet — unticked, not part of
      this queue audit's scope)*
- [x] `12-08` — Doctor subcommand
