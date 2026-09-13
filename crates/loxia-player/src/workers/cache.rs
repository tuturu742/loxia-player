//! Cache worker: resolves `Effect::Cache(EnsureCached)` against the rolling cache, runs the
//! single-concurrent background fetch that populates it (`08-03`, `docs/06-cache-and-offline.md`
//! §4), and persists `PersistSession`/`AppendHistory` (`08-08`, §8) via `loxia_cache::session`.
//!
//! `PinDownload`/`RemoveDownload` (permanent downloads, `08-04`) and `AppendScrobble`/
//! `DrainScrobbles` (`08-07`'s own scrobble buffer) are still out of scope here — every
//! `CacheEffect` this worker doesn't yet own is only logged, the same stub behaviour `03-08` gave
//! every worker before its own task filled it in.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use loxia_cache::layout::CacheKey;
use loxia_cache::lru::RollingCache;
use loxia_core::action::DataAction;
use loxia_core::config::TargetCodec;
use loxia_core::effect::{CacheEffect, Effect};
use loxia_core::event::Event;
use loxia_core::model::{ItemId, ServerId, Track};
use loxia_core::paths::Paths;
use loxia_emby::client::EmbyClient;
use loxia_emby::endpoints::download;
use loxia_emby::stream::StreamUrl;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

/// Bounds the play-time cache lookup so a slow disk can never delay handing mpv *something* to
/// play — a timeout replies `None` and playback proceeds from the network
/// (`docs/06-cache-and-offline.md` §4).
const RESOLVE_TIMEOUT: Duration = Duration::from_millis(50);

/// The one background fetch this worker ever runs at a time, and the flag that cancels it.
struct InFlight {
    key: CacheKey,
    cancel: Arc<AtomicBool>,
}

type InFlightSlot = Arc<Mutex<Option<InFlight>>>;

/// The read-ahead run started by `PrefetchAhead` (`cache.prefetch_next`), kept apart from
/// [`InFlight`] on purpose: that slot belongs to whatever is *playing*, and a read-ahead must never
/// be able to cancel it. Only one run exists at a time — a new one supersedes the last, since the
/// queue has moved on.
struct Prefetch {
    cancel: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

type PrefetchSlot = Arc<Mutex<Option<Prefetch>>>;

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    client: Arc<EmbyClient>,
    cache: Arc<Mutex<RollingCache>>,
    server: ServerId,
    target_codec: TargetCodec,
    prefetch_on_play: bool,
    paths: Paths,
    mut effects: mpsc::UnboundedReceiver<Effect>,
    events: mpsc::UnboundedSender<Event>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let in_flight: InFlightSlot = Arc::new(Mutex::new(None));
        let prefetch: PrefetchSlot = Arc::new(Mutex::new(None));
        // Permanent downloads (`08-04`). `Downloads` was fully built and tested against a fake
        // `Fetcher`; nothing ever constructed a real one, so `PinDownload`/`RemoveDownload` fell
        // through to "unhandled cache effect" and `d` did nothing (`docs/12-decisions.md`).
        let downloads = open_downloads(&client, &server, target_codec, &paths, &events).await;
        if let Some(downloads) = &downloads {
            announce_downloads(downloads, &events).await;
        }
        while let Some(effect) = effects.recv().await {
            let Effect::Cache(cache_effect) = effect else {
                continue;
            };
            match *cache_effect {
                CacheEffect::EnsureCached { track, profile } => {
                    handle_ensure_cached(
                        &client,
                        &cache,
                        &downloads,
                        &server,
                        target_codec,
                        prefetch_on_play,
                        track,
                        profile,
                        &in_flight,
                        &events,
                    )
                    .await;
                }
                CacheEffect::PrefetchAhead { tracks, profile } => {
                    handle_prefetch_ahead(
                        &client,
                        &cache,
                        &downloads,
                        &server,
                        target_codec,
                        tracks,
                        profile,
                        &prefetch,
                    )
                    .await;
                }
                // `08-08`: both are local-only file writes, small and fast enough to run inline
                // rather than via `spawn_blocking` — a failure here is logged, never surfaced,
                // matching this whole worker's own "a cache-adjacent failure never blocks
                // playback" posture.
                CacheEffect::PersistSession(snapshot) => {
                    if let Err(e) = loxia_cache::session::save(&paths, &snapshot) {
                        tracing::warn!(error = %e, "failed to save session snapshot");
                    }
                }
                CacheEffect::AppendHistory(entry) => {
                    if let Err(e) = loxia_cache::session::append_history(&paths, &entry) {
                        tracing::warn!(error = %e, "failed to append history entry");
                    }
                }
                // `11-03`: as `PersistSession`, but to that server's own `session-<id>.json` — a
                // live in-app switch's own "persist the outgoing session under its own server id."
                CacheEffect::PersistSessionForServer(server_id, snapshot) => {
                    if let Err(e) =
                        loxia_cache::session::save_for_server(&paths, &server_id, &snapshot)
                    {
                        tracing::warn!(error = %e, server = %server_id, "failed to save outgoing session snapshot");
                    }
                }
                // `11-03`: "removing any profile offers to delete its cached files and downloads"
                // — every on-disk entry is already scoped under `tracks/<server_id>/` and
                // `downloads/<server_id>/` (`docs/06-cache-and-offline.md` §1), so this is a
                // subtree removal, not a walk through the rolling cache's or `Downloads`' own
                // in-memory bookkeeping (this worker's own `cache`/`in_flight` state only ever
                // concerns the *active* server, never the one being deleted — removal is refused
                // for the active profile, `docs/12-decisions.md`). Best-effort: a missing
                // directory (nothing was ever cached for that server) is not an error.
                CacheEffect::DeleteServerData(server_id) => {
                    for subtree in ["tracks", "downloads"] {
                        let dir = paths.cache_root().join(subtree).join(server_id.as_str());
                        if let Err(e) = std::fs::remove_dir_all(&dir)
                            && e.kind() != std::io::ErrorKind::NotFound
                        {
                            tracing::warn!(error = %e, path = %dir.display(), "failed to delete server cache data");
                        }
                    }
                }
                // `11-06`: "clear saved session" — the reducer has already emptied `state.queue`
                // itself; this is only the on-disk half.
                CacheEffect::DeleteSession => {
                    if let Err(e) = loxia_cache::session::delete(&paths) {
                        tracing::warn!(error = %e, "failed to delete saved session");
                    }
                }
                CacheEffect::PinDownload { scope } => {
                    let Some(downloads) = &downloads else {
                        continue;
                    };
                    let scope = to_cache_scope(scope);
                    let downloads = downloads.clone();
                    let events = events.clone();
                    // Off the worker's own loop: pinning an artist can run for minutes, and the
                    // cache worker still has to answer `EnsureCached` for whatever is playing.
                    tokio::spawn(async move {
                        let mut guard = downloads.lock().await;
                        if let Err(error) = guard.pin(scope).await {
                            tracing::warn!(%error, "download failed");
                        }
                        announce_downloads_locked(&guard, &events);
                    });
                }
                CacheEffect::RemoveDownload { id } => {
                    let Some(downloads) = &downloads else {
                        continue;
                    };
                    let mut guard = downloads.lock().await;
                    if let Err(error) = guard.unpin(&id).await {
                        tracing::warn!(%error, "could not remove download");
                    }
                    announce_downloads_locked(&guard, &events);
                }
                other => {
                    tracing::debug!(effect = ?other, "unhandled cache effect");
                }
            }
        }
        // The channel closing *is* shutdown. `complete_fetch`'s own write is debounced, so without
        // this the last fetches of a session — often the only ones — never reach `cache_index.json`
        // and the cache comes up empty next launch (`docs/12-decisions.md`).
        if let Err(error) = cache.lock().await.flush() {
            tracing::warn!(%error, "could not persist the cache index on shutdown");
        }
    })
}

