//! Queue shuffle (`06-03`): `play_order` becomes a random permutation of `entries`' indices.
//!
//! Shuffling only randomises the *tail* of `play_order` — everything from `position + 1`
//! onward — never `play_order[..=position]`. Everything up to and including the current entry
//! is "played history", and history must keep the order it was actually played in; only what
//! *hasn't* played yet is fair game to reorder. This also means shuffling never moves the
//! current entry, so `position` itself never needs to change.
//!
//! Un-shuffling restores natural order — `play_order = (0..entries.len()).collect()` — and moves
//! `position` to wherever the entry that was playing now sits, so shuffling and un-shuffling
//! never change what is currently playing, only what comes next.

use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

use crate::state::queue::QueueState;

/// Shuffles `queue.play_order[position + 1..]` in place with a seeded, reproducible RNG.
/// `entries` itself is never touched — only the permutation of indices into it — and nothing at
/// or before `position` moves, so the current entry and everything already played keep their
/// order.
pub fn shuffle(queue: &mut QueueState, seed: u64) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let tail_start = (queue.position + 1).min(queue.play_order.len());
    queue.play_order[tail_start..].shuffle(&mut rng);
    queue.shuffled = true;
}

/// Restores `play_order` to natural (insertion) order and repositions `position` so the entry
/// that was playing before un-shuffling is still the entry playing after.
pub fn unshuffle(queue: &mut QueueState) {
    let current = queue.current_slot();
    queue.play_order = (0..queue.entries.len()).collect();
    queue.shuffled = false;
    if let Some(slot) = current
        && let Some(pos) = queue.play_order.iter().position(|&idx| idx == slot)
    {
        queue.position = pos;
    }
}

#[cfg(test)]
mod tests {
    //! `shuffle`/`unshuffle`'s own acceptance tests, plus characterization tests for insert-next
    //! (`i`) and append (`a`) queue edits (`crate::reducer::queue`'s `QueueBatchMode`/
    //! `apply_queue_batch`), added in the same module since both operate on the same
    //! `entries`/`play_order`/`position` invariants and share the `queue_of` fixture below.
    //! Pass/ignore status and rationale belong in the PR description, not here — see that.

    use proptest::prelude::*;

    use super::*;
    use crate::effect::{AudioCommand, Effect};
    use crate::reducer::queue::{QueueBatchMode, apply_queue_batch, load_current};
    use crate::state::player::PlayerState;
    use crate::state::queue::QueueEntry;

    /// `n` tracks, freshly queued in natural (unshuffled) order: `entries[i].track_id ==
    /// format!("track-{i}")`, `play_order == (0..n)`, `position == 0`.
    fn queue_of(n: usize) -> QueueState {
        let mut queue = QueueState::new();
        for i in 0..n {
            let queue_id = queue.alloc_queue_id();
            queue.entries.push(QueueEntry {
                queue_id,
                track_id: format!("track-{i}"),
            });
        }
        queue.play_order = (0..n).collect();
        queue
    }

