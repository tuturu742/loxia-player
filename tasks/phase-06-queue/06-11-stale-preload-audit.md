# 06-11 · Stale-preload audit

**Phase:** 06 — Queue engine · **Agent:** `loxia-core` · **Size:** S · **Prerequisites:** `06-06`, `06-10` · **Reference:** `docs/12-decisions.md` §9

## Goal

Produce a written, tested inventory of every queue mutation that can run while a track is
gapless-preloaded (`AudioCommand::Preload` in `loxia-audio::backend`) and record, for each,
whether anything today tells the audio engine that the preloaded track is no longer the right
one. This is audit-only: it documents the gap for `06-12` to close; it does not close it.

## Files

- `crates/loxia-core/src/reducer/queue.rs` (tests module only — no non-test changes)
- `docs/12-decisions.md` (§9 — one row recording the audit's findings table)

## Specification

No production code changes anywhere in the workspace. For each of the following reducer actions,
add a characterization test asserting *what effect (if any) is currently emitted* that could
reach `loxia-audio`'s preload machinery:

1. Insert-next (post-`06-10` semantics)
2. Remove-from-queue, specifically removing the track that is currently the preload target
3. Clear-queue while a preload target exists
4. Shuffle toggle (on or off) while a preload target exists
5. Sort-profile change while a preload target exists

For each, the test asserts today's actual emitted effect set, which may well be empty — that is
the finding, not a test failure.

The `docs/12-decisions.md` §9 row must be a table with columns: mutation, effect emitted today,
consequence if none (i.e. what plays next instead of what the listener now expects), and a
pointer to `06-12` as the fix.

## Acceptance

- `insert_next_does_not_signal_preload_target_change` (name reflects the actual finding; if the
  audit finds an effect *is* already emitted for this case, name it
  `insert_next_signals_preload_target_change` instead and record that in the findings table)
- `remove_from_queue_does_not_signal_preload_target_change`
- `clear_queue_does_not_signal_preload_target_change`
- `shuffle_toggle_does_not_signal_preload_target_change`
- `sort_profile_change_does_not_signal_preload_target_change`
- A findings table exists in `docs/12-decisions.md` §9, citing this task's ID and every test above
  by name

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test above exists and passes
- [ ] No production code changed anywhere in the workspace
- [ ] `docs/12-decisions.md` §9 updated with the findings table
- [ ] This task's checkbox ticked in `tasks/README.md`
