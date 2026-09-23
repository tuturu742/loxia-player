//! Shuffle and un-shuffle transforms over `QueueState::play_order` (`06-03`).
//!
//! `QueueState::entries` is kept in insertion order and is never reordered — shuffling and
//! un-shuffling only ever touch `play_order` (a permutation of indices into `entries`) and
//! `position` (an index into `play_order`). The entry that is "current" is always
//! `entries[play_order[position]]`.
//!
//! `shuffle` pins whatever is current to the front of the new order (so shuffling never
//! interrupts what's already playing) and randomises the rest with a seeded RNG, for
//! reproducibility in tests. `unshuffle` resets `play_order` to the identity permutation
//! `0..entries.len()` — i.e. back to `entries`' own insertion order — and then relocates
//! `position` to wherever the previously-current entry's index now sits.
//!
//! ## Characterization tests (this module's `tests` block)
//!
//! This task (see the task description this PR implements) pins the observed behaviour of
//! 'play next' (`i`) and 'add to queue' (`a`) — `QueueBatch` / `QueueBatchMode` in
//! `reducer::queue` — against `shuffle`/`unshuffle`. One of the pinned tests,
//! `insert_next_shuffled_then_unshuffle_keeps_entry_after_current`, is expected to **fail**
//! against current behaviour and is marked `#[ignore]` accordingly: 'insert next' only ever
//! edits `play_order` (never `entries`' order), while un-shuffling resets `play_order` to
//! `entries`' raw insertion order and repositions only the single index that was current. An
//! entry inserted "next" while shuffled is not that pinned index, so un-shuffling silently drops
//! it back to wherever its insertion-order position happens to be, rather than keeping it
//! immediately after whatever is current post-un-shuffle. See the PR description / `CHANGELOG.md`
//! for the tracking task id.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::state::queue::QueueState;

/// Shuffles `queue.play_order` in place with a seeded, reproducible RNG. `queue.entries` is
/// never touched. The entry currently at `queue.position` is pinned to the front of the new
/// order (`position` becomes `0`) so shuffling never interrupts what's already playing.
pub fn shuffle(queue: &mut QueueState, seed: u64) {
    if queue.entries.is_empty() {
        return;
    }
    let current = queue.play_order.get(queue.position).copied();
    let mut rest: Vec<usize> = queue
        .play_order
        .iter()
        .copied()
        .filter(|idx| Some(*idx) != current)
        .collect();

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    rest.shuffle(&mut rng);

    let mut new_order = Vec::with_capacity(queue.play_order.len());
    if let Some(idx) = current {
        new_order.push(idx);
    }
    new_order.extend(rest);

    queue.play_order = new_order;
    queue.position = 0;
}

