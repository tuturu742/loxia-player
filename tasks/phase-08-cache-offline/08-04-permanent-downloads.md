# 08-04 · Permanent downloads

**Phase:** 08 — Cache & offline · **Agent:** B · **Size:** L
**Prerequisites:** `08-03`
**Reference:** `docs/06-cache-and-offline.md` §5

## Goal
Pin tracks, albums, artists, and playlists to permanent storage with metadata sidecars, so the
library is browsable and playable with no server at all.

## Files
- `crates/loxia-cache/src/downloads.rs`

## Specification

```
pub enum DownloadScope { Track(Track), Album(ItemId), Artist(ItemId), Playlist(PlaylistId) }
pub async fn pin(&mut self, scope: DownloadScope) -> Result<(), CacheError>;
pub async fn unpin(&mut self, id: &ItemId) -> Result<u64, CacheError>;   // bytes freed
```

**Expansion.** A non-track scope is expanded to a track list, fetching if it is not already loaded.

**Queue.** Bounded concurrency of **2**, resumable via HTTP `Range` against `.part` files. Progress
is reported as `Event::DownloadProgress { id, done_bytes, total_bytes }` so the UI can show per-item
progress and the header's `↓n` badge.

**Quality.** `transcode.download_uncompressed = true` (the default) forces `QualityProfile::Direct`
regardless of the active profile. Pinning is for keeping, not for saving bytes — a user who pins an
album and later finds they kept the 96 kbps version has been badly served.

**Sidecars — this is what makes offline browsing work.** Alongside each audio file write
`<name>.loxia.json` holding the complete serialised `Track`, and fetch `cover.jpg` once per album
directory. Without them, an offline library is a directory of files with no metadata.

`downloads_index.json` maps `item_id → { path, profile, bytes, downloaded_at, sidecar }`, written
atomically like the cache manifest.

**Unpin** deletes the audio file, its sidecar, and — when the directory is left empty — the album
and artist directories, walking upward. `assert_within` guards every deletion.

**Downloads are never auto-evicted** and have no size cap. Settings displays the total.

**Interruption.** A pin interrupted by quit resumes on next launch: the index records queued items
with their `.part` files, and startup re-enqueues anything incomplete.

**Disk full.** `ENOSPC` cancels the whole batch, keeps what completed, and toasts once. Retrying
every remaining file to fail identically would just spam the log.

## Acceptance
- `pin_track_writes_audio_and_sidecar`
- `pin_album_writes_cover_once`
- `pin_expands_artist_to_all_tracks`
- `concurrency_limited_to_two`
- `resume_uses_range_request` — truncate a `.part`, re-pin, assert the `Range` header and a
  byte-exact final file.
- `download_forces_direct_profile`
- `download_forces_direct_even_when_active_profile_is_low`
- `progress_events_emitted`
- `unpin_removes_file_sidecar_and_empty_dirs`
- `unpin_leaves_non_empty_dirs`
- `unpin_returns_bytes_freed`
- `downloads_not_evicted_by_lru` — fill the LRU past its limit and assert downloads are untouched.
- `interrupted_pin_resumes_on_restart`
- `enospc_cancels_batch_and_toasts_once`
- `deletion_guarded_by_assert_within`

## Done when
The global DoD in `tasks/README.md` is satisfied.