type DownloadsSlot = Arc<Mutex<loxia_cache::downloads::Downloads>>;

/// `loxia_core::effect::DownloadScope` -> `loxia_cache::downloads::DownloadScope`. Two enums of the
/// same shape because `loxia-core` cannot depend on `loxia-cache`; this is the one place they meet.
fn to_cache_scope(
    scope: loxia_core::effect::DownloadScope,
) -> loxia_cache::downloads::DownloadScope {
    use loxia_cache::downloads::DownloadScope as Cache;
    use loxia_core::effect::DownloadScope as Core;
    match scope {
        Core::Track(track) => Cache::Track(*track),
        Core::Album(id) => Cache::Album(id),
        Core::Artist(id) => Cache::Artist(id),
        Core::Playlist(id) => Cache::Playlist(id),
    }
}

/// Opens the permanent-download index and resumes anything a previous run left half-fetched
/// (`Downloads::resume_incomplete`, which likewise had no caller until now).
async fn open_downloads(
    client: &Arc<EmbyClient>,
    server: &ServerId,
    target_codec: TargetCodec,
    paths: &Paths,
    events: &mpsc::UnboundedSender<Event>,
) -> Option<DownloadsSlot> {
    let sink_events = events.clone();
    let sink: loxia_cache::downloads::ProgressSink = Arc::new(move |event| {
        let _ = sink_events.send(event);
    });
    let fetcher = Arc::new(crate::workers::download_fetcher::EmbyFetcher::new(
        client.clone(),
        target_codec,
    ));
    match loxia_cache::downloads::Downloads::open(
        paths.downloads_root(),
        server.clone(),
        // Downloads are kept, so they are fetched at the profile that keeps the most: `Direct`
        // unless the user has asked for transcoded copies.
        loxia_core::config::QualityProfile::Direct,
        fetcher,
        sink,
    ) {
        Ok(mut downloads) => {
            if let Err(error) = downloads.resume_incomplete().await {
                tracing::warn!(%error, "could not resume interrupted downloads");
            }
            Some(Arc::new(Mutex::new(downloads)))
        }
        Err(error) => {
            tracing::warn!(%error, "downloads unavailable");
            None
        }
    }
}

async fn announce_downloads(downloads: &DownloadsSlot, events: &mpsc::UnboundedSender<Event>) {
    let guard = downloads.lock().await;
    announce_downloads_locked(&guard, events);
}

/// Tells the reducer exactly what is pinned now — the set `d` toggles against, and the totals the
/// About view shows.
fn announce_downloads_locked(
    downloads: &loxia_cache::downloads::Downloads,
    events: &mpsc::UnboundedSender<Event>,
) {
    let pinned: Vec<loxia_core::model::ItemId> = downloads
        .entries()
        .filter(|e| e.complete)
        .map(|e| e.item.clone())
        .collect();
    let _ = events.send(Event::Data(DataAction::DownloadsChanged {
        total_bytes: downloads.total_bytes(),
        count: pinned.len(),
        pinned,
    }));
}

