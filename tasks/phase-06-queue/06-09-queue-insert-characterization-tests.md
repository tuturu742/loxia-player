# 06-09 · queue-insert characterization tests

**Phase:** 06 — Queue engine · **Agent:** loxia-core · **Size:** S · **Prerequisites:** `06-01`, `06-02`, `06-03` · **Reference:** `docs/02-data-model.md` (queue model), `docs/12-decisions.md`

## Goal

When this task is done, `crates/loxia-core/src/reducer/queue.rs` has a dedicated test module that
pins down exactly where an "insert next" mutation places a track in the queue's `play_order` today,
across every combination of shuffle on/off, single-track and `QueueBatch` (multi-track) inserts,
and an appears-on-grouped current track. No production code changes — this is the safety net
`06-10` uses to prove its fix and to prevent the same inconsistency from silently coming back.

## Files

- `crates/loxia-core/src/reducer/queue.rs` — new `#[cfg(test)] mod insert_next_characterization`
- `crates/loxia-core/src/test_support/scenario.rs` — add a builder helper only if one does not
  already exist for constructing a queue that is mid-shuffle with a known, fixed seed
- `crates/loxia-core/src/test_support/fixtures.rs` — add fixture tracks only if the existing set
  cannot express an appears-on-grouped current track

## Specification

Do not change any non-test code in this task. Every test below asserts on the concrete resulting
`play_order` (or whatever the current field literally is — read `state/queue.rs` first and use its
real name and shape, not an assumed one), not merely on `queue.len()` or "the track is present
somewhere". That concreteness is the point of a characterization test: it records what the code
does today, bug included, so the next task can prove it changed exactly that and nothing else.

Cover:

1. Inserting a single track next into an empty queue.
2. Inserting a single track next into a non-empty, unshuffled queue, immediately after the
   currently-playing entry.
3. The same insertion into a shuffled queue. Document in the test's own doc-comment, in one
   sentence, whether the resulting position matches case 2 or differs from it (it is expected to
   differ — that is the inconsistency this whole task chain exists to fix).
4. Three successive single-track "insert next" calls, asserting the three inserted tracks end up
   immediately after the current track in the order they were issued (not reversed, not
   interleaved with pre-existing queue entries).
5. Inserting a `QueueBatch` (an album's worth of tracks queued as "play next" in one action) next,
   asserting the whole batch lands together, immediately after the current track, in track order.
6. Inserting a single track next while the currently-playing entry belongs to an appears-on group
   (`06-02`), documenting where the insertion lands relative to that group's boundary.

## Acceptance

- `insert_next_into_empty_queue`
- `insert_next_after_current_unshuffled`
- `insert_next_after_current_shuffled`
- `insert_next_repeated_calls_preserve_relative_order`
- `insert_next_batch_lands_together_in_track_order`
- `insert_next_with_appears_on_grouped_current_track`

All six exist in `crates/loxia-core/src/reducer/queue.rs`, pass against the current
implementation unmodified, and `insert_next_after_current_shuffled`'s doc-comment states in one
sentence what, if anything, differs from `insert_next_after_current_unshuffled`.

## Done when

Global DoD in `tasks/README.md`, plus: no file outside `crates/loxia-core` is touched, and no
non-test line is added to or removed from any file this task modifies.
