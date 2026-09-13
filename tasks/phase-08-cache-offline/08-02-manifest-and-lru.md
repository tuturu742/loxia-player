# 08-02 · Manifest and LRU

**Phase:** 08 — Cache & offline · **Agent:** B · **Size:** L
**Prerequisites:** `08-01`
**Reference:** `docs/06-cache-and-offline.md` §§4, 9

## Goal
The rolling cache: an atomic on-disk index, size accounting, eviction with hysteresis, and startup
reconciliation.

## Files
- `crates/loxia-cache/src/manifest.rs`, `lru.rs`

## Specification

**Manifest entry:**
`{ key: CacheKey, path, bytes, created_at, last_access, complete: bool, duration_secs }`

**Atomic writes.** Serialise to `cache_index.json.tmp` in the same directory, `fsync`, rename.
Debounced to at most one write per second. A power cut must never leave a corrupt index — a
half-written JSON file makes the whole cache unreadable on next start.

**Advisory lock.** Take `cache.lock` with `fs4`. If it is held by another instance, run in
**read-only cache mode**: serve hits, write nothing, log once. Refusing to start would be worse — a
user with two terminals open should still get a working player in both.

**Eviction.** Triggered when `total_bytes > rolling_max_gb`. Remove least-recently-accessed
**complete** entries until usage reaches **90 %** of the limit. The hysteresis is what stops the
cache evicting one file per track forever at the boundary.

**Never evict:**
- the currently playing track's key,
- the preloaded next track's key,
- incomplete `.part` files younger than 5 minutes,
- anything in the downloads tier, which has a separate budget and no cap.

The caller passes the protected keys in, so `loxia-cache` needs no knowledge of the queue:
```
pub fn evict(&mut self, limit_bytes: u64, protected: &[CacheKey]) -> Result<u64, CacheError>;
```

**Startup reconciliation** (`reconcile`): scan `tracks/`, drop manifest rows whose file is missing,
delete files with no manifest row, delete `.part` files older than 24 hours, recompute the byte
total, and log the reclaimed space at `info`. A corrupt index is renamed to `.bad` and rebuilt from
the directory scan rather than failing.

Every deletion calls `assert_within` first.

## Acceptance
- `manifest_write_is_atomic` — no `.tmp` remains; a simulated crash between write and rename leaves
  the old index intact.
- `manifest_writes_are_debounced`
- `corrupt_index_is_quarantined_and_rebuilt`
- `lock_held_enters_readonly_mode`
- `evict_removes_least_recently_used_first`
- `evict_stops_at_ninety_percent`
- `evict_never_removes_protected_keys`
- `evict_never_removes_incomplete_recent_parts`
- `evict_never_touches_downloads`
- `evict_returns_bytes_reclaimed`
- `reconcile_drops_orphan_rows_and_files`
- `reconcile_deletes_stale_parts`
- `reconcile_recomputes_total`
- `deletion_calls_assert_within` — a manifest row pointing outside the root is refused, not deleted.
- `fill_past_limit_then_evict` — insert 6 GB against a 5 GB limit and assert the final total is at
  or below 4.5 GB.

## Done when
The global DoD in `tasks/README.md` is satisfied.
