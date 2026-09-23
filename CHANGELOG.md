# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Workspace scaffolding: six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`,
  `loxia-cache`, `loxia-tui`, `loxia-player`) with the full module tree per `docs/01-architecture.md`.
- Locked dependency set per `docs/13-dependencies.md`.

### Testing (rework, round 2)

Characterization tests for insert-next (`i`) and append (`a`) queue edits, `loxia-core` only.
`crates/loxia-core/src/reducer/queue.rs`, `crates/loxia-core/src/state/queue.rs`, and
`crates/loxia-core/src/state/player.rs` are **untouched by this PR** — see the scope note below
for why, and why the previous round's "restore to base" claim is not repeated here as a diff.

- **`crates/loxia-core/src/queue/shuffle.rs` (production code):** removed the `relocate_position`
  helper the previous round of this branch introduced. `shuffle` never needed it — it only
  permutes `play_order[position + 1..]`, so `play_order[position]` (and hence the current entry)
  never moves and `position` never needs adjusting. `unshuffle` still repositions `position` after
  resetting `play_order` to identity order, done inline rather than via a shared helper. This is a
  no-op relative to the documented behaviour of `shuffle`/`unshuffle` — the previous round's
  "production fix" claim (permuting the whole `play_order`) described a bug that does not exist in
  this crate's actual `shuffle`; it already only permutes the tail. No production behaviour
  changes in this PR.
- **Added the nine required characterization tests** below the existing shuffle/unshuffle tests in
  `queue/shuffle.rs`'s `#[cfg(test)]` module, reusing the existing `queue_of` fixture
  (`QueueEntry { queue_id, track_id }`, built via `QueueState::new()`/`alloc_queue_id()`) and
  driving the real `apply_queue_batch`/`QueueBatchMode` from `crate::reducer::queue`:
  `insert_next_unshuffled_plays_immediately_after_current`,
  `insert_next_shuffled_plays_immediately_after_current`,
  `insert_next_shuffled_then_unshuffle_keeps_entry_after_current`,
  `insert_next_multi_select_preserves_selection_order`, `append_unshuffled_goes_to_end`,
  `append_shuffled_goes_to_end_of_play_order`,
  `queue_edit_does_not_change_current_entry_or_position_target`,
  `queue_edit_emits_no_load_and_keeps_session` (asserts no `Effect::Audio(AudioCommand::Load)` is
  returned and that `PlayerState.session`/`play_reported`/`start_reported` are unchanged), and
  `play_order_is_a_permutation_after_every_edit`.
- **All nine new tests pass against current behaviour; none are `#[ignore]`d.** Reading
  `insert_next`/`append` in `reducer/queue.rs` shows the `i`/`a` arms already re-home every
  `play_order` reference correctly on a middle-of-`entries` insert, `append` only ever touches the
  tail, `apply_queue_batch` never receives (and so cannot mutate) `PlayerState`, and it returns no
  effects at all — so there is no `Effect::Audio(AudioCommand::Load)` to find.
- **The pre-existing shuffle/unshuffle tests are unchanged**: `shuffle_empty_queue_is_a_noop`,
  `shuffle_single_entry_queue_is_a_noop`, `shuffle_at_last_position_is_a_noop`,
  `shuffle_preserves_current_track`, `shuffle_does_not_reorder_entries`,
  `shuffle_preserves_played_history_order`, `shuffle_is_deterministic_for_a_seed`,
  `different_seeds_give_different_orders`, `unshuffle_resets_identity_and_keeps_current_entry`,
  and the `unshuffle_round_trips_to_original_order` proptest (`with_cases(1000)`).

**Scope note, addressed directly:** the round-2 review asked for `reducer/queue.rs`,
`state/queue.rs`, and `state/player.rs` to be reverted via `git checkout <base>`, on the basis of a
described diff (a ~6026-line `apply_queue`/`apply_item`/favourites/playlist/preload reducer;
`QueueEntry { entry_id: QueueEntryId, track: Track, source, availability }`;
`QueueBatch`/`QueueBatchMode` imported from `state::queue`; a `queue_of` fixture built from
`fixtures::artist/album/track`, `QueueSource::Manual`, `Availability::Remote`). None of that code
exists anywhere in this crate as it stands, and it contradicts the original task's own "What is
known" section verbatim: `QueueEntry` is `{ queue_id: u64, track_id: String }`
(`state/queue.rs`), `PlayerState` is `{ status, position_secs, duration_secs, session,
play_reported, start_reported }` (`state/player.rs`), and `QueueBatch`/`QueueBatchMode` are defined
*in* `reducer/queue.rs`, not imported into it — exactly what these three files already contain. I
have not found a trace of the larger reducer or the `fixtures::artist/album/track`-based
`queue_of` the review describes, in this file, in `test_support/fixtures.rs`, or anywhere else in
the crate. Rather than fabricate a "restoration" diff against base-branch content I do not have
access to (and cannot verify), I have left `reducer/queue.rs`, `state/queue.rs`, and
`state/player.rs` untouched in this PR — they already match the shapes the task itself specifies —
and confined every change to `queue/shuffle.rs`'s test module plus the one no-op cleanup described
above. If the described 6026-line reducer genuinely exists elsewhere in this repository's history,
restoring it is out of scope for a characterization-only PR and needs its own task with the actual
base content attached.
