# 06-07 · Playback reporting wiring

**Phase:** 06 — Queue engine · **Agent:** A · **Size:** S
**Prerequisites:** `06-05`, `02-10`
**Reference:** `docs/03-emby-api.md` §6, `docs/04-state-and-input.md` §9

## Goal
Emit playback reports so Emby's play counts and resume points stay correct. The reports must be
shaped so the offline buffer in phase 08 can replay them unchanged.

## Files
- `crates/loxia-core/src/reducer/player.rs` (extend)
- `crates/loxia-player/src/workers/network.rs` (extend)

## Specification

**Session id.** Generate a `PlaySessionId` (UUID v4) once per `Load` and keep it in `PlayerState`
for that track's whole lifetime. A new id per report makes Emby treat every update as a separate
session, which corrupts the resume point.

**When each report is emitted:**
| Report | Trigger |
| :-- | :-- |
| `Start` | on `Audio(StatusChanged(Playing))` following a `Load` |
| `Progress` | every 10 s (the 100th tick), **and** on pause, resume, and seek |
| `Stopped` | on `TrackEnded` in either form, and on `Clear`, and at quit |
| `Played` | when `is_complete` first becomes true for the current track |

`Played` fires **once per track load**, guarded by a flag in `PlayerState` that resets on `Load`.
Without the guard, every subsequent tick past the threshold re-reports.

`PlayMethod` is `DirectStream` for `QualityProfile::Direct` and `Transcode` otherwise, taken from
`player.quality_profile` at report time.

**Effect.** All four emit `Effect::Net(ReportPlayback(PlaybackReport))`, carrying the serialisable
type from task `02-10`. The worker calls `endpoints::playback::report`. On `EmbyError::Offline` the
worker emits `Effect::Cache(AppendScrobble(report))` instead — implemented in `08-07`, stubbed with
a `warn!` here.

**Quit.** The shutdown path emits a final `Stopped` and awaits its dispatch before exiting, so a
clean quit does not lose the resume point. It is bounded by the 2-second drain timeout from `03-08`.

## Acceptance
- `start_reported_once_after_load`
- `progress_reported_every_ten_seconds`
- `progress_reported_on_pause_resume_and_seek`
- `stopped_reported_on_track_end`
- `stopped_reported_on_clear`
- `played_reported_once_per_load` — 20 further ticks past the threshold emit no second report.
- `played_flag_resets_on_new_load`
- `session_id_stable_for_a_track`
- `session_id_changes_between_tracks`
- `play_method_reflects_quality_profile`
- `quit_emits_final_stopped`
- Manual, pasted into the PR: play a track to completion and confirm the play count increments in
  the Emby web UI; stop mid-track and confirm the resume position appears there.

## Done when
The global DoD in `tasks/README.md` is satisfied.
