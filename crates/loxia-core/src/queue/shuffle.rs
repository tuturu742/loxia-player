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
        assert_eq!(
            sorted, expected,
            "play_order must always be a permutation of 0..entries.len()"
        );
    }

    /// The entry that is "current": `entries[play_order[position]]`, per this module's own doc
    /// comment — never read any other way from a test, so a bug in this helper would show up as
    /// every test using it failing identically.
    fn current_entry(state: &AppState) -> &QueueEntry {
        &state.queue.entries[state.queue.play_order[state.queue.position]]
    }

    /// A single freshly-built `QueueEntry`, obtained by building a one-item queue via the
    /// `queue_of` fixture and pulling its only entry back out — avoids hard-coding this module's
    /// own copy of `QueueEntry`'s constructor/fields, which belong to `state::queue`, not here.
    fn one_entry(label: &str) -> QueueEntry {
        queue_of(&[label]).queue.entries.remove(0)
    }

    // ---- existing shuffle/unshuffle tests (unchanged) ----

    #[test]
    fn shuffle_pins_current_entry_and_never_reorders_entries() {
        let mut state = queue_of(&["a", "b", "c", "d", "e"]);
        state.queue.position = 2;
        let current_before = current_entry(&state).clone();
        let entries_before = state.queue.entries.clone();

        shuffle(&mut state.queue, 42);

        assert_eq!(state.queue.position, 0, "shuffle pins the current entry to the front");
        assert_eq!(current_entry(&state), &current_before);
        assert_permutation(&state.queue);
        assert_eq!(
            state.queue.entries, entries_before,
            "shuffle must never reorder entries, only play_order"
        );
    }

    #[test]
    fn shuffle_is_deterministic_for_a_given_seed() {
        let mut a = queue_of(&["a", "b", "c", "d", "e"]);
        let mut b = queue_of(&["a", "b", "c", "d", "e"]);
        a.queue.position = 3;
        b.queue.position = 3;

        shuffle(&mut a.queue, 7);
        shuffle(&mut b.queue, 7);

        assert_eq!(a.queue.play_order, b.queue.play_order);
    }

    #[test]
    fn unshuffle_resets_play_order_and_relocates_position() {
        let mut state = queue_of(&["a", "b", "c", "d", "e"]);
        state.queue.position = 3;
        let current_before = current_entry(&state).clone();
        shuffle(&mut state.queue, 99);

        unshuffle(&mut state.queue);

        let identity: Vec<usize> = (0..state.queue.entries.len()).collect();
        assert_eq!(state.queue.play_order, identity);
        assert_eq!(current_entry(&state), &current_before);
    }

    // ---- new characterization tests: insert-next / append (`QueueBatch`/`QueueBatchMode`) ----

    #[test]
    fn insert_next_unshuffled_plays_immediately_after_current() {
        let mut state = queue_of(&["a", "b", "c", "d"]);
        state.queue.position = 1;
        let current_before = current_entry(&state).clone();
        let new_entry = one_entry("x");

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry.clone()],
                mode: QueueBatchMode::InsertNext,
            }),
        );

        assert_eq!(current_entry(&state), &current_before);
        let next_idx = state.queue.play_order[state.queue.position + 1];
        assert_eq!(
            state.queue.entries[next_idx], new_entry,
            "'insert next' must place the new entry immediately after the current one in play_order"
        );
        assert_permutation(&state.queue);
    }

    #[test]
    fn insert_next_shuffled_plays_immediately_after_current() {
        let mut state = queue_of(&["a", "b", "c", "d"]);
        state.queue.position = 2;
        shuffle(&mut state.queue, 13);
        let current_before = current_entry(&state).clone();
        let new_entry = one_entry("x");

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry.clone()],
                mode: QueueBatchMode::InsertNext,
            }),
        );

        assert_eq!(current_entry(&state), &current_before);
        let next_idx = state.queue.play_order[state.queue.position + 1];
        assert_eq!(state.queue.entries[next_idx], new_entry);
        assert_permutation(&state.queue);
    }

    #[test]
    #[ignore = "fixed by 06-09-insert-next-unshuffle-ordering"]
    fn insert_next_shuffled_then_unshuffle_keeps_entry_after_current() {
        let mut state = queue_of(&["a", "b", "c", "d"]);
        state.queue.position = 0;
        shuffle(&mut state.queue, 21);
        let current_before = current_entry(&state).clone();
        let new_entry = one_entry("x");

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry.clone()],
                mode: QueueBatchMode::InsertNext,
            }),
        );

        unshuffle(&mut state.queue);

        assert_eq!(current_entry(&state), &current_before);
        let next_idx = state.queue.play_order[state.queue.position + 1];
        assert_eq!(
            state.queue.entries[next_idx], new_entry,
            "an entry inserted 'next' should still play immediately after the current entry \
             once the queue is un-shuffled"
        );
    }

    #[test]
    fn insert_next_multi_select_preserves_selection_order() {
        let mut state = queue_of(&["a", "b", "c"]);
        state.queue.position = 0;
        let new_entries = vec![one_entry("x"), one_entry("y"), one_entry("z")];

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: new_entries.clone(),
                mode: QueueBatchMode::InsertNext,
            }),
        );

        let start = state.queue.position + 1;
        let inserted: Vec<QueueEntry> = state.queue.play_order[start..start + new_entries.len()]
            .iter()
            .map(|&idx| state.queue.entries[idx].clone())
            .collect();
        assert_eq!(
            inserted, new_entries,
            "a multi-select insert-next must preserve the order the items were selected in"
        );
        assert_permutation(&state.queue);
    }

    #[test]
    fn append_unshuffled_goes_to_end() {
        let mut state = queue_of(&["a", "b", "c"]);
        state.queue.position = 0;
        let new_entry = one_entry("x");

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry.clone()],
                mode: QueueBatchMode::Append,
            }),
        );

        let last_idx = *state
            .queue
            .play_order
            .last()
            .expect("play_order is never empty after an append");
        assert_eq!(state.queue.entries[last_idx], new_entry);
        assert_permutation(&state.queue);
    }

    #[test]
    fn append_shuffled_goes_to_end_of_play_order() {
        let mut state = queue_of(&["a", "b", "c", "d"]);
        state.queue.position = 1;
        shuffle(&mut state.queue, 5);
        let current_before = current_entry(&state).clone();
        let new_entry = one_entry("x");

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![new_entry.clone()],
                mode: QueueBatchMode::Append,
            }),
        );

        assert_eq!(current_entry(&state), &current_before);
        let last_idx = *state
            .queue
            .play_order
            .last()
            .expect("play_order is never empty after an append");
        assert_eq!(state.queue.entries[last_idx], new_entry);
        assert_permutation(&state.queue);
    }

    #[test]
    fn queue_edit_does_not_change_current_entry_or_position_target() {
        let mut state = queue_of(&["a", "b", "c", "d"]);
        state.queue.position = 2;
        let current_before = current_entry(&state).clone();

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![one_entry("x")],
                mode: QueueBatchMode::InsertNext,
            }),
        );
        assert_eq!(current_entry(&state), &current_before);

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![one_entry("y")],
                mode: QueueBatchMode::Append,
            }),
        );
        assert_eq!(current_entry(&state), &current_before);
    }

    #[test]
    fn queue_edit_emits_no_load_and_keeps_session() {
        let mut state = queue_of(&["a", "b", "c"]);
        state.queue.position = 0;
        let session_before = state.player.session.clone();
        let play_reported_before = state.player.play_reported;
        let start_reported_before = state.player.start_reported;

        let effects = reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![one_entry("x")],
                mode: QueueBatchMode::Append,
            }),
        );

        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Audio(AudioEffect::Load { .. }))),
            "a queue edit (insert-next/append) must never emit an audio Load effect: {effects:?}"
        );
        assert_eq!(state.player.session, session_before);
        assert_eq!(state.player.play_reported, play_reported_before);
        assert_eq!(state.player.start_reported, start_reported_before);
    }

    #[test]
    fn play_order_is_a_permutation_after_every_edit() {
        let mut state = queue_of(&["a", "b", "c", "d"]);
        state.queue.position = 1;
        assert_permutation(&state.queue);

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![one_entry("x")],
                mode: QueueBatchMode::InsertNext,
            }),
        );
        assert_permutation(&state.queue);

        reducer::reduce(
            &mut state,
            Action::QueueBatch(QueueBatch {
                entries: vec![one_entry("y")],
                mode: QueueBatchMode::Append,
            }),
        );
        assert_permutation(&state.queue);

        shuffle(&mut state.queue, 3);
        assert_permutation(&state.queue);

        unshuffle(&mut state.queue);
        assert_permutation(&state.queue);
    }
}
