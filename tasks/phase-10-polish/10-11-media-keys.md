# 10-11 · Media keys (MPRIS / SMTC)

**Phase:** 10 — Polish · **Agent:** C · **Size:** M
**Prerequisites:** `06-01`
**Reference:** `design_overview` §4.4

## Goal
Hardware media-key control and OS now-playing integration through `souvlaki` — MPRIS on Linux, SMTC
on Windows, and the CoreAudio now-playing centre on macOS.

## Files
- `crates/loxia-player/src/workers/mpris.rs`

## Specification

```
pub fn spawn(effects: UnboundedReceiver<Effect>, events: UnboundedSender<Event>) -> JoinHandle<()>;
```

**Outbound** — `Effect::Sys(UpdateMpris(MprisMeta))`, emitted on track change, status change, and
every 10 s while playing (position updates). Sets: title, artist, album, duration, position,
playback status, and the artwork file URL when cached.

**Inbound** — `souvlaki`'s media control events map to actions:
| Control | Action |
| :-- | :-- |
| `Play`, `Pause`, `Toggle` | `Player::PlayPause` |
| `Next`, `Previous` | `Player::Next` / `Prev` |
| `Stop` | `Player::Stop` |
| `SetPosition(d)` | `Player::Seek(Absolute(d))` |
| `SetVolume(v)` | `Player::SetVolume` |

These arrive on `souvlaki`'s own callback thread and are forwarded into the Event channel — they
must not touch `AppState` directly, which lives on the main thread.

**Platform setup.**
- Linux: a D-Bus name of `org.mpris.MediaPlayer2.loxia`. On a headless system with no session bus,
  initialisation fails; that is expected.
- Windows: SMTC needs a window handle. `souvlaki` can create a hidden one; do so at startup and keep
  it for the process lifetime.
- macOS: the now-playing centre needs the main thread for setup on some versions — initialise before
  spawning the worker.

**Failure is non-fatal and silent.** No D-Bus, no session, an unsupported platform: log at `info`
once and continue. A terminal music player must work over SSH, where none of this exists.

**Position updates are throttled** to 10 s. MPRIS clients poll `Position` themselves; pushing an
update at 4 Hz spams the bus for no benefit.

## Acceptance
- `metadata_updated_on_track_change`
- `metadata_updated_on_status_change`
- `position_updates_throttled_to_ten_seconds`
- `control_event_mapping` — table test over all six controls.
- `controls_forwarded_via_event_channel_not_direct_mutation`
- `init_failure_is_non_fatal` — a stub that fails; the app runs normally with one `info` line.
- `artwork_url_omitted_when_not_cached`
- Manual, pasted into the PR: on Linux, `playerctl play-pause` and `playerctl metadata` work, and
  keyboard media keys control playback; repeat on one of macOS/Windows.

## Done when
The global DoD in `tasks/README.md` is satisfied.
