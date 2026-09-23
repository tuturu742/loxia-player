//! Shuffle / un-shuffle of `QueueState.play_order` (`06-03`).
//!
//! `QueueState.entries` is kept in insertion order and is never reordered by shuffle — only
//! `play_order` (a permutation of indices into `entries`) and `position` (an index into
//! `play_order`) change. The entry currently playing is always `entries[play_order[position]]`.
//!
//! Un-shuffling restores `play_order` to the identity permutation (`0..entries.len()`) and moves
//! `position` to wherever the entry that was playing now sits — which, once `play_order` is the
//! identity permutation, is simply that entry's own index in `entries`.

use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

use crate::state::queue::QueueState;

/// Shuffles `play_order` with a seeded RNG (deterministic, so tests can reproduce a given
/// shuffle), keeping the entry currently playing selected — `position` is relocated to wherever
/// that entry's index lands in the new `play_order`.
pub fn shuffle(queue: &mut QueueState, seed: u64) {
    if queue.entries.is_empty() {
        return;
    }
    let current = queue.play_order[queue.position];
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut order: Vec<usize> = (0..queue.entries.len()).collect();
    order.shuffle(&mut rng);
    queue.play_order = order;
    queue.position = queue
        .play_order
        .iter()
        .position(|&idx| idx == current)
        .unwrap_or(0);
}

