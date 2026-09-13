//! The rolling cache: size accounting, eviction with hysteresis, and startup reconciliation
//! (`docs/06-cache-and-offline.md` §4).

use std::path::{Path, PathBuf};
use std::time::Duration;

use jiff::Timestamp;

use crate::error::CacheError;
use crate::layout::{CacheKey, assert_within};
use crate::manifest::{Manifest, ManifestEntry};

/// A `.part` file younger than this is presumed to be an active in-progress download, never
/// touched by eviction or reconciliation; older than this and reconciliation treats it as
/// abandoned.
const STALE_PART_AGE: Duration = Duration::from_secs(24 * 3600);

/// What `RollingCache::reconcile` did, for the `info`-level log line
/// (`docs/06-cache-and-offline.md` §4) and for tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub dropped_rows: usize,
    pub deleted_orphan_files: usize,
    pub deleted_stale_parts: usize,
    pub reclaimed_bytes: u64,
}

/// Owns the manifest for the `tracks/` tier (`root` is the cache root — `cache_index.json` and
/// `cache.lock` live directly under it, per `docs/06-cache-and-offline.md` §1 — `tracks/` is one
/// level down). Downloads live in an entirely separate manifest (`downloads.rs`), so nothing here
/// can ever see, and therefore can never evict, a pinned file.
pub struct RollingCache {
    manifest: Manifest,
    tracks_root: PathBuf,
}

