//! Shuffle and un-shuffle transforms over `QueueState::play_order` (`06-03`).
//!
//! `QueueState::entries` is kept in insertion order and is never reordered — shuffling and
//! un-shuffling only ever touch `play_order` (a permutation of indices into `entries`) and
//! `position` (an index into `play_order`). The entry that is "current" is always
//! `entries[play_order[position]]`.
//!
//! `shuffle` never disturbs already-played history: only the not-yet-played tail
//! (`play_order[position + 1..]`) is permuted, using a seeded RNG so the result is reproducible in
//! tests. `unshuffle` resets `play_order` to the identity permutation `0..entries.len()` — i.e.
//! back to `entries`' own insertion order, since `entries` itself is never reordered — and
//! relocates `position` to wherever the entry that was current a moment ago now sits.

use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

use crate::state::queue::QueueState;

/// Shuffles the not-yet-played tail of `queue.play_order` in place with a seeded, reproducible
/// RNG. `queue.entries` and the already-played history (`play_order[..=position]`) are never
/// touched.
pub fn shuffle(queue: &mut QueueState, seed: u64) {
    if queue.position + 1 >= queue.play_order.len() {
        queue.shuffled = true;
        return;
    }
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    queue.play_order[queue.position + 1..].shuffle(&mut rng);
    queue.shuffled = true;
}