    fn assert_permutation(queue: &QueueState) {
        let mut sorted = queue.play_order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..queue.entries.len()).collect::<Vec<_>>());
    }

    // ---- shuffle/unshuffle ----

    #[test]
    fn shuffle_empty_queue_is_a_noop() {
        let mut queue = queue_of(0);
        shuffle(&mut queue, 1);
        assert_eq!(queue.play_order, Vec::<usize>::new());
        assert_eq!(queue.position, 0);
    }

    #[test]
    fn shuffle_single_entry_queue_is_a_noop() {
        let mut queue = queue_of(1);
        shuffle(&mut queue, 1);
        assert_eq!(queue.play_order, vec![0]);
        assert_eq!(queue.position, 0);
    }

    #[test]
    fn shuffle_at_last_position_is_a_noop() {
        let mut queue = queue_of(5);
        queue.position = 4;
        let before = queue.play_order.clone();
        shuffle(&mut queue, 1);
        assert_eq!(queue.play_order, before);
        assert_eq!(queue.position, 4);
    }

    #[test]
    fn shuffle_preserves_current_track() {
        let mut queue = queue_of(10);
        queue.position = 3;
        let current = queue.current_entry().cloned();
        shuffle(&mut queue, 42);
        assert_eq!(queue.current_entry().cloned(), current);
        assert_eq!(queue.position, 3);
    }

    #[test]
    fn shuffle_does_not_reorder_entries() {
        let mut queue = queue_of(10);
        let entries_before = queue.entries.clone();
        shuffle(&mut queue, 7);
        assert_eq!(queue.entries, entries_before);
    }

    #[test]
    fn shuffle_preserves_played_history_order() {
        let mut queue = queue_of(10);
        queue.position = 4;
        let history_before = queue.play_order[..=4].to_vec();
        shuffle(&mut queue, 99);
        assert_eq!(queue.play_order[..=4], history_before[..]);
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
        let mut queue = queue_of(10);
        queue.position = 2;
        shuffle(&mut queue, 5);
        let current = queue.current_entry().cloned();
        unshuffle(&mut queue);
        assert_eq!(queue.play_order, (0..10).collect::<Vec<_>>());
        assert_eq!(queue.current_entry().cloned(), current);
        assert!(!queue.shuffled);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn unshuffle_round_trips_to_original_order(
            n in 1usize..20,
            seed in any::<u64>(),
            pos_seed in 0usize..20,
        ) {
            let mut queue = queue_of(n);
            queue.position = pos_seed % n;
            shuffle(&mut queue, seed);
            unshuffle(&mut queue);
            prop_assert_eq!(queue.play_order, (0..n).collect::<Vec<_>>());
        }
    }

    // ---- insert-next (`i`) / append (`a`) characterization tests ----
    //
    // Driven through the real `apply_queue_batch`/`QueueBatchMode` in `crate::reducer::queue`,
    // reusing `queue_of` above. See the PR description for which of these pass against current
    // behaviour and which (if any) are `#[ignore]`d as known defects.

    #[test]
    fn insert_next_unshuffled_plays_immediately_after_current() {
        let mut queue = queue_of(3);

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["new-track".to_string()],
        );

        let current_slot = queue.play_order[queue.position];
        assert_eq!(queue.entries[current_slot].track_id, "track-0");

        let next_slot = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[next_slot].track_id, "new-track");
    }

    #[test]
    fn insert_next_shuffled_plays_immediately_after_current() {
        let mut queue = queue_of(5);
        queue.position = 1;
        shuffle(&mut queue, 7);
        let current_before = queue.current_entry().cloned();

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["new-track".to_string()],
        );

        assert_eq!(queue.current_entry().cloned(), current_before);
        let next_slot = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[next_slot].track_id, "new-track");
    }

    #[test]
    fn insert_next_shuffled_then_unshuffle_keeps_entry_after_current() {
        let mut queue = queue_of(5);
        queue.position = 1;
        shuffle(&mut queue, 7);
        let current_before = queue.current_entry().cloned();

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["new-track".to_string()],
        );

        unshuffle(&mut queue);

        assert_eq!(queue.current_entry().cloned(), current_before);
        let next_slot = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[next_slot].track_id, "new-track");
    }

    #[test]
    fn insert_next_multi_select_preserves_selection_order() {
        let mut queue = queue_of(3);

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        );

        let after_current: Vec<&str> = queue.play_order[queue.position + 1..queue.position + 4]
            .iter()
            .map(|&slot| queue.entries[slot].track_id.as_str())
            .collect();
        assert_eq!(after_current, vec!["a", "b", "c"]);
    }

    #[test]
    fn append_unshuffled_goes_to_end() {
        let mut queue = queue_of(3);

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::Append,
            vec!["new-track".to_string()],
        );

        assert_eq!(queue.entries.last().unwrap().track_id, "new-track");
        let last_slot = *queue.play_order.last().unwrap();
        assert_eq!(queue.entries[last_slot].track_id, "new-track");
    }

    #[test]
    fn append_shuffled_goes_to_end_of_play_order() {
        let mut queue = queue_of(5);
        queue.position = 1;
        shuffle(&mut queue, 3);

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::Append,
            vec!["new-track".to_string()],
        );

        let last_slot = *queue.play_order.last().unwrap();
        assert_eq!(queue.entries[last_slot].track_id, "new-track");
    }

    #[test]
    fn queue_edit_does_not_change_current_entry_or_position_target() {
        let mut queue = queue_of(5);
        queue.position = 2;
        let current_before = queue.current_entry().cloned();
        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["x".to_string()],
        );
        assert_eq!(queue.current_entry().cloned(), current_before);
        assert_eq!(queue.position, 2);

        let mut queue2 = queue_of(5);
        queue2.position = 2;
        let current_before2 = queue2.current_entry().cloned();
        apply_queue_batch(&mut queue2, QueueBatchMode::Append, vec!["y".to_string()]);
        assert_eq!(queue2.current_entry().cloned(), current_before2);
        assert_eq!(queue2.position, 2);
    }

    #[test]
    fn queue_edit_emits_no_load_and_keeps_session() {
        let mut queue = queue_of(3);
        let mut player = PlayerState::default();
        // Establish a session via a fresh Load, the only path allowed to touch these fields.
        let _ = load_current(&queue, &mut player);
        let player_before = player.clone();

        let effects = apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["x".to_string()],
        );

        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Audio(AudioCommand::Load { .. }))),
            "queue edit must not emit an Effect::Audio(AudioCommand::Load)"
        );
        assert_eq!(player.session, player_before.session);
        assert_eq!(player.play_reported, player_before.play_reported);
        assert_eq!(player.start_reported, player_before.start_reported);
    }

    #[test]
    fn play_order_is_a_permutation_after_every_edit() {
        let mut queue = queue_of(4);
        queue.position = 1;
        shuffle(&mut queue, 11);
        assert_permutation(&queue);

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["a".to_string(), "b".to_string()],
        );
        assert_permutation(&queue);

        apply_queue_batch(&mut queue, QueueBatchMode::Append, vec!["c".to_string()]);
        assert_permutation(&queue);

        unshuffle(&mut queue);
        assert_permutation(&queue);
    }
}
