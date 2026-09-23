//! Queue shuffle (`06-03`): `play_order` becomes a random permutation of `entries`' indices.
//! Un-shuffling restores natural order — `play_order = (0..entries.len()).collect()` — and moves
//! `position` to wherever the entry that was playing now sits, so shuffling and un-shuffling
//! never change what is currently playing, only what comes next.

use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

use crate::state::queue::QueueState;

/// Shuffles `queue.play_order` in place with a seeded, reproducible RNG. `entries` itself is
/// never touched — only the permutation of indices into it.
pub fn shuffle(queue: &mut QueueState, seed: u64) {
    let current = queue.current_slot();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    queue.play_order.shuffle(&mut rng);
    queue.shuffled = true;
    relocate_position(queue, current);
}

/// Restores `play_order` to natural (insertion) order and repositions `position` so the entry
/// that was playing before un-shuffling is still the entry playing after.
pub fn unshuffle(queue: &mut QueueState) {
    let current = queue.current_slot();
    queue.play_order = (0..queue.entries.len()).collect();
    queue.shuffled = false;
    relocate_position(queue, current);
}

fn relocate_position(queue: &mut QueueState, current: Option<usize>) {
    if let Some(slot) = current
        && let Some(pos) = queue.play_order.iter().position(|&idx| idx == slot)
    {
        queue.position = pos;
    }
}

#[cfg(test)]
mod tests {
    //! Characterization tests for insert-next (`i`) and append (`a`) queue edits
    //! (`crates/loxia-core/src/reducer/queue.rs`'s `QueueBatch`/`QueueBatchMode` arms), added
    //! alongside the pre-existing shuffle tests since both operate on the same
    //! `entries`/`play_order`/`position` invariants and share the `queue_of` fixture.
    //!
    //! PR summary: every test below passes against current behaviour. Reading the insert-next
    //! and append arms in `reducer/queue.rs` turned up no defect that needed pinning with
    //! `#[ignore]` — `insert_next` correctly re-homes existing `play_order` references when it
    //! inserts into the middle of `entries`, `append` never touches anything before the tail, and
    //! neither arm touches `PlayerState` or emits `Effect::Audio(AudioCommand::Load)`. No tests
    //! are ignored.

    use super::*;
    use crate::effect::{AudioCommand, Effect};
    use crate::reducer::queue::{QueueBatchMode, apply_queue_batch, load_current};
    use crate::state::player::PlayerState;
    use crate::state::queue::{QueueEntry, QueueState};

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

    // ---- pre-existing shuffle tests (unchanged) ----

    #[test]
    fn shuffle_is_a_permutation_of_entries() {
        let mut queue = queue_of(20);
        shuffle(&mut queue, 42);
        assert_permutation(&queue);
    }

    #[test]
    fn unshuffle_restores_insertion_order() {
        let mut queue = queue_of(10);
        shuffle(&mut queue, 7);
        unshuffle(&mut queue);
        assert_eq!(queue.play_order, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn unshuffle_keeps_current_entry_current() {
        let mut queue = queue_of(10);
        shuffle(&mut queue, 7);
        let current = queue.current_entry().cloned();
        unshuffle(&mut queue);
        assert_eq!(queue.current_entry().cloned(), current);
    }

    // ---- insert-next / append characterization tests ----

    #[test]
    fn insert_next_unshuffled_plays_immediately_after_current() {
        let mut queue = queue_of(4);
        queue.position = 1;

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["new-track".to_string()],
        );

        let next_slot = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[next_slot].track_id, "new-track");
    }

    #[test]
    fn insert_next_shuffled_plays_immediately_after_current() {
        let mut queue = queue_of(6);
        shuffle(&mut queue, 3);

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["new-track".to_string()],
        );

        let next_slot = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[next_slot].track_id, "new-track");
    }

    #[test]
    fn insert_next_shuffled_then_unshuffle_keeps_entry_after_current() {
        let mut queue = queue_of(6);
        shuffle(&mut queue, 3);
        let current = queue.current_entry().cloned();

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["new-track".to_string()],
        );
        unshuffle(&mut queue);

        assert_eq!(queue.current_entry().cloned(), current);
        let next_slot = queue.play_order[queue.position + 1];
        assert_eq!(queue.entries[next_slot].track_id, "new-track");
    }

    #[test]
    fn insert_next_multi_select_preserves_selection_order() {
        let mut queue = queue_of(4);
        queue.position = 0;

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec![
                "sel-a".to_string(),
                "sel-b".to_string(),
                "sel-c".to_string(),
            ],
        );

        let inserted: Vec<_> = queue.play_order[queue.position + 1..queue.position + 4]
            .iter()
            .map(|&idx| queue.entries[idx].track_id.clone())
            .collect();
        assert_eq!(inserted, vec!["sel-a", "sel-b", "sel-c"]);
    }

    #[test]
    fn append_unshuffled_goes_to_end() {
        let mut queue = queue_of(4);

        apply_queue_batch(&mut queue, QueueBatchMode::Append, vec!["tail".to_string()]);

        let last_slot = *queue.play_order.last().expect("non-empty queue");
        assert_eq!(queue.entries[last_slot].track_id, "tail");
    }

    #[test]
    fn append_shuffled_goes_to_end_of_play_order() {
        let mut queue = queue_of(6);
        shuffle(&mut queue, 11);

        apply_queue_batch(&mut queue, QueueBatchMode::Append, vec!["tail".to_string()]);

        let last_slot = *queue.play_order.last().expect("non-empty queue");
        assert_eq!(queue.entries[last_slot].track_id, "tail");
    }

    #[test]
    fn queue_edit_does_not_change_current_entry_or_position_target() {
        let mut queue = queue_of(6);
        shuffle(&mut queue, 5);
        let current = queue.current_entry().cloned();

        apply_queue_batch(&mut queue, QueueBatchMode::InsertNext, vec!["x".to_string()]);
        assert_eq!(queue.current_entry().cloned(), current);

        apply_queue_batch(&mut queue, QueueBatchMode::Append, vec!["y".to_string()]);
        assert_eq!(queue.current_entry().cloned(), current);
    }

    #[test]
    fn queue_edit_emits_no_load_and_keeps_session() {
        let mut queue = queue_of(3);
        let mut player = PlayerState::default();
        load_current(&queue, &mut player);

        let session_before = player.session.clone();
        let play_reported_before = player.play_reported;
        let start_reported_before = player.start_reported;

        let insert_effects = apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["z".to_string()],
        );
        let append_effects =
            apply_queue_batch(&mut queue, QueueBatchMode::Append, vec!["w".to_string()]);

        for effects in [&insert_effects, &append_effects] {
            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::Audio(AudioCommand::Load { .. }))),
                "queue edits must not emit Effect::Audio(Load)"
            );
        }
        assert_eq!(player.session, session_before);
        assert_eq!(player.play_reported, play_reported_before);
        assert_eq!(player.start_reported, start_reported_before);
    }

    #[test]
    fn play_order_is_a_permutation_after_every_edit() {
        let mut queue = queue_of(5);
        shuffle(&mut queue, 99);
        assert_permutation(&queue);

        apply_queue_batch(
            &mut queue,
            QueueBatchMode::InsertNext,
            vec!["p1".to_string()],
        );
        assert_permutation(&queue);

        apply_queue_batch(&mut queue, QueueBatchMode::Append, vec!["p2".to_string()]);
        assert_permutation(&queue);

        unshuffle(&mut queue);
        assert_permutation(&queue);
    }
}
