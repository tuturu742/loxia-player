//! Non-destructive shuffle and unshuffle (`06-03`).

use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

use crate::state::queue::QueueState;

/// Fisher-Yates over `play_order[position + 1 ..]` only. `entries` is never touched, and neither
/// is `play_order[..= position]` — the entry playing right now, and everything already played,
/// keep their order, so History and the "played" dimming in the queue view stay meaningful. A
/// no-op on `play_order` itself when there is nothing after `position` to permute (empty queue,
/// single entry, or already at the last position) — `shuffled` is still set, since the user's
/// toggle happened regardless of whether there was anything to shuffle.
pub fn shuffle(q: &mut QueueState, seed: u64) {
    if q.position + 1 < q.play_order.len() {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        q.play_order[q.position + 1..].shuffle(&mut rng);
    }
    q.shuffled = true;
}

/// Resets `play_order` to the identity permutation `0..entries.len()`, repositioning `position`
/// so the same entry (by index into `entries`, not by play-order slot) is still current. The
/// round-trip with [`shuffle`] is exact: shuffle then unshuffle returns `play_order` to the
/// identity and leaves the same track playing.
pub fn unshuffle(q: &mut QueueState) {
    let current_index = q.play_order.get(q.position).copied();
    q.play_order = (0..q.entries.len()).collect();
    if let Some(current_index) = current_index {
        q.position = current_index;
    }
    q.shuffled = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::QueueEntryId;
    use crate::state::queue::{Availability, QueueEntry, QueueSource};
    use crate::test_support::fixtures;

    fn queue_of(n: usize) -> QueueState {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let entries = (0..n)
            .map(|i| QueueEntry {
                entry_id: QueueEntryId(i as u64),
                track: fixtures::track(&format!("T{i}"), i as u32, &alb, &[&a]),
                source: QueueSource::Manual,
                availability: Availability::Remote,
            })
            .collect();
        QueueState {
            entries,
            play_order: (0..n).collect(),
            position: 0,
            ..QueueState::default()
        }
    }

    #[test]
    fn shuffle_empty_queue_is_a_noop() {
        let mut q = queue_of(0);
        shuffle(&mut q, 1);
        assert!(q.play_order.is_empty());
        assert!(q.shuffled);
    }

    #[test]
    fn shuffle_single_entry_queue_is_a_noop() {
        let mut q = queue_of(1);
        shuffle(&mut q, 1);
        assert_eq!(q.play_order, vec![0]);
        assert!(q.shuffled);
    }

    #[test]
    fn shuffle_at_last_position_is_a_noop() {
        let mut q = queue_of(5);
        q.position = 4;
        let before = q.play_order.clone();
        shuffle(&mut q, 1);
        assert_eq!(q.play_order, before);
    }

    #[test]
    fn shuffle_preserves_current_track() {
        let mut q = queue_of(10);
        q.position = 3;
        let current_before = q.play_order[q.position];
        shuffle(&mut q, 42);
        assert_eq!(q.play_order[q.position], current_before);
    }

    #[test]
    fn shuffle_does_not_reorder_entries() {
        let mut q = queue_of(10);
        q.position = 2;
        let entries_before = q.entries.clone();
        shuffle(&mut q, 42);
        assert_eq!(q.entries, entries_before);
    }

    #[test]
    fn shuffle_preserves_played_history_order() {
        let mut q = queue_of(10);
        q.position = 4;
        let played_before = q.play_order[..=4].to_vec();
        shuffle(&mut q, 7);
        assert_eq!(q.play_order[..=4], played_before[..]);
    }

    #[test]
    fn shuffle_is_deterministic_for_a_seed() {
        let mut q1 = queue_of(20);
        let mut q2 = queue_of(20);
        shuffle(&mut q1, 99);
        shuffle(&mut q2, 99);
        assert_eq!(q1.play_order, q2.play_order);
    }

    #[test]
    fn different_seeds_give_different_orders() {
        let mut q1 = queue_of(20);
        let mut q2 = queue_of(20);
        shuffle(&mut q1, 1);
        shuffle(&mut q2, 2);
        assert_ne!(q1.play_order, q2.play_order);
    }

    #[test]
    fn unshuffle_resets_identity_and_keeps_current_entry() {
        let mut q = queue_of(10);
        q.position = 3;
        let current_entry_index = q.play_order[q.position];
        shuffle(&mut q, 42);
        unshuffle(&mut q);
        assert_eq!(q.play_order, (0..10).collect::<Vec<_>>());
        assert_eq!(q.play_order[q.position], current_entry_index);
        assert!(!q.shuffled);
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(1000))]
        #[test]
        fn unshuffle_round_trips_to_original_order(
            n in 1usize..30,
            position in 0usize..30,
            seed in proptest::prelude::any::<u64>(),
        ) {
            let mut q = queue_of(n);
            q.position = position % n;
            let current_entry_index = q.play_order[q.position];

            shuffle(&mut q, seed);
            unshuffle(&mut q);

            proptest::prop_assert_eq!(&q.play_order, &(0..n).collect::<Vec<_>>());
            proptest::prop_assert_eq!(q.play_order[q.position], current_entry_index);
        }
    }
}
