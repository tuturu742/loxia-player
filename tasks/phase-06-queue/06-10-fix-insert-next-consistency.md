# 06-10 · fix insert-next consistency

**Phase:** 06 — Queue engine · **Agent:** loxia-core · **Size:** M · **Prerequisites:** `06-09` · **Reference:** `docs/02-data-model.md`, `docs/12-decisions.md` §9

## Goal

When this task is done, "insert next" (single track or `QueueBatch`) places its tracks at the same
logical position — immediately after the currently-playing entry in `play_order` — whether or not
shuffle is active, and whether or not the current track belongs to an appears-on group. The
inconsistency `06-09` documented no longer exists.

## Files

- `crates/loxia-core/src/reducer/queue.rs` — the reducer arm(s) handling "insert next"
- `crates/loxia-core/src/queue.rs` — shared insert-position logic, if it lives here rather than
  inline in the reducer
- `crates/loxia-core/src/queue/shuffle.rs` — whatever bookkeeping the shuffle bag/history keeps
  that currently causes a shuffled insert to land somewhere other than "right after current"
- `docs/02-data-model.md` — update the queue-insert description to match the corrected behaviour
- `docs/12-decisions.md` §9 — new row: what the old (inconsistent) behaviour was, what it is now,
  and why the position was standardised on "immediately after current" rather than the shuffled
  variant's prior behaviour

## Specification

Standardise on: an "insert next" mutation — regardless of shuffle state — inserts its track(s)
into `play_order` at the index immediately following the currently-playing entry's index, and (if
shuffle bookkeeping is a separate structure from `play_order` itself) updates that bookkeeping so a
subsequent shuffle re-roll does not immediately displace the just-inserted track before it plays.

This must not change:
- the currently-playing entry itself (no re-trigger, no seek reset),
- the relative order of tracks that were already queued before or after the insertion point,
- `06-01`'s existing single-track insert behaviour for the unshuffled case (that case was already
  correct per `06-09`'s characterization test and must keep passing unmodified).

## Acceptance

- `insert_next_after_current_shuffled` (from `06-09`) is updated in place so its assertion now
  matches `insert_next_after_current_unshuffled`'s position exactly, and its doc-comment is
  rewritten to state the two now agree.
- `insert_next_position_consistent_regardless_of_shuffle` — a new test that runs both the
  unshuffled and shuffled scenarios side by side and asserts the resulting relative position (index
  of the inserted track minus index of the current track) is identical.
- `insert_next_does_not_disturb_currently_playing_entry` — asserts the reducer's `current` pointer
  (or equivalent) and playback position are untouched by an insert-next mutation.
- The other four tests added in `06-09` (`insert_next_into_empty_queue`,
  `insert_next_repeated_calls_preserve_relative_order`,
  `insert_next_batch_lands_together_in_track_order`,
  `insert_next_with_appears_on_grouped_current_track`) still pass unmodified.

## Done when

Global DoD in `tasks/README.md`, plus: `docs/02-data-model.md` and `docs/12-decisions.md` §9 are
updated in the same PR, and no file outside `crates/loxia-core` (and the two docs above) is
touched.
