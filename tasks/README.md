# loxia task library

loxia is built from a pre-written task library, not ad-hoc feature requests (see
`CONTRIBUTING.md`). **114 tasks in 13 phases. Execute in filename order** — task IDs are numeric
(`<phase>-<sequence>`), and a task's prerequisites are guaranteed to be numerically earlier than
it. Working strictly in filename order therefore never leaves a prerequisite unmet; you do not
need to cross-reference the `Prerequisites` line of every later task before starting on an
earlier one.

Pick the lowest-numbered unticked task whose prerequisites are already ticked. A ticked checkbox
below is the single source of truth for "this task is done" — do not tick one without the work
behind it, and do not untick one you didn't just finish.

## How to use a task file

Every task file follows this template:

```
# <id> · <title>

**Phase:** <phase number> — <phase name> · **Agent:** <any | core | audio | tui | player | ...> ·
**Size:** S | M | L · **Prerequisites:** <task ids, or "none"> · **Reference:** <docs file(s)>

## Goal

What this task achieves and why, in a paragraph or two.

## Files

The exact files this task is allowed to touch. Touching anything else — especially across a
crate boundary — is out of scope unless this section (or a dedicated authorisation subsection)
says so explicitly.

## Specification

The exact behaviour to implement: types, function signatures, edge cases, error conditions.

## Acceptance

The named tests that must exist and pass. A reviewer checks these by name, not by vibes.

## Done when

See "Global Definition of Done" below, plus any task-specific conditions listed here.
```

A task file is self-contained: you should not need to read `design_overview` to execute one.
Background and rationale for the task library itself live in `docs/`; `docs/12-decisions.md`
records every place implementation diverged from that design, with the reason.

## Size legend

- **S** — small: a single sitting, one or two files, no new public surface of note.
- **M** — medium: a few hours, several files, may add a small new type or trait method.
- **L** — large: a full day or more, many files, or introduces a new subsystem/module tree.

## Global Definition of Done

Every task, no exceptions:

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the task's Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] The task's checkbox is ticked in `tasks/README.md`, in the same PR that completes it

Run `just check-all` before opening a PR.

## Hard rules

1. `loxia-core` has zero I/O — no `tokio`, `reqwest`, `ratatui`, or filesystem access.
2. No `unwrap()` / `expect()` outside `main.rs` bootstrap and tests.
3. `crossterm` is never a direct dependency — use `ratatui::crossterm`.
4. Never hardcode a keybinding in UI text — render through `KeyMap::hint_for(ActionId)`.
5. Never log a token or a stream URL — redact in `Debug`/`Display`.
6. One task = one branch = one PR (branch name `feat/<task-id>-<slug>`); never cross a crate
   boundary in a single task unless the task file explicitly authorises it.

## Tasks

### Phase 00 — Scaffolding

- [ ] `00-01` — Workspace skeleton
- [ ] `00-02` — Workspace dependencies
- [ ] `00-03` — Dev tooling and licence
- [ ] `00-04` — CI workflow
- [ ] `00-05` — cargo-deny policy

### Phase 01 — Config

- [ ] `01-01` — Config schema
- [ ] `01-02` — Config defaults and validation
- [ ] `01-03` — Path resolution
- [ ] `01-04` — Config file I/O
- [ ] `01-05` — Terminal guard
- [ ] `01-06` — CLI and logging
- [ ] `01-07` — Domain model types
- [ ] `01-08` — Lyrics model and LRC parser

### Phase 02 — Emby client

- [ ] `02-01` — API audit
- [ ] `02-02` — HTTP client and auth
- [ ] `02-03` — Errors and retry
- [ ] `02-04` — DTOs and conversion
- [ ] `02-05` — Item query builder
- [ ] `02-06` — Discography & appears-on
- [ ] `02-07` — Search, favourites, instant mix
- [ ] `02-08` — Playlists
- [ ] `02-09` — PlaybackInfo and stream URLs
- [ ] `02-10` — Playback reporting
- [ ] `02-11` — Lyrics
- [ ] `02-12` — Images
- [ ] `02-13` — Probe example

### Phase 03 — State machine

