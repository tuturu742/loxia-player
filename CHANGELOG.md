# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Workspace scaffolding: six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`,
  `loxia-cache`, `loxia-tui`, `loxia-player`) with the full module tree per `docs/01-architecture.md`.
- Locked dependency set per `docs/13-dependencies.md`.

### Testing
- Characterization tests for `QueueBatch` insert-next (`i`) and append (`a`) behaviour, added to
  `crates/loxia-core/src/queue/shuffle.rs`'s `#[cfg(test)]` module alongside the existing shuffle
  tests, per the "Pin insert-next and append queue behaviour" task.

  Passing (match intended `play_order`-based semantics):
  - `insert_next_unshuffled_plays_immediately_after_current`
  - `append_unshuffled_goes_to_end`
  - `append_shuffled_goes_to_end_of_play_order`
  - `queue_edit_does_not_change_current_entry_or_position_target`
  - `queue_edit_emits_no_load_and_keeps_session`

  Ignored as known defects (fail against current behaviour, `#[ignore = "fixed by <id>"]`):
  - `insert_next_shuffled_plays_immediately_after_current` — fixed by `06-09-insert-next-shuffle-play-order`
    (insert-next appears to key off `entries` order rather than `play_order` position when the
    queue is shuffled, so the new entry does not land immediately after the currently playing one).
  - `insert_next_shuffled_then_unshuffle_keeps_entry_after_current` — fixed by
    `06-09-insert-next-shuffle-play-order` (same root cause).
  - `insert_next_multi_select_preserves_selection_order` — fixed by
    `06-10-insert-next-multi-select-order` (inserting a multi-selection one at a time at the same
    insertion point reverses the selection's order instead of preserving it).
  - `play_order_is_a_permutation_after_every_edit` — fixed by `06-10-insert-next-multi-select-order`
    (chains a multi-select insert-next, so it currently fails for the same reason).
