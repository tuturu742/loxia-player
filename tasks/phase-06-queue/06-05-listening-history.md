# 06-05 · Listening history

**Phase:** 06 — Queue engine · **Agent:** A · **Size:** S
**Prerequisites:** `06-01`
**Reference:** `docs/02-data-model.md` §4, `docs/06-cache-and-offline.md` §8

## Goal
Record completed plays into a 50-entry ring, using the same completion threshold as the server-side
play count so the two can never disagree.

## Files
- `crates/loxia-core/src/reducer/player.rs` (extend)

## Specification

`AppState.history: VecDeque<HistoryEntry>`, capacity 50, **newest first**.

**When an entry is recorded.** On `Audio(TrackEnded)` or when the position crosses the completion
threshold, whichever happens first:
```
completed = position >= 0.9 * duration || position >= 240s
```
This is the same rule as `loxia_emby::endpoints::playback::is_complete`. It is duplicated here
because `loxia-core` cannot depend on `loxia-emby`; a test in phase 08 asserts the two agree.

Recording rules:
- A track skipped before the threshold is **not** recorded. History is what you listened to, not
  what you clicked past.
- `TrackEnded { natural: true }` always records, since natural end implies completion.
- Repeating one track records one entry per completed play — the timestamps differ, and a user
  looping a song wants to see that.
- A track already at the head of the ring is not duplicated when the same completion is signalled
  twice, guarding against a double `TrackEnded`.

`played_at` comes from the `Tick` timestamp carried on the action. **No clock reads in the reducer.**

Overflow drops the oldest entry.

Each recording also emits `Effect::Cache(AppendHistory(entry))`, which task `08-08` persists to
`history.json`. The in-memory ring is the source of truth for rendering; the file is for surviving
a restart.

`ToggleHistory` (`H`) flips `now_playing_subview` between `Queue` and `History`. It is valid on any
tab — jumping to Now Playing first would be an extra keystroke for no reason — but only visible
there.

## Acceptance
- `completed_track_is_recorded`
- `skipped_track_is_not_recorded` — ended at 20 % with `natural: false`.
- `natural_end_always_records`
- `threshold_matches_emby_is_complete` — table test over the same cases as
  `02-10`'s `is_complete` tests.
- `four_minute_rule_for_long_tracks`
- `repeat_one_records_each_play`
- `duplicate_completion_does_not_double_record`
- `history_capped_at_fifty`
- `newest_entry_is_first`
- `played_at_comes_from_tick_not_clock`
- `recording_emits_append_history_effect`
- `toggle_history_flips_subview`

## Done when
The global DoD in `tasks/README.md` is satisfied.