- [ ] `03-01` — AppState and substates
- [ ] `03-02` — Test-support fixtures
- [ ] `03-03` — Action, Effect, Event
- [ ] `03-04` — Key chords and parser
- [ ] `03-05` — Default keymap and validation
- [ ] `03-06` — Reducer: navigation
- [ ] `03-07` — Reducer: modals
- [ ] `03-08` — Runtime event loop
- [ ] `03-09` — Input mapping

### Phase 04 — Miller UI

- [ ] `04-01` — Theme system
- [ ] `04-02` — Root layout
- [ ] `04-03` — Text helpers
- [ ] `04-04` — Hit map
- [ ] `04-05` — Sidebar and header
- [ ] `04-06` — Column widget
- [ ] `04-07` — Miller view
- [ ] `04-08` — Inspector
- [ ] `04-09` — Player bar
- [ ] `04-10` — Network worker and wiring
- [ ] `04-11` — Inline filter

### Phase 05 — Audio

- [ ] `05-01` — Backend trait and types
- [ ] `05-02` — Mock engine
- [ ] `05-03` — mpv handle
- [ ] `05-04` — mpv event pump
- [ ] `05-05` — Custom headers and diagnostics
- [ ] `05-06` — Audio worker

### Phase 06 — Queue engine

- [ ] `06-01` — Queue state basics
- [ ] `06-02` — Appears-on queue rules
- [x] `06-03` — Shuffle
- [ ] `06-04` — Sort profiles
- [ ] `06-05` — Listening history
- [x] `06-06` — Gapless preloading
- [x] `06-07` — Playback reporting wiring
- [ ] `06-08` — Instant mix
- [ ] `06-09` — Queue insert characterization tests
- [ ] `06-10` — Insert-next consistency fix
- [ ] `06-11` — Stale preload audit
- [ ] `06-12` — Stale preload retraction
- [ ] `06-13` — Play-order consumer audit

### Phase 07 — Views

- [ ] `07-01` — Search tab
- [ ] `07-02` — Favourites tab
- [ ] `07-03` — Playlists tab
- [ ] `07-04` — Genres tab
- [ ] `07-05` — Folders tab
- [ ] `07-06` — Now playing view
- [ ] `07-07` — Lyrics pane

### Phase 08 — Cache & offline

- [ ] `08-01` — Cache paths and sanitiser
- [ ] `08-02` — Manifest and LRU
- [ ] `08-03` — Cache write-through
- [ ] `08-04` — Permanent downloads
- [ ] `08-05` — Offline browse index
- [ ] `08-06` — Connectivity state machine
- [ ] `08-07` — Scrobble buffer
- [ ] `08-08` — Session and history persistence

### Phase 09 — Advanced audio

- [x] `09-01` — Device enumeration and swap
- [ ] `09-02` — Bit-perfect mode
- [ ] `09-03` — Equalizer engine
- [ ] `09-04` — Replay gain
- [ ] `09-05` — Sleep timer
- [ ] `09-06` — Quality profiles

### Phase 10 — Polish

- [ ] `10-01` — Album art
- [ ] `10-02` — Zen mode
- [ ] `10-03` — Help modal
- [ ] `10-04` — Mouse support
- [ ] `10-05` — Device picker modal
- [ ] `10-06` — Equalizer modal
- [ ] `10-07` — Sleep timer modal
- [ ] `10-08` — Save playlist modal
- [ ] `10-09` — Sort profile modal
- [ ] `10-10` — Desktop notifications
- [ ] `10-11` — Media keys
- [ ] `10-12` — WebSocket remote control
- [ ] `10-13` — Toasts and empty states

### Phase 11 — Settings

- [ ] `11-01` — Settings view
- [ ] `11-02` — Keymap editor
- [ ] `11-03` — Server profiles
- [ ] `11-04` — Sort profile editor
- [ ] `11-05` — EQ preset manager
- [x] `11-06` — Session restore wiring
- [ ] `11-07` — About view

### Phase 12 — Packaging

- [ ] `12-01` — cargo-dist setup
- [ ] `12-02` — Windows packaging
- [ ] `12-03` — macOS packaging
- [ ] `12-04` — Linux packaging
- [ ] `12-05` — Licence compliance checks
- [ ] `12-06` — Branding assets
- [ ] `12-07` — README and user docs
- [ ] `12-08` — doctor subcommand