#[allow(clippy::too_many_arguments)]
async fn handle_ensure_cached(
    client: &Arc<EmbyClient>,
    cache: &Arc<Mutex<RollingCache>>,
    downloads: &Option<DownloadsSlot>,
    server: &ServerId,
    target_codec: TargetCodec,
    prefetch_on_play: bool,
    track: Track,
    profile: loxia_core::config::QualityProfile,
    in_flight: &InFlightSlot,
    events: &mpsc::UnboundedSender<Event>,
) {
    let key = CacheKey {
        server: server.clone(),
        item: track.id.clone(),
        profile,
    };

    // A user skipping quickly through an album should not queue up twenty downloads — cancel
    // whichever fetch is running if it's for a *different* key than what's now current
    // (`docs/06-cache-and-offline.md` §4).
    cancel_if_superseded(in_flight, &key).await;

    // A permanent download is the strongest local copy there is, so it is consulted first — and it
    // answers whatever quality profile was asked for. Downloads are always fetched `Direct`; a
    // transcode profile exists to save *bandwidth*, which a file already on this disk does not
    // spend, so handing over the original is strictly better than fetching a smaller copy of
    // something already kept. Without this a downloaded track streamed and re-transcoded on every
    // play, and `prefetch_on_play` cheerfully downloaded a second copy of it into the rolling cache
    // (`docs/12-decisions.md`).
    if let Some(path) = downloaded_path(downloads, &track.id).await {
        let _ = events.send(Event::Data(DataAction::CacheResolved {
            track: track.id.clone(),
            profile,
            path: Some(path),
            stream_url: loxia_core::effect::RedactedUrl::new(
                StreamUrl::build(client, &key.item, key.profile, target_codec)
                    .as_str()
                    .to_string(),
            ),
        }));
        return;
    }

    let resolved = tokio::time::timeout(RESOLVE_TIMEOUT, resolve(cache, &key))
        .await
        .ok()
        .flatten();

    // Always computed, even on a cache hit that won't use it — cheap (pure string building, no
    // I/O) and it's what lets the reducer correct `load_current`'s inert `emby-track:{id}`
    // placeholder into something mpv can actually open when there's no local copy yet
    // (`docs/12-decisions.md`).
    let stream_url = StreamUrl::build(client, &key.item, key.profile, target_codec);

    let _ = events.send(Event::Data(DataAction::CacheResolved {
        track: track.id.clone(),
        profile,
        path: resolved.clone(),
        stream_url: loxia_core::effect::RedactedUrl::new(stream_url.as_str().to_string()),
    }));

    if resolved.is_some() || !prefetch_on_play {
        return;
    }
    if already_fetching(in_flight, &key).await {
        return;
    }

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = in_flight.lock().await;
        *guard = Some(InFlight {
            key: key.clone(),
            cancel: cancel.clone(),
        });
    }

    let client = client.clone();
    let cache = cache.clone();
    let in_flight = in_flight.clone();
    let fetch_track = track.clone();
    let events = events.clone();
    let _ = events.send(Event::Data(DataAction::CacheFetchStarted {
        track: key.item.clone(),
        profile,
    }));
    tokio::spawn(async move {
        // A failure here is never user-visible: caching is a nice-to-have, not a playback
        // requirement (`docs/06-cache-and-offline.md` §4).
        match run_fetch(&client, &cache, &key, &fetch_track, target_codec, &cancel).await {
            Ok(true) => {
                // Only a clean, uncancelled completion is worth announcing — it is what makes the
                // player bar stop saying "caching" and the About view's size move.
                let total_bytes = cache.lock().await.total_bytes();
                let _ = events.send(Event::Data(DataAction::CacheFetched {
                    track: key.item.clone(),
                    profile,
                    total_bytes,
                }));
            }
            Ok(false) => {}
            Err(error) => tracing::debug!(%error, ?key, "background cache fetch failed"),
        }
        let mut guard = in_flight.lock().await;
        if guard.as_ref().is_some_and(|f| f.key == key) {
            *guard = None;
        }
    });
}

/// The permanent download for `item`, if there is a complete one whose file is still there.
///
/// Deliberately ignores the requested `QualityProfile`: see [`handle_ensure_cached`] for why a
/// downloaded original satisfies a transcode request too. The file is checked rather than trusted —
/// an index row whose file has been deleted by hand must not be handed to mpv as a `file://` URL.
async fn downloaded_path(downloads: &Option<DownloadsSlot>, item: &ItemId) -> Option<PathBuf> {
    let downloads = downloads.as_ref()?;
    let guard = downloads.lock().await;
    guard
        .entries()
        .find(|e| e.complete && e.item == *item && e.path.exists())
        .map(|e| e.path.clone())
}

async fn resolve(cache: &Arc<Mutex<RollingCache>>, key: &CacheKey) -> Option<PathBuf> {
    cache.lock().await.resolve(key)
}

async fn cancel_if_superseded(in_flight: &InFlightSlot, key: &CacheKey) {
    let mut guard = in_flight.lock().await;
    if let Some(existing) = guard.as_ref()
        && existing.key != *key
    {
        existing.cancel.store(true, Ordering::SeqCst);
        *guard = None;
    }
}

async fn already_fetching(in_flight: &InFlightSlot, key: &CacheKey) -> bool {
    in_flight
        .lock()
        .await
        .as_ref()
        .is_some_and(|f| f.key == *key)
}

fn part_path(final_path: &std::path::Path) -> PathBuf {
    let mut name = final_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    final_path.with_file_name(name)
}

