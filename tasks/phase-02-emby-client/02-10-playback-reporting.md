# 02-10 · Playback reporting

**Phase:** 02 — Emby client · **Agent:** B · **Size:** S
**Prerequisites:** `02-09`
**Reference:** `docs/03-emby-api.md` §6

## Goal
Report playback to Emby so play counts and resume points work. Every call must be serialisable to
the offline scrobble buffer, so the payload types live here and are reused by `08-07`.

## Files
- `crates/loxia-emby/src/endpoints/playback.rs`

## Specification

```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlaybackReport {
    Start   { item: ItemId, session: PlaySessionId },
    Progress{ item: ItemId, session: PlaySessionId, position: Duration, paused: bool },
    Stopped { item: ItemId, session: PlaySessionId, position: Duration },
    Played  { item: ItemId },
}

pub async fn report(c: &EmbyClient, r: &PlaybackReport) -> Result<(), EmbyError>;
pub fn is_complete(position: Duration, duration: Duration) -> bool;
```

| Variant | Request |
| :-- | :-- |
| `Start` | `POST /Sessions/Playing` `{ ItemId, PlaySessionId, CanSeek: true, PositionTicks: 0 }` |
| `Progress` | `POST /Sessions/Playing/Progress` `{ ItemId, PlaySessionId, PositionTicks, IsPaused, PlayMethod }` |
| `Stopped` | `POST /Sessions/Playing/Stopped` `{ ItemId, PlaySessionId, PositionTicks }` |
| `Played` | `POST /Users/{uid}/PlayedItems/{id}` |

All ticks go through `dto::ticks::duration_to_ticks`.

`PlayMethod` is `"DirectStream"` for the `Direct` profile and `"Transcode"` otherwise; the caller
passes it in with the report.

**`is_complete`**: true when `position >= 0.9 * duration` **or** `position >= 240 s`, whichever
comes first. A zero `duration` is never complete. This one function gates both the `Played` report
and the history entry, so the two can never disagree.

`PlaySessionId` is a UUID v4 generated once per track load and reused for that track's whole
lifetime. A fresh id per report would make Emby treat each update as a new session.

These are mutations: **no retry**. Failures propagate so the caller can buffer them offline.

## Acceptance
- `report_snapshots` — an `insta` snapshot of the request path and JSON body for all four variants.
- `positions_are_converted_to_ticks`
- `play_method_reflects_quality_profile`
- `is_complete_at_90_percent`
- `is_complete_at_four_minutes_for_long_track` — a 60-minute track is complete at 4:00.
- `is_complete_false_for_zero_duration`
- `report_is_serde_roundtrippable` — required by the offline buffer in `08-07`.
- `report_does_not_retry`

## Done when
The global DoD in `tasks/README.md` is satisfied.
