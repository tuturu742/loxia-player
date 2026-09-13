# 08-08 · Session and history persistence

**Phase:** 08 — Cache & offline · **Agent:** B · **Size:** M
**Prerequisites:** `08-07`, `06-05`
**Reference:** `docs/06-cache-and-offline.md` §8, `docs/12-decisions.md` §6

## Goal
Save and restore the play queue, position, and view state across restarts, and persist listening
history separately.

## Files
- `crates/loxia-cache/src/session.rs`

## Specification

```
pub struct SessionSnapshot {
    pub schema_version: u32, pub server_id: ServerId, pub queue: QueueState,
    pub position_secs: f64, pub active_tab: Tab, pub zen_mode: bool, pub volume: u8,
    pub quality_profile: QualityProfile, pub eq: EqState, pub saved_at: Timestamp,
}
pub fn save(paths: &Paths, s: &SessionSnapshot) -> Result<(), CacheError>;
pub fn load(paths: &Paths) -> Result<Option<SessionSnapshot>, CacheError>;
pub fn append_history(paths: &Paths, e: &HistoryEntry) -> Result<(), CacheError>;
pub fn load_history(paths: &Paths) -> Result<VecDeque<HistoryEntry>, CacheError>;
```

**Save triggers:** on quit, on every track change, and every 15 s while playing (the 150th tick).
Writes are atomic — `.tmp`, `fsync`, rename.

**Restore**, when `ui.restore_session` is on:
1. Validate `schema_version`; a mismatch discards the snapshot with a `warn!`.
2. Validate `server_id == config.active_server`; a mismatch discards it. Restoring one server's
   queue against another's library would produce a queue of unplayable entries.
3. Rebuild `QueueState` **from the snapshot itself** — do not refetch tracks at startup. A
   50-entry queue would otherwise mean 50 requests before the UI is usable.
4. Restore tab, Zen state, volume, quality profile, and EQ.
5. **Restore paused**, at `position_secs`. The player bar shows `⏸ resumed at 01:24`. Auto-play on
   launch seizes the audio device and startles the user; `ui.restore_autoplay = true` opts in
   (`docs/12-decisions.md` §6).
6. Entries are marked `Unavailable` **lazily**, on first access, if the server no longer resolves
   them — not eagerly at startup.

**Corruption.** A snapshot that fails to parse is renamed `session.json.bad` and startup continues
with an empty queue plus an `Info` toast. Losing a queue is annoying; failing to start is worse.

**History** lives in its own `history.json` — a JSON array capped at 50, newest first — so clearing
the queue does not clear history. It is loaded at startup into `AppState.history`.

## Acceptance
- `snapshot_roundtrips` — save then load yields an identical `QueueState`.
- `save_is_atomic`
- `schema_version_mismatch_discards`
- `server_mismatch_discards`
- `restore_does_not_refetch_tracks` — wiremock asserts zero requests during restore.
- `session_restore_does_not_autoplay` — status is `Paused` after restore.
- `restore_autoplay_opt_in_works`
- `unavailable_marked_lazily_not_eagerly`
- `corrupt_snapshot_is_quarantined_and_startup_succeeds`
- `history_persists_separately_from_session`
- `clearing_queue_does_not_clear_history`
- `history_capped_at_fifty_on_disk`
- Manual, pasted into the PR: play a track, quit mid-song, relaunch; the queue, position, tab, and
  volume return and playback is **paused** at the saved position.

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 08's exit criteria in
`docs/08-roadmap.md` are met.
