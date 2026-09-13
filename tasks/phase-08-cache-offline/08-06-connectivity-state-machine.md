# 08-06 · Connectivity state machine

**Phase:** 08 — Cache & offline · **Agent:** B · **Size:** M
**Prerequisites:** `08-05`
**Reference:** `docs/06-cache-and-offline.md` §6

## Goal
Detect going offline and coming back, and switch the app's data source accordingly without the UI
needing to know.

## Files
- `crates/loxia-core/src/reducer/mod.rs` (extend)
- `crates/loxia-player/src/workers/network.rs` (extend)

## Specification

```
Online ──(2 consecutive EmbyError::Offline)──► Offline
Offline ──(probe succeeds)──► Reconnecting ──(scrobbles drained)──► Online
```

**Failure counting.** Only `EmbyError::Offline` counts. `Unauthorized`, `NotFound`, and `Transient`
do **not** — a 404 on one album is not a network outage, and treating it as one would put a working
app into offline mode.

The counter resets on any successful request.

**Probe.** `GET /System/Info/Public` on a jittered backoff of 5 s → 30 s while offline, plus an
**immediate** probe whenever the user takes an action needing the network. A user who reconnects
their VPN and presses a key should not wait 30 seconds.

**On entering Offline:**
- Set `connectivity = Offline`, toast once (`working offline`), show the header badge.
- The network worker begins serving reads from `OfflineIndex`.
- Playback reports are routed to the scrobble buffer (task `08-07`).
- In-flight requests are allowed to fail naturally rather than being cancelled.

**On entering Reconnecting:** toast `reconnected — syncing`, drain the scrobble buffer, then go
`Online` and mark visible columns `LoadState::Idle` so they refetch on next view. Do **not** refetch
everything at once — a large library would issue hundreds of requests at the moment the connection
is most fragile.

**Availability marking.** Entering Offline recomputes every queue entry's `availability` against the
cache manifest and downloads index: `Downloaded`, `Cached`, or `Unavailable`. Returning Online marks
everything `Remote` again except downloads.

**Reducer guards.** While offline, refuse and toast: favourite toggles, all playlist mutations,
instant mix, and queueing an `Unavailable` entry. Each has its own message naming what needs a
connection.

## Acceptance
- `two_offline_errors_trigger_offline`
- `one_offline_error_does_not`
- `success_resets_failure_counter`
- `not_found_does_not_count_as_offline`
- `unauthorized_does_not_count_as_offline`
- `probe_backoff_is_jittered_and_capped`
- `user_action_triggers_immediate_probe`
- `entering_offline_toasts_once` — a third failure does not re-toast.
- `entering_offline_recomputes_availability`
- `reconnect_drains_scrobbles_before_going_online`
- `reconnect_marks_columns_idle_not_refetch_all`
- `offline_refuses_mutations_with_specific_toasts` — table test over favourite, playlist add,
  playlist delete, instant mix.
- Manual, pasted into the PR: disconnect the network mid-session; confirm the badge appears,
  downloaded content still browses and plays, and reconnecting syncs.

## Done when
The global DoD in `tasks/README.md` is satisfied.
