# 06-09 — Queue-insert characterization tests

## Prerequisites

- `06-01` — Queue state basics
- `06-02` — Appears-on queue rules
- `06-03` — Shuffle
- `06-04` — Sort profiles

## Goal

Lock down the *current, shipped* behaviour of every queue-editing operation — "play next",
"add to queue" (append), and any bulk `QueueBatch`-style insert — as a suite of characterization
tests, before anything about that behaviour is changed. This task adds tests only. It must not
change `crates/loxia-core/src/reducer/queue.rs`, `crates/loxia-core/src/state/queue.rs`,
`crates/loxia-core/src/queue.rs`, or any of `crates/loxia-core/src/queue/{shuffle,sort,appears_on}.rs`
in any way that alters behaviour. If a test in this task fails against the current implementation,
that is the finding — do not "fix" the code to make it pass; report it in the PR description and
leave the test red-marked with `#[ignore = "06-10: insert-next consistency"]` plus a comment
explaining what it caught.

## Context

`06-10` (insert-next consistency fix) and `06-11` (stale-preload audit) both need a known-good
baseline of what "insert next" currently does — including its interaction with an active shuffle
order and with `appears_on`-grouped entries — so that a later behaviour change has something to
diff against. Nothing in the existing `06-01`..`06-08` task suites tests insert position under an
*active* shuffle order specifically; each tests its own feature in isolation.

## Files to touch

- `crates/loxia-core/src/reducer/queue.rs` — tests module only (new `#[cfg(test)]` functions).
- `crates/loxia-core/src/queue/shuffle.rs` — tests module only, for insert-under-shuffle cases.
- `crates/loxia-core/src/test_support/scenario.rs` — extend only if a missing builder helper is
  needed to construct a queue with an active shuffle order plus a pending insert; do not change
  the signature or behaviour of an existing helper.

## Specification

Write characterization tests for each of the following, each asserting today's *actual* observed
output (captured by running the existing code, not derived from the spec in `docs/`):

1. **Insert-next on an unshuffled queue.** Playing track at index `i`; "play next" on track `t`
   lands immediately after `i`, before whatever was previously at `i+1`.
2. **Insert-next twice in a row.** Two consecutive "play next" calls on `t1` then `t2`: whichever
   order they land in relative to each other and to the (moving) play cursor is recorded exactly
   as observed, not as assumed.
3. **Insert-next while a shuffle order is active.** Does the insert go into the *shuffle* order
   immediately after the current position, or into the underlying unshuffled order (potentially
   becoming reachable much later, or not until the shuffle re-rolls)? Record the actual answer.
4. **Insert-next on an `appears_on`-grouped queue.** Does the inserted track break a group's
   contiguity, and if so how does `appears_on.rs`'s rendering/grouping logic react?
5. **Append ("add to queue") interaction with a pending insert-next.** Order of the two relative
   to each other.
6. **Bulk insert (`QueueBatch`, or whatever the current multi-track "add these next" path is
   named in `reducer/queue.rs`)** — same four questions as above, for a batch of 3+ tracks: are
   they inserted as a contiguous block in list order, reversed, or interleaved with existing
   post-cursor tracks?

Each test must name, in a doc-comment directly above it, which of the above numbered scenarios it
covers, so `06-10`'s PR can cite it directly.

## Acceptance

- `queue_insert_next_lands_immediately_after_cursor_unshuffled`
- `queue_insert_next_twice_in_a_row_order`
- `queue_insert_next_under_active_shuffle_order`
- `queue_insert_next_preserves_or_breaks_appears_on_grouping` (name whichever it actually does)
- `queue_append_after_pending_insert_next`
- `queue_batch_insert_next_block_order`
- Every test above exists, passes against the current implementation (or is `#[ignore]`d with a
  named reason if it fails — see Goal), and the PR description states plainly, in prose, what each
  one actually found.
