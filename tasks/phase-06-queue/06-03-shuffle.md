# 06-03 · Non-destructive shuffle

**Phase:** 06 — Queue engine · **Agent:** A · **Size:** M
**Prerequisites:** `06-01`
**Reference:** `docs/02-data-model.md` §4

## Goal
Shuffle that permutes upcoming playback without touching the queue's canonical order, and unshuffles
back to exactly where it started.

## Files
- `crates/loxia-core/src/queue/shuffle.rs`

## Specification

```
pub fn shuffle(q: &mut QueueState, seed: u64);
pub fn unshuffle(q: &mut QueueState);
```

**`shuffle`:**
1. Fisher–Yates over `play_order[position + 1 ..]` only.
2. `play_order[position]` — the entry playing right now — does not move.
3. `entries` is **never** touched.
4. Already-played entries (`play_order[..position]`) keep their order, so History and the "played"
   dimming in the queue view stay meaningful.
5. Set `shuffled = true`.

**`unshuffle`:**
1. Record the currently playing entry's index into `entries`.
2. Reset `play_order` to the identity permutation `0..entries.len()`.
3. Set `position` so the same entry is still current.
4. Set `shuffled = false`.

The round-trip must be **exact**: shuffle then unshuffle returns `play_order` to the identity and
leaves the same track playing. That is the whole promise of the feature — a user can shuffle an
album, hear something they like, and unshuffle back into the record's real sequence.

**RNG.** `rand_chacha::ChaCha8Rng::seed_from_u64(seed)`. The seed arrives on the action, supplied by
the runtime from the tick timestamp. `loxia-core` must not read a clock or a global RNG, or the
reducer stops being deterministic and every shuffle test becomes flaky.

**`ToggleShuffle`** calls whichever is appropriate and then emits `Effect::Audio(Preload)`, because
the next entry has almost certainly changed.

**Queue mutations while shuffled.** Appending pushes new indices onto the end of `play_order`; they
are **not** interleaved into the already-shuffled remainder. A user who queues an album while
shuffle is on expects it to play after what is already pending, not to be scattered through it.

## Acceptance
- `shuffle_preserves_current_track`
- `unshuffle_round_trips_to_original_order` (proptest, 1000 random queues and positions) —
  `play_order` is the identity and the same entry is current.
- `shuffle_does_not_reorder_entries`
- `shuffle_preserves_played_history_order`
- `shuffle_is_deterministic_for_a_seed`
- `different_seeds_give_different_orders`
- `shuffle_empty_queue_is_a_noop`
- `shuffle_single_entry_queue_is_a_noop`
- `shuffle_at_last_position_is_a_noop` — nothing after the current entry to permute.
- `append_while_shuffled_goes_to_the_end`
- `toggle_shuffle_emits_preload`
- `queue_invariants_hold_after_shuffle` — the three invariants from `06-01`.

## Done when
The global DoD in `tasks/README.md` is satisfied.
