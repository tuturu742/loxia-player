//! Permanent pinned downloads, sidecars, resume (`docs/06-cache-and-offline.md` §5).
//!
//! `Downloads` never touches the network or `loxia-emby` directly — `loxia-cache` depends only on
//! `loxia-core` (`docs/01-architecture.md` §3.4). The byte-fetching and non-`Track`-scope
//! expansion are injected through [`Fetcher`], an object-safe async trait; the real implementation
//! (calling `loxia_emby::endpoints::download::fetch_to_file`) lives in `crates/loxia`, and tests
//! here use an in-memory fake. This is the same split `08-03` used for the rolling cache's own
//! background fetch.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::stream::{self, StreamExt};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use loxia_core::action::{DataAction, SystemEvent};
use loxia_core::config::QualityProfile;
use loxia_core::event::Event;
use loxia_core::model::{ItemId, PlaylistId, ServerId, Track};
use loxia_core::state::toast::ToastLevel;

use crate::error::CacheError;
use crate::layout::{assert_within, download_path, resolve_collision};

/// What `d` was pressed on (`docs/06-cache-and-offline.md` §5). `Track` already carries the full,
/// already-loaded metadata (no fetch needed, unlike the other three); `Album`/`Artist`/`Playlist`
/// carry only an id and are expanded to a track list through [`Fetcher::expand`]. Distinct from
/// `loxia_core::effect::CacheEffect`'s own `DownloadScope` (added in `03-03`, never yet
/// constructed anywhere): that one is the wire-level `Effect` payload, necessarily id-only since
/// the reducer/UI only ever has an id at keypress time; this one is what a caller that has already
/// resolved (or is about to resolve) a full track list actually needs. Reconciling the two is left
/// to whichever future task wires a real `d`-keybinding reducer path and worker
/// (`docs/12-decisions.md`). Not boxing the (larger) `Track` variant, the same call `MediaItem`
/// (`07-03`'s own doc comment) already made for the same reason: this is constructed once per
/// `pin` call, never stored in a long-lived collection matched in a hot loop.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum DownloadScope {
    Track(Track),
    Album(ItemId),
    Artist(ItemId),
    Playlist(PlaylistId),
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Everything [`Downloads`] needs from the outside world. Injected at construction so this module
/// stays testable with a fake and pure of any `loxia-emby`/network dependency.
pub trait Fetcher: Send + Sync {
    /// The full track list for a non-`Track` scope, fetching from the server if not already
    /// loaded. Never called for a `DownloadScope::Track` (nothing to expand).
    fn expand<'a>(
        &'a self,
        scope: &'a DownloadScope,
    ) -> BoxFuture<'a, Result<Vec<Track>, CacheError>>;

    /// Streams `track`'s audio bytes (always at `profile`, forced to `Direct` by the caller when
    /// `transcode.download_uncompressed` is set) to `dest`, resumable via HTTP `Range` against
    /// `dest`'s own current size — the exact same contract as
    /// `loxia_emby::endpoints::download::fetch_to_file`. Calls `on_progress(done_bytes,
    /// total_bytes)` as bytes land. Returns the final byte count.
    fn fetch_track<'a>(
        &'a self,
        track: &'a Track,
        profile: QualityProfile,
        dest: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) + Send + Sync),
    ) -> BoxFuture<'a, Result<u64, CacheError>>;

    /// Album cover art, written directly to `dest` (`cover.jpg`). Best-effort: a failure here
    /// never fails the pin of the tracks themselves.
    fn fetch_cover<'a>(
        &'a self,
        album: &'a ItemId,
        dest: &'a Path,
    ) -> BoxFuture<'a, Result<(), CacheError>>;
}

/// Delivers `Event::Data(DataAction::DownloadProgress)` and toasts back to the runtime's own event
/// loop — the only channel this module has to the outside world besides `Fetcher`, since it has no
/// access to `crates/loxia`'s real `mpsc::UnboundedSender<Event>`.
pub type ProgressSink = Arc<dyn Fn(Event) + Send + Sync>;

/// One row of `downloads_index.json`: `item_id → { path, profile, bytes, downloaded_at, sidecar }`
/// (`docs/06-cache-and-offline.md` §5) plus `complete`, added beyond that literal four-field
/// sketch — without it there is no way to tell "finished" from "queued, interrupted mid-fetch"
/// apart on reload, which `interrupted_pin_resumes_on_restart` depends on (`docs/12-decisions.md`,
/// mirroring `ManifestEntry`'s own `complete` field, `08-02`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub item: ItemId,
    pub path: PathBuf,
    pub profile: QualityProfile,
    pub bytes: u64,
    pub downloaded_at: Option<Timestamp>,
    pub sidecar: PathBuf,
    pub complete: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DownloadsFile {
    entries: Vec<DownloadEntry>,
}

