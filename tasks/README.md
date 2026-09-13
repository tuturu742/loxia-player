# loxia — Task Library

109 tasks in 13 phases. **Execute in filename order.** Every task's prerequisites are numerically
earlier, so if you always take the lowest unticked task whose prerequisites are ticked, you can
never be blocked by ordering.

## How to use a task file

Each file is self-contained. You should not need to read `design_overview` to execute one — the
task states the types, signatures, and behaviour rules directly. `docs/` is linked for background
when you want the surrounding rationale.

```
# <id> · <title>
**Phase / Agent / Size / Prerequisites / Reference**
## Goal            — two sentences on what exists when you're finished
## Files           — exact paths to create or modify
## Specification   — types, signatures, behaviour rules. No design judgement required.
## Acceptance      — named tests that must exist and pass
## Done when       — the global DoD
```

**Size:** S ≈ one focused session · M ≈ two · L ≈ three or more.

## Global Definition of Done

Every task, no exceptions:

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked below

## Hard rules

1. **`loxia-core` has no I/O.** No `tokio`, no `reqwest`, no `ratatui`, no filesystem. If a task
   seems to need one, you have misread the task.
2. **No `unwrap()` / `expect()`** outside `main.rs` bootstrap and tests.
3. **`crossterm` is never a direct dependency** — use `ratatui::crossterm`.
4. **Never hardcode a keybinding in UI text** — render through `KeyMap::hint_for(ActionId)`.
5. **Never log a token or a stream URL.** Redact in `Debug`/`Display`.
6. **One task = one branch = one PR**, named `feat/<id>-<slug>`.

## Progress

### Phase 00 — Scaffolding
- [x] `00-01` workspace skeleton
- [x] `00-02` workspace dependencies
- [x] `00-03` dev tooling and licence
- [x] `00-04` CI workflow
- [x] `00-05` cargo-deny policy

### Phase 01 — Config, paths & bootstrap
- [x] `01-01` config schema
- [x] `01-02` config defaults and validation
- [x] `01-03` path resolution
- [x] `01-04` config file I/O
- [x] `01-05` terminal guard
- [x] `01-06` CLI and logging
- [x] `01-07` domain model types
- [x] `01-08` lyrics model and LRC parser

### Phase 02 — Emby client
- [x] `02-01` API audit against the live server
- [x] `02-02` HTTP client and auth
- [x] `02-03` errors and retry
- [x] `02-04` DTOs and model conversion
- [x] `02-05` item query builder
- [x] `02-06` discography and the appears-on split
- [x] `02-07` search, favourites, instant mix
- [x] `02-08` playlists
- [x] `02-09` PlaybackInfo and stream URLs
- [x] `02-10` playback reporting
- [x] `02-11` lyrics
- [x] `02-12` images
- [x] `02-13` probe example

### Phase 03 — State machine core
- [x] `03-01` AppState and sub-states
- [x] `03-02` test-support fixtures
- [x] `03-03` Action, Effect, Event
- [x] `03-04` key chords and binding parser
- [x] `03-05` default keymap and conflict validation
- [x] `03-06` reducer: navigation
- [x] `03-07` reducer: modals
- [x] `03-08` runtime event loop
- [x] `03-09` input mapping

### Phase 04 — Miller columns UI
- [x] `04-01` theme system
- [x] `04-02` root layout
- [x] `04-03` text measurement helpers
- [x] `04-04` hit map
- [x] `04-05` sidebar and header
- [x] `04-06` column widget
- [x] `04-07` miller view
- [x] `04-08` inspector
- [x] `04-09` player bar
- [x] `04-10` network worker and wiring
- [x] `04-11` inline filter

### Phase 05 — Audio MVP
- [x] `05-01` backend trait and types
- [x] `05-02` mock engine
- [x] `05-03` mpv handle
- [x] `05-04` mpv event pump
- [x] `05-05` custom headers and diagnostics
- [x] `05-06` audio worker

### Phase 06 — Queue engine
- [x] `06-01` queue state basics
- [x] `06-02` appears-on queue rules
- [x] `06-03` non-destructive shuffle
- [x] `06-04` sort profiles
- [x] `06-05` listening history
- [x] `06-06` gapless preloading
- [x] `06-07` playback reporting wiring
- [x] `06-08` instant mix

### Phase 07 — Remaining views
- [x] `07-01` search tab
- [x] `07-02` favourites tab
- [x] `07-03` playlists tab
- [x] `07-04` genres tab
- [x] `07-05` folders tab
- [x] `07-06` now playing view
- [x] `07-07` lyrics pane

### Phase 08 — Cache & offline
- [x] `08-01` cache paths and sanitiser
- [x] `08-02` manifest and LRU
- [x] `08-03` cache write-through
- [x] `08-04` permanent downloads
- [x] `08-05` offline browse index
- [x] `08-06` connectivity state machine
- [x] `08-07` scrobble buffer
- [x] `08-08` session and history persistence

### Phase 09 — Advanced audio
- [x] `09-01` device enumeration and hot-swap
- [x] `09-02` bit-perfect mode
- [x] `09-03` equalizer engine
- [x] `09-04` replay gain
- [x] `09-05` sleep timer
- [x] `09-06` quality profiles

### Phase 10 — Polish & integrations
- [x] `10-01` album art
- [x] `10-02` zen mode
- [x] `10-03` help modal
- [x] `10-04` mouse support
- [x] `10-05` device picker modal
- [x] `10-06` equalizer modal
- [x] `10-07` sleep timer modal
- [x] `10-08` save playlist modal
- [x] `10-09` sort profile modal
- [x] `10-10` desktop notifications
- [x] `10-11` media keys (MPRIS/SMTC)
- [x] `10-12` websocket remote control
- [x] `10-13` toasts and empty states

### Phase 11 — Settings & keymapper
- [x] `11-01` settings view
- [x] `11-02` keymap editor
- [x] `11-03` server profiles
- [x] `11-04` sort profile editor
- [x] `11-05` EQ preset manager
- [x] `11-06` session restore wiring
- [x] `11-07` about view

### Phase 12 — Packaging & release
- [ ] `12-01` cargo-dist setup
- [ ] `12-02` windows packaging
- [ ] `12-03` macOS packaging
- [ ] `12-04` linux packaging
- [ ] `12-05` licence compliance checks
- [ ] `12-06` branding assets
- [ ] `12-07` README and user docs
- [ ] `12-08` doctor subcommand
