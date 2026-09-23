# 06-09 · Queue insert characterization tests

**Phase:** 06 — Queue engine · **Agent:** core · **Size:** S · **Prerequisites:** 06-01, 06-03 ·
**Reference:** docs/02-data-model.md, docs/04-state-and-input.md

## Goal

Before touching any queue-insert behaviour, pin down what it actually does today. The
investigation that produced this task found queue-insert paths cited from several unrelated call
sites (appears-on insertion, instant mix, "play next") with no single test suite proving they
agree with each other or with the shuffle order. This task adds tests that record the *current*,
possibly-inconsistent behaviour, so `06-10` has a concrete, named regression to fix against rather
than a vague complaint.

This task changes no production code. If a test in this task fails against current behaviour
because that behaviour is already correct, that's fine — the point is coverage, not a predetermined
outcome. If a test demonstrates an inconsistency, the assertion should still describe the actual,
current output and carry a `// characterization:` comment saying so; `06-10` is where the fix
(and the corresponding assertion changes) happen.

## Files

- `crates/loxia-core/src/reducer/queue.rs` (`#[cfg(test)]` module only)
- `crates/loxia-core/src/queue.rs` (`#[cfg(test)]` module only)
- `crates/loxia-core/src/queue/shuffle.rs` (`#[cfg(test)]` module only)

No non-test line may change in this task.

## Specification

Add characterization tests covering, at minimum:

- Inserting a track to play next, unshuffled: the track lands immediately after the currently
  playing index and before what was previously next.
- Inserting a track to play next while shuffled: whether the insert targets the *shuffled* play
  order or the underlying unshuffled order (record whichever it currently is).
- Two consecutive "insert next" calls: whether the second inserted track ends up immediately after
  the first (stack order) or immediately after the original current track (queue order) — record
  whichever it currently is.
- Whatever call site currently backs "insert next" from more than one place (e.g. appears-on
  insertion vs. a generic queue-insert reducer path) is exercised from each call site with the same
  starting state, to check whether they currently agree.

Each test asserts on the real, current output — not on what the spec *should* say.

## Acceptance

- `insert_next_places_after_current_track_unshuffled`
- `insert_next_position_when_shuffled`
- `repeated_insert_next_relative_order`
- `insert_next_agrees_across_call_sites` (may assert current disagreement; the assertion itself,
  and its `// characterization:` comment, is the deliverable)

## Done when

See the Global Definition of Done in `tasks/README.md`, plus:
- every test named above exists and passes
- `git diff` for this task touches only `#[cfg(test)]` modules in the three files listed above
