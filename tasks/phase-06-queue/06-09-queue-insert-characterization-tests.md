# 06-09 · Queue-insert characterization tests

**Phase:** 06 — Queue engine · **Agent:** `loxia-core` · **Size:** S · **Prerequisites:** `06-01`, `06-03` · **Reference:** `docs/12-decisions.md` §9

## Goal

Pin down, with tests, the actual current behaviour of "insert next" in both an unshuffled and a
shuffled queue, including repeated inserts and an insert immediately followed by a shuffle toggle
— so `06-10`'s fix has a documented, tested baseline to diverge from, and a decision-log entry can
cite exactly what changed.

This task adds **no production code changes** — it characterizes what exists today, correct or
not.

## Files

- `crates/loxia-core/src/reducer/queue.rs` (tests only)
- `crates/loxia-core/src/queue.rs` (tests only)

## Specification

Add characterization tests that drive whatever insert-next action/reducer entry point already
exists in `reducer/queue.rs` (match its real name and signature; do not invent a new one) and
assert the queue's actual resulting state for each of the following. Every assertion records
*current* behaviour, not desired behaviour — if a case is wrong, mark it with a `// BUG:` comment
explaining what's wrong and what `06-10` is expected to change, rather than "fixing" it here.

- Insert-next on an unshuffled queue: the inserted track's position relative to
  `current_index`.
- Insert-next on a shuffled queue: the inserted track's position in `play_order` relative to
  `current_index`, and whether it matches or diverges from the unshuffled case's index semantics.
- Three consecutive insert-next calls with no playback advancing in between: the resulting
  relative order of the three inserted tracks.
- An insert-next immediately followed by a shuffle toggle: whether the inserted track keeps its
  "plays next" slot or is treated as an ordinary shuffle candidate.
- Insert-next on an empty queue: the inserted track becomes current.

## Acceptance

- `insert_next_position_when_unshuffled`
- `insert_next_uses_play_order_when_shuffled`
- `insert_next_multiple_calls_ordering`
- `insert_next_then_shuffle_toggle_position`
- `insert_next_on_empty_queue`

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] No `docs/12-decisions.md` row added by this task — it changes no behaviour; `06-10` records
      the deviation once the behaviour actually changes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