impl RollingCache {
    pub fn open(cache_root: &Path) -> Result<Self, CacheError> {
        let tracks_root = cache_root.join("tracks");
        std::fs::create_dir_all(&tracks_root).map_err(|source| CacheError::Io {
            path: tracks_root.clone(),
            source,
        })?;
        let manifest = Manifest::open(cache_root)?;
        Ok(RollingCache {
            manifest,
            tracks_root,
        })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn manifest_mut(&mut self) -> &mut Manifest {
        &mut self.manifest
    }

    pub fn total_bytes(&self) -> u64 {
        self.manifest.total_bytes()
    }

    /// Where `key`'s file *would* live, given a caller-computed relative path
    /// (`layout::cache_relative_path`) — regardless of whether anything is there yet.
    ///
    /// The relative path comes from the caller because the key alone cannot produce it: the album
    /// artist, album and title all live on the `Track`, not in the key. Only *creating* an entry
    /// needs this; once a row exists the manifest holds the real path, so `resolve` never
    /// recomputes one and entries written under an older scheme keep working.
    pub fn path_for(&self, relative: &Path) -> PathBuf {
        self.tracks_root.join(relative)
    }

    /// The play-time decision (`08-03`, `docs/06-cache-and-offline.md` §4): `Some(path)` only for
    /// a **complete** entry — touches `last_access` so LRU ordering reflects real use. `None` for
    /// a miss, an incomplete (still-downloading) entry, or a stale row whose file has since gone
    /// missing (this does not itself repair the manifest; that's `reconcile`'s own job).
    pub fn resolve(&mut self, key: &CacheKey) -> Option<PathBuf> {
        let complete_and_present = self
            .manifest
            .get(key)
            .is_some_and(|e| e.complete && e.path.exists());
        if !complete_and_present {
            return None;
        }
        let path = self.manifest.get(key)?.path.clone();
        if let Some(entry) = self.manifest.get_mut(key) {
            entry.last_access = Timestamp::now();
        }
        Some(path)
    }

    /// Registers a background fetch as starting: an incomplete manifest row at `path_for(key)`,
    /// so `evict`/`reconcile` both already know about it (even though eviction can never remove
    /// an incomplete entry regardless — `docs/12-decisions.md`, `08-02`). A no-op if one already
    /// exists (a resumed fetch reuses its own row rather than resetting `created_at`).
    pub fn begin_fetch(&mut self, key: &CacheKey, relative: &Path) -> PathBuf {
        // A resumed fetch keeps whatever path its row already has, so a change of naming scheme
        // never orphans a partially-downloaded file.
        if let Some(existing) = self.manifest.get(key) {
            return existing.path.clone();
        }
        let proposed = self.path_for(relative);
        // Two different items can propose one name (the same album present in two libraries). The
        // check is against the **manifest**, not the filesystem: `begin_fetch` reserves a row
        // before a single byte is written, so the clashing path routinely doesn't exist yet — which
        // is precisely the case `layout::resolve_collision` short-circuits past.
        let taken_by_other = |candidate: &Path| {
            self.manifest
                .entries()
                .any(|e| e.path == candidate && &e.key != key)
        };
        let path = if taken_by_other(&proposed) {
            (2..=99)
                .map(|n| crate::layout::suffixed(&proposed, n))
                .find(|candidate| !taken_by_other(candidate))
                // Ninety-eight same-named entries is not a real library; keeping the proposed path
                // means the two share a file, which is worse than nothing but unreachable.
                .unwrap_or(proposed)
        } else {
            proposed
        };
        if self.manifest.get(key).is_none() {
            let now = Timestamp::now();
            self.manifest.insert(ManifestEntry {
                key: key.clone(),
                path: path.clone(),
                bytes: 0,
                created_at: now,
                last_access: now,
                complete: false,
                duration_secs: None,
            });
        }
        path
    }

    /// The fetch that `begin_fetch` started succeeded: marks the entry complete with its real
    /// byte count.
    /// A fetch finished cleanly: mark the row complete **and get the index onto disk**.
    ///
    /// The flush is the whole point. Without it `cache_index.json` was never written during an
    /// ordinary session — nothing else called `flush`, and there is no shutdown hook — so every
    /// restart came up with an empty manifest: already-downloaded files could never be resolved,
    /// every track re-streamed and re-downloaded, `reconcile` (once it started running) deleted
    /// them all as orphans, and the About page's cache size read 0 forever. That is what "caching
    /// is unreliable most of the times" was (`docs/12-decisions.md`).
    ///
    /// `maybe_flush`, not `flush`: writing the whole index on every completed track would be
    /// wasteful, and the debounce is exactly what `Manifest` provides it for. A failure is logged,
    /// never propagated — a cache that cannot write its index still serves this session.
    pub fn complete_fetch(&mut self, key: &CacheKey, bytes: u64) {
        if let Some(entry) = self.manifest.get_mut(key) {
            entry.complete = true;
            entry.bytes = bytes;
            entry.last_access = Timestamp::now();
        }
        if let Err(error) = self.manifest.maybe_flush() {
            tracing::warn!(%error, "could not persist the cache index");
        }
    }

    /// Persists the index unconditionally — for shutdown, where the debounce would swallow the
    /// last few completed fetches of the session.
    pub fn flush(&mut self) -> Result<(), CacheError> {
        self.manifest.flush()
    }

    /// The fetch that `begin_fetch` started failed or was cancelled: drops the manifest row
    /// (`docs/06-cache-and-offline.md` §4: a failure is logged at `debug`, never user-visible).
    /// The `.part` file itself is deliberately left on disk — a later resume reads its own
    /// current size directly to pick up an HTTP `Range` request where it left off, independent of
    /// whatever the manifest says; `reconcile`'s 24-hour rule is what eventually cleans up a
    /// `.part` nothing ever resumes.
    pub fn abandon_fetch(&mut self, key: &CacheKey) {
        self.manifest.remove(key);
    }

    fn delete_tracked_file(&self, path: &Path) -> Result<(), CacheError> {
        // The safety net: refuse rather than delete if a manifest row (corrupt state, or a
        // hostile server-supplied path from an earlier version) points outside the tracks root.
        assert_within(&self.tracks_root, path)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(CacheError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Removes least-recently-accessed **complete** entries — never `protected` (the playing and
    /// preloaded tracks' own keys), and, since downloads live in a wholly separate manifest, never
    /// anything from that tier either — until usage reaches 90% of `limit_bytes`, the hysteresis
    /// that stops the cache evicting one file per track forever right at the boundary
    /// (`docs/06-cache-and-offline.md` §4). A no-op (and `Ok(0)`) if already at or under the
    /// limit. Returns the number of bytes actually reclaimed. An entry whose own path escapes the
    /// tracks root is skipped entirely — refused, not deleted, and its manifest row survives too.
    pub fn evict(&mut self, limit_bytes: u64, protected: &[CacheKey]) -> Result<u64, CacheError> {
        let total = self.manifest.total_bytes();
        if total <= limit_bytes {
            return Ok(0);
        }
        let target = (limit_bytes as f64 * 0.9) as u64;

        let mut candidates: Vec<CacheKey> = self
            .manifest
            .entries()
            .filter(|e| e.complete && !protected.contains(&e.key))
            .map(|e| e.key.clone())
            .collect();
        candidates.sort_by_key(|k| self.manifest.get(k).map(|e| e.last_access));

        let mut remaining = total;
        let mut reclaimed = 0u64;
        for key in candidates {
            if remaining <= target {
                break;
            }
            let Some(entry) = self.manifest.get(&key) else {
                continue;
            };
            let path = entry.path.clone();
            let bytes = entry.bytes;
            if assert_within(&self.tracks_root, &path).is_err() {
                tracing::warn!(
                    ?path,
                    "cache entry escapes the tracks root; refusing to evict"
                );
                continue;
            }
            self.delete_tracked_file(&path)?;
            self.manifest.remove(&key);
            remaining = remaining.saturating_sub(bytes);
            reclaimed += bytes;
        }
        self.manifest.flush()?;
        Ok(reclaimed)
    }

    /// Startup reconciliation (`docs/06-cache-and-offline.md` §4): drops manifest rows whose file
    /// is missing, deletes files under `tracks/` with no manifest row, deletes `.part` files older
    /// than 24 hours, and leaves the manifest's own `total_bytes()` correct by construction (it's
    /// always summed from whatever rows remain, never cached separately). A corrupt index was
    /// already quarantined to `cache_index.json.bad` by `Manifest::open` before this ever runs —
    /// this is what actually reclaims the files that quarantine leaves orphaned.
    pub fn reconcile(&mut self) -> Result<ReconcileReport, CacheError> {
        let mut report = ReconcileReport::default();

        let missing: Vec<CacheKey> = self
            .manifest
            .entries()
            .filter(|e| !e.path.exists())
            .map(|e| e.key.clone())
            .collect();
        for key in missing {
            self.manifest.remove(&key);
            report.dropped_rows += 1;
        }

        if self.tracks_root.exists() {
            // Walks the whole tree, not just `tracks/<server>/*`: cached files now sit under
            // `<server>/<artist>/<album>/`, and a two-level scan would never see them — leaving
            // every orphan and stale `.part` beneath an album directory to accumulate forever
            // (`docs/12-decisions.md`).
            let mut files = Vec::new();
            collect_files(&self.tracks_root, &mut files)?;
            for path in files {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();

                if name.ends_with(".part") {
                    if file_age(&path)? > STALE_PART_AGE {
                        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        self.delete_tracked_file(&path)?;
                        report.deleted_stale_parts += 1;
                        report.reclaimed_bytes += bytes;
                    }
                    continue;
                }

                let has_row = self.manifest.entries().any(|e| e.path == path);
                if !has_row {
                    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    self.delete_tracked_file(&path)?;
                    report.deleted_orphan_files += 1;
                    report.reclaimed_bytes += bytes;
                }
            }
            // Eviction and the deletions above leave album/artist directories behind; without this
            // the cache tree only ever grows a skeleton of empty folders.
            prune_empty_dirs(&self.tracks_root)?;
        }

        self.manifest.flush()?;
        tracing::info!(
            dropped_rows = report.dropped_rows,
            deleted_orphan_files = report.deleted_orphan_files,
            deleted_stale_parts = report.deleted_stale_parts,
            reclaimed_bytes = report.reclaimed_bytes,
            "cache reconciliation complete"
        );
        Ok(report)
    }
}

fn read_dir_entries(dir: &Path) -> Result<Vec<std::fs::DirEntry>, CacheError> {
    std::fs::read_dir(dir)
        .map_err(|source| CacheError::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| CacheError::Io {
            path: dir.to_path_buf(),
            source,
        })
}

fn file_age(path: &Path) -> Result<Duration, CacheError> {
    let metadata = std::fs::metadata(path).map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let modified = metadata.modified().map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(std::time::SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default())
}

/// Every regular file beneath `root`, at any depth. Errors on a directory it cannot read rather
/// than silently skipping it — a missed file here is an orphan that never gets reclaimed.
fn collect_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), CacheError> {
    for entry in read_dir_entries(root)? {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| CacheError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            collect_files(&path, out)?;
        } else if file_type.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Removes directories left empty beneath `root`, deepest first. `root` itself always stays.
/// Best-effort: a directory that cannot be removed (a race with a concurrent fetch, say) is left
/// alone rather than failing the whole reconciliation.
fn prune_empty_dirs(root: &Path) -> Result<(), CacheError> {
    for entry in read_dir_entries(root)? {
        let path = entry.path();
        let is_dir = entry
            .file_type()
            .map(|t| t.is_dir())
            .map_err(|source| CacheError::Io {
                path: path.clone(),
                source,
            })?;
        if !is_dir {
            continue;
        }
        prune_empty_dirs(&path)?;
        if read_dir_entries(&path)?.is_empty() {
            let _ = std::fs::remove_dir(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ManifestEntry;
    use jiff::Timestamp;
    use loxia_core::config::QualityProfile;
    use loxia_core::model::{ItemId, ServerId};
    use std::fs;
    use tempfile::tempdir;

    fn key(item: &str) -> CacheKey {
        CacheKey {
            server: ServerId::from("srv1"),
            item: ItemId::from(item),
            profile: QualityProfile::Direct,
        }
    }

    /// The defect behind "caching is unreliable most of the times": nothing ever wrote
    /// `cache_index.json` during an ordinary session. `complete_fetch` updated the manifest in
    /// memory and the only `flush` callers were eviction and reconciliation, neither of which runs
    /// on a normal play — so every restart came up with an empty index, could resolve nothing, and
    /// re-downloaded the lot (`docs/12-decisions.md`).
    #[test]
    fn a_completed_fetch_puts_the_index_on_disk() {
        let dir = tempdir().unwrap();
        let index = dir.path().join("cache_index.json");

        {
            let mut cache = RollingCache::open(dir.path()).unwrap();
            let key = key("item-1");
            let path = cache.begin_fetch(&key, std::path::Path::new("srv1/item-1.direct.flac"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"xxxx").unwrap();
            cache.complete_fetch(&key, 4);
            assert!(
                index.exists(),
                "the index must be written, not just held in memory"
            );
        }

        // And a fresh instance can actually find it again — the whole point.
        let mut reopened = RollingCache::open(dir.path()).unwrap();
        reopened.reconcile().unwrap();
        assert!(
            reopened.resolve(&key("item-1")).is_some(),
            "a cached track must survive a restart"
        );
        assert_eq!(reopened.total_bytes(), 4);
    }

    /// Creates a real (small) file at `tracks/srv1/<item>.direct.flac` and inserts a matching
    /// manifest row — `bytes` is the *claimed* size in the manifest, independent of the file's own
    /// real size, so bulk byte-accounting tests don't need to allocate real gigabytes on disk.
    fn insert_real_entry(
        cache: &mut RollingCache,
        item: &str,
        bytes: u64,
        last_access_secs: i64,
        complete: bool,
    ) -> PathBuf {
        let dir = cache.tracks_root.join("srv1");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{item}.direct.flac"));
        fs::write(&path, b"x").unwrap();
        cache.manifest.insert(ManifestEntry {
            key: key(item),
            path: path.clone(),
            bytes,
            created_at: Timestamp::from_second(1_700_000_000).unwrap(),
            last_access: Timestamp::from_second(1_700_000_000 + last_access_secs).unwrap(),
            complete,
            duration_secs: Some(180),
        });
        path
    }

    #[test]
    fn evict_removes_least_recently_used_first() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        insert_real_entry(&mut cache, "oldest", 40, 0, true);
        insert_real_entry(&mut cache, "middle", 40, 10, true);
        insert_real_entry(&mut cache, "newest", 40, 20, true);

        // Limit forces exactly one eviction to reach the 90% target.
        cache.evict(100, &[]).unwrap();

        assert!(cache.manifest.get(&key("oldest")).is_none());
        assert!(cache.manifest.get(&key("middle")).is_some());
        assert!(cache.manifest.get(&key("newest")).is_some());
    }

    #[test]
    fn evict_stops_at_ninety_percent() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        for i in 0..10 {
            insert_real_entry(&mut cache, &format!("item{i}"), 100, i, true);
        }
        // 1000 bytes total against a 500-byte limit; 90% target = 450.
        cache.evict(500, &[]).unwrap();
        assert!(cache.total_bytes() <= 450);
        assert!(
            cache.total_bytes() > 0,
            "must not over-evict past the target"
        );
    }

    #[test]
    fn evict_never_removes_protected_keys() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        insert_real_entry(&mut cache, "playing", 100, 0, true);
        insert_real_entry(&mut cache, "other", 100, 1, true);

        cache.evict(50, &[key("playing")]).unwrap();
        assert!(cache.manifest.get(&key("playing")).is_some());
    }

    #[test]
    fn evict_never_removes_incomplete_recent_parts() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        insert_real_entry(&mut cache, "downloading", 100, 0, false);
        insert_real_entry(&mut cache, "old_complete", 100, 1, true);

        cache.evict(50, &[]).unwrap();
        assert!(cache.manifest.get(&key("downloading")).is_some());
    }

    #[test]
    fn evict_never_touches_downloads() {
        // Downloads simply have no representation in `RollingCache`'s own manifest at all — the
        // only way to prove "never touches downloads" is to show a download-tier file sitting
        // right next to the cache root survives untouched, since nothing here ever looks outside
        // `tracks/` in the first place.
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        let downloads_dir = dir.path().join("downloads_tier");
        fs::create_dir_all(&downloads_dir).unwrap();
        let pinned = downloads_dir.join("pinned.flac");
        fs::write(&pinned, b"x").unwrap();

        insert_real_entry(&mut cache, "cached", 1000, 0, true);
        cache.evict(1, &[]).unwrap();

        assert!(
            pinned.exists(),
            "download-tier file must survive cache eviction"
        );
    }

    #[test]
    fn evict_returns_bytes_reclaimed() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        insert_real_entry(&mut cache, "a", 100, 0, true);
        insert_real_entry(&mut cache, "b", 100, 1, true);

        // 200 bytes total against a 150-byte limit (90% target = 135): evicting just "a" (the
        // older of the two, 100 bytes) already brings the total to 100, under the target.
        let reclaimed = cache.evict(150, &[]).unwrap();
        assert_eq!(
            reclaimed, 100,
            "exactly the one evicted entry's own byte count"
        );
    }

    #[test]
    fn deletion_calls_assert_within() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        // A corrupted/hostile row pointing well outside the tracks root.
        let escaping_path = dir.path().join("..").join("escaped.flac");
        cache.manifest.insert(ManifestEntry {
            key: key("escapee"),
            path: escaping_path.clone(),
            bytes: 100,
            created_at: Timestamp::from_second(1_700_000_000).unwrap(),
            last_access: Timestamp::from_second(1_700_000_000).unwrap(),
            complete: true,
            duration_secs: None,
        });
        insert_real_entry(&mut cache, "normal", 100, 1, true);

        cache.evict(1, &[]).unwrap();
        // Refused, not deleted: the row (and, transitively, whatever it points at) survives.
        assert!(cache.manifest.get(&key("escapee")).is_some());
    }

    #[test]
    fn reconcile_drops_orphan_rows_and_files() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();

        // A manifest row with no file on disk.
        cache.manifest.insert(ManifestEntry {
            key: key("ghost"),
            path: cache.tracks_root.join("srv1").join("ghost.direct.flac"),
            bytes: 50,
            created_at: Timestamp::from_second(1_700_000_000).unwrap(),
            last_access: Timestamp::from_second(1_700_000_000).unwrap(),
            complete: true,
            duration_secs: None,
        });
        // A real file with no manifest row.
        let orphan_dir = cache.tracks_root.join("srv1");
        fs::create_dir_all(&orphan_dir).unwrap();
        fs::write(orphan_dir.join("orphan.direct.flac"), b"data").unwrap();

        let report = cache.reconcile().unwrap();
        assert_eq!(report.dropped_rows, 1);
        assert_eq!(report.deleted_orphan_files, 1);
        assert!(cache.manifest.get(&key("ghost")).is_none());
        assert!(!orphan_dir.join("orphan.direct.flac").exists());
    }

    /// Cached files live under `<server>/<artist>/<album>/` now. The old scan only looked at
    /// `tracks/<server>/*` and skipped anything that wasn't a file, so every orphan and stale
    /// `.part` inside an album directory would have accumulated forever (`docs/12-decisions.md`).
    #[test]
    fn reconcile_reaches_orphans_nested_under_artist_and_album() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();

        let album_dir = cache
            .tracks_root
            .join("srv1")
            .join("Boy Harsher")
            .join("Care");
        fs::create_dir_all(&album_dir).unwrap();
        let orphan = album_dir.join("01-03 - Motion.direct.flac");
        fs::write(&orphan, b"data").unwrap();

        let report = cache.reconcile().unwrap();

        assert_eq!(
            report.deleted_orphan_files, 1,
            "the nested orphan was missed"
        );
        assert!(!orphan.exists());
    }

