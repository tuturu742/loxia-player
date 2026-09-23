# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Workspace scaffolding: six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`,
  `loxia-cache`, `loxia-tui`, `loxia-player`) with the full module tree per `docs/01-architecture.md`.
- Locked dependency set per `docs/13-dependencies.md`.
- Characterization tests for insert-next (`i`) and append (`a`) queue edits (`QueueBatch` /
  `QueueBatchMode` in `crates/loxia-core/src/reducer/queue.rs`), added to the `#[cfg(test)]`
  module of `crates/loxia-core/src/queue/shuffle.rs` alongside the existing shuffle tests, per the
  "Pin insert-next and append queue behaviour with characterization tests" task.

  **Passing** (confirm existing behaviour):
  - `insert_next_unshuffled_plays_immediately_after_current`
  - `insert_next_shuffled_plays_immediately_after_current`
  - `insert_next_multi_select_preserves_selection_order`
  - `append_unshuffled_goes_to_end`
  - `append_shuffled_goes_to_end_of_play_order`
  - `queue_edit_does_not_change_current_entry_or_position_target`
  - `queue_edit_emits_no_load_and_keeps_session`
  - `play_order_is_a_permutation_after_every_edit`

  **Ignored as a known defect** (fails against current behaviour, kept in CI as
  `#[ignore = "fixed by 06-09-insert-next-unshuffle-ordering"]` so CI stays green until that fix
  task lands):
  - `insert_next_shuffled_then_unshuffle_keeps_entry_after_current` — 'insert next' only edits
    `play_order`, never `entries`' insertion order, while `queue::shuffle::unshuffle` resets
    `play_order` back to `entries`' raw insertion order and repositions only the single index that
    was current. An entry inserted "next" while shuffled therefore loses its "plays right after
    current" placement the moment the queue is un-shuffled.
