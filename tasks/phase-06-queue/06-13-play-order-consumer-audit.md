# 06-13 · `play_order` consumer audit

**Phase:** 06 — Queue engine · **Agent:** `loxia-core` · **Size:** S · **Prerequisites:** `06-01`, `06-10` · **Reference:** `docs/12-decisions.md` §9

## Goal

Enumerate every reader of `QueueState`'s `play_order` inside `loxia-core` and confirm each still
holds after `06-10` redefined what "insert next" does to it. Separately, list — without touching —
every out-of-crate consumer (`loxia-tui`'s Now Playing/queue rendering, `loxia-player`'s workers)
so a future task has a ready-made checklist instead of having to grep cold.

## Files

- `crates/loxia-core/src/state/queue.rs` (tests module only)
- `crates/loxia-core/src/reducer/nav.rs` (tests module only, if it reads `play_order` for
  "jump to track")
- `crates/loxia-core/src/reducer/queue.rs` (tests module only)
- `docs/12-decisions.md` (§9 — one row listing the out-of-crate consumers found)

## Specification

No production code changes anywhere, in any crate.

In `loxia-core`: for every internal reader of `play_order` (advance-to-next-track,
jump-to-index, the "currently playing index" accessor, and history recording on advance), add or
extend a test asserting it still returns the correct track immediately after an insert-next, per
`06-10`'s corrected semantics.

Separately, without modifying anything outside `loxia-core`: grep `crates/loxia-tui` and
`crates/loxia-player` for `play_order` and list every hit (file and a one-line context, not a full
analysis) in the `docs/12-decisions.md` §9 row for this task, flagged for follow-up if a hit looks
like it assumes the pre-`06-10` behaviour. This task does not fix any such hit — it only lists
them, so `06-12`'s cross-crate precedent is not repeated for something that might not need a code
change at all.

## Acceptance

- `advance_to_next_reads_play_order_post_insert_next`
- `jump_to_index_reads_play_order_post_insert_next`
- `currently_playing_index_stable_across_insert_next`
- `history_records_correct_track_after_insert_next`
- `docs/12-decisions.md` §9 contains the out-of-crate consumer list for this task's ID

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test above exists and passes
- [ ] No production code changed in any crate
- [ ] `docs/12-decisions.md` §9 updated with the consumer list
- [ ] This task's checkbox ticked in `tasks/README.md`
