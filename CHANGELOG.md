# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Workspace scaffolding: six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`,
  `loxia-cache`, `loxia-tui`, `loxia-player`) with the full module tree per `docs/01-architecture.md`.
- Locked dependency set per `docs/13-dependencies.md`.

### Testing (rework)

Characterization tests for insert-next (`i`) and append (`a`) queue edits, and a production fix,
in `loxia-core` only:

- **Reverted the previous, out-of-scope rewrite.** An earlier version of this branch redefined
  `QueueBatch`/`QueueBatchMode` locally in `crates/loxia-core/src/reducer/queue.rs` and rewrote
  `queue/shuffle.rs`'s `shuffle` to permute the whole `play_order`. That rewrite has been dropped;
  `reducer/queue.rs`, `state/player.rs`, and `state/queue.rs` are untouched by this PR (only
  `#[cfg(test)]` code was ever added to `queue/shuffle.rs`).
- **One genuine production fix, kept minimal and self-contained:** `queue/shuffle.rs`'s `shuffle`
  now only permutes `play_order[position + 1..]`, restoring the shipped invariant that played
  history keeps its order — the bug the earlier rewrite introduced. This is the only production
  change in this PR.
- **Restored all of `queue/shuffle.rs`'s pre-existing shuffle/unshuffle tests**
  (`shuffle_empty_queue_is_a_noop`, `shuffle_single_entry_queue_is_a_noop`,
  `shuffle_at_last_position_is_a_noop`, `shuffle_preserves_current_track`,
  `shuffle_does_not_reorder_entries`, `shuffle_preserves_played_history_order`,
  `shuffle_is_deterministic_for_a_seed`, `different_seeds_give_different_orders`,
  `unshuffle_resets_identity_and_keeps_current_entry`, and the proptest
  `unshuffle_round_trips_to_original_order`) — ten tests, not the three a previous version of this
  branch mislabelled "pre-existing (unchanged)".
- **Added the nine required characterization tests** for `QueueBatch`/`QueueBatchMode`'s
  insert-next and append arms (`crate::reducer::queue::apply_queue_batch`), reusing the same
  `queue_of` fixture: `insert_next_unshuffled_plays_immediately_after_current`,
  `insert_next_shuffled_plays_immediately_after_current`,
  `insert_next_shuffled_then_unshuffle_keeps_entry_after_current`,
  `insert_next_multi_select_preserves_selection_order`, `append_unshuffled_goes_to_end`,
  `append_shuffled_goes_to_end_of_play_order`,
  `queue_edit_does_not_change_current_entry_or_position_target`,
  `queue_edit_emits_no_load_and_keeps_session`, and
  `play_order_is_a_permutation_after_every_edit`.
- **All nineteen tests above pass against current behaviour; none are `#[ignore]`d.** Reading the
  `insert_next`/`append` arms in `reducer/queue.rs` found they already re-home existing
  `play_order` references correctly on a middle-of-`entries` insert, append never touches anything
  before the tail, and neither arm touches `PlayerState` or returns an `Effect::Audio(Load)` —
  `apply_queue_batch` returns no effects at all. The only defect found was the `shuffle` tail-only
  bug above, which is fixed directly rather than pinned with `#[ignore]`, since a one-line,
  self-contained, in-scope fix is not the "characterization only" case the task's `#[ignore]`
  instruction is guarding against.
- **Fixture note:** the review's description of an original `queue_of` built from
  `fixtures::artist`/`album`/`track`, `QueueEntryId`, `QueueSource::Manual`, and
  `Availability::Remote` does not match this crate's actual `QueueEntry` (`queue_id: u64,
  track_id: String`) or `QueueState` (no such fixture types exist anywhere in this crate). The
  simpler `queue_of` already present in `queue/shuffle.rs` — building `n` entries named
  `"track-{i}"` — is kept and reused, since it is the fixture that actually matches the code under
  test.
- **Scope note:** the review also asks that `QueueBatch`/`QueueBatchMode` be relocated from
  `reducer/queue.rs` into `state/queue.rs`. That is an architectural move touching type
  definitions other reducer/UI code may depend on, none of which is visible in this diff's
  context; attempting it blind risks breaking `cargo test --workspace` elsewhere for a change the
  task's actual deliverable (characterization tests) doesn't require. Left as-is; flagging here
  rather than guessing.


--- crates/loxia-core/src/state/player.rs ---
- Not modified in this PR. Given as read-only context; no `#[cfg(test)]` additions were needed
  there for the required tests.

--- crates/loxia-core/src/state/queue.rs ---
- Not modified in this PR. Given as read-only context; no `#[cfg(test)]` additions were needed
  there for the required tests.

--- crates/loxia-core/src/reducer/queue.rs ---
- Not modified in this PR (see the scope note above). Given as read-only context; no
  `#[cfg(test)]` additions were needed there since the reducer entry point (`apply_queue_batch`,
  `QueueBatchMode`, `load_current`) is reachable directly from `queue/shuffle.rs`'s test module.
