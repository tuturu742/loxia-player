# 06-10 — Insert-next consistency fix

## Prerequisites

- `06-09` — Queue-insert characterization tests

## Goal

Make "play next" behave the same way regardless of whether a shuffle order is active: the
inserted track must always land immediately after the currently-playing position *in the order
the user is actually looking at* (the active order — shuffled or not), never silently falling back
to the underlying unshuffled order. Fix whichever of `06-09`'s `#[ignore]`d tests exposed a
divergence; if `06-09` found no divergence, this task still tightens the assertions the
`#[ignore]` markers stood in for, converts them to real `#[test]`s, and closes with no code change
beyond that.

## Context

`06-09` characterizes today's behaviour without changing it. This task is the fix for whatever
inconsistency that characterization surfaced — most likely: insert-next writing into the
unshuffled base order while the shuffle order is what's actually consulted for "what plays next",
so a "play next" track can vanish for the rest of the shuffled session, or two consecutive
"play next" calls landing in the wrong relative order once a shuffle is active.

## Files to touch

- `crates/loxia-core/src/reducer/queue.rs` — the insert-next handling.
- `crates/loxia-core/src/queue/shuffle.rs` — only if the fix requires the shuffle order itself to
  expose an "insert immediately after current position" operation rather than reindexing.
- `crates/loxia-core/src/state/queue.rs` — only if `QueueState`'s own invariants need documenting
  or a new field is required to track "which order is authoritative for insert-next".

## Specification

- Un-ignore every test `06-09` marked `#[ignore = "06-10: insert-next consistency"]`, and make
  each pass for real — not by relaxing the assertion, by fixing the insert path.
- "Play next" must insert into whichever order (`shuffle::ShuffleOrder` when a shuffle is active,
  else the base queue order) is authoritative for "what plays after the current track" at the
  moment of insertion, and must not require a shuffle re-roll to become reachable.
- Two consecutive "play next" invocations must produce a stable, tested relative order (this task
  picks one and documents it in a doc-comment on the insert function — e.g. "last called wins the
  earliest slot" — since `06-09` found the current behaviour undocumented, not that either
  ordering is wrong per se).
- Appears-on grouping (`06-02`) must not be silently broken by an insert-next; if the fix cannot
  preserve grouping, it must explicitly ungroup the affected block rather than leave a
  partially-grouped, unlabelled run of tracks.

## Acceptance

- All tests un-ignored from `06-09` pass.
- `insert_next_reachable_immediately_under_active_shuffle` — a "play next" track is the very next
  track consumed when advancing, with a shuffle order active, with no re-roll in between.
- `insert_next_relative_order_is_documented_and_tested` — two consecutive "play next" calls match
  the doc-commented rule.
- `insert_next_appears_on_grouping_explicit` — grouping is either preserved or explicitly broken,
  never left inconsistent with what `appears_on.rs` renders.
- No public API of `loxia-tui` or `loxia-audio` needed to change for this fix (queue insert
  behaviour is entirely a `loxia-core` reducer concern) — if that turns out to be false, this task
  is blocked, not silently expanded; stop and split out a new task instead.
