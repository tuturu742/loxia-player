# 09-05 · Sleep timer

**Phase:** 09 — Advanced audio · **Agent:** C · **Size:** M
**Prerequisites:** `06-01`
**Reference:** `design_overview` §3.6, `docs/04-state-and-input.md` §9

## Goal
Stop playback after a duration, the current track, or the end of the queue, with an optional
fade-out and an optional application quit.

## Files
- `crates/loxia-core/src/reducer/player.rs` (extend)

## Specification

```
pub struct SleepTimer { pub trigger: SleepTrigger, pub fade_out: bool,
                        pub quit_after: bool, pub armed_at: Timestamp }
pub enum SleepTrigger { Duration(Duration), EndOfTrack, EndOfQueue }
```

**Evaluation happens on `Tick`**, using the timestamp carried on the action. The reducer must not
read a clock.

| Trigger | Fires when |
| :-- | :-- |
| `Duration(d)` | `tick_now >= armed_at + d` |
| `EndOfTrack` | `TrackEnded` for the entry that was current when armed |
| `EndOfQueue` | `TrackEnded` with no next entry, honouring repeat mode |

`EndOfQueue` with `Repeat::All` would never fire, so arming it disables repeat and toasts that it
did — silently arming a timer that cannot fire is worse than changing a setting the user can see.

**Fade-out.** When `fade_out` is on, begin a linear volume ramp over the **last 10 seconds** before
the trigger, reaching zero at the stop. For `EndOfTrack` and `EndOfQueue`, the ramp starts 10 s
before the track's end, computed from `duration - position`. The reducer emits
`Effect::Audio(SetVolume)` per tick during the ramp and **restores the original volume after
stopping** — otherwise the user finds their volume at zero the next morning, which is exactly the
scenario this feature is used in.

A track shorter than 10 s ramps over whatever time remains.

**On firing:** emit `Effect::Audio(Stop)`, restore volume, clear the timer, toast `sleep timer
finished`. When `quit_after` is set, also emit `Effect::Sys(Exit)` after the session snapshot.

**Header badge** shows the remaining time as `⏱23m` for `Duration`, `⏱track` for `EndOfTrack`, or
`⏱queue (3)` for `EndOfQueue` with the remaining entry count.

Re-arming replaces the existing timer. `Esc` in the modal cancels without disarming an already-armed
timer; disarming is an explicit choice in the modal.

## Acceptance
- `duration_trigger_fires_at_deadline`
- `end_of_track_fires_on_that_entry_only` — skipping to another track does not fire it early.
- `end_of_queue_honours_repeat_mode`
- `arming_end_of_queue_disables_repeat_all_and_toasts`
- `fade_ramps_over_last_ten_seconds`
- `fade_on_short_track_ramps_over_remaining_time`
- `volume_restored_after_stop`
- `volume_restored_even_when_quit_after_set`
- `quit_after_emits_exit_following_snapshot`
- `badge_text_per_trigger` — table test.
- `rearming_replaces_existing_timer`
- `timer_evaluated_from_tick_not_clock`

## Done when
The global DoD in `tasks/README.md` is satisfied.