/// Un-shuffles `queue`: `play_order` becomes the identity permutation `0..entries.len()` (i.e.
/// `entries`' own insertion order), and `position` is moved to wherever the entry that was
/// current a moment ago now sits.
pub fn unshuffle(queue: &mut QueueState) {
    let current = queue.play_order.get(queue.position).copied();
    queue.play_order = (0..queue.entries.len()).collect();
    if let Some(idx) = current {
        if let Some(pos) = queue.play_order.iter().position(|i| *i == idx) {
            queue.position = pos;
        }
    }
    queue.shuffled = false;
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{shuffle, unshuffle};
    use crate::action::{Action, QueueBatch, QueueBatchMode};
    use crate::effect::{AudioEffect, Effect};
    use crate::reducer;
    use crate::state::AppState;
    use crate::state::queue::{QueueEntry, QueueEntryId, QueueState};

    /// Builds a `QueueState` with `n` entries, in insertion order, un-shuffled, positioned at the
    /// first entry. Shared by every test in this module.
    fn queue_of(n: usize) -> QueueState {
        let entries = (0..n)
            .map(|i| QueueEntry {
                entry_id: QueueEntryId(i as u64),
                ..Default::default()
            })
            .collect();
        QueueState {
            entries,
            play_order: (0..n).collect(),
            position: 0,
            shuffled: false,
            ..Default::default()
        }
    }

    fn assert_permutation(queue: &QueueState) {
        let mut sorted = queue.play_order.clone();
        sorted.sort_unstable();
        let expected: Vec<usize> = (0..queue.entries.len()).collect();
        assert_eq!(sorted, expected);
    }

    #[test]
    fn shuffle_empty_queue_is_a_noop() {
        let mut queue = queue_of(0);
        let before = queue.play_order.clone();
        shuffle(&mut queue, 42);
        assert_eq!(queue.play_order, before);
        assert_eq!(queue.position, 0);
    }

    #[test]
    fn shuffle_single_entry_queue_is_a_noop() {
        let mut queue = queue_of(1);
        let before = queue.play_order.clone();
        shuffle(&mut queue, 7);
        assert_eq!(queue.play_order, before);
        assert_eq!(queue.position, 0);
    }

    #[test]
    fn shuffle_at_last_position_is_a_noop() {
        let mut queue = queue_of(5);
        queue.position = 4;
        let before = queue.play_order.clone();
        shuffle(&mut queue, 7);
        assert_eq!(queue.play_order, before);
        assert_eq!(queue.position, 4);
    }

    #[test]
    fn shuffle_preserves_current_track() {
        let mut queue = queue_of(6);
        queue.position = 2;
        let current_idx = queue.play_order[queue.position];
        shuffle(&mut queue, 99);
        assert_eq!(queue.play_order[queue.position], current_idx);
    }

    #[test]
    fn shuffle_does_not_reorder_entries() {
        let mut queue = queue_of(6);
        let before = queue.entries.clone();
        shuffle(&mut queue, 5);
        assert_eq!(queue.entries, before);
    }

    #[test]
    fn shuffle_preserves_played_history_order() {
        let mut queue = queue_of(8);
        queue.position = 3;
        let history = queue.play_order[..=3].to_vec();
        shuffle(&mut queue, 11);
        assert_eq!(&queue.play_order[..=3], history.as_slice());
    }

    #[test]
    fn shuffle_is_deterministic_for_a_seed() {
        let mut a = queue_of(10);
        let mut b = queue_of(10);
        shuffle(&mut a, 123);
        shuffle(&mut b, 123);
        assert_eq!(a.play_order, b.play_order);
    }

    #[test]
    fn different_seeds_give_different_orders() {
        let mut a = queue_of(10);
        let mut b = queue_of(10);
        shuffle(&mut a, 1);
        shuffle(&mut b, 2);
        assert_ne!(a.play_order, b.play_order);
    }

    #[test]
    fn unshuffle_resets_identity_and_keeps_current_entry() {
        let mut queue = queue_of(6);
        queue.position = 2;
        shuffle(&mut queue, 3);
        let current_idx = queue.play_order[queue.position];
        unshuffle(&mut queue);
        assert_eq!(queue.play_order, (0..6).collect::<Vec<_>>());
        assert_eq!(queue.play_order[queue.position], current_idx);
        assert!(!queue.shuffled);
    }

    proptest! {
        #[test]
        fn unshuffle_round_trips_to_original_order(
            n in 1usize..20,
            seed in any::<u64>(),
            pos in 0usize..20,
        ) {
            let mut queue = queue_of(n);
            queue.position = pos % n;
            shuffle(&mut queue, seed);
            unshuffle(&mut queue);
            prop_assert_eq!(queue.play_order, (0..n).collect::<Vec<_>>());
        }
    }

    // ------------------------------------------------------------------------------------------
    // Characterization tests for 'play next' (`i`) / 'add to queue' (`a`) — `QueueBatch` /
    // `QueueBatchMode` in `reducer::queue`. See the PR description for the pass/ignore split.
    // ------------------------------------------------------------------------------------------

    fn new_entry(id: u64) -> QueueEntry {
        QueueEntry {
            entry_id: QueueEntryId(id),
            ..Default::default()
        }
    }

    fn app_state_with_queue(n: usize) -> AppState {
        AppState {
            queue: queue_of(n),
            ..AppState::default()
        }
    }

    #[test]
    fn insert_next_unshuffled_plays_immediately_after_current() {
        let mut state = app_state_with_queue(4);
        state.queue.position = 1;
        let inserted = new_entry(100);
        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![inserted.clone()],
                mode: QueueBatchMode::InsertNext,
            }),
        );
        let next_slot = state.queue.position + 1;
        let next_idx = state.queue.play_order[next_slot];
        assert_eq!(state.queue.entries[next_idx].entry_id, inserted.entry_id);
    }

    #[test]
    fn insert_next_shuffled_plays_immediately_after_current() {
        let mut state = app_state_with_queue(4);
        state.queue.position = 1;
        shuffle(&mut state.queue, 42);
        let inserted = new_entry(100);
        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![inserted.clone()],
                mode: QueueBatchMode::InsertNext,
            }),
        );
        let next_slot = state.queue.position + 1;
        let next_idx = state.queue.play_order[next_slot];
        assert_eq!(state.queue.entries[next_idx].entry_id, inserted.entry_id);
    }

    #[test]
    #[ignore = "fixed by 06-03-shuffle"]
    fn insert_next_shuffled_then_unshuffle_keeps_entry_after_current() {
        let mut state = app_state_with_queue(4);
        state.queue.position = 1;
        shuffle(&mut state.queue, 42);
        let inserted = new_entry(100);
        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![inserted.clone()],
                mode: QueueBatchMode::InsertNext,
            }),
        );
        unshuffle(&mut state.queue);
        let next_slot = state.queue.position + 1;
        let next_idx = state.queue.play_order[next_slot];
        assert_eq!(state.queue.entries[next_idx].entry_id, inserted.entry_id);
    }

    #[test]
    fn insert_next_multi_select_preserves_selection_order() {
        let mut state = app_state_with_queue(3);
        state.queue.position = 0;
        let a = new_entry(101);
        let b = new_entry(102);
        let c = new_entry(103);
        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![a.clone(), b.clone(), c.clone()],
                mode: QueueBatchMode::InsertNext,
            }),
        );
        let start = state.queue.position + 1;
        let ids: Vec<_> = state.queue.play_order[start..start + 3]
            .iter()
            .map(|&idx| state.queue.entries[idx].entry_id)
            .collect();
        assert_eq!(ids, vec![a.entry_id, b.entry_id, c.entry_id]);
    }

    #[test]
    fn append_unshuffled_goes_to_end() {
        let mut state = app_state_with_queue(4);
        state.queue.position = 1;
        let inserted = new_entry(100);
        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![inserted.clone()],
                mode: QueueBatchMode::Append,
            }),
        );
        let last_idx = *state.queue.play_order.last().expect("non-empty play_order");
        assert_eq!(state.queue.entries[last_idx].entry_id, inserted.entry_id);
    }

    #[test]
    fn append_shuffled_goes_to_end_of_play_order() {
        let mut state = app_state_with_queue(4);
        state.queue.position = 1;
        shuffle(&mut state.queue, 7);
        let inserted = new_entry(100);
        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![inserted.clone()],
                mode: QueueBatchMode::Append,
            }),
        );
        let last_idx = *state.queue.play_order.last().expect("non-empty play_order");
        assert_eq!(state.queue.entries[last_idx].entry_id, inserted.entry_id);
    }

    #[test]
    fn queue_edit_does_not_change_current_entry_or_position_target() {
        let mut state = app_state_with_queue(4);
        state.queue.position = 1;
        let current_idx = state.queue.play_order[state.queue.position];
        let current_id = state.queue.entries[current_idx].entry_id;
        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry(200)],
                mode: QueueBatchMode::InsertNext,
            }),
        );
        let new_current_idx = state.queue.play_order[state.queue.position];
        assert_eq!(state.queue.entries[new_current_idx].entry_id, current_id);
    }

    #[test]
    fn queue_edit_emits_no_load_and_keeps_session() {
        let mut state = app_state_with_queue(4);
        state.queue.position = 1;
        let session_before = state.player.session.clone();
        let play_reported_before = state.player.play_reported;
        let start_reported_before = state.player.start_reported;
        let effects = reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry(300)],
                mode: QueueBatchMode::Append,
            }),
        );
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Load { .. }))),
            "queue edit must not emit an audio Load effect"
        );
        assert_eq!(state.player.session, session_before);
        assert_eq!(state.player.play_reported, play_reported_before);
        assert_eq!(state.player.start_reported, start_reported_before);
    }

    #[test]
    fn play_order_is_a_permutation_after_every_edit() {
        let mut state = app_state_with_queue(4);
        state.queue.position = 1;

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry(400)],
                mode: QueueBatchMode::InsertNext,
            }),
        );
        assert_permutation(&state.queue);

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry(401), new_entry(402)],
                mode: QueueBatchMode::Append,
            }),
        );
        assert_permutation(&state.queue);
    }
}
