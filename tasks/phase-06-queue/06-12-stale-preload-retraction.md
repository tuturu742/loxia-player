# 06-12 · stale-preload retraction

**Phase:** 06 — Queue engine · **Agent:** loxia-core, loxia-audio, loxia-player · **Size:** L · **Prerequisites:** `06-11` · **Reference:** `docs/05-audio-engine.md`, `docs/12-decisions.md` §9

## Crate-boundary authorisation

This task file explicitly authorises crossing the `loxia-audio`, `loxia-core`, and `loxia-player`
crate boundaries in a single PR, per `CONTRIBUTING.md` rule 2 ("Never cross a crate boundary in a
single task unless the task explicitly says to"). The fix is inherently a three-crate contract
change: `loxia-core`'s reducer must know when the previously-preloaded track is no longer next and
emit a retraction effect; `loxia-audio`'s backend must accept and act on a cancel/replace command;
`loxia-player`'s audio worker must translate one into the other. Splitting this across three PRs
would leave the contract half-implemented at every intermediate commit.

## Goal

When this task is done, every "not retracted — bug" row in `docs/16-preload-audit.md` (`06-11`) is
fixed: whenever a queue mutation changes which track is next while a preload for the old next
track is outstanding, loxia cancels that stale preload and issues a fresh one for the actual new
next track, before the stale one can ever be consumed by mpv's gapless transition.

## Files

- `crates/loxia-core/src/effect.rs` — add an effect the reducer can emit to retract/replace a
  preload (e.g. alongside the existing `Preload` effect variant)
- `crates/loxia-core/src/reducer/queue.rs` — detect, for each scenario in `06-11`'s audit, that the
  "next" track changed while a preload for the old one may be outstanding, and emit the new effect
  instead of (or in addition to) the existing preload effect
- `crates/loxia-audio/src/backend.rs` — add the corresponding `AudioCommand` variant (e.g.
  `CancelPreload`) named per `06-11`'s finding on the existing vocabulary
- `crates/loxia-audio/src/gapless.rs` — track which URL/track is currently the outstanding
  preload target and implement cancelling/replacing it
- `crates/loxia-audio/src/mock.rs` — the mock engine implements the new command deterministically,
  so higher-level tests can assert on it without a real mpv instance
- `crates/loxia-audio/src/mpv/handle.rs` — the real mpv-side implementation (clearing whatever
  mechanism `06-06` used to hand mpv the next URL)
- `crates/loxia-player/src/workers/audio.rs` — dispatch the new effect to the new command
- `docs/05-audio-engine.md` — document the retraction contract
- `docs/12-decisions.md` §9 — new row: the stale-preload bug `06-11` found, the fix, and why
  retraction (rather than e.g. always waiting for the in-flight preload to finish before accepting
  a queue mutation) was chosen

## Specification

- The retraction effect/command must be idempotent: issuing it when there is no outstanding
  preload is a no-op, not an error.
- After a retraction, the reducer must emit a fresh preload effect for whatever track is actually
  next post-mutation, using the same effect the existing `06-06` preload path already uses — this
  task does not invent a second preload mechanism, only a way to invalidate the first one before
  it fires.
- The mock engine (`crates/loxia-audio/src/mock.rs`) must expose, for tests, which track (if any)
  it currently considers "preloaded", so a test can assert the stale one was replaced rather than
  merely that some cancel message was sent.

## Acceptance

- `reducer_emits_retraction_when_insert_next_changes_upcoming_track` (`crates/loxia-core/src/reducer/queue.rs`)
- `reducer_emits_retraction_when_remove_changes_upcoming_track` (`crates/loxia-core/src/reducer/queue.rs`)
- `reducer_retraction_is_followed_by_a_fresh_preload_for_the_real_next_track` (`crates/loxia-core/src/reducer/queue.rs`)
- `mock_engine_replaces_stale_preload_on_cancel` (`crates/loxia-audio/src/mock.rs`)
- `gapless_state_tracks_at_most_one_outstanding_preload` (`crates/loxia-audio/src/gapless.rs`)
- `audio_worker_forwards_retraction_effect_to_cancel_command` (`crates/loxia-player/src/workers/audio.rs`)

## Done when

Global DoD in `tasks/README.md`, plus: `docs/05-audio-engine.md` and `docs/12-decisions.md` §9 are
updated in the same PR, and every "not retracted — bug" row identified in `docs/16-preload-audit.md`
is either fixed or explicitly re-marked in that file as out of scope with a reason.
