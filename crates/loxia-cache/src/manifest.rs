//! Atomic JSON cache index and its advisory lock (`docs/06-cache-and-offline.md` §§4, 9).

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::error::CacheError;
use crate::layout::CacheKey;

/// `{ key, path, bytes, created_at, last_access, complete, duration_secs }`
/// (`docs/06-cache-and-offline.md` §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub key: CacheKey,
    pub path: PathBuf,
    pub bytes: u64,
    pub created_at: Timestamp,
    pub last_access: Timestamp,
    pub complete: bool,
    pub duration_secs: Option<u64>,
}

/// The on-disk shape of `cache_index.json` — a plain list; `Manifest` itself keeps the more
/// useful `HashMap<CacheKey, _>` in memory and only flattens back to this for serialisation.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ManifestFile {
    entries: Vec<ManifestEntry>,
}

/// A write debounced to at most once per second, per `docs/06-cache-and-offline.md` §4.
const WRITE_DEBOUNCE: Duration = Duration::from_secs(1);

/// The rolling cache's on-disk index plus its advisory `cache.lock` (`fs4`) — held for this
/// value's own lifetime, so the lock releases automatically when it's dropped.
pub struct Manifest {
    root: PathBuf,
    entries: HashMap<CacheKey, ManifestEntry>,
    read_only: bool,
    _lock_file: Option<File>,
    dirty: bool,
    last_write: Option<Instant>,
}

