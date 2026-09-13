//! Background workers owning I/O resources.
//!
//! `03-08` wired only the scaffolding: five stub tasks, one per worker in the runtime diagram
//! (`docs/01-architecture.md` §4), each holding its own `UnboundedSender<Effect>`/
//! `UnboundedReceiver<Effect>` pair and logging whatever it receives at `debug`. Every worker's
//! channel carries the *whole* `Effect` enum, not a narrowed-down sub-type — confirmed by three
//! later tasks' own fixed signatures (`workers::notify::spawn(effects: UnboundedReceiver<Effect>)
//! -> JoinHandle<()>`, `10-10`; `workers::audio::spawn(backend, effects, events)`, `05-06`;
//! `workers::mpris::spawn(effects, events)`, `10-11`), so each worker's own receive loop decides
//! which variants it cares about and ignores the rest. `network.rs` (`04-10`), `audio.rs`
//! (`05-06`), `cache.rs` (`08-03`), `notify.rs` (`10-10`), and `mpris.rs` (`10-11`) have all since
//! replaced their stub with real behaviour — every worker named in the runtime diagram now is.

pub mod audio;
pub mod cache;
pub mod download_fetcher;
pub mod mpris;
pub mod network;
pub mod notify;

use std::sync::Arc;
use std::time::Duration;

use loxia_audio::backend::AudioBackend;
use loxia_cache::lru::RollingCache;
use loxia_core::config::TargetCodec;
use loxia_core::effect::Effect;
use loxia_core::event::Event;
use loxia_core::model::{ItemId, ServerId};
use loxia_emby::client::EmbyClient;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

/// `08-03`: just enough of `Config` for the cache worker's own needs, bundled so `Workers::spawn`
/// doesn't need four more positional parameters of its own. `paths` (`08-08`) is what lets this
/// same worker also handle `PersistSession`/`AppendHistory` — both are local-only (no network),
/// so piggybacking them on the *cache* worker specifically means they silently don't happen
/// whenever `cache` is `None` (no server connection, or the cache root itself failed to open,
/// `docs/12-decisions.md`) even though neither reason should actually prevent saving a session.
pub struct CacheWorkerConfig {
    pub server: ServerId,
    pub target_codec: TargetCodec,
    pub prefetch_on_play: bool,
    pub paths: loxia_core::paths::Paths,
}

/// One `UnboundedSender<Effect>` per worker, the shared reply receiver every worker's sender
/// clone feeds into, and the join handles `drain` awaits on shutdown.
pub struct Workers {
    /// Decoded cover images, written by the network worker and read by the render layer's own
    /// art cache. Not part of `AppState`: a decoded image is render data, and actions/state stay
    /// plain serialisable values (`docs/12-decisions.md`).
    pub art_store: loxia_tui::widgets::album_art::ArtStore,
    pub network: mpsc::UnboundedSender<Effect>,
    pub audio: mpsc::UnboundedSender<Effect>,
    pub cache: mpsc::UnboundedSender<Effect>,
    pub mpris: mpsc::UnboundedSender<Effect>,
    pub notify: mpsc::UnboundedSender<Effect>,
    pub events: mpsc::UnboundedReceiver<Event>,
    /// `11-03`: kept (not just moved into each worker at `spawn` time) so `reconnect` can hand a
    /// fresh network/cache worker pair the exact same shared reply channel every other worker
    /// already feeds into — a live server switch must not disturb `events`, which `runtime::run`'s
    /// own `select!` is already reading from.
    events_tx: mpsc::UnboundedSender<Event>,
    handles: Vec<JoinHandle<()>>,
}

