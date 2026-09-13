# 08-05 · Offline browse index

**Phase:** 08 — Cache & offline · **Agent:** B · **Size:** M
**Prerequisites:** `08-04`
**Reference:** `docs/06-cache-and-offline.md` §6

## Goal
Build a browsable library from download sidecars, so the Artists, Albums, and Playlists tabs work
with no server.

## Files
- `crates/loxia-cache/src/offline_index.rs`

## Specification

```
pub struct OfflineIndex { .. }
impl OfflineIndex {
    pub fn build(downloads_root: &Path) -> Result<OfflineIndex, CacheError>;
    pub fn artists(&self) -> Vec<Artist>;
    pub fn discography(&self, artist: &ItemId) -> Discography;
    pub fn album_tracks(&self, album: &ItemId) -> Vec<Track>;
    pub fn search(&self, q: &str) -> SearchResults;
    pub fn contains(&self, id: &ItemId) -> bool;
}
```

**Build** walks `downloads_root` reading every `.loxia.json` sidecar and reconstructs the hierarchy
in memory. A sidecar that fails to parse is skipped with a `warn!` — one bad file must not make the
whole offline library unavailable.

Artists and albums are synthesised from the tracks' own fields, since only tracks have sidecars.
Counts (`album_count`, `track_count`) reflect **what is downloaded**, not what exists on the server;
showing a server-side count next to a partial offline library would be misleading.

**`discography` applies the same split as online**, using the same `AlbumArtists`-versus-`ArtistIds`
comparison from task `02-06`, so the ALBUMS / APPEARS ON sections and the `a`/`A` queue rules behave
identically offline. Extract the classification into a shared function in `loxia-core` in this task
rather than duplicating it — two copies will drift.

**Build cost.** Runs once at startup on the blocking pool, and is rebuilt on any pin or unpin. For a
10,000-track download set it must complete in under 2 seconds; if it does not, cache the built index
to disk keyed by the downloads-index mtime.

**Wiring.** When `connectivity == Offline`, the network worker serves `Effect::Net(FetchColumn)` and
`FetchDiscography` from `OfflineIndex` instead of HTTP, replying with the same `Event`s. The reducer
and UI need no offline-specific branches — that is the point of routing through effects.

Tabs the index cannot serve — Genres, Folders — return an empty result with an explanatory state
(tasks `07-04`, `07-05`).

## Acceptance
- `build_reconstructs_artists_and_albums_from_sidecars`
- `malformed_sidecar_is_skipped_with_warning`
- `counts_reflect_downloaded_not_server`
- `offline_discography_split_matches_online` — the same fixture data through both paths yields
  identical primary and appears-on sets.
- `classification_function_is_shared_not_duplicated` — a grep test, or the shared function's
  presence in `loxia-core`.
- `search_matches_titles_and_artists`
- `rebuild_on_pin_and_unpin`
- `build_10k_tracks_under_two_seconds`
- `offline_worker_serves_from_index` — with the HTTP mock returning connection errors, browsing
  still returns downloaded artists.

## Done when
The global DoD in `tasks/README.md` is satisfied.
