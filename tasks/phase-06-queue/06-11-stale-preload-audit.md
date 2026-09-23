# 06-11 · stale-preload audit

**Phase:** 06 — Queue engine · **Agent:** loxia-core, loxia-audio, loxia-player (read-only) · **Size:** S · **Prerequisites:** `06-06`, `06-10` · **Reference:** `docs/05-audio-engine.md`, `docs/12-decisions.md`

## Goal

When this task is done, `docs/16-preload-audit.md` exists and enumerates every queue mutation that
can change which track is "next" while an `AudioCommand::Preload` for the *previous* "next" track
is still outstanding, together with a verdict for each: whether the current code retracts that
stale preload before it is consumed, or leaves it in place (a bug). This task modifies no
production code — it is the input `06-12` uses to scope its fix.

## Scope note

Producing the report requires reading across `loxia-core` (the reducer that emits the preload
effect and the mutations that can invalidate it), `loxia-audio` (the backend that holds the
outstanding preload), and `loxia-player` (the worker that turns effects into commands). No file
inside any of those three crates is modified by this task — the only file created or changed is
the report below — so this does not fall under `CONTRIBUTING.md` rule 2's "never cross a crate
boundary in a single task" restriction, which governs modifications, not reading for an audit.

## Files

- `docs/16-preload-audit.md` (create)

Read (do not modify): `crates/loxia-core/src/reducer/queue.rs`, `crates/loxia-core/src/queue.rs`,
`crates/loxia-core/src/effect.rs`, `crates/loxia-audio/src/gapless.rs`,
`crates/loxia-audio/src/backend.rs`, `crates/loxia-audio/src/mpv/handle.rs`,
`crates/loxia-audio/src/mock.rs`, `crates/loxia-player/src/workers/audio.rs`.

## Specification

`docs/16-preload-audit.md` must contain a table with one row per scenario below, each naming the
exact call site(s) involved (file and function name) and a verdict of `retracted`,
`not retracted — bug`, or `not applicable` with a one-sentence justification:

1. `InsertNext` (post-`06-10`) arrives while the current track's preload is already in flight.
2. The queued track a preload targets is removed from the queue before playback reaches it.
3. A shuffle re-roll (`06-03`) changes which track follows the current one while a preload for the
   old "next" is in flight.
4. A manual skip lands on a track other than the one that was preloaded (e.g. because of a
   mid-song insert per scenario 1).
5. Repeat-one is toggled on, changing "next" to the currently-playing track itself, while a
   preload for the track that used to follow it is in flight.

The audit must also state, in one paragraph, whether `loxia-audio`'s current `AudioCommand`/
`AudioEvent` vocabulary (`crates/loxia-audio/src/backend.rs`) has any existing way to cancel or
replace an outstanding preload, or whether `06-12` must add one.

## Acceptance

This task adds no automated tests — its deliverable is the report. Verify instead that:

- `docs/16-preload-audit.md` exists and has a verdict row for all five scenarios listed above.
- `cargo test --workspace` remains green, unchanged from before this task (no code was touched).
- `cargo clippy --workspace --all-targets -- -D warnings` remains clean (no code was touched).

## Done when

Global DoD in `tasks/README.md`, plus: no file inside `crates/*` is modified; `docs/16-preload-audit.md`
is the only new or changed file.
