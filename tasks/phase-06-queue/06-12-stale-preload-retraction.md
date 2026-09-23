# 06-12 · Stale preload retraction

**Phase:** 06 — Queue engine · **Agent:** core, audio, player (crosses crate boundaries; see
authorisation below) · **Size:** M · **Prerequisites:** 06-11 ·
**Reference:** docs/05-audio-engine.md, docs/12-decisions.md

## Goal

Fix every staleness case `06-11`'s audit found: when a queue mutation invalidates an
already-issued preload, retract it (and, if there is a new correct next track, preload that
instead) before mpv reaches the end of the currently-playing track — so a user who inserts a
track to play next, removes the queued-next track, or reshuffles never hears the stale track play
gaplessly anyway.

## Crate-boundary authorisation

**This task explicitly authorises crossing `loxia-audio`, `loxia-core`, and `loxia-player` in one
PR**, per `CONTRIBUTING.md`'s rule that a task must say so to permit it. The reason a
single-crate change cannot close the gap `06-11` documented:

- The state that *knows* a preload is now stale — the queue, and the mutation that just happened
  to it — lives in `loxia-core`'s reducer.
- The engine that *holds* the stale preload — mpv's internal playlist, via `AudioBackend` — lives
  in `loxia-audio`.
- The wiring that turns "the reducer decided a preload is stale" into "the audio backend received
  a command about it" — the effect dispatcher and audio worker — lives in `loxia-player`.

Detecting staleness without a way to act on it (core-only), or adding a retraction command with no
caller that ever knows to send it (audio-only), each leave the bug exactly as `06-11` found it.
All three are required for one working fix.

## Files

- `crates/loxia-core/src/reducer/queue.rs`
- `crates/loxia-core/src/effect.rs`
- `crates/loxia-audio/src/backend.rs`
- `crates/loxia-audio/src/gapless.rs`
- `crates/loxia-audio/src/mpv/handle.rs`
- `crates/loxia-player/src/workers/audio.rs`
- `crates/loxia-player/src/dispatch.rs`
- `docs/audits/06-11-stale-preload-audit.md` (append a "Resolved by `06-12`" note per finding)

## Specification

- Add an effect (e.g. `Effect::Audio::CancelPreload`) emitted by the reducer whenever a queue
  mutation invalidates the currently-preloaded next track. If the mutation also produces a new,
  correct next track, emit the existing preload effect for it in the same step so there is no gap
  where nothing is preloaded.
- Add the corresponding `AudioCommand` variant. In the real mpv backend, this must remove the
  appended-but-not-yet-playing playlist entry (mpv's own `playlist-remove`, targeting the
  *next* index, never the currently-playing one) rather than doing anything that could disturb
  playback in progress.
- Wire the new effect through `loxia-player`'s dispatch and audio worker, exactly as the existing
  `Preload` effect already is.
- Update `06-11`'s reproduction tests: staleness must no longer occur, so their
  `// characterization:` comments become `// regression:` comments describing the fix, and their
  assertions flip to the now-correct outcome.

## Acceptance

- `cancel_preload_effect_emitted_on_stale_insert_next`
- `cancel_preload_effect_emitted_on_stale_removal`
- `mock_engine_playlist_remove_targets_next_not_current`
- `dispatch_forwards_cancel_preload_to_audio_worker`
- `06-11`'s `preload_goes_stale_after_insert_next` and `preload_goes_stale_after_removal` are
  updated in place and now assert the stale preload is retracted, not left in place.

## Done when

See the Global Definition of Done in `tasks/README.md`, plus:
- all tests named above exist and pass
- `docs/audits/06-11-stale-preload-audit.md` has a "Resolved by `06-12`" note against every
  finding it previously recorded as unresolved
