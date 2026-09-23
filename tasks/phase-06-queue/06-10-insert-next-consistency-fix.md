# 06-10 · Insert-next consistency fix

**Phase:** 06 — Queue engine · **Agent:** `loxia-core` · **Size:** M · **Prerequisites:** `06-09` · **Reference:** `docs/12-decisions.md` §9

## Goal

Make "insert next" behave identically whether the queue is shuffled or not: a single inserted
track always lands immediately after the *currently playing* position in `play_order`, and
repeated inserts stack in a single well-defined order. When this task is done, only `06-09`'s
characterization test(s) for the cases that conflict with this specification are the ones that
changed (and are documented as a deviation); everything else `06-09` pinned down is unchanged.

## Files

- `crates/loxia-core/src/reducer/queue.rs`
- `crates/loxia-core/src/queue.rs`
- `docs/12-decisions.md` (§9 — add the row for this behaviour change, per `CONTRIBUTING.md`
  workflow item 4)

## Specification

- "Insert next" always operates on `play_order`, never on the unshuffled source list. For a
  single insert-next call, the inserted track's `play_order` index is `current_index + 1`,
  whether the queue is shuffled or not. (If unshuffled, `play_order` and source order coincide, so
  this is unobservable in that case; if shuffled, it is the fix.)
- Calling insert-next repeatedly without playback advancing stacks the inserted tracks in **call
  order**: the k-th consecutive insert (k starting at 0) lands at `current_index + 1 + k`, and
  earlier inserts are not displaced. Concretely, the first call's track lands at
  `current_index + 1`; a second call's track lands at `current_index + 2`, immediately after the
  first, not before it. This is deliberate: queueing "play next" a second time should extend the
  line you just started, not jump the first one — the alternative (each new insert taking
  `current_index + 1` and pushing the previous insert one slot later) makes the *last* thing you
  queued next play soonest, which is the opposite of what "play next" means when you do it more
  than once in a row.
- Insert-next on an empty queue still makes the inserted track current, unchanged from `06-09`.
- A shuffle toggle immediately after an insert-next does **not** treat the just-inserted track as
  a shuffle candidate for repositioning relative to the current track — it keeps its "plays next"
  slot. If `06-09` recorded the opposite as today's actual behaviour, that finding is what this
  task changes.
- Update whichever `06-09` test(s) recorded behaviour that conflicts with this specification
  (`insert_next_uses_play_order_when_shuffled` and/or `insert_next_then_shuffle_toggle_position`)
  to assert the corrected behaviour, and remove their `// BUG:` comments. Every other `06-09` test
  must still pass unmodified.

## Acceptance

- All tests from `06-09` still exist and pass (updated only where this task's spec requires it)
- `insert_next_stacks_contiguously_in_call_order` — asserts that after N consecutive insert-next
  calls with no playback advancing, the k-th inserted track (k from 0) sits at
  `current_index + 1 + k` in `play_order`, in the order the calls were made
- `insert_next_position_identical_shuffled_and_unshuffled`
- A new row in `docs/12-decisions.md` §9 recording the deviation: which prior behaviour it
  replaces (citing the specific `06-09` test name(s) that changed), and why the old behaviour was
  wrong per this task's spec

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] `docs/12-decisions.md` §9 updated in the same PR
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