    /// A file the manifest *does* know about, however deeply nested, must survive.
    #[test]
    fn reconcile_keeps_nested_files_that_have_a_row() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();

        let relative = Path::new("srv1/Boy Harsher/Care/01-03 - Motion.direct.flac");
        let path = cache.begin_fetch(&key("item-1"), relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"data").unwrap();
        cache.complete_fetch(&key("item-1"), 4);

        let report = cache.reconcile().unwrap();

        assert_eq!(report.deleted_orphan_files, 0);
        assert!(path.exists(), "a tracked file must not be reclaimed");
    }

    /// Eviction and orphan-deletion leave the artist/album folders behind; without pruning, the
    /// cache tree grows a permanent skeleton of empty directories.
    #[test]
    fn reconcile_prunes_directories_it_has_emptied() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();

        let album_dir = cache
            .tracks_root
            .join("srv1")
            .join("Boy Harsher")
            .join("Care");
        fs::create_dir_all(&album_dir).unwrap();
        fs::write(album_dir.join("orphan.direct.flac"), b"data").unwrap();

        cache.reconcile().unwrap();

        assert!(
            !album_dir.exists(),
            "emptied album directory should be gone"
        );
        assert!(
            cache.tracks_root.exists(),
            "the tracks root itself always stays"
        );
    }

    /// Two different items proposing the same path — the same album present in two libraries —
    /// must not end up sharing one file.
    #[test]
    fn colliding_cache_paths_are_disambiguated() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        let relative = Path::new("srv1/Boy Harsher/Care/01-03 - Motion.direct.flac");

        let first = cache.begin_fetch(&key("item-1"), relative);
        let second = cache.begin_fetch(&key("item-2"), relative);

        assert_ne!(first, second, "two items must not share one cached file");
        assert_eq!(
            cache.begin_fetch(&key("item-1"), relative),
            first,
            "an existing entry keeps its own path"
        );
    }

    #[test]
    fn reconcile_deletes_stale_parts() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        let server_dir = cache.tracks_root.join("srv1");
        fs::create_dir_all(&server_dir).unwrap();

        let stale = server_dir.join("stale.direct.flac.part");
        fs::write(&stale, b"partial").unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let file = fs::OpenOptions::new().write(true).open(&stale).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();

        let fresh = server_dir.join("fresh.direct.flac.part");
        fs::write(&fresh, b"partial").unwrap();

        let report = cache.reconcile().unwrap();
        assert_eq!(report.deleted_stale_parts, 1);
        assert!(!stale.exists());
        assert!(
            fresh.exists(),
            "a young .part file must survive reconciliation"
        );
    }

    #[test]
    fn reconcile_recomputes_total() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        insert_real_entry(&mut cache, "kept", 100, 0, true);
        cache.manifest.insert(ManifestEntry {
            key: key("ghost"),
            path: cache.tracks_root.join("srv1").join("ghost.direct.flac"),
            bytes: 999,
            created_at: Timestamp::from_second(1_700_000_000).unwrap(),
            last_access: Timestamp::from_second(1_700_000_000).unwrap(),
            complete: true,
            duration_secs: None,
        });

        cache.reconcile().unwrap();
        assert_eq!(cache.total_bytes(), 100);
    }

    #[test]
    fn corrupt_index_orphans_get_reclaimed_by_reconcile() {
        let dir = tempdir().unwrap();
        let server_dir = dir.path().join("tracks").join("srv1");
        fs::create_dir_all(&server_dir).unwrap();
        fs::write(server_dir.join("a.direct.flac"), b"data").unwrap();
        fs::write(dir.path().join("cache_index.json"), b"{ not json").unwrap();

        let mut cache = RollingCache::open(dir.path()).unwrap();
        assert_eq!(cache.total_bytes(), 0);
        let report = cache.reconcile().unwrap();
        assert_eq!(report.deleted_orphan_files, 1);
        assert!(!server_dir.join("a.direct.flac").exists());
        assert!(dir.path().join("cache_index.json.bad").exists());
    }

    #[test]
    fn fill_past_limit_then_evict() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        const GB: u64 = 1024 * 1024 * 1024;
        for i in 0..6 {
            insert_real_entry(&mut cache, &format!("gb{i}"), GB, i as i64, true);
        }
        assert_eq!(cache.total_bytes(), 6 * GB);

        cache.evict(5 * GB, &[]).unwrap();
        assert!(cache.total_bytes() <= (4.5 * GB as f64) as u64);
    }

    #[test]
    fn resolve_is_none_before_a_fetch_completes() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        let k = key("a");
        cache.begin_fetch(&k, Path::new("srv/A/B/track.direct.flac"));
        assert!(cache.resolve(&k).is_none());
    }

    #[test]
    fn resolve_returns_path_and_updates_last_access() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        let k = key("a");
        let path = cache.begin_fetch(&k, Path::new("srv/A/B/track.direct.flac"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"data").unwrap();
        cache.complete_fetch(&k, 4);

        let before = cache.manifest().get(&k).unwrap().last_access;
        let resolved = cache.resolve(&k).unwrap();
        assert_eq!(resolved, path);
        assert!(cache.manifest().get(&k).unwrap().last_access >= before);
    }

    #[test]
    fn resolve_is_none_when_the_file_is_missing() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        let k = key("a");
        let path = cache.begin_fetch(&k, Path::new("srv/A/B/track.direct.flac"));
        cache.complete_fetch(&k, 4); // marked complete, but the file was never actually written
        assert!(!path.exists());
        assert!(cache.resolve(&k).is_none());
    }

    #[test]
    fn abandon_fetch_drops_the_row_but_not_the_part_file() {
        let dir = tempdir().unwrap();
        let mut cache = RollingCache::open(dir.path()).unwrap();
        let k = key("a");
        let path = cache.begin_fetch(&k, Path::new("srv/A/B/track.direct.flac"));
        let part = PathBuf::from(format!("{}.part", path.display()));
        fs::create_dir_all(part.parent().unwrap()).unwrap();
        fs::write(&part, b"partial").unwrap();

        cache.abandon_fetch(&k);
        assert!(cache.manifest().get(&k).is_none());
        assert!(part.exists(), "the .part file survives for a later resume");
    }
}