impl Manifest {
    /// Opens (creating if absent) `root/cache_index.json`, taking `root/cache.lock`. If the lock
    /// is already held by another instance, this one runs in **read-only cache mode** instead of
    /// refusing to start (`docs/06-cache-and-offline.md` §9) — logged once, here.
    pub fn open(root: &Path) -> Result<Self, CacheError> {
        std::fs::create_dir_all(root).map_err(|source| CacheError::Io {
            path: root.to_path_buf(),
            source,
        })?;

        let lock_path = root.join("cache.lock");
        let lock_file = File::create(&lock_path).map_err(|source| CacheError::Io {
            path: lock_path.clone(),
            source,
        })?;
        // Fully-qualified: `std::fs::File` has its own inherent `try_lock` since Rust 1.89 that
        // would otherwise shadow `fs4`'s (inherent methods always win method resolution over an
        // in-scope trait) — calling through `FileExt` explicitly is what actually exercises the
        // cross-platform `fs4` implementation `docs/06-cache-and-offline.md` §9 asks for.
        let (lock_file, read_only) = match fs4::FileExt::try_lock(&lock_file) {
            Ok(()) => (Some(lock_file), false),
            Err(_) => {
                tracing::warn!(
                    "cache lock ({}) already held by another instance; running read-only",
                    lock_path.display()
                );
                (None, true)
            }
        };

        let entries = load_or_quarantine(&root.join("cache_index.json"), root);

        Ok(Manifest {
            root: root.to_path_buf(),
            entries,
            read_only,
            _lock_file: lock_file,
            dirty: false,
            last_write: None,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn get(&self, key: &CacheKey) -> Option<&ManifestEntry> {
        self.entries.get(key)
    }

    /// A no-op (returns `None`) in read-only mode — a caller mutating the entry it gets back
    /// would otherwise silently update in-memory state that a read-only instance promises never
    /// to persist.
    pub fn get_mut(&mut self, key: &CacheKey) -> Option<&mut ManifestEntry> {
        if self.read_only {
            return None;
        }
        self.dirty = true;
        self.entries.get_mut(key)
    }

    pub fn entries(&self) -> impl Iterator<Item = &ManifestEntry> {
        self.entries.values()
    }

    pub fn total_bytes(&self) -> u64 {
        self.entries.values().map(|e| e.bytes).sum()
    }

    /// A no-op in read-only mode (`docs/06-cache-and-offline.md` §9: serve hits, write nothing).
    pub fn insert(&mut self, entry: ManifestEntry) {
        if self.read_only {
            return;
        }
        self.entries.insert(entry.key.clone(), entry);
        self.dirty = true;
    }

    /// A no-op in read-only mode.
    pub fn remove(&mut self, key: &CacheKey) -> Option<ManifestEntry> {
        if self.read_only {
            return None;
        }
        let removed = self.entries.remove(key);
        if removed.is_some() {
            self.dirty = true;
        }
        removed
    }

    /// Persists immediately, ignoring the debounce — for shutdown and for callers (like
    /// `lru::RollingCache::reconcile`) that need the result on disk right away. A no-op in
    /// read-only mode.
    pub fn flush(&mut self) -> Result<(), CacheError> {
        if self.read_only || !self.dirty {
            return Ok(());
        }
        write_atomic(&self.root, &self.entries)?;
        self.dirty = false;
        self.last_write = Some(Instant::now());
        Ok(())
    }

    /// Writes only if dirty and at least [`WRITE_DEBOUNCE`] has passed since the last write
    /// (`docs/06-cache-and-offline.md` §4) — the routine call a periodic tick should make;
    /// `flush` is for the moments that can't wait.
    pub fn maybe_flush(&mut self) -> Result<(), CacheError> {
        if !self.dirty || self.read_only {
            return Ok(());
        }
        if let Some(last) = self.last_write
            && last.elapsed() < WRITE_DEBOUNCE
        {
            return Ok(());
        }
        self.flush()
    }
}

/// `.tmp` in the same directory, `fsync`, rename — a power cut must never leave a corrupt index
/// (`docs/06-cache-and-offline.md` §9). Never escapes `root` since both paths it touches are
/// always `root`'s own direct children, constructed here rather than from any external input.
fn write_atomic(root: &Path, entries: &HashMap<CacheKey, ManifestEntry>) -> Result<(), CacheError> {
    let index_path = root.join("cache_index.json");
    let tmp_path = root.join("cache_index.json.tmp");

    let file = ManifestFile {
        entries: entries.values().cloned().collect(),
    };
    let json =
        serde_json::to_vec_pretty(&file).expect("ManifestEntry/ManifestFile always serialise");

    let mut f = File::create(&tmp_path).map_err(|source| CacheError::Io {
        path: tmp_path.clone(),
        source,
    })?;
    f.write_all(&json).map_err(|source| CacheError::Io {
        path: tmp_path.clone(),
        source,
    })?;
    f.sync_all().map_err(|source| CacheError::Io {
        path: tmp_path.clone(),
        source,
    })?;
    drop(f);
    std::fs::rename(&tmp_path, &index_path).map_err(|source| CacheError::Io {
        path: index_path.clone(),
        source,
    })?;
    Ok(())
}

/// A missing index (first run) is just an empty cache. A corrupt one is quarantined to
/// `cache_index.json.bad` (overwriting any previous quarantine — only the latest failure is worth
/// keeping around) and this instance starts fresh rather than refusing to run
/// (`docs/06-cache-and-offline.md` §9); `lru::RollingCache::reconcile` is what actually reclaims
/// the now-orphaned files this leaves behind on the next full reconciliation pass.
fn load_or_quarantine(index_path: &Path, root: &Path) -> HashMap<CacheKey, ManifestEntry> {
    let Ok(text) = std::fs::read_to_string(index_path) else {
        return HashMap::new();
    };
    match serde_json::from_str::<ManifestFile>(&text) {
        Ok(file) => file
            .entries
            .into_iter()
            .map(|e| (e.key.clone(), e))
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "corrupt cache index; quarantining and starting fresh");
            let bad_path = root.join("cache_index.json.bad");
            let _ = std::fs::rename(index_path, &bad_path);
            HashMap::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::config::QualityProfile;
    use loxia_core::model::{ItemId, ServerId};
    use tempfile::tempdir;

    fn entry(item: &str, bytes: u64) -> ManifestEntry {
        ManifestEntry {
            key: CacheKey {
                server: ServerId::from("srv1"),
                item: ItemId::from(item),
                profile: QualityProfile::Direct,
            },
            path: PathBuf::from(format!("/cache/tracks/srv1/{item}.direct.flac")),
            bytes,
            created_at: Timestamp::from_second(1_700_000_000).unwrap(),
            last_access: Timestamp::from_second(1_700_000_000).unwrap(),
            complete: true,
            duration_secs: Some(180),
        }
    }

    #[test]
    fn manifest_write_is_atomic() {
        let dir = tempdir().unwrap();
        let mut m = Manifest::open(dir.path()).unwrap();
        m.insert(entry("a", 100));
        m.flush().unwrap();

        assert!(dir.path().join("cache_index.json").exists());
        assert!(!dir.path().join("cache_index.json.tmp").exists());

        // Re-opening re-reads exactly what was written.
        let reopened = Manifest::open(dir.path()).unwrap();
        assert_eq!(reopened.total_bytes(), 100);
    }

    #[test]
    fn manifest_writes_are_debounced() {
        let dir = tempdir().unwrap();
        let mut m = Manifest::open(dir.path()).unwrap();
        m.insert(entry("a", 100));
        m.maybe_flush().unwrap();
        let first_write = std::fs::metadata(dir.path().join("cache_index.json"))
            .unwrap()
            .modified()
            .unwrap();

        m.insert(entry("b", 50));
        m.maybe_flush().unwrap();
        let second_write = std::fs::metadata(dir.path().join("cache_index.json"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            first_write, second_write,
            "a second maybe_flush within the debounce window must not write again"
        );

        // An explicit flush always writes, regardless of debounce.
        m.flush().unwrap();
        let reopened = Manifest::open(dir.path()).unwrap();
        assert_eq!(reopened.total_bytes(), 150);
    }

    #[test]
    fn corrupt_index_is_quarantined_and_rebuilt() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("cache_index.json"), b"{ not json").unwrap();

        let m = Manifest::open(dir.path()).unwrap();
        assert_eq!(m.total_bytes(), 0);
        assert!(dir.path().join("cache_index.json.bad").exists());
        assert!(
            std::fs::read_to_string(dir.path().join("cache_index.json.bad"))
                .unwrap()
                .contains("not json")
        );
    }

    #[test]
    fn lock_held_enters_readonly_mode() {
        let dir = tempdir().unwrap();
        let first = Manifest::open(dir.path()).unwrap();
        assert!(!first.is_read_only());

        let second = Manifest::open(dir.path()).unwrap();
        assert!(second.is_read_only());
        drop(first);
    }

    #[test]
    fn readonly_manifest_writes_nothing() {
        let dir = tempdir().unwrap();
        let first = Manifest::open(dir.path()).unwrap();
        let mut second = Manifest::open(dir.path()).unwrap();
        assert!(second.is_read_only());

        second.insert(entry("a", 100));
        assert_eq!(
            second.total_bytes(),
            0,
            "read-only mode writes nothing, even in memory"
        );
        second.flush().unwrap();
        drop(first);
    }
}
