# 08-07 · Scrobble buffer

**Phase:** 08 — Cache & offline · **Agent:** B · **Size:** M
**Prerequisites:** `08-06`, `06-07`
**Reference:** `docs/06-cache-and-offline.md` §7

## Goal
Buffer playback reports while offline and replay them on reconnect, so play counts survive a network
drop.

## Files
- `crates/loxia-cache/src/scrobble.rs`

## Specification

```
pub struct ScrobbleBuffer { .. }
impl ScrobbleBuffer {
    pub fn append(&mut self, r: PlaybackReport, at: Timestamp) -> Result<(), CacheError>;
    pub fn pending(&self) -> usize;
    pub async fn drain<F>(&mut self, send: F) -> Result<usize, CacheError>;
}
```

**Storage.** Append-only **JSON Lines** at `scrobbles.json`, one record per line:
`{ kind, item_id, server_id, position_ticks, occurred_at, play_session_id }`. Line-oriented so an
append is a single write with no read-modify-write, and a truncated final line loses one record
rather than the file.

**Collapsing on drain.** Keep only the **last** `Progress` per `play_session_id`; `Start`,
`Stopped`, and `Played` are all kept. A ten-minute offline album produces sixty `Progress` records
per track that convey nothing beyond the last one, and replaying them wastes the reconnect window.

**Order.** Replay chronologically by `occurred_at`. A record that fails stays in the buffer for the
next reconnect; the drain stops at the first failure rather than continuing, since a failure almost
certainly means the connection dropped again.

**Deduplication** on `(item_id, play_session_id, kind)` — guarding against a partial drain that
already delivered some records.

**Cap** at 5000 records. On overflow drop the **oldest `Progress` records first**; never drop
`Played`, which is the only one whose loss a user can observe in Emby.

**Corrupt lines** are skipped with a `warn!` and the drain continues. One malformed line must not
strand every other pending scrobble.

The buffer is loaded at startup so records survive a restart while still offline, and `pending()`
feeds the header badge.

## Acceptance
- `append_writes_one_line_per_record`
- `truncated_final_line_loses_only_that_record`
- `drain_collapses_progress_per_session`
- `drain_keeps_start_stopped_and_played`
- `drain_replays_chronologically`
- `drain_stops_at_first_failure_and_retains_remainder`
- `drain_deduplicates_by_session_and_kind`
- `overflow_drops_oldest_progress_first`
- `overflow_never_drops_played`
- `corrupt_line_is_skipped_and_drain_continues`
- `buffer_survives_restart`
- `pending_count_matches_records`
- Integration: with wiremock refusing connections, play three tracks to completion, restore the
  mock, and assert exactly three `Played` requests arrive in order.

## Done when
The global DoD in `tasks/README.md` is satisfied.
