# 06-13 · play_order consumer audit

**Phase:** 06 — Queue engine · **Agent:** loxia-core, loxia-tui, loxia-player (read-only) · **Size:** S · **Prerequisites:** `06-10`, `06-12` · **Reference:** `docs/02-data-model.md`, `docs/07-ui-spec.md`, `docs/05-audio-engine.md`

## Goal

When this task is done, `docs/17-play-order-consumers.md` exists and lists every reader of the
queue's `play_order` field across the workspace, with a verdict for each on whether it agrees with
the "what is next" semantics `06-10` (insert consistency) and `06-12` (preload retraction)
established. This task modifies no production code — any inconsistency it finds is filed as a new
task, not fixed in place.

## Scope note

Producing the report requires reading `loxia-core` (the field's owner and mutators), `loxia-tui`
(the Now Playing queue view, which renders upcoming order to the user), and `loxia-player` (workers
that act on "what's next" — audio preloading and any cache prefetch of the next track). No file
inside any of those crates is modified by this task — the only file created is the report below —
so this does not fall under `CONTRIBUTING.md` rule 2's restriction on crossing a crate boundary
when modifying code.

## Files

- `docs/17-play-order-consumers.md` (create)

Read (do not modify): `crates/loxia-core/src/state/queue.rs`, `crates/loxia-core/src/reducer/queue.rs`,
`crates/loxia-core/src/reducer/player.rs`, `crates/loxia-core/src/queue/sort.rs`,
`crates/loxia-core/src/queue/shuffle.rs`, `crates/loxia-tui/src/views/now_playing.rs`,
`crates/loxia-player/src/workers/audio.rs`, `crates/loxia-player/src/workers/cache.rs`.

## Specification

`docs/17-play-order-consumers.md` must contain a table with one row per consumer found, each
stating: the file and function that reads `play_order` (or the queue's actual equivalent field —
name it exactly as found, not as assumed), what it uses "next" for (rendering, preloading, cache
prefetch, playback-reporting, etc.), and a verdict of `consistent` or
`inconsistent — see task <new-id>` (a new task must be filed and referenced by ID for any
`inconsistent` row; this audit does not fix the inconsistency itself). At minimum, check:

1. The Now Playing view's upcoming-tracks list (`crates/loxia-tui/src/views/now_playing.rs`).
2. The audio worker's preload target selection (`crates/loxia-player/src/workers/audio.rs`),
   post-`06-12`.
3. Any cache/download prefetch-the-next-track logic in `crates/loxia-player/src/workers/cache.rs`.
4. The reducer's own "advance to next on track end" logic (`crates/loxia-core/src/reducer/player.rs`).

## Acceptance

This task adds no automated tests — its deliverable is the report. Verify instead that:

- `docs/17-play-order-consumers.md` exists and has a verdict row for all four consumers listed
  above, plus any others found during the read-through.
- Every `inconsistent` row references a real, newly filed task ID under `tasks/`.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` remain
  green and clean, unchanged from before this task (no code was touched).

## Done when

Global DoD in `tasks/README.md`, plus: no file inside `crates/*` is modified;
`docs/17-play-order-consumers.md` is the only new or changed file (aside from any newly filed task
files it references).
