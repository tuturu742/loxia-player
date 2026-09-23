//! Shuffle and un-shuffle transforms over `QueueState::play_order` (`06-03`).

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
    use crate::effect::Effect;
    use crate::reducer;
    use crate::state::AppState;
    use crate::state::queue::{Availability, QueueEntry, QueueEntryId, QueueSource, QueueState};
    use crate::test_support::fixtures;

    /// Builds a `QueueState` with `n` entries, in insertion order, un-shuffled, positioned at the
    /// first entry. Shared by every test in this module.
    fn queue_of(n: usize) -> QueueState {
        let entries = (0..n)
            .map(|i| QueueEntry {
                entry_id: QueueEntryId(i as u64),
                track: fixtures::track(i as u64),
                source: QueueSource::Manual,
                availability: Availability::Remote,
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
        let mut queue = queue_of(20);
        queue.position = 5;
        let current_entry = queue.entries[queue.play_order[queue.position]].entry_id;
        shuffle(&mut queue, 99);
        let after_entry = queue.entries[queue.play_order[queue.position]].entry_id;
        assert_eq!(current_entry, after_entry);
    }

    #[test]
    fn shuffle_does_not_reorder_entries() {
        let mut queue = queue_of(20);
        let entries_before = queue.entries.clone();
        shuffle(&mut queue, 1);
        assert_eq!(queue.entries, entries_before);
    }

    #[test]
    fn shuffle_preserves_played_history_order() {
        let mut queue = queue_of(20);
        queue.position = 6;
        let history_before = queue.play_order[..=queue.position].to_vec();
        shuffle(&mut queue, 3);
        let history_after = queue.play_order[..=queue.position].to_vec();
        assert_eq!(history_before, history_after);
    }

    #[test]
    fn shuffle_is_deterministic_for_a_seed() {
        let mut a = queue_of(20);
        let mut b = queue_of(20);
        shuffle(&mut a, 123);
        shuffle(&mut b, 123);
        assert_eq!(a.play_order, b.play_order);
    }

    #[test]
    fn different_seeds_give_different_orders() {
        let mut a = queue_of(20);
        let mut b = queue_of(20);
        shuffle(&mut a, 1);
        shuffle(&mut b, 2);
        assert_ne!(a.play_order, b.play_order);
    }

    #[test]
    fn unshuffle_resets_identity_and_keeps_current_entry() {
        let mut queue = queue_of(20);
        queue.position = 4;
        shuffle(&mut queue, 55);
        let current_entry = queue.entries[queue.play_order[queue.position]].entry_id;
        unshuffle(&mut queue);
        assert_eq!(
            queue.play_order,
            (0..queue.entries.len()).collect::<Vec<_>>()
        );
        let after_entry = queue.entries[queue.play_order[queue.position]].entry_id;
        assert_eq!(current_entry, after_entry);
        assert!(!queue.shuffled);
    }

    proptest! {
        #[test]
        fn unshuffle_round_trips_to_original_order(
            n in 1usize..30,
            seed in any::<u64>(),
            position_seed in 0usize..30,
        ) {
            let mut queue = queue_of(n);
            queue.position = position_seed % n;
            shuffle(&mut queue, seed);
            unshuffle(&mut queue);
            prop_assert_eq!(queue.play_order, (0..n).collect::<Vec<_>>());
        }
    }

    // --- insert-next / append characterization tests -----------------------------------------
    //
    // `QueueBatch` / `QueueBatchMode` (`crates/loxia-core/src/reducer/queue.rs`) already
    // implement 'play next' (`i`) and 'add to queue' (`a`). These tests pin the current
    // behaviour of those reducer arms; they do not change it.

    fn app_with(queue: QueueState) -> AppState {
        AppState {
            queue,
            ..AppState::default()
        }
    }

    /// A fresh entry with `entry_id` `id`, in the same shape `queue_of` builds its own entries.
    fn new_entry(id: u64) -> QueueEntry {
        QueueEntry {
            entry_id: QueueEntryId(id),
            track: fixtures::track(id),
            source: QueueSource::Manual,
            availability: Availability::Remote,
        }
    }

    fn current_entry_id(queue: &QueueState) -> QueueEntryId {
        queue.entries[queue.play_order[queue.position]].entry_id
    }

    fn dispatch(state: &mut AppState, action: Action) -> Vec<Effect> {
        reducer::reduce(state, action)
    }

    fn effect_is_audio_load(effect: &Effect) -> bool {
        matches!(effect, Effect::Audio(_)) && format!("{effect:?}").contains("Load")
    }

    #[test]
    fn insert_next_unshuffled_plays_immediately_after_current() {
        let mut state = app_with(queue_of(3));
        state.queue.position = 1;
        let current = current_entry_id(&state.queue);

        let inserted = new_entry(3);
        let action = Action::QueueBatch(QueueBatch {
            entries: vec![inserted.clone()],
            mode: QueueBatchMode::InsertNext,
        });
        dispatch(&mut state, action);

        assert_eq!(current_entry_id(&state.queue), current);
        let next_idx = state.queue.play_order[state.queue.position + 1];
        assert_eq!(state.queue.entries[next_idx].entry_id, inserted.entry_id);
    }

    #[test]
    fn insert_next_shuffled_plays_immediately_after_current() {
        let mut state = app_with(queue_of(6));
        state.queue.position = 2;
        shuffle(&mut state.queue, 11);
        let current = current_entry_id(&state.queue);

        let inserted = new_entry(6);
        let action = Action::QueueBatch(QueueBatch {
            entries: vec![inserted.clone()],
            mode: QueueBatchMode::InsertNext,
        });
        dispatch(&mut state, action);

        assert_eq!(current_entry_id(&state.queue), current);
        let next_idx = state.queue.play_order[state.queue.position + 1];
        assert_eq!(state.queue.entries[next_idx].entry_id, inserted.entry_id);
    }

    /// Known defect: `unshuffle` resets `play_order` to `entries`' own insertion order, and an
    /// entry inserted via insert-next is appended to the *end* of `entries` (`entries` is never
    /// reordered). Unless the current entry happened to sit second-to-last in `entries`, the
    /// "plays immediately after current" guarantee insert-next gives while shuffled does not
    /// survive an unshuffle.
    #[test]
    #[ignore = "fixed by <fix task id>"]
    fn insert_next_shuffled_then_unshuffle_keeps_entry_after_current() {
        let mut state = app_with(queue_of(6));
        state.queue.position = 2;
        shuffle(&mut state.queue, 11);

        let inserted = new_entry(6);
        let action = Action::QueueBatch(QueueBatch {
            entries: vec![inserted.clone()],
            mode: QueueBatchMode::InsertNext,
        });
        dispatch(&mut state, action);

        let current = current_entry_id(&state.queue);
        unshuffle(&mut state.queue);

        assert_eq!(current_entry_id(&state.queue), current);
        let next_idx = state.queue.play_order[state.queue.position + 1];
        assert_eq!(state.queue.entries[next_idx].entry_id, inserted.entry_id);
    }

    #[test]
    fn insert_next_multi_select_preserves_selection_order() {
        let mut state = app_with(queue_of(4));
        state.queue.position = 1;
        let current = current_entry_id(&state.queue);

        let inserted: Vec<QueueEntry> = vec![new_entry(4), new_entry(5), new_entry(6)];
        let action = Action::QueueBatch(QueueBatch {
            entries: inserted.clone(),
            mode: QueueBatchMode::InsertNext,
        });
        dispatch(&mut state, action);

        assert_eq!(current_entry_id(&state.queue), current);
        for (offset, entry) in inserted.iter().enumerate() {
            let idx = state.queue.play_order[state.queue.position + 1 + offset];
            assert_eq!(state.queue.entries[idx].entry_id, entry.entry_id);
        }
    }

    #[test]
    fn append_unshuffled_goes_to_end() {
        let mut state = app_with(queue_of(3));
        let inserted = new_entry(3);
        let action = Action::QueueBatch(QueueBatch {
            entries: vec![inserted.clone()],
            mode: QueueBatchMode::Append,
        });
        dispatch(&mut state, action);

        let last_idx = *state.queue.play_order.last().unwrap();
        assert_eq!(state.queue.entries[last_idx].entry_id, inserted.entry_id);
    }

    #[test]
    fn append_shuffled_goes_to_end_of_play_order() {
        let mut state = app_with(queue_of(6));
        state.queue.position = 2;
        shuffle(&mut state.queue, 5);
        let before_len = state.queue.play_order.len();

        let inserted = new_entry(6);
        let action = Action::QueueBatch(QueueBatch {
            entries: vec![inserted.clone()],
            mode: QueueBatchMode::Append,
        });
        dispatch(&mut state, action);

        assert_eq!(state.queue.play_order.len(), before_len + 1);
        let last_idx = *state.queue.play_order.last().unwrap();
        assert_eq!(state.queue.entries[last_idx].entry_id, inserted.entry_id);
    }

    #[test]
    fn queue_edit_does_not_change_current_entry_or_position_target() {
        let mut state = app_with(queue_of(5));
        state.queue.position = 3;
        let current = current_entry_id(&state.queue);

        let action = Action::QueueBatch(QueueBatch {
            entries: vec![new_entry(5)],
            mode: QueueBatchMode::InsertNext,
        });
        dispatch(&mut state, action);
        assert_eq!(current_entry_id(&state.queue), current);

        let action = Action::QueueBatch(QueueBatch {
            entries: vec![new_entry(6)],
            mode: QueueBatchMode::Append,
        });
        dispatch(&mut state, action);
        assert_eq!(current_entry_id(&state.queue), current);
    }

    #[test]
    fn queue_edit_emits_no_load_and_keeps_session() {
        let mut state = app_with(queue_of(4));
        state.queue.position = 1;
        let session_before = state.player.session.clone();
        let play_reported_before = state.player.play_reported;
        let start_reported_before = state.player.start_reported;

        let action = Action::QueueBatch(QueueBatch {
            entries: vec![new_entry(4)],
            mode: QueueBatchMode::InsertNext,
        });
        let effects = dispatch(&mut state, action);

        assert!(!effects.iter().any(effect_is_audio_load));
        assert_eq!(state.player.session, session_before);
        assert_eq!(state.player.play_reported, play_reported_before);
        assert_eq!(state.player.start_reported, start_reported_before);
    }

    #[test]
    fn play_order_is_a_permutation_after_every_edit() {
        let mut state = app_with(queue_of(5));
        state.queue.position = 2;
        shuffle(&mut state.queue, 3);

        let action = Action::QueueBatch(QueueBatch {
            entries: vec![new_entry(5)],
            mode: QueueBatchMode::InsertNext,
        });
        dispatch(&mut state, action);
        assert_permutation(&state.queue);

        let action = Action::QueueBatch(QueueBatch {
            entries: vec![new_entry(6), new_entry(7)],
            mode: QueueBatchMode::Append,
        });
        dispatch(&mut state, action);
        assert_permutation(&state.queue);
    }
}