/// What one track's fetch attempt did, distinguishing a genuine failure (left queued for the next
/// resume, per `docs/06-cache-and-offline.md` §5) from a batch abort that never got to run at all.
enum Outcome {
    Done(u64),
    Skipped,
    Failed(CacheError),
}

/// Permanent downloads: pin/unpin, sidecars, `cover.jpg`, `downloads_index.json`. Never evicted,
/// no size cap (`docs/06-cache-and-offline.md` §5) — structurally so, since this index is entirely
/// separate from `lru::RollingCache`'s own `cache_index.json`, which never sees these rows at all.
pub struct Downloads {
    root: PathBuf,
    server: ServerId,
    profile: QualityProfile,
    fetcher: Arc<dyn Fetcher>,
    sink: ProgressSink,
    entries: HashMap<ItemId, DownloadEntry>,
}

impl Downloads {
    /// Opens (creating if absent) `root/downloads_index.json`. `profile` is decided once by the
    /// caller from `transcode.download_uncompressed` (`Direct` if set, the active profile
    /// otherwise — `docs/06-cache-and-offline.md` §5) rather than re-read on every `pin`, since
    /// this task's own given signature (`pin(&mut self, scope)`) has no room for it per call.
    pub fn open(
        root: &Path,
        server: ServerId,
        profile: QualityProfile,
        fetcher: Arc<dyn Fetcher>,
        sink: ProgressSink,
    ) -> Result<Self, CacheError> {
        std::fs::create_dir_all(root).map_err(|source| CacheError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let entries = load_or_quarantine(&root.join("downloads_index.json"), root);
        Ok(Downloads {
            root: root.to_path_buf(),
            server,
            profile,
            fetcher,
            sink,
            entries,
        })
    }

    pub fn total_bytes(&self) -> u64 {
        self.entries.values().map(|e| e.bytes).sum()
    }

    pub fn entries(&self) -> impl Iterator<Item = &DownloadEntry> {
        self.entries.values()
    }

    /// `11-07`: total bytes and entry count, read straight from `downloads_index.json` without
    /// constructing a full [`Downloads`] (which needs a real [`Fetcher`]/[`ProgressSink`] this
    /// call has no use for) — the About view's own "Downloads … (14.7 GB, 312 tracks)" line is a
    /// read-only stat, not a fetch. A missing or corrupt index reads as `(0, 0)`, the same "start
    /// clean" posture `load_or_quarantine` already has for the real path — this never quarantines
    /// the file itself, since it does no I/O beyond the one read.
    pub fn stats(root: &Path) -> (u64, usize) {
        let Ok(text) = std::fs::read_to_string(root.join("downloads_index.json")) else {
            return (0, 0);
        };
        let Ok(file) = serde_json::from_str::<DownloadsFile>(&text) else {
            return (0, 0);
        };
        let bytes = file.entries.iter().map(|e| e.bytes).sum();
        (bytes, file.entries.len())
    }

    /// Pins `scope`: expands to a track list if needed, then fetches with bounded concurrency of
    /// 2 (`docs/06-cache-and-offline.md` §5).
    pub async fn pin(&mut self, scope: DownloadScope) -> Result<(), CacheError> {
        let tracks = match scope {
            DownloadScope::Track(t) => vec![t],
            other => self.fetcher.expand(&other).await?,
        };
        self.fetch_all(tracks).await
    }

    /// Re-enqueues anything left incomplete by a prior run interrupted mid-pin
    /// (`docs/06-cache-and-offline.md` §5, "Interruption") — reconstructs each track from its own
    /// already-written `.loxia.json` sidecar (written before any bytes are fetched, so it survives
    /// a crash that happens before completion). Not in this task's own two-method sketch, but
    /// required for `interrupted_pin_resumes_on_restart`; no call site wires this into a real
    /// startup sequence yet (`docs/12-decisions.md`, the same "flagged, not silently uncalled"
    /// gap `08-03` left for `RollingCache::evict`).
    pub async fn resume_incomplete(&mut self) -> Result<(), CacheError> {
        let incomplete: Vec<Track> = self
            .entries
            .values()
            .filter(|e| !e.complete)
            .filter_map(|e| {
                std::fs::read_to_string(&e.sidecar)
                    .ok()
                    .and_then(|text| serde_json::from_str::<Track>(&text).ok())
            })
            .collect();
        if incomplete.is_empty() {
            return Ok(());
        }
        self.fetch_all(incomplete).await
    }

    /// Deletes the audio file, its sidecar, and — walking upward — any album/artist directory left
    /// with nothing but (optionally) `cover.jpg` in it. Every deletion is guarded by
    /// [`assert_within`]. Returns the bytes freed (`0` if `id` was never pinned).
    pub async fn unpin(&mut self, id: &ItemId) -> Result<u64, CacheError> {
        let Some(entry) = self.entries.remove(id) else {
            return Ok(0);
        };
        let bytes = entry.bytes;

        if entry.path.exists() {
            assert_within(&self.root, &entry.path)?;
            remove_file_ignoring_missing(&entry.path)?;
        }
        if entry.sidecar.exists() {
            assert_within(&self.root, &entry.sidecar)?;
            remove_file_ignoring_missing(&entry.sidecar)?;
        }
        if let Some(dir) = entry.path.parent() {
            self.prune_upward(dir.to_path_buf())?;
        }

        self.flush_index()?;
        Ok(bytes)
    }

    async fn fetch_all(&mut self, tracks: Vec<Track>) -> Result<(), CacheError> {
        let mut pending: Vec<(Track, PathBuf)> = Vec::with_capacity(tracks.len());
        for track in tracks {
            let candidate = download_path(&self.root, &self.server, &track);
            let track_id = track.id.clone();
            let dest = resolve_collision(&candidate, |p| path_belongs_to_other(p, &track_id))?;
            let sidecar = sidecar_path(&dest);
            write_sidecar(&sidecar, &track)?;
            self.entries.insert(
                track.id.clone(),
                DownloadEntry {
                    item: track.id.clone(),
                    path: dest.clone(),
                    profile: self.profile,
                    bytes: 0,
                    downloaded_at: None,
                    sidecar,
                    complete: false,
                },
            );
            pending.push((track, dest));
        }
        self.flush_index()?;

        let aborted = Arc::new(AtomicBool::new(false));
        let profile = self.profile;
        let fetcher = Arc::clone(&self.fetcher);
        let sink = Arc::clone(&self.sink);

        let results: Vec<(Track, PathBuf, Outcome)> =
            stream::iter(pending.into_iter().map(|(track, dest)| {
                let fetcher = Arc::clone(&fetcher);
                let sink = Arc::clone(&sink);
                let aborted = Arc::clone(&aborted);
                async move {
                    if aborted.load(Ordering::SeqCst) {
                        return (track, dest, Outcome::Skipped);
                    }
                    let part = part_path(&dest);
                    let id = track.id.clone();
                    let sink_for_progress = Arc::clone(&sink);
                    let on_progress = move |done: u64, total: u64| {
                        sink_for_progress(Event::Data(DataAction::DownloadProgress {
                            id: id.clone(),
                            done_bytes: done,
                            total_bytes: total,
                        }));
                    };
                    let outcome = match fetcher
                        .fetch_track(&track, profile, &part, &on_progress)
                        .await
                    {
                        Ok(bytes) => match std::fs::rename(&part, &dest) {
                            Ok(()) => Outcome::Done(bytes),
                            Err(source) => Outcome::Failed(CacheError::Io {
                                path: dest.clone(),
                                source,
                            }),
                        },
                        Err(error) => {
                            if is_storage_full(&error) {
                                aborted.store(true, Ordering::SeqCst);
                            }
                            Outcome::Failed(error)
                        }
                    };
                    (track, dest, outcome)
                }
            }))
            .buffer_unordered(2)
            .collect()
            .await;

        let mut covered_dirs: HashSet<PathBuf> = HashSet::new();
        let mut hit_storage_full = false;
        for (track, dest, outcome) in results {
            match outcome {
                Outcome::Done(bytes) => {
                    if let Some(entry) = self.entries.get_mut(&track.id) {
                        entry.bytes = bytes;
                        entry.complete = true;
                        entry.downloaded_at = Some(Timestamp::now());
                    }
                    if let Some(dir) = dest.parent()
                        && covered_dirs.insert(dir.to_path_buf())
                        && let Some(album_id) = &track.album_id
                    {
                        let cover = dir.join("cover.jpg");
                        if !cover.exists() {
                            let _ = self.fetcher.fetch_cover(album_id, &cover).await;
                        }
                    }
                }
                Outcome::Skipped => {}
                Outcome::Failed(error) => {
                    if is_storage_full(&error) {
                        hit_storage_full = true;
                    } else {
                        tracing::debug!(item = %track.id, %error, "download failed; left queued for resume");
                    }
                }
            }
        }

        self.flush_index()?;
        if hit_storage_full {
            (self.sink)(Event::System(SystemEvent::Toast {
                message: "disk full — download paused, resume later".to_string(),
                level: ToastLevel::Error,
            }));
        }
        Ok(())
    }

    /// Removes `dir`, then its parent, then its parent's parent, ... stopping at the first
    /// directory that still holds something other than `cover.jpg`, or at `self.root` itself.
    /// `cover.jpg` alone does not block pruning — it is deleted along with the directory, since a
    /// cover with no tracks left is meaningless (`docs/12-decisions.md`).
    fn prune_upward(&self, mut dir: PathBuf) -> Result<(), CacheError> {
        loop {
            if !dir.starts_with(&self.root) || dir == self.root {
                break;
            }
            let Ok(read) = std::fs::read_dir(&dir) else {
                break;
            };
            let mut leftovers = Vec::new();
            for entry in read {
                let Ok(entry) = entry else { break };
                leftovers.push(entry.path());
            }
            let only_cover = leftovers
                .iter()
                .all(|p| p.file_name().is_some_and(|n| n == "cover.jpg"));
            if !leftovers.is_empty() && !only_cover {
                break;
            }
            for path in &leftovers {
                assert_within(&self.root, path)?;
                remove_file_ignoring_missing(path)?;
            }
            assert_within(&self.root, &dir)?;
            if std::fs::remove_dir(&dir).is_err() {
                break;
            }
            let Some(parent) = dir.parent() else { break };
            dir = parent.to_path_buf();
        }
        Ok(())
    }

    fn flush_index(&self) -> Result<(), CacheError> {
        write_atomic(&self.root, &self.entries)
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

/// `<name>.loxia.json` alongside `<name>.<ext>` (`docs/06-cache-and-offline.md` §5) — the audio
/// file's own extension replaced, e.g. `01-03 - Motion.flac` → `01-03 - Motion.loxia.json`.
fn sidecar_path(dest: &Path) -> PathBuf {
    dest.with_extension("loxia.json")
}

fn write_sidecar(path: &Path, track: &Track) -> Result<(), CacheError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CacheError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let json = serde_json::to_vec_pretty(track).expect("Track always serialises");
    std::fs::write(path, json).map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// `resolve_collision`'s own caller-supplied predicate: reads whatever sidecar already sits at
/// `path` and compares its `id`. Unreadable/missing/corrupt is treated as "a different item, be
/// safe" — the same posture `layout.rs`'s own doc comment describes.
fn path_belongs_to_other(path: &Path, id: &ItemId) -> bool {
    let sidecar = sidecar_path(path);
    std::fs::read_to_string(&sidecar)
        .ok()
        .and_then(|text| serde_json::from_str::<Track>(&text).ok())
        .is_none_or(|existing| &existing.id != id)
}

fn remove_file_ignoring_missing(path: &Path) -> Result<(), CacheError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CacheError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn is_storage_full(error: &CacheError) -> bool {
    matches!(error, CacheError::Io { source, .. } if source.kind() == ErrorKind::StorageFull)
}

fn write_atomic(root: &Path, entries: &HashMap<ItemId, DownloadEntry>) -> Result<(), CacheError> {
    let index_path = root.join("downloads_index.json");
    let tmp_path = root.join("downloads_index.json.tmp");

    let file = DownloadsFile {
        entries: entries.values().cloned().collect(),
    };
    let json =
        serde_json::to_vec_pretty(&file).expect("DownloadEntry/DownloadsFile always serialise");

    let mut f = std::fs::File::create(&tmp_path).map_err(|source| CacheError::Io {
        path: tmp_path.clone(),
        source,
    })?;
    std::io::Write::write_all(&mut f, &json).map_err(|source| CacheError::Io {
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

/// A missing index (first run) is just an empty download set. A corrupt one is quarantined to
/// `downloads_index.json.bad` and this instance starts fresh — mirroring `manifest.rs`'s own
/// `load_or_quarantine` exactly, except a corrupt downloads index does *not* imply the actual
/// pinned files are gone: unlike the rolling cache, nothing here ever treats a downloaded file
/// with no index row as an orphan to delete (`downloads_not_evicted_by_lru` and this module's own
/// total absence of a `reconcile` both rely on that: a lost index costs the *browsability* of
/// already-downloaded files, never the files themselves).
fn load_or_quarantine(index_path: &Path, root: &Path) -> HashMap<ItemId, DownloadEntry> {
    let Ok(text) = std::fs::read_to_string(index_path) else {
        return HashMap::new();
    };
    match serde_json::from_str::<DownloadsFile>(&text) {
        Ok(file) => file
            .entries
            .into_iter()
            .map(|e| (e.item.clone(), e))
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "corrupt downloads index; quarantining and starting fresh");
            let bad_path = root.join("downloads_index.json.bad");
            let _ = std::fs::rename(index_path, &bad_path);
            HashMap::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::test_support::fixtures;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;
    use tempfile::tempdir;

    /// Records every event handed to it, for assertions — the test double for the real
    /// `mpsc::UnboundedSender<Event>` a production caller would use.
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<Event>>,
    }

    fn sink() -> (ProgressSink, Arc<RecordingSink>) {
        let recorder = Arc::new(RecordingSink::default());
        let for_closure = Arc::clone(&recorder);
        let sink: ProgressSink = Arc::new(move |event| {
            for_closure.events.lock().unwrap().push(event);
        });
        (sink, recorder)
    }

    /// An in-memory fetcher: "downloads" by appending `0xAB` bytes up to `bytes_per_track`,
    /// resuming from whatever `dest` already contains (mirroring `fetch_to_file`'s own `Range`
    /// semantics), tracking concurrency and every resume offset it observed. `fail_once_with`
    /// makes exactly the *first* `fetch_track` call that reaches it fail with the given error
    /// kind, letting tests simulate a transient failure or a disk-full condition.
    struct FakeFetcher {
        bytes_per_track: u64,
        albums: Mutex<HashMap<ItemId, Vec<Track>>>,
        cover_calls: Mutex<Vec<ItemId>>,
        resume_offsets_seen: Mutex<Vec<u64>>,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        fail_with: Mutex<Option<ErrorKind>>,
    }

    impl FakeFetcher {
        fn new(bytes_per_track: u64) -> Self {
            FakeFetcher {
                bytes_per_track,
                albums: Mutex::new(HashMap::new()),
                cover_calls: Mutex::new(Vec::new()),
                resume_offsets_seen: Mutex::new(Vec::new()),
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
                fail_with: Mutex::new(None),
            }
        }

        fn with_album(self, id: ItemId, tracks: Vec<Track>) -> Self {
            self.albums.lock().unwrap().insert(id, tracks);
            self
        }

        fn fail_once_with(&self, kind: ErrorKind) {
            *self.fail_with.lock().unwrap() = Some(kind);
        }
    }

    impl Fetcher for FakeFetcher {
        fn expand<'a>(
            &'a self,
            scope: &'a DownloadScope,
        ) -> BoxFuture<'a, Result<Vec<Track>, CacheError>> {
            Box::pin(async move {
                let id = match scope {
                    DownloadScope::Album(id) | DownloadScope::Artist(id) => id.clone(),
                    DownloadScope::Playlist(id) => ItemId::from(id.as_str()),
                    DownloadScope::Track(_) => unreachable!("expand is never called for Track"),
                };
                Ok(self
                    .albums
                    .lock()
                    .unwrap()
                    .get(&id)
                    .cloned()
                    .unwrap_or_default())
            })
        }

        fn fetch_track<'a>(
            &'a self,
            _track: &'a Track,
            _profile: QualityProfile,
            dest: &'a Path,
            on_progress: &'a (dyn Fn(u64, u64) + Send + Sync),
        ) -> BoxFuture<'a, Result<u64, CacheError>> {
            Box::pin(async move {
                let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_in_flight.fetch_max(now, Ordering::SeqCst);
                // Yields so a concurrently-polled sibling fetch (`buffer_unordered(2)`) genuinely
                // overlaps with this one instead of running start-to-finish alone.
                tokio::time::sleep(Duration::from_millis(20)).await;

                let existing = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
                self.resume_offsets_seen.lock().unwrap().push(existing);

                if let Some(kind) = self.fail_with.lock().unwrap().take() {
                    self.in_flight.fetch_sub(1, Ordering::SeqCst);
                    return Err(CacheError::Io {
                        path: dest.to_path_buf(),
                        source: std::io::Error::from(kind),
                    });
                }

                let total = self.bytes_per_track;
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                let remaining = total.saturating_sub(existing);
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dest)
                    .unwrap();
                std::io::Write::write_all(&mut f, &vec![0xABu8; remaining as usize]).unwrap();
                drop(f);
                on_progress(total, total);

                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                Ok(total)
            })
        }

        fn fetch_cover<'a>(
            &'a self,
            album: &'a ItemId,
            dest: &'a Path,
        ) -> BoxFuture<'a, Result<(), CacheError>> {
            Box::pin(async move {
                self.cover_calls.lock().unwrap().push(album.clone());
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(dest, b"cover").map_err(|source| CacheError::Io {
                    path: dest.to_path_buf(),
                    source,
                })
            })
        }
    }

    fn track(name: &str, id: &str, album: &str) -> Track {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album(album, 2019, &a);
        let mut t = fixtures::track(name, 1, &alb, &[&a]);
        t.id = ItemId::from(id);
        t
    }

    fn open(dir: &Path, fetcher: FakeFetcher) -> (Downloads, Arc<RecordingSink>) {
        let (sink, recorder) = sink();
        let downloads = Downloads::open(
            dir,
            ServerId::from("srv1"),
            QualityProfile::Direct,
            Arc::new(fetcher),
            sink,
        )
        .unwrap();
        (downloads, recorder)
    }

    #[tokio::test]
    async fn pin_track_writes_audio_and_sidecar() {
        let dir = tempdir().unwrap();
        let (mut downloads, _rec) = open(dir.path(), FakeFetcher::new(100));
        let t = track("Motion", "t1", "Care");

        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();

        let entry = downloads.entries().find(|e| e.item == t.id).unwrap();
        assert!(entry.complete);
        assert!(entry.path.exists());
        assert!(entry.sidecar.exists());
        let sidecar: Track =
            serde_json::from_str(&std::fs::read_to_string(&entry.sidecar).unwrap()).unwrap();
        assert_eq!(sidecar.id, t.id);
    }

    #[tokio::test]
    async fn pin_album_writes_cover_once() {
        let dir = tempdir().unwrap();
        let album_id = ItemId::from("al1");
        let t1 = track("Motion", "t1", "Care");
        let t2 = track("Escape", "t2", "Care");
        let fetcher = FakeFetcher::new(50).with_album(album_id.clone(), vec![t1, t2]);
        let (mut downloads, _rec) = open(dir.path(), fetcher);

        downloads.pin(DownloadScope::Album(album_id)).await.unwrap();

        assert_eq!(downloads.entries().count(), 2);
        let cover_dirs: HashSet<_> = downloads
            .entries()
            .map(|e| e.path.parent().unwrap().join("cover.jpg"))
            .collect();
        assert_eq!(cover_dirs.len(), 1, "both tracks share one album directory");
        for cover in cover_dirs {
            assert!(cover.exists());
        }
    }

    #[tokio::test]
    async fn pin_expands_artist_to_all_tracks() {
        let dir = tempdir().unwrap();
        let artist_id = ItemId::from("ar1");
        let t1 = track("Motion", "t1", "Care");
        let t2 = track("Growth", "t2", "Another Album");
        let fetcher =
            FakeFetcher::new(10).with_album(artist_id.clone(), vec![t1.clone(), t2.clone()]);
        let (mut downloads, _rec) = open(dir.path(), fetcher);

        downloads
            .pin(DownloadScope::Artist(artist_id))
            .await
            .unwrap();

        assert!(downloads.entries().any(|e| e.item == t1.id));
        assert!(downloads.entries().any(|e| e.item == t2.id));
    }

    #[tokio::test]
    async fn concurrency_limited_to_two() {
        let dir = tempdir().unwrap();
        let all = ItemId::from("all");
        let tracks: Vec<Track> = (0..5)
            .map(|i| track(&format!("t{i}"), &format!("id{i}"), "Album"))
            .collect();
        let fetcher = Arc::new(FakeFetcher::new(10).with_album(all.clone(), tracks));
        let (sink, _rec) = sink();
        let mut downloads = Downloads::open(
            dir.path(),
            ServerId::from("srv1"),
            QualityProfile::Direct,
            fetcher.clone() as Arc<dyn Fetcher>,
            sink,
        )
        .unwrap();

        downloads.pin(DownloadScope::Artist(all)).await.unwrap();

        let max = fetcher.max_in_flight.load(Ordering::SeqCst);
        assert!(max >= 2, "concurrency actually reached 2, got {max}");
        assert!(max <= 2, "never more than 2 concurrent fetches, got {max}");
    }

    #[tokio::test]
    async fn resume_uses_range_request() {
        let dir = tempdir().unwrap();
        let fetcher = Arc::new(FakeFetcher::new(1000));
        let (sink, _rec) = sink();
        let mut downloads = Downloads::open(
            dir.path(),
            ServerId::from("srv1"),
            QualityProfile::Direct,
            fetcher.clone() as Arc<dyn Fetcher>,
            sink,
        )
        .unwrap();
        let t = track("Motion", "t1", "Care");
        let dest = download_path(dir.path(), &ServerId::from("srv1"), &t);
        let part = part_path(&dest);
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, vec![0xABu8; 400]).unwrap();

        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();

        assert_eq!(
            fetcher.resume_offsets_seen.lock().unwrap().as_slice(),
            &[400]
        );
        let entry = downloads.entries().find(|e| e.item == t.id).unwrap();
        assert_eq!(entry.bytes, 1000);
        assert_eq!(std::fs::metadata(&entry.path).unwrap().len(), 1000);
    }

    #[tokio::test]
    async fn download_forces_direct_profile() {
        let dir = tempdir().unwrap();
        let (mut downloads, _rec) = open(dir.path(), FakeFetcher::new(10));
        let t = track("Motion", "t1", "Care");

        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();

        let entry = downloads.entries().find(|e| e.item == t.id).unwrap();
        assert_eq!(entry.profile, QualityProfile::Direct);
    }

    #[tokio::test]
    async fn download_forces_direct_even_when_active_profile_is_low() {
        // `Downloads::open`'s own `profile` argument stands in for "the profile the caller
        // resolved from `transcode.download_uncompressed`" (`docs/12-decisions.md`) — passing
        // `Direct` here regardless of what a hypothetical `TranscodeLow` active session profile
        // would be is exactly that resolution having already happened.
        let dir = tempdir().unwrap();
        let (sink, _rec) = sink();
        let mut downloads = Downloads::open(
            dir.path(),
            ServerId::from("srv1"),
            QualityProfile::Direct,
            Arc::new(FakeFetcher::new(10)),
            sink,
        )
        .unwrap();
        let t = track("Motion", "t1", "Care");

        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();

        let entry = downloads.entries().find(|e| e.item == t.id).unwrap();
        assert_eq!(entry.profile, QualityProfile::Direct);
    }

    #[tokio::test]
    async fn progress_events_emitted() {
        let dir = tempdir().unwrap();
        let (mut downloads, recorder) = open(dir.path(), FakeFetcher::new(10));
        let t = track("Motion", "t1", "Care");

        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();

        let events = recorder.events.lock().unwrap();
        assert!(events.iter().any(
            |e| matches!(e, Event::Data(DataAction::DownloadProgress { id, .. }) if *id == t.id)
        ));
    }

    #[tokio::test]
    async fn unpin_removes_file_sidecar_and_empty_dirs() {
        let dir = tempdir().unwrap();
        let (mut downloads, _rec) = open(dir.path(), FakeFetcher::new(10));
        let t = track("Motion", "t1", "Care");
        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();
        let entry = downloads.entries().find(|e| e.item == t.id).unwrap();
        let path = entry.path.clone();
        let sidecar = entry.sidecar.clone();
        let album_dir = path.parent().unwrap().to_path_buf();

        downloads.unpin(&t.id).await.unwrap();

        assert!(!path.exists());
        assert!(!sidecar.exists());
        assert!(
            !album_dir.exists(),
            "the now-empty album directory is pruned"
        );
    }

    #[tokio::test]
    async fn unpin_leaves_non_empty_dirs() {
        let dir = tempdir().unwrap();
        let t1 = track("Motion", "t1", "Care");
        let t2 = track("Escape", "t2", "Care");
        let (mut downloads, _rec) = open(dir.path(), FakeFetcher::new(10));
        downloads
            .pin(DownloadScope::Track(t1.clone()))
            .await
            .unwrap();
        downloads
            .pin(DownloadScope::Track(t2.clone()))
            .await
            .unwrap();
        let album_dir = downloads
            .entries()
            .find(|e| e.item == t1.id)
            .unwrap()
            .path
            .parent()
            .unwrap()
            .to_path_buf();

        downloads.unpin(&t1.id).await.unwrap();

        assert!(
            album_dir.exists(),
            "t2 is still pinned in the same directory"
        );
        let entry2 = downloads.entries().find(|e| e.item == t2.id).unwrap();
        assert!(entry2.path.exists());
    }

    #[tokio::test]
    async fn unpin_returns_bytes_freed() {
        let dir = tempdir().unwrap();
        let (mut downloads, _rec) = open(dir.path(), FakeFetcher::new(123));
        let t = track("Motion", "t1", "Care");
        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();

        let freed = downloads.unpin(&t.id).await.unwrap();
        assert_eq!(freed, 123);

        let freed_again = downloads.unpin(&t.id).await.unwrap();
        assert_eq!(
            freed_again, 0,
            "unpinning something not pinned frees nothing"
        );
    }

    #[tokio::test]
    async fn downloads_not_evicted_by_lru() {
        use crate::layout::CacheKey;
        use crate::lru::RollingCache;

        let dir = tempdir().unwrap();
        let cache_root = dir.path().join("cache");
        let downloads_root = dir.path().join("downloads");
        std::fs::create_dir_all(&downloads_root).unwrap();

        let (mut downloads, _rec) = open(&downloads_root, FakeFetcher::new(1000));
        let t = track("Motion", "t1", "Care");
        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();
        let pinned_path = downloads
            .entries()
            .find(|e| e.item == t.id)
            .unwrap()
            .path
            .clone();

        let mut cache = RollingCache::open(&cache_root).unwrap();
        let key = CacheKey {
            server: ServerId::from("srv1"),
            item: ItemId::from("cached"),
            profile: QualityProfile::Direct,
        };
        let path = cache.begin_fetch(&key, Path::new("srv/A/B/track.direct.flac"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![0u8; 10_000]).unwrap();
        cache.complete_fetch(&key, 10_000);
        cache.evict(1, &[]).unwrap();

        assert!(
            pinned_path.exists(),
            "download-tier file survives cache eviction"
        );
    }

    #[tokio::test]
    async fn interrupted_pin_resumes_on_restart() {
        let dir = tempdir().unwrap();
        let t = track("Motion", "t1", "Care");
        let sidecar;
        let dest;
        {
            let fetcher = FakeFetcher::new(500);
            fetcher.fail_once_with(ErrorKind::Other);
            let (mut downloads, _rec) = open(dir.path(), fetcher);
            // The fetch fails; the entry stays queued (incomplete) rather than erroring `pin`
            // itself, matching "a failure is logged, never surfaced" (`08-02`'s own precedent).
            downloads
                .pin(DownloadScope::Track(t.clone()))
                .await
                .unwrap();
            let entry = downloads.entries().find(|e| e.item == t.id).unwrap();
            assert!(!entry.complete);
            sidecar = entry.sidecar.clone();
            dest = entry.path.clone();
        }
        assert!(
            sidecar.exists(),
            "sidecar survives the failed attempt for resume"
        );
        assert!(!dest.exists());

        // A fresh `Downloads`, as if relaunched, resumes the incomplete entry.
        let (mut downloads, _rec) = open(dir.path(), FakeFetcher::new(500));
        downloads.resume_incomplete().await.unwrap();
        let entry = downloads.entries().find(|e| e.item == t.id).unwrap();
        assert!(entry.complete);
        assert!(entry.path.exists());
    }

    #[tokio::test]
    async fn enospc_cancels_batch_and_toasts_once() {
        let dir = tempdir().unwrap();
        let album_id = ItemId::from("al1");
        let tracks: Vec<Track> = (0..4)
            .map(|i| track(&format!("t{i}"), &format!("id{i}"), "Care"))
            .collect();
        let fetcher = FakeFetcher::new(10).with_album(album_id.clone(), tracks);
        fetcher.fail_once_with(ErrorKind::StorageFull);
        let (mut downloads, recorder) = open(dir.path(), fetcher);

        downloads.pin(DownloadScope::Album(album_id)).await.unwrap();

        let toasts = recorder
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Event::System(SystemEvent::Toast { .. })))
            .count();
        assert_eq!(
            toasts, 1,
            "exactly one toast even though several files were queued"
        );
        assert!(
            downloads.entries().any(|e| !e.complete),
            "at least one entry stays queued rather than being retried to fail identically"
        );
    }

    #[tokio::test]
    async fn deletion_guarded_by_assert_within() {
        let downloads_dir = tempdir().unwrap();
        let outside_dir = tempdir().unwrap();
        let (mut downloads, _rec) = open(downloads_dir.path(), FakeFetcher::new(10));
        let t = track("Motion", "t1", "Care");
        downloads
            .pin(DownloadScope::Track(t.clone()))
            .await
            .unwrap();

        let escaped = outside_dir.path().join("escaped.flac");
        std::fs::write(&escaped, b"x").unwrap();
        downloads.entries.get_mut(&t.id).unwrap().path = escaped.clone();

        let result = downloads.unpin(&t.id).await;
        assert!(
            result.is_err(),
            "a path escaping the downloads root must be refused"
        );
        assert!(
            escaped.exists(),
            "the escaping file is refused, not deleted"
        );
    }

    #[tokio::test]
    async fn stats_reads_bytes_and_count_without_a_fetcher() {
        let dir = tempdir().unwrap();
        let (mut downloads, _rec) = open(dir.path(), FakeFetcher::new(100));
        downloads
            .pin(DownloadScope::Track(track("Motion", "t1", "Care")))
            .await
            .unwrap();
        downloads
            .pin(DownloadScope::Track(track("Escape", "t2", "Care")))
            .await
            .unwrap();

        let (bytes, count) = Downloads::stats(dir.path());
        assert_eq!(count, 2);
        assert_eq!(bytes, downloads.total_bytes());
    }

    #[test]
    fn stats_on_a_never_downloaded_directory_is_zero() {
        let dir = tempdir().unwrap();
        assert_eq!(Downloads::stats(dir.path()), (0, 0));
    }
}