impl Workers {
    /// As `spawn_stubs`, except the network worker is the real one from `network.rs`, owning
    /// `client` and `library` (the first music library — `bootstrap::connect` resolves both
    /// before the runtime loop starts), the audio worker is the real one from `audio.rs`, owning
    /// `audio_backend` (selected in `main` independently of whether the Emby server connected —
    /// audio and library connectivity are orthogonal), and the cache worker is the real one from
    /// `cache.rs` (`08-03`) whenever `cache` is `Some` — `None` if the rolling cache's own root
    /// couldn't be opened (`docs/06-cache-and-offline.md` §9: caching is a nicety, never a hard
    /// requirement for playback, so this degrades to the same stub `spawn_stubs` always used
    /// rather than losing the *network* connection too just because the cache didn't open).
    /// `mpris`/`notify` stay stubs until their own tasks (`docs/12-decisions.md`).
    ///
    /// `offline` (`08-05`) is threaded straight into the network worker, which serves
    /// `FetchColumn { Artists }`/`FetchDiscography` from its index whenever its flag is set —
    /// nothing in this crate flips that flag or populates the index yet (`08-06`'s connectivity
    /// state machine will); `spawn`'s own callers construct a fresh, inert
    /// `network::OfflineHandle` (`Arc::new(AtomicBool::new(false))`, `Arc::new(StdMutex::new(None))`)
    /// until then.
    pub fn spawn(
        client: Arc<EmbyClient>,
        library: Option<ItemId>,
        audio_backend: Box<dyn AudioBackend>,
        cache: Option<(Arc<Mutex<RollingCache>>, CacheWorkerConfig)>,
        offline: network::OfflineHandle,
        enable_websocket: bool,
    ) -> Workers {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let art_store: loxia_tui::widgets::album_art::ArtStore = Default::default();

        let (network_tx, network_rx) = mpsc::unbounded_channel();
        let (audio_tx, audio_rx) = mpsc::unbounded_channel();
        let (cache_tx, cache_rx) = mpsc::unbounded_channel();
        let (mpris_tx, mpris_rx) = mpsc::unbounded_channel();
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();

        let cache_handle = match cache {
            Some((rolling_cache, cache_config)) => cache::spawn(
                client.clone(),
                rolling_cache,
                cache_config.server,
                cache_config.target_codec,
                cache_config.prefetch_on_play,
                cache_config.paths,
                cache_rx,
                events_tx.clone(),
            ),
            None => spawn_stub("cache", cache_rx, events_tx.clone()),
        };

        let handles = vec![
            network::spawn(
                client,
                library,
                network_rx,
                events_tx.clone(),
                offline,
                enable_websocket,
                art_store.clone(),
            ),
            audio::spawn(audio_backend, audio_rx, events_tx.clone()),
            cache_handle,
            mpris::spawn(mpris_rx, events_tx.clone()),
            notify::spawn(notify_rx),
        ];

        Workers {
            art_store: art_store.clone(),
            network: network_tx,
            audio: audio_tx,
            cache: cache_tx,
            mpris: mpris_tx,
            notify: notify_tx,
            events: events_rx,
            events_tx,
            handles,
        }
    }

    /// As `spawn`, but without a real Emby connection — used when `bootstrap::connect` couldn't
    /// reach a server. The audio worker is still real: audio and library connectivity are
    /// independent of each other.
    pub fn spawn_stubs(audio_backend: Box<dyn AudioBackend>) -> Workers {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let art_store: loxia_tui::widgets::album_art::ArtStore = Default::default();

        let (network_tx, network_rx) = mpsc::unbounded_channel();
        let (audio_tx, audio_rx) = mpsc::unbounded_channel();
        let (cache_tx, cache_rx) = mpsc::unbounded_channel();
        let (mpris_tx, mpris_rx) = mpsc::unbounded_channel();
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();

        let handles = vec![
            spawn_network_stub(network_rx, events_tx.clone()),
            audio::spawn(audio_backend, audio_rx, events_tx.clone()),
            spawn_stub("cache", cache_rx, events_tx.clone()),
            mpris::spawn(mpris_rx, events_tx.clone()),
            notify::spawn(notify_rx),
        ];

        Workers {
            art_store: art_store.clone(),
            network: network_tx,
            audio: audio_tx,
            cache: cache_tx,
            mpris: mpris_tx,
            notify: notify_tx,
            events: events_rx,
            events_tx,
            handles,
        }
    }

