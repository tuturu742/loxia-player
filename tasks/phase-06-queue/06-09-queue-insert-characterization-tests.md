# 06-09 · Queue-insert characterization tests

**Phase:** 06 — Queue engine · **Agent:** `loxia-core` · **Size:** S · **Prerequisites:** `06-01`, `06-02`, `06-03` · **Reference:** `docs/12-decisions.md` §9

## Goal

Pin down, in tests, exactly what `reducer::queue`'s "insert next" and "add to end" paths do
*today* — including whatever they do when the queue is shuffled — before `06-10` is allowed to
change any of it. When this task is done, every current insert-position behaviour (correct or
not) has a named test asserting it, so `06-10`'s diff is reviewable as "these tests changed
because the behaviour changed", not "we hope we didn't break something nobody tested."

## Files

- `crates/loxia-core/src/reducer/queue.rs` (tests module only — no non-test changes)
- `crates/loxia-core/src/queue.rs` (tests module only, if `QueueBatch`/`play_order` helpers are
  exercised directly rather than only through the reducer)
- `crates/loxia-core/src/queue/shuffle.rs` (tests module only, for the shuffle+insert interaction
  case)

## Specification

No production code changes anywhere. This task is characterization only — if a test reveals
behaviour that looks wrong, it is still asserted exactly as observed (with a `// BUG:` comment
pointing at `06-10`), never silently fixed here.

Cover, at minimum:

1. **Insert-next on an unshuffled queue** — the inserted track lands immediately after the
   currently-playing index.
2. **Insert-next called twice in a row** — record the actual resulting order of the two inserted
   tracks relative to each other and to the current track, whichever order today's code produces.
3. **Insert-next while the queue is shuffled** — record whether the insertion uses the shuffled
   `play_order` position or the pre-shuffle source order. This is the specific fact `06-06`'s
   gapless preload and `06-10`'s fix both need to know precisely.
4. **Insert-next on an empty queue** — the inserted track becomes the current track.
5. **Enqueue-at-end** — the track lands after the last entry of `play_order`, not the last entry
   of the unshuffled source list, on a queue where the two differ.
6. **Insert-next immediately followed by a shuffle toggle** — record whether the just-inserted
   track keeps its "plays next" position or gets redistributed like any other queued track.

## Acceptance

- `insert_next_places_track_after_current_unshuffled`
- `insert_next_repeated_calls_stack_order`
- `insert_next_uses_play_order_when_shuffled`
- `insert_next_on_empty_queue_becomes_current`
- `enqueue_end_appends_after_play_order_tail`
- `insert_next_then_shuffle_toggle_position`

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] No production code in `reducer/queue.rs`, `queue.rs`, or `queue/shuffle.rs` changed
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
