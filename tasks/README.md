# Task Library

loxia is built entirely from this pre-written task library — see `CONTRIBUTING.md` for the
workflow. There are 114 tasks across 13 phases, each in its own file under `tasks/phase-NN-*/`.

## How to use a task file

Each task file is self-contained: it states the goal, the exact files to touch, the
specification, and the acceptance tests. Read the task file itself, not `design_overview`, to
execute it. Pick the lowest-numbered unticked task below whose prerequisites are already
ticked, open its file, and do exactly what it says — no more, no less.

Every task file has the same shape:

- **Goal** — one paragraph, what this task adds and why.
- **Prerequisites** — task IDs that must already be ticked below.
- **Files** — the exact file list this task is allowed to touch.
- **Spec** — the behaviour to implement, in enough detail that no design judgement is needed.
- **Acceptance** — the named tests that must exist and pass.

Size: most tasks are a half-day to a day of focused work for someone already familiar with the
crate. A task that regularly takes longer than two days is a sign the task itself should have
been split — raise that as a documentation issue, not a reason to skip ahead.

## Hard rules

1. Do the lowest-numbered unticked task whose prerequisites are ticked. Do not skip ahead because
   a later task looks more interesting.
2. Tick a task's checkbox in the same PR that completes it — this file is the single source of
   truth for progress, not memory or a project board. Do not untick one you didn't just finish.
3. Touch only the files a task's own **Files** section names. If you need to touch something
   else, that's a sign you have the wrong task, or the task file needs a documented correction.
4. If a task's spec conflicts with something you find in the code, `docs/12-decisions.md` is
   where you record the resolution — the task's own text is not amended silently.
5. `#[cfg(target_os = ...)]` is confined to `loxia-audio::device` and `loxia-core::paths`; nowhere
   else needs to know what platform it's running on.
6. Never cross a crate boundary in a single task unless the task file explicitly authorises it.

## Progress

### Phase 00 — Scaffolding
- [x] `00-01` workspace skeleton
- [x] `00-02` workspace dependencies
- [x] `00-03` dev tooling and licence
- [x] `00-04` ci workflow
- [x] `00-05` cargo deny policy

### Phase 01 — Config
- [x] `01-01` config schema
- [x] `01-02` config defaults and validation
- [x] `01-03` path resolution
- [x] `01-04` config file io
- [x] `01-05` terminal guard
- [x] `01-06` cli and logging
- [x] `01-07` domain model types
- [x] `01-08` lyrics model and lrc parser

### Phase 02 — Emby client
- [x] `02-01` api audit
- [x] `02-02` http client and auth
- [x] `02-03` errors and retry
- [x] `02-04` dtos and conversion
- [x] `02-05` item query builder
- [x] `02-06` discography appears on
- [x] `02-07` search favourites instant mix
- [x] `02-08` playlists
- [x] `02-09` playbackinfo and stream urls
- [x] `02-10` playback reporting
- [x] `02-11` lyrics
- [x] `02-12` images
- [x] `02-13` probe example

### Phase 03 — State machine
- [x] `03-01` appstate and substates
- [x] `03-02` test support fixtures
- [x] `03-03` action effect event
- [x] `03-04` key chords and parser
- [x] `03-05` default keymap and validation
- [x] `03-06` reducer navigation
- [x] `03-07` reducer modals
- [x] `03-08` runtime event loop
- [x] `03-09` input mapping

### Phase 04 — Miller UI
- [x] `04-01` theme system
- [x] `04-02` root layout
- [x] `04-03` text helpers
- [x] `04-04` hit map
- [x] `04-05` sidebar and header
- [x] `04-06` column widget
- [x] `04-07` miller view
- [x] `04-08` inspector
- [x] `04-09` player bar
- [x] `04-10` network worker and wiring
- [x] `04-11` inline filter

### Phase 05 — Audio
- [x] `05-01` backend trait and types
- [x] `05-02` mock engine
- [x] `05-03` mpv handle
- [x] `05-04` mpv event pump
- [x] `05-05` custom headers and diagnostics
- [x] `05-06` audio worker

### Phase 06 — Queue
- [x] `06-01` queue state basics
- [x] `06-02` appears on queue rules
- [x] `06-03` shuffle
- [x] `06-04` sort profiles
- [x] `06-05` listening history
- [x] `06-06` gapless preloading
- [x] `06-07` playback reporting wiring
- [x] `06-08` instant mix
- [ ] `06-09` queue insert characterization tests
- [ ] `06-10` insert-next consistency fix
- [ ] `06-11` stale preload audit
- [ ] `06-12` stale preload retraction
- [ ] `06-13` play-order consumer audit

### Phase 07 — Views
- [x] `07-01` search tab
- [x] `07-02` favourites tab
- [x] `07-03` playlists tab
- [x] `07-04` genres tab
- [x] `07-05` folders tab
- [x] `07-06` now playing view
- [x] `07-07` lyrics pane

### Phase 08 — Cache and offline
- [x] `08-01` cache paths and sanitiser
- [x] `08-02` manifest and lru
- [x] `08-03` cache write through
- [x] `08-04` permanent downloads
- [x] `08-05` offline browse index
- [x] `08-06` connectivity state machine
- [x] `08-07` scrobble buffer
- [x] `08-08` session and history persistence

### Phase 09 — Advanced audio
- [x] `09-01` device enumeration and swap
- [x] `09-02` bit perfect mode
- [x] `09-03` equalizer engine
- [x] `09-04` replay gain
- [x] `09-05` sleep timer
- [x] `09-06` quality profiles

### Phase 10 — Polish
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
- [x] `10-11` media keys
- [x] `10-12` websocket remote control
- [x] `10-13` toasts and empty states

### Phase 11 — Settings
- [x] `11-01` settings view
- [x] `11-02` keymap editor
- [x] `11-03` server profiles
- [x] `11-04` sort profile editor
- [x] `11-05` eq preset manager
- [x] `11-06` session restore wiring
- [x] `11-07` about view

### Phase 12 — Packaging
- [ ] `12-01` cargo dist setup
- [ ] `12-02` windows packaging
- [ ] `12-03` macos packaging
- [ ] `12-04` linux packaging
- [ ] `12-05` licence compliance checks
- [ ] `12-06` branding assets
- [ ] `12-07` readme and user docs
- [ ] `12-08` doctor subcommand