    /// `11-03`: "switching servers... reconnect" — replaces the network and cache workers in
    /// place with a fresh pair bound to `client`/`library`/`cache`, leaving `audio`/`mpris`/
    /// `notify` (and `events`) completely untouched, since a server switch has nothing to do with
    /// audio-device or MPRIS/notification connectivity. The old network/cache senders are dropped
    /// here, which signals those two worker loops to end once whatever was already queued drains
    /// — their `JoinHandle`s are appended to `self.handles` (not awaited here) so `drain` still
    /// sees them at final shutdown; nothing in this method blocks the caller.
    pub fn reconnect(
        &mut self,
        client: Arc<EmbyClient>,
        library: Option<ItemId>,
        cache: Option<(Arc<Mutex<RollingCache>>, CacheWorkerConfig)>,
        offline: network::OfflineHandle,
        enable_websocket: bool,
    ) {
        let (network_tx, network_rx) = mpsc::unbounded_channel();
        let (cache_tx, cache_rx) = mpsc::unbounded_channel();

        let cache_handle = match cache {
            Some((rolling_cache, cache_config)) => cache::spawn(
                client.clone(),
                rolling_cache,
                cache_config.server,
                cache_config.target_codec,
                cache_config.prefetch_on_play,
                cache_config.paths,
                cache_rx,
                self.events_tx.clone(),
            ),
            None => spawn_stub("cache", cache_rx, self.events_tx.clone()),
        };
        let network_handle = network::spawn(
            client,
            library,
            network_rx,
            self.events_tx.clone(),
            offline,
            enable_websocket,
            // The same store across a reconnect — already-decoded covers stay valid, and the
            // render layer holds a clone of this exact handle.
            self.art_store.clone(),
        );

        self.network = network_tx;
        self.cache = cache_tx;
        self.handles.push(network_handle);
        self.handles.push(cache_handle);
    }

    /// Drops every effect sender — signalling each worker loop to end once it has drained
    /// whatever was already queued — then waits up to 2 seconds for all of them to finish. A
    /// worker that never notices its channel closing (hung) cannot block shutdown past that
    /// timeout; `run`'s shutdown path proceeds regardless.
    pub async fn drain(self) {
        let Workers {
            network,
            audio,
            cache,
            mpris,
            notify,
            handles,
            ..
        } = self;
        drop(network);
        drop(audio);
        drop(cache);
        drop(mpris);
        drop(notify);

        if tokio::time::timeout(Duration::from_secs(2), futures::future::join_all(handles))
            .await
            .is_err()
        {
            tracing::warn!("worker drain timed out after 2s; exiting anyway");
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test() -> (Workers, [mpsc::UnboundedReceiver<Effect>; 5]) {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let art_store: loxia_tui::widgets::album_art::ArtStore = Default::default();
        let (network_tx, network_rx) = mpsc::unbounded_channel();
        let (audio_tx, audio_rx) = mpsc::unbounded_channel();
        let (cache_tx, cache_rx) = mpsc::unbounded_channel();
        let (mpris_tx, mpris_rx) = mpsc::unbounded_channel();
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();

        let workers = Workers {
            art_store: art_store.clone(),
            network: network_tx,
            audio: audio_tx,
            cache: cache_tx,
            mpris: mpris_tx,
            notify: notify_tx,
            events: events_rx,
            events_tx,
            handles: Vec::new(),
        };
        (
            workers,
            [network_rx, audio_rx, cache_rx, mpris_rx, notify_rx],
        )
    }

    #[cfg(test)]
    pub(crate) fn with_handles(handles: Vec<JoinHandle<()>>) -> Workers {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let art_store: loxia_tui::widgets::album_art::ArtStore = Default::default();
        let (network_tx, _network_rx) = mpsc::unbounded_channel();
        let (audio_tx, _audio_rx) = mpsc::unbounded_channel();
        let (cache_tx, _cache_rx) = mpsc::unbounded_channel();
        let (mpris_tx, _mpris_rx) = mpsc::unbounded_channel();
        let (notify_tx, _notify_rx) = mpsc::unbounded_channel();
        Workers {
            art_store: art_store.clone(),
            network: network_tx,
            audio: audio_tx,
            cache: cache_tx,
            mpris: mpris_tx,
            notify: notify_tx,
            events: events_rx,
            events_tx,
            handles,
        }
    }
}

fn spawn_stub(
    name: &'static str,
    mut effects: mpsc::UnboundedReceiver<Effect>,
    _events: mpsc::UnboundedSender<Event>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(effect) = effects.recv().await {
            tracing::debug!(worker = name, ?effect, "stub worker received effect");
        }
        tracing::debug!(worker = name, "stub worker channel closed; exiting");
    })
}

