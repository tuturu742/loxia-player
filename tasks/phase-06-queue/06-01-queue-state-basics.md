# 06-01 · Queue state basics

**Phase:** 06 — Queue engine · **Agent:** A · **Size:** M
**Prerequisites:** `03-06`, `05-06`
**Reference:** `docs/02-data-model.md` §4, `docs/04-state-and-input.md` §4

## Goal
The core queue reducer: append, insert, remove, reorder, advance, and repeat. Shuffle and sorting
are separate tasks that build on these invariants.

## Files
- `crates/loxia-core/src/reducer/queue.rs`

## Specification

`QueueState` keeps `entries` (canonical order) and `play_order` (indices into `entries`).
**Every operation must maintain three invariants**, checked by a debug assertion after each mutation:
1. `play_order` is a permutation of `0..entries.len()` — no duplicates, no gaps.
2. `position < play_order.len()`, or the queue is empty.
3. `entry_id`s are unique.

| Action | Behaviour |
| :-- | :-- |
| `QueueSelection` | append entries; if nothing is playing, set position 0 and emit `Effect::Audio(Load)` |
| `InsertNext` | insert into `entries` and place the indices immediately after `position` in `play_order` |
| `RemoveEntry` | remove by `QueueEntryId`; renumber `play_order`; if it was the current entry, advance |
| `MoveEntry { from, to }` | reorder within `play_order` only; `entries` is untouched |
| `Clear` | empty everything, emit `Effect::Audio(Stop)` |
| `JumpTo(id)` | set `position` to that entry, emit `Load` |
| `CycleRepeat` | `Off → All → One → Off` |

**Advance on `Audio(TrackEnded { natural: true })`:**
- `Repeat::One` → reload the same entry from position 0.
- `position + 1 < len` → advance and `Load`.
- At the end with `Repeat::All` → wrap to 0 and `Load`.
- At the end with `Repeat::Off` → `PlayStatus::Stopped`, position stays at the last entry so the
  user can see what finished.

`TrackEnded { natural: false }` never advances — it is the result of an explicit stop or a skip that
already moved the position.

**`Next`/`Prev`.** `Next` advances regardless of repeat mode, except that `Repeat::One` still moves
to the next entry — a user pressing skip wants the next track, not the same one again. `Prev`
restarts the current track when position is past 3 seconds, and only otherwise moves back; this is
the behaviour every music player has and users expect without being told.

**Availability gate.** Queueing an entry whose `availability` is `Unavailable` while
`connectivity == Offline` is refused with the toast `not available offline`. Mixed selections queue
the available entries and toast the count skipped.

Every queue mutation that changes the entry at `position + 1` must emit `Effect::Audio(Preload)` —
implemented in task `06-06`, but leave the call site marked so it is not forgotten.

## Acceptance
- `queue_invariants_hold` (proptest) — 200 random operation sequences; all three invariants hold
  after every step.
- `append_to_empty_queue_starts_playback`
- `append_to_playing_queue_does_not_interrupt`
- `insert_next_places_after_current`
- `remove_current_entry_advances`
- `remove_renumbers_play_order`
- `move_entry_does_not_reorder_entries`
- `repeat_one_replays_same_entry`
- `repeat_all_wraps_at_end`
- `auto_advance_stops_at_end_when_repeat_off`
- `unnatural_track_ended_does_not_advance`
- `next_skips_past_repeat_one`
- `prev_restarts_after_three_seconds`
- `prev_moves_back_before_three_seconds`
- `offline_refuses_to_queue_unavailable_track`
- `mixed_selection_queues_available_and_toasts_count`
- `duplicate_track_gets_distinct_entry_ids`

## Done when
The global DoD in `tasks/README.md` is satisfied.
