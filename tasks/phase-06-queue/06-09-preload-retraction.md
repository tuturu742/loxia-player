# 06-09 — Preload retraction

## Status

Confirmed necessary. See
`tasks/confirm-or-refute-the-stale-gapless-preload-after-a-queue-edit.md` for the investigation:
a `Preload` already sent to mpv for a queue entry that a subsequent queue edit demotes from
"next" is never retracted, and mpv will still play it back to back with the current track once it
ends (`gapless-audio`/`prefetch-playlist`). The queue reducer's own `play_order` is unaffected and
correct in isolation — the divergence is between the reducer's queue state and mpv's *playlist*
state, which is mutated only by additive commands.

## Mechanism (re-derived from the actual code, not presumed)

- `crates/loxia-audio/src/backend.rs::AudioCommand` has no variant that removes or replaces an
  already-appended file — `Preload` only appends (confirmed against mpv's own `loadfile ... append`
  in `mpv::handle::apply_command`, and against the "appending... instead of replacing" module doc
  in `gapless.rs`).
- `crates/loxia-core/src/reducer/queue.rs::preload_effects` guards against *re-sending* a
  `Preload` for the same target via `PlayerState.last_preloaded`, but has no path that compares
  the *previous* value of `last_preloaded` against the *new* "next" target to detect that the
  previously-preloaded entry needs to be undone.
- `crates/loxia-player/src/workers/audio.rs`'s effect→command translation is stateless per call —
  it does not remember which `entry_id` a previous `Preload` was for, so it has nowhere to hang a
  retraction even if the core emitted one.

## Fix shape

1. **`crates/loxia-core/src/effect.rs`**: add `Effect::CancelPreload { entry_id: QueueEntryId }`.
2. **`crates/loxia-core/src/reducer/queue.rs::preload_effects`**: change its signature/call sites
   so it is given (or can read) the *previous* `last_preloaded` value before it is overwritten.
   When the newly-computed `next_entry()` id differs from the previous `last_preloaded` and that
   previous value is `Some(_)`, prepend `Effect::CancelPreload { entry_id: <old value> }` to the
   returned effects, ahead of the new `Effect::Preload`. Update `PlayerState.last_preloaded` to
   the new target (or `None` if there is no next entry) in the same step, so state and the emitted
   effects agree.
3. **`crates/loxia-audio/src/backend.rs::AudioCommand`**: add a variant carrying enough
   information to remove the specific pending entry from mpv's playlist — mpv's own
   `playlist-remove <index>` needs an index, not a URL, so the worker (not the core, which does not
   talk to mpv) must be the one that maps `entry_id` to "the playlist slot after the currently
   playing one" at the point it applies the command (that slot is always index `1` relative to
   mpv's own current playlist position immediately after a single prior `Preload`, since nothing
   else appends to mpv's playlist in this design).
4. **`crates/loxia-audio/src/mpv/handle.rs::apply_command`**: handle the new variant with
   `mpv.command("playlist-remove", &["1"])` (or the current-relative equivalent), guarding against
   the case where mpv has already started playing that slot (i.e. the retraction lost the race —
   in which case this becomes a no-op, not an error, since the file is now legitimately playing).
5. **`crates/loxia-player/src/workers/audio.rs`**: translate `Effect::CancelPreload` into the new
   `AudioCommand` variant, and track the `entry_id` of the most recent `Preload` sent so a
   worker-side sanity check can be added later if needed (not required for correctness, since the
   core is the source of truth for *which* id to cancel).

## Acceptance

- `crates/loxia-core/src/reducer/queue.rs::tests::insert_next_after_preload_retargets_next_track`
  (already added, currently `#[ignore = "fixed by 06-09-preload-retraction"]`) passes and the
  `#[ignore]` is removed.
- A new `loxia-audio` unit test asserts `AudioCommand`'s new variant round-trips through
  `mpv::handle::apply_command` into `playlist-remove`.
- The `mpv-tests` gapless real-mpv suite (`gapless.rs`) gains a case: preload track B, then
  retract it and preload track C before A finishes; assert the transition lands on C, not B.
