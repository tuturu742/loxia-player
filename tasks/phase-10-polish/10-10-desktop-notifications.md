# 10-10 · Desktop notifications

**Phase:** 10 — Polish · **Agent:** B · **Size:** S
**Prerequisites:** `06-01`, `10-01`
**Reference:** `design_overview` §4.3

## Goal
Native OS toast notifications on track change, with artwork.

## Files
- `crates/loxia-player/src/workers/notify.rs`

## Specification

```
pub fn spawn(effects: UnboundedReceiver<Effect>) -> JoinHandle<()>;
```
Handles `Effect::Sys(Notify(TrackChange))`, emitted by the reducer when the current entry changes
and `ui.desktop_notifications` is on.

**Content:** summary = track title; body = `<artist>\n<album>`; icon = the cached large artwork
file path, when one exists. `notify-rust` takes a file path rather than bytes, so use the image
cache path from task `02-12`; when the art has not been fetched, send with no icon rather than
delaying the notification.

**Rate limiting.** At most one notification per 3 seconds. A user skipping rapidly through an album
would otherwise fill their notification centre with twenty entries — the classic misbehaviour of
music-player notifications. Coalesce by dropping intermediate changes and notifying for the track
that is playing when the window closes.

**Suppression:**
- No notification when the app has focus, where the player bar already shows the same information.
  Terminal focus is detectable through crossterm's `FocusGained`/`FocusLost` events; when the
  terminal does not report focus, notify normally.
- No notification for a pause, resume, or seek — only a genuine track change.
- No notification on session restore at startup, which is not a track the user just chose.

**Failure is silent.** A missing notification daemon, a D-Bus error, or an unsupported platform
logs at `debug` and is otherwise ignored. Notifications are decoration; a toast complaining that a
toast could not be shown is absurd.

`notify-rust` blocks on some platforms, so run the send on the blocking pool.

## Acceptance
- `notifies_on_track_change`
- `does_not_notify_on_pause_resume_or_seek`
- `does_not_notify_on_session_restore`
- `rate_limited_to_one_per_three_seconds`
- `rapid_skips_coalesce_to_final_track`
- `suppressed_when_terminal_focused`
- `notifies_when_focus_unknown`
- `missing_artwork_sends_without_icon`
- `disabled_by_config`
- `daemon_failure_is_silent` — no toast, no error, one `debug` line.

## Done when
The global DoD in `tasks/README.md` is satisfied.
