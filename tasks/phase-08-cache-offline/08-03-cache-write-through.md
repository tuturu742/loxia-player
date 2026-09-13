# 08-03 · Cache write-through

**Phase:** 08 — Cache & offline · **Agent:** B · **Size:** M
**Prerequisites:** `08-02`, `06-01`
**Reference:** `docs/06-cache-and-offline.md` §4

## Goal
Serve cached files to the audio engine and populate the cache in the background, so a second play of
a track needs no network at all.

## Files
- `crates/loxia-cache/src/lru.rs` (extend)
- `crates/loxia-player/src/workers/cache.rs`

## Specification

**The play-time decision.** mpv fetches its own stream over HTTP and we cannot cheaply tee it, so:
```
if manifest.get(key).is_some_and(|e| e.complete) {
    hand mpv  file://<path>
} else {
    hand mpv  <network url>
    and start an independent background fetch of the same URL
}
```
A first play costs double bandwidth. In exchange the logic is trivially correct, resumable, and
shares one code path with pinning. `cache.prefetch_on_play = false` disables the background fetch
for metered connections.

**Background fetch.** At most **one concurrent** track, so it never competes with playback for
bandwidth. Streams to `<path>.part`, renames to the final path on completion, then marks
`complete: true`. A failure leaves the `.part` for a later resume and is logged at `debug` — a
caching failure is never user-visible, because playback is unaffected.

**Cancellation.** When the track being background-fetched is no longer the current or preloaded
entry, cancel it. A user skipping quickly through an album should not queue up twenty downloads.

**Reducer integration.** Before emitting `Effect::Audio(Load)`, the reducer emits
`Effect::Cache(EnsureCached { track, profile })`. The cache worker replies with
`Event::CacheResolved { key, path: Option<PathBuf> }`, and the reducer emits the `Load` with either
the local path or the network URL. This keeps the reducer free of I/O while still letting it decide.

To avoid delaying playback when the cache is slow, the resolve is bounded by a 50 ms timeout in the
worker; a timeout replies `None` and playback proceeds from the network.

**Access tracking.** A cache hit updates `last_access` so the LRU ordering reflects real use.

## Acceptance
- `cache_hit_returns_file_path`
- `cache_miss_returns_none_and_starts_fetch`
- `second_play_makes_no_network_request` — wiremock counts requests across two plays.
- `background_fetch_limited_to_one_concurrent`
- `prefetch_disabled_by_config`
- `partial_download_leaves_part_file`
- `fetch_cancelled_when_track_no_longer_relevant`
- `cache_resolve_times_out_at_50ms`
- `cache_hit_updates_last_access`
- `failed_cache_fetch_does_not_surface_to_user` — no toast is emitted.
- `different_quality_profiles_are_separate_entries` — playing Direct then Med192 produces two files.

## Done when
The global DoD in `tasks/README.md` is satisfied.
