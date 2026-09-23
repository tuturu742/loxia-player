# 06-13 · Play-order consumer audit

**Phase:** 06 — Queue engine · **Agent:** `loxia-core` · **Size:** S · **Prerequisites:** `06-01`, `06-10` · **Reference:** `docs/12-decisions.md` §9

## Goal

Inventory every place in `loxia-core` that reads `QueueState::play_order` (or indexes into it)
directly, rather than through the current-index/next-index accessor(s), so a future queue-
semantics change has a complete, tested list of call sites to check instead of relying on memory
— exactly the kind of gap that let `06-09`/`06-10`'s bug exist unnoticed.

This task makes **no behaviour change**.

## Files

- `crates/loxia-core/src/state/queue.rs`
- `crates/loxia-core/src/queue.rs`
- `crates/loxia-core/src/reducer/queue.rs` (tests + doc comment only)

## Specification

- Add a doc comment on `QueueState::play_order`'s definition inventorying every consumer (module
  and function) that reads it directly, distinguishing "reads via the current-index/next-index
  helper(s)" from "reads the raw `Vec` and computes its own offset" — the second category is what
  a future change is most likely to break silently.
- Add a grep-style test, mirroring `loxia-audio::device::cfg_blocks_confined_to_device_modules`,
  asserting that every direct index/offset computation against `play_order` outside the accessor
  helper(s) is confined to the modules the doc comment names, so an unreviewed new direct consumer
  added later fails CI instead of becoming another undocumented assumption.

## Acceptance

- `play_order_direct_consumers_confined_to_documented_modules`
- `doc_comment_lists_at_least_the_reducer_and_state_modules`

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] No `docs/12-decisions.md` row required (pure audit, no behaviour change) unless the audit
      itself surfaces a needed behaviour change, in which case add one
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