/// Un-shuffles `queue`: `play_order` becomes the identity permutation `0..entries.len()` (i.e.
/// `entries`' own insertion order, since `entries` itself is never reordered by [`shuffle`]), and
/// `position` is moved to wherever the entry that was current a moment ago now sits.
pub fn unshuffle(queue: &mut QueueState) {
    let current = queue.play_order.get(queue.position).copied();
    queue.play_order = (0..queue.entries.len()).collect();
    if let Some(idx) = current {
        if let Some(pos) = queue.play_order.iter().position(|i| *i == idx) {
            queue.position = pos;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{shuffle, unshuffle};
    use crate::action::{Action, QueueBatch, QueueBatchMode};
    use crate::effect::{AudioEffect, Effect};
    use crate::reducer;
    use crate::state::queue::QueueEntry;
    use crate::state::AppState;
    use crate::test_support::fixtures::queue_of;

    fn assert_permutation(queue: &crate::state::queue::QueueState) {
        let mut sorted = queue.play_order.clone();
        sorted.sort_unstable();
        let expected: Vec<usize> = (0..queue.entries.len()).collect();
        assert_eq!(sorted, expected, "play_order must remain a permutation of entries' indices");
    }

    /// Builds `total` fixture entries via [`queue_of`] and returns the slice starting at `from`
    /// — i.e. entries that are guaranteed not to already be present in a `queue_of(from)` queue,
    /// for use as "new" entries being inserted/appended into one.
    fn extra_entries(total: usize, from: usize) -> Vec<QueueEntry> {
        queue_of(total).entries[from..].to_vec()
    }

    fn batch(mode: QueueBatchMode, entries: Vec<QueueEntry>) -> Action {
        Action::QueueBatch(QueueBatch { entries, mode })
    }

    // ---- existing shuffle/unshuffle characterization tests (unchanged) --------------------

    #[test]
    fn shuffle_produces_a_permutation_and_keeps_current_playing() {
        let mut queue = queue_of(6);
        queue.position = 3;
        let current = queue.entries[queue.play_order[queue.position]].clone();

        shuffle(&mut queue, 1234);

        assert_permutation(&queue);
        assert_eq!(queue.entries[queue.play_order[queue.position]], current);
    }

    #[test]
    fn shuffle_is_deterministic_for_a_given_seed() {
        let mut a = queue_of(8);
        let mut b = queue_of(8);

        shuffle(&mut a, 55);
        shuffle(&mut b, 55);

        assert_eq!(a.play_order, b.play_order);
    }

    #[test]
    fn unshuffle_restores_insertion_order_and_repositions_current() {
        let mut queue = queue_of(6);
        queue.position = 2;
        let current = queue.entries[queue.play_order[queue.position]].clone();

        shuffle(&mut queue, 9);
        unshuffle(&mut queue);

        assert_eq!(queue.play_order, (0..queue.entries.len()).collect::<Vec<_>>());
        assert_eq!(queue.entries[queue.play_order[queue.position]], current);
    }

    // ---- insert-next / append characterization tests (new) ---------------------------------

    #[test]
    fn insert_next_unshuffled_plays_immediately_after_current() {
        let mut state = AppState::default();
        state.queue = queue_of(4);
        state.queue.position = 1;

        let new_entry = extra_entries(5, 4).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::InsertNext, vec![new_entry.clone()]));

        let inserted_index = state.queue.entries.len() - 1;
        assert_eq!(state.queue.position, 1, "inserting next must not move the current position");
        assert_eq!(state.queue.play_order[state.queue.position + 1], inserted_index);
        assert_eq!(state.queue.entries[inserted_index], new_entry);
    }

    #[test]
    fn insert_next_shuffled_plays_immediately_after_current() {
        let mut state = AppState::default();
        state.queue = queue_of(5);
        shuffle(&mut state.queue, 42);
        let current_before = state.queue.play_order[state.queue.position];

        let new_entry = extra_entries(6, 5).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::InsertNext, vec![new_entry.clone()]));

        let inserted_index = state.queue.entries.len() - 1;
        assert_eq!(state.queue.play_order[state.queue.position], current_before);
        assert_eq!(state.queue.play_order[state.queue.position + 1], inserted_index);
        assert_eq!(state.queue.entries[inserted_index], new_entry);
    }

    #[test]
    #[ignore = "fixed by 06-09-insert-next-unshuffle-ordering"]
    fn insert_next_shuffled_then_unshuffle_keeps_entry_after_current() {
        let mut state = AppState::default();
        state.queue = queue_of(5);
        shuffle(&mut state.queue, 7);

        let new_entry = extra_entries(6, 5).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::InsertNext, vec![new_entry]));
        let inserted_index = state.queue.entries.len() - 1;

        unshuffle(&mut state.queue);

        // The entry inserted "next" should still play immediately after whatever is current
        // once un-shuffled — but `unshuffle` resets `play_order` to raw insertion order and
        // only relocates the single index that was current, so this currently fails whenever
        // the inserted entry's insertion-order index doesn't happen to land right after it.
        assert_eq!(
            state.queue.play_order[state.queue.position + 1],
            inserted_index,
            "insert-next ordering must survive an unshuffle"
        );
    }

    #[test]
    fn insert_next_multi_select_preserves_selection_order() {
        let mut state = AppState::default();
        state.queue = queue_of(3);
        state.queue.position = 0;

        let picked = extra_entries(6, 3);
        reducer::reduce(&mut state, batch(QueueBatchMode::InsertNext, picked.clone()));

        let start = state.queue.entries.len() - picked.len();
        let inserted_indices: Vec<usize> = (start..state.queue.entries.len()).collect();
        let got: Vec<usize> = state.queue.play_order
            [state.queue.position + 1..state.queue.position + 1 + picked.len()]
            .to_vec();

        assert_eq!(got, inserted_indices, "multi-select insert-next must preserve selection order");
    }

    #[test]
    fn append_unshuffled_goes_to_end() {
        let mut state = AppState::default();
        state.queue = queue_of(4);

        let new_entry = extra_entries(5, 4).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::Append, vec![new_entry.clone()]));

        let inserted_index = state.queue.entries.len() - 1;
        assert_eq!(*state.queue.play_order.last().unwrap(), inserted_index);
        assert_eq!(state.queue.entries[inserted_index], new_entry);
    }

    #[test]
    fn append_shuffled_goes_to_end_of_play_order() {
        let mut state = AppState::default();
        state.queue = queue_of(5);
        shuffle(&mut state.queue, 99);

        let new_entry = extra_entries(6, 5).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::Append, vec![new_entry.clone()]));

        let inserted_index = state.queue.entries.len() - 1;
        assert_eq!(*state.queue.play_order.last().unwrap(), inserted_index);
        assert_eq!(state.queue.entries[inserted_index], new_entry);
    }

    #[test]
    fn queue_edit_does_not_change_current_entry_or_position_target() {
        let mut state = AppState::default();
        state.queue = queue_of(4);
        state.queue.position = 2;
        let current_before = state.queue.entries[state.queue.play_order[state.queue.position]].clone();
        let position_before = state.queue.position;

        let new_entry = extra_entries(5, 4).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::InsertNext, vec![new_entry]));

        assert_eq!(state.queue.position, position_before);
        assert_eq!(
            state.queue.entries[state.queue.play_order[state.queue.position]],
            current_before
        );

        let another = extra_entries(6, 5).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::Append, vec![another]));

        assert_eq!(state.queue.position, position_before);
        assert_eq!(
            state.queue.entries[state.queue.play_order[state.queue.position]],
            current_before
        );
    }

    #[test]
    fn queue_edit_emits_no_load_and_keeps_session() {
        let mut state = AppState::default();
        state.queue = queue_of(3);
        let player_before = state.player.clone();

        let new_entry = extra_entries(4, 3).remove(0);
        let effects =
            reducer::reduce(&mut state, batch(QueueBatchMode::InsertNext, vec![new_entry]));

        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Load { .. }))),
            "queue edits must not trigger a reload"
        );
        assert_eq!(state.player.session, player_before.session);
        assert_eq!(state.player.play_reported, player_before.play_reported);
        assert_eq!(state.player.start_reported, player_before.start_reported);
    }

    #[test]
    fn play_order_is_a_permutation_after_every_edit() {
        let mut state = AppState::default();
        state.queue = queue_of(4);
        state.queue.position = 1;

        let one = extra_entries(5, 4).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::InsertNext, vec![one]));
        assert_permutation(&state.queue);

        let two = extra_entries(6, 5).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::Append, vec![two]));
        assert_permutation(&state.queue);

        shuffle(&mut state.queue, 3);
        assert_permutation(&state.queue);

        let three = extra_entries(7, 6).remove(0);
        reducer::reduce(&mut state, batch(QueueBatchMode::InsertNext, vec![three]));
        assert_permutation(&state.queue);

        unshuffle(&mut state.queue);
        assert_permutation(&state.queue);
    }
}
