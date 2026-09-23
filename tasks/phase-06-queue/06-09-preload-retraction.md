# 06-09 — Retract a stale gapless preload after a queue edit

## Status

Not started. Filed as the fix for the bug **confirmed** by
`tasks/confirm-or-refute-the-stale-gapless-preload-after-a-queue-edit.md` — read that finding
first; this task exists specifically because the bug was not refuted.

## Problem

`reducer::queue::preload_effects` (`crates/loxia-core/src/reducer/queue.rs`) sends
`AudioCommand::Preload` for `play_order[position + 1]` and records the target in
`PlayerState.last_preloaded` (`crates/loxia-core/src/state/player.rs`) so it is never re-sent for
the same entry. `mpv::handle::apply_command` (`crates/loxia-audio/src/mpv/handle.rs`) turns that
into an **append** to mpv's own internal playlist, per `crates/loxia-audio/src/gapless.rs`'s
module doc: `Preload` appends "instead of replacing the current file". Nothing removes that
appended entry if the queue is edited (insert-next, remove-next, reorder, clear) before it is ever
played, because `AudioCommand` (`crates/loxia-audio/src/backend.rs`) has no variant capable of
expressing "forget that preload" — its full variant list is `Load`, `Preload`, `Play`, `Pause`,
`Stop`, `Seek`, `SetVolume`, `SetMute`, `SetEq`, `SetReplayGain`, `SetDevice`,
`EnumerateDevices`, `Shutdown`.

Net effect: mpv's playlist and `loxia-core`'s queue state can disagree about what plays next, and
mpv may gaplessly advance into a track the user no longer has queued.

## Fix shape (for whoever picks this up)

1. **`crates/loxia-audio/src/backend.rs`**: add an `AudioCommand` variant that identifies a
   specific pending preload to drop — e.g. `RetractPreload { url: RedactedUrl }` (matching by URL,
   since that's what was sent, mirroring `Preload`'s own shape) — or, if mpv's playlist index is
   stable enough to reason about, a positional `ClearPreload`. Prefer URL-matching over index-
   matching unless `mpv::handle` already tracks the exact playlist index a given preload landed at.
2. **`crates/loxia-audio/src/mpv/handle.rs`** (`apply_command`): translate the new variant into
   mpv's `playlist-remove` command (or equivalent) targeting the previously-appended, not-yet-
   current entry — never the currently-playing one.
3. **`crates/loxia-core/src/reducer/queue.rs`** (`preload_effects`): when
   `play_order[position + 1]` no longer matches `PlayerState.last_preloaded`, emit a retraction
   effect for the stale target *in addition to* the existing preload effect for the new target,
   and only then update `last_preloaded`. Also handle the "no next entry any more" case (e.g. the
   queue shrank to one item) the same way — retract, don't just stop preloading.
4. Extend `crates/loxia-audio/src/mock.rs`'s `MockEngine`/`MockControl` to record and expose
   retractions, mirroring how it already records `AudioCommand`s, so higher-level tests can assert
   on retraction without a real mpv instance.
5. Fold `crates/loxia-core/tests/stale_preload_regression.rs`'s
   `insert_next_after_preload_retargets_next_track` into `reducer::queue`'s own `#[cfg(test)] mod
   tests`, remove its `#[ignore]`, and update its "no retraction effect exists" assertion to assert
   the *opposite* — that the specific retraction effect for `stale_next` is present.

## Acceptance

- `insert_next_after_preload_retargets_next_track` passes without `#[ignore]`.
- A new `reducer::queue` test covers remove-next and reorder edits retargeting away from a
  preloaded entry, not just insert-next.
- `crates/loxia-audio/src/mpv/handle.rs` gains a unit test (and, gated behind `mpv-tests`, a real-
  mpv test alongside `gapless.rs`'s existing ones) proving a retracted entry is actually removed
  from mpv's playlist and is not what plays next.
- `docs/05-audio-engine.md` §2's `AudioCommand`/`AudioEvent` table and `docs/06-cache-and-offline.md`
  or `docs/05-audio-engine.md`'s gapless section (wherever `06-06`'s original preload behaviour is
  documented) are updated to describe retraction; add a row to `docs/12-decisions.md` §9 noting the
  original `06-06` preload design had no retraction path and why one was added.