/// Restores `play_order` to insertion order and relocates `position` to the entry that was
/// playing before the un-shuffle.
pub fn unshuffle(queue: &mut QueueState) {
    if queue.entries.is_empty() {
        queue.play_order = Vec::new();
        queue.position = 0;
        return;
    }
    let current = queue.play_order[queue.position];
    queue.play_order = (0..queue.entries.len()).collect();
    // `play_order` is now the identity permutation, so the index of `current` within it is
    // `current` itself.
    queue.position = current;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{AudioEffect, Effect};
    use crate::reducer;
    use crate::state::AppState;
    use crate::state::queue::QueueEntry;

    /// Builds a `QueueState` with one entry per label, in the given order, unshuffled
    /// (`play_order` is the identity permutation) and positioned on the first entry.
    fn queue_of(labels: &[&str]) -> QueueState {
        let entries: Vec<QueueEntry> = labels.iter().map(|label| entry(label)).collect();
        let play_order: Vec<usize> = (0..entries.len()).collect();
        QueueState {
            entries,
            play_order,
            position: 0,
        }
    }

    fn entry(id: &str) -> QueueEntry {
        QueueEntry::new(id)
    }

    fn app_state_with(queue: QueueState) -> AppState {
        let mut state = AppState::default();
        state.queue = queue;
        state
    }

    fn current_id(queue: &QueueState) -> &str {
        queue.entries[queue.play_order[queue.position]].id.as_str()
    }

    fn assert_permutation(queue: &QueueState) {
        let mut sorted = queue.play_order.clone();
        sorted.sort_unstable();
        let expected: Vec<usize> = (0..queue.entries.len()).collect();
        assert_eq!(sorted, expected);
    }

    // ---- pre-existing shuffle tests ---------------------------------------------------------

    #[test]
    fn shuffle_produces_a_permutation_of_all_indices() {
        let mut queue = queue_of(&["a", "b", "c", "d", "e"]);
        shuffle(&mut queue, 1);
        assert_permutation(&queue);
    }

    #[test]
    fn shuffle_relocates_position_to_the_still_playing_entry() {
        let mut queue = queue_of(&["a", "b", "c", "d"]);
        queue.position = 2; // currently playing "c"
        shuffle(&mut queue, 99);
        assert_eq!(current_id(&queue), "c");
    }

    #[test]
    fn unshuffle_restores_insertion_order_and_relocates_position() {
        let mut queue = queue_of(&["a", "b", "c", "d"]);
        queue.position = 3; // currently playing "d"
        shuffle(&mut queue, 5);
        let playing = current_id(&queue).to_string();
        unshuffle(&mut queue);
        assert_eq!(queue.play_order, vec![0, 1, 2, 3]);
        assert_eq!(current_id(&queue), playing);
    }

    // ---- insert-next / append characterization tests ----------------------------------------

    #[test]
    fn insert_next_unshuffled_plays_immediately_after_current() {
        let queue = queue_of(&["a", "b", "c"]);
        let mut state = app_state_with(queue);
        state.queue.position = 0; // current = "a"

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::InsertNext,
            entries: vec![entry("x")],
        };
        let _effects = reducer::queue::apply_batch(&mut state, batch);

        let queue = &state.queue;
        let current_idx = queue.play_order[queue.position];
        let next_idx = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[current_idx].id, "a");
        assert_eq!(queue.entries[next_idx].id, "x");
    }

    #[test]
    #[ignore = "fixed by 06-09-insert-next-shuffle-play-order"]
    fn insert_next_shuffled_plays_immediately_after_current() {
        let mut queue = queue_of(&["a", "b", "c", "d"]);
        shuffle(&mut queue, 42);
        let mut state = app_state_with(queue);
        let current_before = current_id(&state.queue).to_string();

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::InsertNext,
            entries: vec![entry("x")],
        };
        let _effects = reducer::queue::apply_batch(&mut state, batch);

        let queue = &state.queue;
        let current_idx = queue.play_order[queue.position];
        let next_idx = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[current_idx].id, current_before);
        assert_eq!(queue.entries[next_idx].id, "x");
    }

    #[test]
    #[ignore = "fixed by 06-09-insert-next-shuffle-play-order"]
    fn insert_next_shuffled_then_unshuffle_keeps_entry_after_current() {
        let mut queue = queue_of(&["a", "b", "c", "d"]);
        shuffle(&mut queue, 7);
        let mut state = app_state_with(queue);
        let current_before = current_id(&state.queue).to_string();

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::InsertNext,
            entries: vec![entry("x")],
        };
        let _effects = reducer::queue::apply_batch(&mut state, batch);

        unshuffle(&mut state.queue);

        let queue = &state.queue;
        let current_idx = queue.play_order[queue.position];
        let next_idx = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[current_idx].id, current_before);
        assert_eq!(queue.entries[next_idx].id, "x");
    }

    #[test]
    #[ignore = "fixed by 06-10-insert-next-multi-select-order"]
    fn insert_next_multi_select_preserves_selection_order() {
        let queue = queue_of(&["a", "b"]);
        let mut state = app_state_with(queue);

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::InsertNext,
            entries: vec![entry("x"), entry("y"), entry("z")],
        };
        let _effects = reducer::queue::apply_batch(&mut state, batch);

        let queue = &state.queue;
        let ids: Vec<&str> = queue
            .play_order
            .iter()
            .skip(queue.position + 1)
            .take(3)
            .map(|&idx| queue.entries[idx].id.as_str())
            .collect();
        assert_eq!(ids, vec!["x", "y", "z"]);
    }

    #[test]
    fn append_unshuffled_goes_to_end() {
        let queue = queue_of(&["a", "b", "c"]);
        let mut state = app_state_with(queue);

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::Append,
            entries: vec![entry("x")],
        };
        let _effects = reducer::queue::apply_batch(&mut state, batch);

        let queue = &state.queue;
        let last_idx = *queue.play_order.last().expect("non-empty play_order");
        assert_eq!(queue.entries[last_idx].id, "x");
    }

    #[test]
    fn append_shuffled_goes_to_end_of_play_order() {
        let mut queue = queue_of(&["a", "b", "c", "d"]);
        shuffle(&mut queue, 3);
        let mut state = app_state_with(queue);

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::Append,
            entries: vec![entry("x")],
        };
        let _effects = reducer::queue::apply_batch(&mut state, batch);

        let queue = &state.queue;
        let last_idx = *queue.play_order.last().expect("non-empty play_order");
        assert_eq!(queue.entries[last_idx].id, "x");
    }

    #[test]
    fn queue_edit_does_not_change_current_entry_or_position_target() {
        let queue = queue_of(&["a", "b", "c"]);
        let mut state = app_state_with(queue);
        state.queue.position = 1; // current = "b"
        let before = current_id(&state.queue).to_string();

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::Append,
            entries: vec![entry("x")],
        };
        let _effects = reducer::queue::apply_batch(&mut state, batch);

        let after = current_id(&state.queue).to_string();
        assert_eq!(before, after);
    }

    #[test]
    fn queue_edit_emits_no_load_and_keeps_session() {
        let queue = queue_of(&["a", "b", "c"]);
        let mut state = app_state_with(queue);
        let session_before = state.player.session.clone();
        let play_reported_before = state.player.play_reported;
        let start_reported_before = state.player.start_reported;

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::InsertNext,
            entries: vec![entry("x")],
        };
        let effects = reducer::queue::apply_batch(&mut state, batch);

        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Audio(AudioEffect::Load { .. }))),
            "queue edit must not emit Effect::Audio(Load)"
        );
        assert_eq!(state.player.session, session_before);
        assert_eq!(state.player.play_reported, play_reported_before);
        assert_eq!(state.player.start_reported, start_reported_before);
    }

    #[test]
    #[ignore = "fixed by 06-10-insert-next-multi-select-order"]
    fn play_order_is_a_permutation_after_every_edit() {
        let queue = queue_of(&["a", "b", "c"]);
        let mut state = app_state_with(queue);
        assert_permutation(&state.queue);

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::InsertNext,
            entries: vec![entry("x"), entry("y")],
        };
        let _ = reducer::queue::apply_batch(&mut state, batch);
        assert_permutation(&state.queue);

        let batch = crate::action::QueueBatch {
            mode: crate::action::QueueBatchMode::Append,
            entries: vec![entry("z")],
        };
        let _ = reducer::queue::apply_batch(&mut state, batch);
        assert_permutation(&state.queue);

        shuffle(&mut state.queue, 11);
        assert_permutation(&state.queue);

        unshuffle(&mut state.queue);
        assert_permutation(&state.queue);
    }
}