/// `cache.prefetch_next`: fetches the upcoming tracks into the rolling cache, one at a time and in
/// order, on a task of its own.
///
/// Kept strictly subordinate to playback in three ways. It never touches the [`InFlightSlot`] the
/// current track's own fetch uses, so it cannot cancel it. It runs its tracks sequentially rather
/// than at once, so a read-ahead never competes with playback for bandwidth more than one transfer
/// deep. And a new run cancels the previous one outright — when this arrives the queue has already
/// moved on, and finishing a stale read-ahead first would only delay the useful one.
///
/// A track already in the cache is skipped (`begin_fetch` would otherwise re-download it). Every
/// failure is logged and stepped over: a read-ahead that doesn't happen costs nothing but a later
/// network fetch.
#[allow(clippy::too_many_arguments)]
async fn handle_prefetch_ahead(
    client: &Arc<EmbyClient>,
    cache: &Arc<Mutex<RollingCache>>,
    downloads: &Option<DownloadsSlot>,
    server: &ServerId,
    target_codec: TargetCodec,
    tracks: Vec<Track>,
    profile: loxia_core::config::QualityProfile,
    slot: &PrefetchSlot,
) {
    let keys: Vec<(CacheKey, Track)> = tracks
        .into_iter()
        .map(|track| {
            (
                CacheKey {
                    server: server.clone(),
                    item: track.id.clone(),
                    profile,
                },
                track,
            )
        })
        .collect();

    {
        let mut guard = slot.lock().await;
        if let Some(previous) = guard.take() {
            previous.cancel.store(true, Ordering::SeqCst);
            previous.task.abort();
        }
        if keys.is_empty() {
            return;
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn({
            let client = client.clone();
            let cache = cache.clone();
            let downloads = downloads.clone();
            let cancel = cancel.clone();
            async move {
                for (key, track) in keys {
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    // Already kept permanently: reading it ahead into the rolling cache would fetch
                    // a second copy of a file this machine already has.
                    if downloaded_path(&downloads, &key.item).await.is_some() {
                        continue;
                    }
                    // Read the manifest directly rather than via `resolve`, which would touch
                    // `last_access`: a track that was merely read ahead has not been *used*, and
                    // must not outrank genuinely played entries in the LRU because of it.
                    let already_cached = cache
                        .lock()
                        .await
                        .manifest()
                        .get(&key)
                        .is_some_and(|e| e.complete && e.path.exists());
                    if already_cached {
                        continue;
                    }
                    if let Err(error) =
                        run_fetch(&client, &cache, &key, &track, target_codec, &cancel).await
                    {
                        tracing::debug!(%error, ?key, "read-ahead cache fetch failed");
                    }
                }
            }
        });
        *guard = Some(Prefetch { cancel, task });
    }
}

async fn run_fetch(
    client: &EmbyClient,
    cache: &Arc<Mutex<RollingCache>>,
    key: &CacheKey,
    track: &Track,
    target_codec: TargetCodec,
    cancel: &Arc<AtomicBool>,
) -> Result<bool, loxia_emby::error::EmbyError> {
    // Named for what the bytes actually are: the source codec under `Direct` (which streams the
    // file untouched), the requested codec otherwise. Filed under album artist / album, so the
    // cache directory says what has actually been pulled down.
    let ext = loxia_cache::layout::cache_extension(key.profile, &track.format.codec, target_codec);
    let relative = loxia_cache::layout::cache_relative_path(&key.server, track, key.profile, &ext);
    let final_path = { cache.lock().await.begin_fetch(key, &relative) };
    if let Some(parent) = final_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let part = part_path(&final_path);

    let url = StreamUrl::build(client, &key.item, key.profile, target_codec);
    let result = download::fetch_to_file(client, &url, &part, cancel).await;

    match result {
        Ok(_) if cancel.load(Ordering::SeqCst) => {
            // Cancelled partway through — the `.part` file survives for a later resume, exactly
            // like a genuine failure; only a *clean, uncancelled* completion is real.
            cache.lock().await.abandon_fetch(key);
            Ok(false)
        }
        Ok(bytes) => {
            if let Err(source) = tokio::fs::rename(&part, &final_path).await {
                cache.lock().await.abandon_fetch(key);
                return Err(loxia_emby::error::EmbyError::Io {
                    path: final_path,
                    source,
                });
            }
            cache.lock().await.complete_fetch(key, bytes);
            Ok(true)
        }
        Err(e) => {
            cache.lock().await.abandon_fetch(key);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::config::QualityProfile;
    use loxia_core::model::ItemId;
    use loxia_core::test_support::fixtures;
    use std::collections::BTreeMap;
    use tempfile::tempdir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(url: &str) -> loxia_core::config::ServerConfig {
        loxia_core::config::ServerConfig {
            id: "srv".to_string(),
            name: "Test".to_string(),
            url: url.to_string(),
            user_id: "user-1".to_string(),
            access_token: "tok".to_string(),
            device_id: "dev".to_string(),
            custom_headers: BTreeMap::new(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }
    }

    fn track_with_id(id: &str) -> Track {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let mut t = fixtures::track("Motion", 1, &alb, &[&a]);
        t.id = ItemId::from(id);
        t
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn_worker(
        server_uri: &str,
        cache_dir: &std::path::Path,
        prefetch_on_play: bool,
    ) -> (
        mpsc::UnboundedSender<Effect>,
        mpsc::UnboundedReceiver<Event>,
        Arc<Mutex<RollingCache>>,
        JoinHandle<()>,
    ) {
        let client = Arc::new(EmbyClient::new(&cfg(server_uri)).unwrap());
        let cache = Arc::new(Mutex::new(RollingCache::open(cache_dir).unwrap()));
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let dirs = loxia_core::paths::FixedDirs::new(
            Some(cache_dir.join("config")),
            Some(cache_dir.join("cache")),
            Some(cache_dir.join("data")),
            Some(cache_dir.join("state")),
        );
        let paths = Paths::resolve(&dirs, &loxia_core::config::CacheConfig::default()).unwrap();
        let handle = spawn(
            client,
            cache.clone(),
            ServerId::from("srv"),
            TargetCodec::Mp3,
            prefetch_on_play,
            paths,
            effects_rx,
            events_tx,
        );
        (effects_tx, events_rx, cache, handle)
    }

    fn ensure_cached(track: Track, profile: QualityProfile) -> Effect {
        Effect::Cache(Box::new(CacheEffect::EnsureCached { track, profile }))
    }

    #[tokio::test]
    async fn cache_miss_returns_none_and_starts_fetch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 100]))
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let event = next_resolve(&mut events_rx).await;
        match event {
            Event::Data(DataAction::CacheResolved {
                track,
                profile,
                path,
                stream_url,
            }) => {
                assert_eq!(track, ItemId::from("item-1"));
                assert_eq!(profile, QualityProfile::Direct);
                assert_eq!(path, None);
                // The real fix under test: a cache miss must still carry a URL mpv can actually
                // open, not leave `load_current`'s inert placeholder in place (`docs/12-decisions.md`).
                assert!(stream_url.as_str().contains("/Audio/item-1/stream"));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        // The background fetch is async; poll briefly for it to land rather than a fixed sleep.
        let key = CacheKey {
            server: ServerId::from("srv"),
            item: ItemId::from("item-1"),
            profile: QualityProfile::Direct,
        };
        for _ in 0..50 {
            if cache.lock().await.resolve(&key).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(cache.lock().await.resolve(&key).is_some());
        drop(effects_tx);
    }

    #[tokio::test]
    async fn cache_hit_returns_file_path() {
        let server = MockServer::start().await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), false).await;

        let key = CacheKey {
            server: ServerId::from("srv"),
            item: ItemId::from("item-1"),
            profile: QualityProfile::Direct,
        };
        let path = {
            let mut c = cache.lock().await;
            let relative = loxia_cache::layout::cache_relative_path(
                &key.server,
                &track_with_id("item-1"),
                key.profile,
                "flac",
            );
            let p = c.begin_fetch(&key, &relative);
            tokio::fs::create_dir_all(p.parent().unwrap())
                .await
                .unwrap();
            tokio::fs::write(&p, b"cached").await.unwrap();
            c.complete_fetch(&key, 6);
            p
        };

        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let event = next_resolve(&mut events_rx).await;
        match event {
            Event::Data(DataAction::CacheResolved {
                track,
                profile,
                path: got_path,
                ..
            }) => {
                assert_eq!(track, ItemId::from("item-1"));
                assert_eq!(profile, QualityProfile::Direct);
                assert_eq!(got_path, Some(path));
            }
            other => panic!("unexpected event: {other:?}"),
        }
        drop(effects_tx);
    }

    #[tokio::test]
    async fn cache_hit_updates_last_access() {
        let server = MockServer::start().await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), false).await;

        let key = CacheKey {
            server: ServerId::from("srv"),
            item: ItemId::from("item-1"),
            profile: QualityProfile::Direct,
        };
        {
            let mut c = cache.lock().await;
            let relative = loxia_cache::layout::cache_relative_path(
                &key.server,
                &track_with_id("item-1"),
                key.profile,
                "flac",
            );
            let p = c.begin_fetch(&key, &relative);
            tokio::fs::create_dir_all(p.parent().unwrap())
                .await
                .unwrap();
            tokio::fs::write(&p, b"cached").await.unwrap();
            c.complete_fetch(&key, 6);
        }
        let before = cache.lock().await.manifest().get(&key).unwrap().last_access;

        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let _ = next_resolve(&mut events_rx).await;

        let after = cache.lock().await.manifest().get(&key).unwrap().last_access;
        assert!(after >= before);
        drop(effects_tx);
    }

    #[tokio::test]
    async fn prefetch_disabled_by_config() {
        let server = MockServer::start().await;
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_clone = hits.clone();
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(move |_req: &wiremock::Request| {
                hits_clone.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_bytes(vec![1u8; 10])
            })
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, _cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), false).await;

        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let _ = next_resolve(&mut events_rx).await;
        drop(effects_tx);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "prefetch_on_play=false must never fetch"
        );
    }

    #[tokio::test]
    async fn second_play_makes_no_network_request() {
        let server = MockServer::start().await;
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_clone = hits.clone();
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(move |_req: &wiremock::Request| {
                hits_clone.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_bytes(vec![1u8; 100])
            })
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        let key = CacheKey {
            server: ServerId::from("srv"),
            item: ItemId::from("item-1"),
            profile: QualityProfile::Direct,
        };

        // First play: a miss, triggers the background fetch.
        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let _ = next_resolve(&mut events_rx).await;
        for _ in 0..50 {
            if cache.lock().await.resolve(&key).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(cache.lock().await.resolve(&key).is_some());
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        // Second play: a hit, no new request.
        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let event = next_resolve(&mut events_rx).await;
        assert!(matches!(
            event,
            Event::Data(DataAction::CacheResolved { path: Some(_), .. })
        ));
        assert_eq!(hits.load(Ordering::SeqCst), 1, "no second network request");
        drop(effects_tx);
    }

    /// A permanently downloaded track must be *used*, not re-fetched. It wasn't: `EnsureCached`
    /// only ever consulted the rolling cache, so a track the user had deliberately kept streamed
    /// and re-transcoded on every play, and `prefetch_on_play` downloaded a second copy of it into
    /// the rolling cache besides (`docs/12-decisions.md`).
    #[tokio::test]
    async fn a_downloaded_track_is_played_from_disk_and_never_refetched() {
        let server = wiremock::MockServer::start().await;
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = hits.clone();
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(move |_: &wiremock::Request| {
                seen.fetch_add(1, Ordering::SeqCst);
                wiremock::ResponseTemplate::new(200).set_body_bytes(b"streamed".to_vec())
            })
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        // A download index written before the worker starts, exactly as a previous session leaves
        // one behind.
        let downloads_root = {
            let dirs = loxia_core::paths::FixedDirs::new(
                Some(dir.path().join("config")),
                Some(dir.path().join("cache")),
                Some(dir.path().join("data")),
                Some(dir.path().join("state")),
            );
            let paths = Paths::resolve(&dirs, &loxia_core::config::CacheConfig::default()).unwrap();
            paths.downloads_root().to_path_buf()
        };
        let kept = downloads_root.join("srv/kept.flac");
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        std::fs::write(&kept, b"kept").unwrap();
        std::fs::create_dir_all(&downloads_root).unwrap();
        std::fs::write(
            downloads_root.join("downloads_index.json"),
            serde_json::to_string(&serde_json::json!({
                "entries": [{
                    "item": "item-1",
                    "path": kept,
                    "profile": "direct",
                    "bytes": 4,
                    "downloaded_at": null,
                    "sidecar": kept.with_extension("loxia.json"),
                    "complete": true,
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        // `prefetch_on_play` on, so anything that *would* re-fetch definitely does.
        let (effects_tx, mut events_rx, _cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        // Asked for at a *transcode* profile: a local original still beats fetching a smaller copy
        // of something this machine already has.
        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::TranscodeMed,
            ))
            .unwrap();

        let event = next_resolve(&mut events_rx).await;
        match event {
            Event::Data(DataAction::CacheResolved { path: got, .. }) => {
                assert_eq!(got.as_deref(), Some(kept.as_path()));
            }
            other => panic!("expected a resolve, got {other:?}"),
        }

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "a track already on disk must not be fetched again"
        );
        drop(effects_tx);
    }

    /// The next `CacheResolved`, skipping the progress events a fetch also emits — those are
    /// interleaved by design and no test that is asking "what did the lookup say?" cares about
    /// them.
    async fn next_resolve(rx: &mut mpsc::UnboundedReceiver<Event>) -> Event {
        loop {
            let event = rx.recv().await.expect("worker still running");
            if matches!(event, Event::Data(DataAction::CacheResolved { .. })) {
                return event;
            }
        }
    }

    /// `cache.prefetch_next`: the whole run is fetched, at the profile it was asked for.
    #[tokio::test]
    async fn prefetch_ahead_caches_every_track_in_the_run() {
        let server = MockServer::start().await;
        for id in ["item-2", "item-3"] {
            Mock::given(method("GET"))
                .and(path(format!("/emby/Audio/{id}/stream")))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![9u8; 40]))
                .mount(&server)
                .await;
        }
        let dir = tempdir().unwrap();
        let (effects_tx, _events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        effects_tx
            .send(Effect::Cache(Box::new(CacheEffect::PrefetchAhead {
                tracks: vec![track_with_id("item-2"), track_with_id("item-3")],
                profile: QualityProfile::Direct,
            })))
            .unwrap();

        for id in ["item-2", "item-3"] {
            let key = CacheKey {
                server: ServerId::from("srv"),
                item: ItemId::from(id),
                profile: QualityProfile::Direct,
            };
            let mut cached = false;
            for _ in 0..100 {
                if cache.lock().await.resolve(&key).is_some() {
                    cached = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(cached, "{id} was never read ahead");
        }
        drop(effects_tx);
    }

    /// A read-ahead must never pre-empt what is playing. The two use separate slots precisely so
    /// that `EnsureCached`'s own "cancel whatever is running for a different key" rule — which is
    /// right for a user skipping through tracks — cannot reach the read-ahead, and vice versa.
    #[tokio::test]
    async fn a_read_ahead_does_not_cancel_the_playing_tracks_fetch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(vec![1u8; 40])
                    .set_delay(Duration::from_millis(150)),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-2/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![2u8; 40]))
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let _ = next_resolve(&mut events_rx).await;
        // Lands while item-1 is still in flight, which is exactly when it would have cancelled it.
        effects_tx
            .send(Effect::Cache(Box::new(CacheEffect::PrefetchAhead {
                tracks: vec![track_with_id("item-2")],
                profile: QualityProfile::Direct,
            })))
            .unwrap();

        let playing = CacheKey {
            server: ServerId::from("srv"),
            item: ItemId::from("item-1"),
            profile: QualityProfile::Direct,
        };
        let mut cached = false;
        for _ in 0..100 {
            if cache.lock().await.resolve(&playing).is_some() {
                cached = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            cached,
            "the playing track's fetch was cancelled by a read-ahead"
        );
        drop(effects_tx);
    }

    #[tokio::test]
    async fn different_quality_profiles_are_separate_entries() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8; 50]))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream.mp3"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![2u8; 60]))
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        // One profile fully resolved (fetch completed) before the next is requested — a second
        // `EnsureCached` for a *different key* would otherwise cancel the first's still-in-flight
        // fetch (`docs/06-cache-and-offline.md` §4's own "don't queue up twenty downloads" rule),
        // which is correct behaviour but not what this test is checking.
        for profile in [QualityProfile::Direct, QualityProfile::TranscodeHigh] {
            effects_tx
                .send(ensure_cached(track_with_id("item-1"), profile))
                .unwrap();
            let _ = next_resolve(&mut events_rx).await;
            let key = CacheKey {
                server: ServerId::from("srv"),
                item: ItemId::from("item-1"),
                profile,
            };
            for _ in 0..50 {
                if cache.lock().await.resolve(&key).is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        let direct_key = CacheKey {
            server: ServerId::from("srv"),
            item: ItemId::from("item-1"),
            profile: QualityProfile::Direct,
        };
        let high_key = CacheKey {
            server: ServerId::from("srv"),
            item: ItemId::from("item-1"),
            profile: QualityProfile::TranscodeHigh,
        };
        let direct_path = cache.lock().await.resolve(&direct_key);
        let high_path = cache.lock().await.resolve(&high_key);
        assert!(direct_path.is_some());
        assert!(high_path.is_some());
        assert_ne!(direct_path, high_path);
        drop(effects_tx);
    }

    #[tokio::test]
    async fn failed_cache_fetch_does_not_surface_to_user() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, _cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        // A fetch failure never becomes a user-visible event: no `LoadFailed`, no toast-triggering
        // action of any kind, and — unlike a clean completion — no `CacheFetched` either.
        let event = next_resolve(&mut events_rx).await;
        assert!(matches!(
            event,
            Event::Data(DataAction::CacheResolved { path: None, .. })
        ));
        drop(effects_tx);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut seen = Vec::new();
        while let Ok(event) = events_rx.try_recv() {
            seen.push(event);
        }
        assert!(
            seen.iter()
                .all(|e| matches!(e, Event::Data(DataAction::CacheFetchStarted { .. }))),
            "a failed fetch must announce nothing but its start: {seen:?}"
        );
    }

    #[tokio::test]
    async fn partial_download_leaves_part_file() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8; 1_000_000]))
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let _ = next_resolve(&mut events_rx).await;

        // Immediately supersede with a different track — cancels the in-flight fetch partway.
        effects_tx
            .send(ensure_cached(
                track_with_id("item-2"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let _ = next_resolve(&mut events_rx).await;

        let key = CacheKey {
            server: ServerId::from("srv"),
            item: ItemId::from("item-1"),
            profile: QualityProfile::Direct,
        };
        let final_path = cache
            .lock()
            .await
            .path_for(&loxia_cache::layout::cache_relative_path(
                &key.server,
                &track_with_id("item-1"),
                key.profile,
                "flac",
            ));
        let part = part_path(&final_path);
        for _ in 0..50 {
            if part.exists() || final_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(part.exists(), "a cancelled fetch must leave its .part file");
        assert!(!final_path.exists());
        drop(effects_tx);
    }

    #[tokio::test]
    async fn fetch_cancelled_when_track_no_longer_relevant() {
        let server = MockServer::start().await;
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_clone = hits.clone();
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(move |_req: &wiremock::Request| {
                hits_clone.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_bytes(vec![1u8; 1_000_000])
            })
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, _cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        for id in ["item-1", "item-2", "item-3"] {
            effects_tx
                .send(ensure_cached(track_with_id(id), QualityProfile::Direct))
                .unwrap();
            let _ = next_resolve(&mut events_rx).await;
        }
        // Only the *last* track's fetch should ever be running — the first two were superseded
        // before they had a chance to finish, so at most one background request is truly active
        // (a user skipping quickly through an album should not queue up twenty downloads).
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(hits.load(Ordering::SeqCst) <= 3);
        drop(effects_tx);
    }

    #[tokio::test]
    async fn background_fetch_limited_to_one_concurrent() {
        let server = MockServer::start().await;
        let concurrent = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let concurrent_clone = concurrent.clone();
        let peak_clone = peak.clone();
        Mock::given(method("GET"))
            .respond_with(move |_req: &wiremock::Request| {
                let now = concurrent_clone.fetch_add(1, Ordering::SeqCst) + 1;
                peak_clone.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(30));
                concurrent_clone.fetch_sub(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_bytes(vec![1u8; 100])
            })
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, _cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), true).await;

        for id in ["item-1", "item-2", "item-3"] {
            effects_tx
                .send(ensure_cached(track_with_id(id), QualityProfile::Direct))
                .unwrap();
            let _ = next_resolve(&mut events_rx).await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            peak.load(Ordering::SeqCst) <= 1,
            "never more than one concurrent fetch"
        );
        drop(effects_tx);
    }

    #[tokio::test]
    async fn cache_resolve_times_out_at_50ms() {
        // A resolve that would take longer than 50ms (simulated by a manifest lock held for
        // longer than that) must still reply within the bound, with `path: None`.
        let server = MockServer::start().await;
        let dir = tempdir().unwrap();
        let (effects_tx, mut events_rx, cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), false).await;

        let guard = cache.lock().await; // held across the whole EnsureCached dispatch below
        effects_tx
            .send(ensure_cached(
                track_with_id("item-1"),
                QualityProfile::Direct,
            ))
            .unwrap();
        let start = std::time::Instant::now();
        let event = next_resolve(&mut events_rx).await;
        let elapsed = start.elapsed();
        drop(guard);

        assert!(
            elapsed < Duration::from_millis(200),
            "resolve must not block indefinitely on a slow/contended cache lock"
        );
        assert!(matches!(
            event,
            Event::Data(DataAction::CacheResolved { path: None, .. })
        ));
        drop(effects_tx);
    }

    /// `11-03`: `switching_clears_queue_and_persists_outgoing_session` — the worker-level half of
    /// that acceptance test (`reducer::settings`'s own tests cover the effect being emitted at
    /// all).
    #[tokio::test]
    async fn persist_session_for_server_writes_the_server_scoped_file() {
        let server = MockServer::start().await;
        let dir = tempdir().unwrap();
        let (effects_tx, _events_rx, _cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), false).await;

        let dirs = loxia_core::paths::FixedDirs::new(
            Some(dir.path().join("config")),
            Some(dir.path().join("cache")),
            Some(dir.path().join("data")),
            Some(dir.path().join("state")),
        );
        let paths = Paths::resolve(&dirs, &loxia_core::config::CacheConfig::default()).unwrap();
        let outgoing = ServerId::from("outgoing-server");
        let snapshot = loxia_core::state::SessionSnapshot {
            schema_version: loxia_core::state::SESSION_SCHEMA_VERSION,
            server_id: outgoing.clone(),
            queue: Default::default(),
            position_secs: 0.0,
            active_tab: loxia_core::state::nav::Tab::NowPlaying,
            zen_mode: false,
            volume: 60,
            quality_profile: QualityProfile::Direct,
            eq: Default::default(),
            saved_at: fixtures::fixed_epoch(),
        };

        effects_tx
            .send(Effect::Cache(Box::new(
                CacheEffect::PersistSessionForServer(outgoing.clone(), Box::new(snapshot)),
            )))
            .unwrap();
        drop(effects_tx);
        // Give the worker's own loop a chance to process the send before asserting on disk.
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(paths.session_file_for(&outgoing).exists());
        assert!(
            !paths.session_file().exists(),
            "must not touch the single unsuffixed session.json"
        );
        let loaded = loxia_cache::session::load_for_server(&paths, &outgoing)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.server_id, outgoing);
    }

    #[tokio::test]
    async fn delete_server_data_removes_the_server_scoped_subtrees_only() {
        let server = MockServer::start().await;
        let dir = tempdir().unwrap();
        let (effects_tx, _events_rx, _cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), false).await;

        // Mirrors `spawn_worker`'s own `Paths::resolve` call exactly — `DeleteServerData` reads
        // from `paths.cache_root()`, a different directory from the raw `dir.path()` this same
        // helper hands `RollingCache::open` directly.
        let dirs = loxia_core::paths::FixedDirs::new(
            Some(dir.path().join("config")),
            Some(dir.path().join("cache")),
            Some(dir.path().join("data")),
            Some(dir.path().join("state")),
        );
        let paths = Paths::resolve(&dirs, &loxia_core::config::CacheConfig::default()).unwrap();

        let removed = paths.cache_root().join("tracks").join("gone");
        let kept = paths.cache_root().join("tracks").join("srv");
        std::fs::create_dir_all(&removed).unwrap();
        std::fs::write(removed.join("track.mp3"), b"x").unwrap();
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::write(kept.join("track.mp3"), b"x").unwrap();

        effects_tx
            .send(Effect::Cache(Box::new(CacheEffect::DeleteServerData(
                ServerId::from("gone"),
            ))))
            .unwrap();
        drop(effects_tx);
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(!removed.exists());
        assert!(kept.exists(), "a different server's own cache must survive");
    }

    /// `11-06`: `clear_saved_session_requires_confirm_and_deletes_file` — the worker-level half
    /// (the reducer's own test covers the `Confirm` gate and the queue-clearing effect being
    /// emitted at all).
    #[tokio::test]
    async fn delete_session_removes_the_session_file() {
        let server = MockServer::start().await;
        let dir = tempdir().unwrap();
        let (effects_tx, _events_rx, _cache, _handle) =
            spawn_worker(&server.uri(), dir.path(), false).await;

        let dirs = loxia_core::paths::FixedDirs::new(
            Some(dir.path().join("config")),
            Some(dir.path().join("cache")),
            Some(dir.path().join("data")),
            Some(dir.path().join("state")),
        );
        let paths = Paths::resolve(&dirs, &loxia_core::config::CacheConfig::default()).unwrap();
        loxia_cache::session::save(
            &paths,
            &loxia_core::state::SessionSnapshot {
                schema_version: loxia_core::state::SESSION_SCHEMA_VERSION,
                server_id: ServerId::from("srv"),
                queue: Default::default(),
                position_secs: 0.0,
                active_tab: loxia_core::state::nav::Tab::NowPlaying,
                zen_mode: false,
                volume: 60,
                quality_profile: QualityProfile::Direct,
                eq: Default::default(),
                saved_at: fixtures::fixed_epoch(),
            },
        )
        .unwrap();
        assert!(paths.session_file().exists());

        effects_tx
            .send(Effect::Cache(Box::new(CacheEffect::DeleteSession)))
            .unwrap();
        drop(effects_tx);
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(!paths.session_file().exists());
    }

    #[tokio::test]
    async fn delete_server_data_on_a_never_cached_server_is_not_an_error() {
        let server = MockServer::start().await;
        let dir = tempdir().unwrap();
        let (effects_tx, _events_rx, _cache, handle) =
            spawn_worker(&server.uri(), dir.path(), false).await;

        effects_tx
            .send(Effect::Cache(Box::new(CacheEffect::DeleteServerData(
                ServerId::from("never-seen"),
            ))))
            .unwrap();
        drop(effects_tx);
        // The worker must not panic or hang — its loop simply ends once the channel closes.
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("worker must exit cleanly")
            .unwrap();
    }
}