/// A real bug found in the field: `spawn_stubs` (no server configured yet, or the last connect
/// attempt failed) used the plain `spawn_stub` above for its own "network" channel, which drops
/// every effect after only logging it. `Effect::Net(NetEffect::TestServerConnection)` — the
/// server-profile editor's own "Test connection"/"Save", which deliberately needs neither an
/// active `client` nor `library`, precisely so it can work before the very first server is ever
/// added — vanished into that log line exactly as silently as everything else, and nothing ever
/// replied: `draft.testing` stayed `true` forever, `Test connection`/`Save` looked permanently
/// "hung." This is a dedicated stub for the network channel specifically, which still handles
/// that one effect for real (the same `test_server_connection` helper `network::spawn`'s own real
/// worker uses) and falls back to the ordinary log-and-drop stub behaviour for everything else —
/// exactly what the real worker's own `TestServerConnection` arm already does, since it too
/// ignores its own connected `client` entirely (`docs/12-decisions.md`).
fn spawn_network_stub(
    mut effects: mpsc::UnboundedReceiver<Effect>,
    events: mpsc::UnboundedSender<Event>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(effect) = effects.recv().await {
            match effect {
                Effect::Net(loxia_core::effect::NetEffect::TestServerConnection {
                    url,
                    headers,
                    device_id,
                    username,
                    password,
                }) => {
                    let events = events.clone();
                    tokio::spawn(async move {
                        let event = network::test_server_connection(
                            &url,
                            &headers,
                            &device_id,
                            &username,
                            password.expose(),
                        )
                        .await;
                        let _ = events.send(event);
                    });
                }
                other => {
                    tracing::debug!(worker = "network", effect = ?other, "stub worker received effect");
                }
            }
        }
        tracing::debug!(worker = "network", "stub worker channel closed; exiting");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::action::DataAction;
    use loxia_core::effect::{NetEffect, RedactedSecret};
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// A real bug found in the field: adding the very first server profile ever (no server
    /// configured yet, so `bootstrap::connect` returns `None` and `spawn_stubs` is what's
    /// actually running) left "Test connection"/"Save" stuck showing "testing…" forever — the
    /// plain `spawn_stub` dropped `TestServerConnection` after only logging it, so no reply ever
    /// arrived to clear `draft.testing`. Proves the fix end to end, through the exact
    /// `Workers::spawn_stubs` path a first-run add-server flow actually uses, not just the real
    /// worker's own already-covered `TestServerConnection` handling.
    #[tokio::test]
    async fn spawn_stubs_network_channel_still_answers_test_server_connection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/AuthenticateByName"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "User": { "Id": "user-1" },
                "AccessToken": "fresh-token",
                "ServerId": "server-1",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ServerName": "Home Library",
                "Version": "4.8.0.80",
            })))
            .mount(&server)
            .await;

        let (mock_engine, _control) = loxia_audio::mock::MockEngine::new();
        let mut workers = Workers::spawn_stubs(Box::new(mock_engine));

        workers
            .network
            .send(Effect::Net(NetEffect::TestServerConnection {
                url: server.uri(),
                headers: BTreeMap::new(),
                device_id: "dev".to_string(),
                username: "alice".to_string(),
                password: RedactedSecret::new("s3cr3t-pw"),
            }))
            .unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(2), workers.events.recv())
            .await
            .expect("a reply must actually arrive, not hang forever")
            .unwrap();
        assert_eq!(
            event,
            Event::Data(DataAction::ServerTestSucceeded {
                user_id: "user-1".to_string(),
                access_token: "fresh-token".to_string(),
                server_name: "Home Library".to_string(),
                version: "4.8.0.80".to_string(),
            })
        );
    }

    /// Anything the stub network channel doesn't specifically own still gets the plain
    /// log-and-drop behaviour — never a reply, but never a panic either.
    #[tokio::test]
    async fn spawn_stubs_network_channel_still_drops_other_effects_silently() {
        let (mock_engine, _control) = loxia_audio::mock::MockEngine::new();
        let mut workers = Workers::spawn_stubs(Box::new(mock_engine));

        workers
            .network
            .send(Effect::Net(NetEffect::Reconnect))
            .unwrap();

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), workers.events.recv())
                .await;
        assert!(result.is_err(), "no reply expected for an unhandled effect");
    }
}
