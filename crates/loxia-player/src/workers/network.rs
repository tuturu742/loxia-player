//! Owns EmbyClient; serves network effects (`04-10`, `docs/01-architecture.md` §§4-5).
//!
//! Only the effect/endpoint mapping `04-10` scoped is real: `FetchColumn { Artists }` (browse
//! artists), `FetchDiscography` (an artist's ALBUMS/APPEARS ON split), and `FetchAlbumTracks` (an
//! album's tracklist, for column population). `06-02` adds the two queue-directed fetches,
//! `FetchAlbumTracksForQueue`/`FetchArtistTracksForQueue` — same underlying endpoints, but always
//! unfiltered and replying with `DataAction::TracksLoaded` instead of `ItemsLoaded`, since they
//! feed the queue rather than a Miller column. `06-07` adds `ReportPlayback` — fire-and-forget,
//! no reply. `06-08` adds `InstantMix`, replying with the same `DataAction::TracksLoaded` the
//! queue-directed fetches use (`source: QueueSource::InstantMix`). `07-01` adds `Search`, always
//! replying `DataAction::SearchResultsLoaded` (never `LoadFailed` — a per-section failure is
//! folded into the reply's own per-section error fields, `02-07`). `07-03` adds `FetchColumn` for
//! `Playlists`/`PlaylistTracks`, plus the three fire-and-forget playlist mutations. `07-04` adds
//! `FetchColumn` for `Genres`/`GenreArtists` — levels 3/4 of that same tab reuse the Artists tab's
//! own `FetchDiscography`/`FetchAlbumTracks` unchanged. `07-05` adds `FetchColumn` for `Folders`
//! and `FetchFolderTracksForQueue`. A live user found the top-level Albums tab
//! (`ColumnKind::Albums { of_artist: None }`) spinning forever — it was never wired to an endpoint
//! at all despite `seed_column_for_tab` emitting a bare `FetchColumn` for it same as `Artists`; see
//! `docs/12-decisions.md`. `ColumnKind::Albums { of_artist: Some(_) }`/`Tracks`/`ArtistTracks` are
//! still reached only via their own dedicated effects (`FetchDiscography`/`FetchAlbumTracks`), never
//! a bare `FetchColumn`; `SearchResults` is likewise still out of scope and only logged.
//!
//! `08-05`: whenever `offline` is set, `FetchColumn { Artists }` and `FetchDiscography` are served
//! straight from an `OfflineIndex` instead of ever touching `client` — the same `Event`s either
//! way, so the reducer and UI need no offline-specific branch (`docs/06-cache-and-offline.md` §6).
//! Every other effect still goes to HTTP regardless of `offline` (unwired until whichever task
//! extends this list — `docs/12-decisions.md`). Both `offline`/`offline_index` are wired through
//! `Workers::spawn` but nothing yet flips `offline` to `true` or populates the index — that is
//! `08-06`'s own connectivity state machine.

use std::collections::HashSet;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use loxia_cache::offline_index::OfflineIndex;
use loxia_core::action::{DataAction, LoadTarget, SystemEvent};
use loxia_core::effect::{Effect, NetEffect};
use loxia_core::event::Event;
use loxia_core::model::{Artist, Genre, ItemId, MediaItem};
use loxia_core::state::Connectivity;
use loxia_core::state::nav::ColumnKind;
use loxia_core::state::queue::QueueSource;
use loxia_core::state::toast::ToastLevel;
use loxia_emby::client::EmbyClient;
use loxia_emby::endpoints::{
    discography, favorites, images, instant_mix, items, lyrics, playback, playlists, probe, search,
};
use loxia_emby::error::EmbyError;
use loxia_emby::query::Page;
use loxia_emby::ws::{self, WsEvent};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, Semaphore, mpsc};
use tokio::task::JoinHandle;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::protocol::Message;

/// An unbounded fan-out on a large library would exhaust the server's connection pool
/// (task spec) — every request, regardless of kind, shares this one limit.
const MAX_CONCURRENT_REQUESTS: usize = 4;

/// One entry per outstanding request, used to drop an exact duplicate fired by fast scrolling
/// before it ever reaches the semaphore. Keyed on what uniquely identifies the request's
/// *intent*, not a generic incrementing id — a second identical intent while the first is still
/// in flight is always redundant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum InFlightKey {
    Column(ColumnKind, usize),
    Discography(ItemId),
    AlbumTracks(ItemId),
    AlbumTracksForQueue(ItemId),
    ArtistTracksForQueue(ItemId),
    FolderTracksForQueue(ItemId, bool),
    GenreTracksForQueue(String),
    PlaylistTracksForQueue(loxia_core::model::PlaylistId),
    InstantMix(ItemId),
    Search(String),
    Favourites,
}

/// `08-05`: the offline-serving half of the network worker, bundled into one `Clone` handle so
/// `spawn`/`handle` don't each need two more positional parameters (the same idiom
/// `CacheWorkerConfig` established, `08-03`). `offline` and `index` are set independently of each
/// other deliberately: a connectivity flip can arrive before (or without) an index ever finishing
/// its first build.
#[derive(Clone)]
pub struct OfflineHandle {
    pub offline: Arc<AtomicBool>,
    pub index: Arc<StdMutex<Option<OfflineIndex>>>,
}

/// Spawns the real network worker: owns `client` and `library` (the first music library returned
/// by `music_libraries`, picked once at bootstrap — see `docs/12-decisions.md` for the
/// single-library scope decision) for the lifetime of the task, consuming `effects` and replying
/// on `events` until the channel closes.
///
/// `10-12`: also starts the WebSocket connection (when `enable_websocket` is set) as its own
/// detached background task, alongside this one — not tracked in `Workers.handles`
/// (`docs/12-decisions.md`): shutdown never needs to wait on it, since the whole tokio runtime
/// (and this task with it) ends the moment the process exits, right after `Workers::drain`'s own
/// bounded wait for the *tracked* handles returns.
pub fn spawn(
    client: Arc<EmbyClient>,
    library: Option<ItemId>,
    mut effects: mpsc::UnboundedReceiver<Effect>,
    events: mpsc::UnboundedSender<Event>,
    offline: OfflineHandle,
    enable_websocket: bool,
    art_store: loxia_tui::widgets::album_art::ArtStore,
) -> JoinHandle<()> {
    spawn_ws_session(
        client.clone(),
        events.clone(),
        offline.clone(),
        enable_websocket,
    );

    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
        let in_flight: Arc<Mutex<HashSet<InFlightKey>>> = Arc::new(Mutex::new(HashSet::new()));

        while let Some(effect) = effects.recv().await {
            let Effect::Net(net) = effect else {
                continue; // every worker's channel carries the whole `Effect`; only ours matters
            };
            handle(
                net,
                client.clone(),
                library.clone(),
                semaphore.clone(),
                in_flight.clone(),
                events.clone(),
                offline.clone(),
                art_store.clone(),
            );
        }
        tracing::debug!(worker = "network", "channel closed; exiting");
    })
}

/// Dispatches one effect. Whatever it needs to fetch runs as its own `tokio::spawn`ed task —
/// gated on the shared semaphore, not on this function returning — so a slow fetch never blocks
/// the next effect from being read off the channel. Each spawned task registers its
/// `InFlightKey` first and bails immediately (before ever touching the semaphore) if an identical
/// request is already outstanding.
#[allow(clippy::too_many_arguments)]
fn handle(
    net: NetEffect,
    client: Arc<EmbyClient>,
    library: Option<ItemId>,
    semaphore: Arc<Semaphore>,
    in_flight: Arc<Mutex<HashSet<InFlightKey>>>,
    events: mpsc::UnboundedSender<Event>,
    offline: OfflineHandle,
    art_store: loxia_tui::widgets::album_art::ArtStore,
) {
    if offline.offline.load(Ordering::Relaxed)
        && let Some(event) = try_serve_offline(&net, &offline.index)
    {
        let _ = events.send(event);
        return;
    }
    match net {
        NetEffect::FetchColumn {
            tab,
            depth,
            kind,
            page,
        } => match kind {
            // Two endpoints, one shape: `/Artists` lists every performer credited anywhere,
            // `/Artists/AlbumArtists` only those credited as an album artist — the same `Artist`
            // rows either way, so only the call differs.
            ColumnKind::Artists | ColumnKind::AlbumArtists => {
                let album_artists_only = kind == ColumnKind::AlbumArtists;
                let key = InFlightKey::Column(kind.clone(), page);
                run_guarded(in_flight, semaphore, key, events, async move {
                    let page_arg = Page { index: page };
                    let fetched = if album_artists_only {
                        items::album_artists(&client, library.as_ref(), page_arg).await
                    } else {
                        items::artists(&client, library.as_ref(), page_arg).await
                    };
                    match fetched {
                        Ok(paged) => Event::Data(DataAction::ItemsLoaded {
                            tab,
                            depth,
                            kind,
                            items: paged.items.into_iter().map(MediaItem::Artist).collect(),
                            total: paged.total,
                            page,
                        }),
                        Err(e) => Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::Column { tab, depth },
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }),
                    }
                });
            }
            // `07-03`: no pagination on either — `list`/`items` both fetch everything in one
            // request (a user's playlist count, and a single playlist's track count, are both
            // small enough that `04-10`'s pagination-lookahead machinery would be pure overhead).
            ColumnKind::Playlists => {
                let key = InFlightKey::Column(kind.clone(), page);
                run_guarded(in_flight, semaphore, key, events, async move {
                    match playlists::list(&client, Page::FIRST).await {
                        Ok(paged) => Event::Data(DataAction::ItemsLoaded {
                            tab,
                            depth,
                            kind,
                            items: paged.items.into_iter().map(MediaItem::Playlist).collect(),
                            total: paged.total,
                            page,
                        }),
                        Err(e) => Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::Column { tab, depth },
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }),
                    }
                });
            }
            // `Albums { of_artist: Some(_) }` (levels 3/4 of the Artists/Genres tabs) is *not*
            // handled here — those go through `FetchDiscography`/`FetchAlbumTracks` below instead,
            // since that's how `drill_right` emits them. Only the bare top-level Albums tab
            // (`of_artist: None`, this module's own `seed_column_for_tab` seed) reaches `FetchColumn`
            // with this kind at all. This arm was missing entirely until a live user found the
            // Albums tab spun forever ("unhandled effect" in the logs) — see `docs/12-decisions.md`.
            ColumnKind::Albums { of_artist: None } => {
                let key = InFlightKey::Column(kind.clone(), page);
                run_guarded(in_flight, semaphore, key, events, async move {
                    match items::albums(&client, library.as_ref(), Page { index: page }).await {
                        Ok(paged) => Event::Data(DataAction::ItemsLoaded {
                            tab,
                            depth,
                            kind,
                            items: paged.items.into_iter().map(MediaItem::Album).collect(),
                            total: paged.total,
                            page,
                        }),
                        Err(e) => Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::Column { tab, depth },
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }),
                    }
                });
            }
            ColumnKind::Genres => {
                let key = InFlightKey::Column(kind.clone(), page);
                run_guarded(in_flight, semaphore, key, events, async move {
                    match items::genres(&client, library.as_ref(), Page { index: page }).await {
                        Ok(paged) => Event::Data(DataAction::ItemsLoaded {
                            tab,
                            depth,
                            kind,
                            items: paged.items.into_iter().map(MediaItem::Genre).collect(),
                            total: paged.total,
                            page,
                        }),
                        Err(e) => Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::Column { tab, depth },
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }),
                    }
                });
            }
            ColumnKind::GenreArtists { ref of_genre } => {
                // `items::genre_artists` only ever reads `.name` off the `Genre` it's given
                // (confirmed by reading its source) — the effect carries just the name, so a
                // placeholder with every other field empty is built here, the same
                // already-established pattern `placeholder_artist` uses for `FetchDiscography`.
                let genre = placeholder_genre(of_genre.clone());
                let key = InFlightKey::Column(kind.clone(), page);
                run_guarded(in_flight, semaphore, key, events, async move {
                    match items::genre_artists(&client, &genre, Page { index: page }).await {
                        Ok(paged) => Event::Data(DataAction::ItemsLoaded {
                            tab,
                            depth,
                            kind,
                            items: paged.items.into_iter().map(MediaItem::Artist).collect(),
                            total: paged.total,
                            page,
                        }),
                        Err(e) => Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::Column { tab, depth },
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }),
                    }
                });
            }
            // `of_parent: None` is the Folders tab's own root. A live user with several music
            // libraries found only one showing — the root used to be hard-wired to the *first*
            // library's own children (`bootstrap::connect` only ever resolves `library[0]`). It now
            // lists **every** music library as a top-level folder to drill into, so all of them are
            // reachable regardless of how many there are (`docs/12-decisions.md`).
            ColumnKind::Folders { of_parent: None } => {
                let key = InFlightKey::Column(kind.clone(), page);
                run_guarded(in_flight, semaphore, key, events, async move {
                    match items::music_libraries(&client).await {
                        Ok(libs) => {
                            let items: Vec<MediaItem> = libs
                                .into_iter()
                                .map(|lib| {
                                    MediaItem::Folder(loxia_core::model::Folder {
                                        id: lib.id,
                                        name: lib.name,
                                    })
                                })
                                .collect();
                            let total = items.len();
                            Event::Data(DataAction::ItemsLoaded {
                                tab,
                                depth,
                                kind,
                                items,
                                total,
                                page: 0,
                            })
                        }
                        Err(e) => Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::Column { tab, depth },
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }),
                    }
                });
            }
            ColumnKind::Folders {
                of_parent: Some(ref parent),
            } => {
                let parent = parent.clone();
                let key = InFlightKey::Column(kind.clone(), page);
                run_guarded(in_flight, semaphore, key, events, async move {
                    match items::folder_children(&client, &parent, Page { index: page }).await {
                        Ok(paged) => Event::Data(DataAction::ItemsLoaded {
                            tab,
                            depth,
                            kind,
                            items: paged.items,
                            total: paged.total,
                            page,
                        }),
                        Err(e) => Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::Column { tab, depth },
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }),
                    }
                });
            }
            ColumnKind::PlaylistTracks { ref of_playlist } => {
                let playlist = loxia_core::model::PlaylistId::from(of_playlist.as_str());
                let key = InFlightKey::Column(kind.clone(), page);
                run_guarded(in_flight, semaphore, key, events, async move {
                    match playlists::items(&client, &playlist).await {
                        Ok(entries) => {
                            let total = entries.len();
                            let items = entries
                                .into_iter()
                                .map(|entry| {
                                    let mut track = entry.track;
                                    track.playlist_entry_id = Some(entry.entry_id);
                                    MediaItem::Track(track)
                                })
                                .collect();
                            Event::Data(DataAction::ItemsLoaded {
                                tab,
                                depth,
                                kind,
                                items,
                                total,
                                page: 0,
                            })
                        }
                        Err(e) => Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::Column { tab, depth },
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }),
                    }
                });
            }
            _ => {
                tracing::warn!(?kind, "unhandled effect: FetchColumn for this column kind");
            }
        },
        NetEffect::FetchDiscography { tab, depth, artist } => {
            let key = InFlightKey::Discography(artist.clone());
            run_guarded(in_flight, semaphore, key, events, async move {
                // `discography()` needs a full `Artist` but only ever reads `.id` (confirmed by
                // reading its source) — the effect carries just the id, so a placeholder with
                // every other field empty is built here rather than changing the
                // already-committed, already-tested `loxia-emby` public signature
                // (`docs/12-decisions.md`).
                let placeholder = placeholder_artist(artist);
                match discography::discography(&client, &placeholder).await {
                    Ok(d) => Event::Data(DataAction::DiscographyLoaded {
                        tab,
                        depth,
                        primary: d.primary,
                        appears_on: d.appears_on,
                    }),
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::Discography { tab, depth },
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        NetEffect::FetchAlbumTracks {
            tab,
            depth,
            album,
            filter_artist,
        } => {
            let key = InFlightKey::AlbumTracks(album.clone());
            run_guarded(in_flight, semaphore, key, events, async move {
                match items::album_tracks(&client, &album).await {
                    Ok(mut tracks) => {
                        if let Some(artist_id) = &filter_artist {
                            tracks.retain(|t| t.artist_ids.contains(artist_id));
                        }
                        let total = tracks.len();
                        // Populating a Tracks column reuses the generic `ItemsLoaded` path
                        // (`kind: ColumnKind::Tracks`) rather than `DataAction::TracksLoaded` —
                        // that variant carries `source`/`full_context` for queue-fill, has no
                        // `tab`/`depth`, and the reducer already treats it as a no-op for column
                        // population (see `reducer::nav::apply_data`'s own doc comment). See
                        // `docs/12-decisions.md`.
                        Event::Data(DataAction::ItemsLoaded {
                            tab,
                            depth,
                            kind: ColumnKind::Tracks { of_album: album },
                            items: tracks.into_iter().map(MediaItem::Track).collect(),
                            total,
                            page: 0,
                        })
                    }
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::Column { tab, depth },
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        NetEffect::FetchAlbumTracksForQueue { album } => {
            let key = InFlightKey::AlbumTracksForQueue(album.clone());
            run_guarded(in_flight, semaphore, key, events, async move {
                match items::album_tracks(&client, &album).await {
                    // Unfiltered, always — the `ArtistOnly` filter (if any) is applied by the
                    // reducer once this reply lands (`docs/12-decisions.md`, `06-02`), never here.
                    Ok(tracks) => Event::Data(DataAction::TracksLoaded {
                        tracks,
                        source: QueueSource::Album { id: album },
                        full_context: false,
                    }),
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::QueueFetch,
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        NetEffect::FetchArtistTracksForQueue { artist } => {
            let key = InFlightKey::ArtistTracksForQueue(artist.clone());
            run_guarded(in_flight, semaphore, key, events, async move {
                let placeholder = placeholder_artist(artist.clone());
                match discography::artist_tracks(&client, &placeholder).await {
                    Ok(tracks) => Event::Data(DataAction::TracksLoaded {
                        tracks,
                        source: QueueSource::Artist { id: artist },
                        full_context: false,
                    }),
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::QueueFetch,
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        NetEffect::FetchGenreTracksForQueue { genre } => {
            let key = InFlightKey::GenreTracksForQueue(genre.clone());
            run_guarded(in_flight, semaphore, key, events, async move {
                // `genre_tracks` filters by name, so a bare `Genre` with an empty id is all it
                // needs — the same placeholder shape `FetchArtistTracksForQueue` uses.
                let placeholder = loxia_core::model::Genre {
                    id: loxia_core::model::ItemId::from(String::new()),
                    name: genre.clone(),
                };
                match items::genre_tracks(&client, &placeholder).await {
                    Ok(tracks) => Event::Data(DataAction::TracksLoaded {
                        tracks,
                        source: QueueSource::Genre { name: genre },
                        full_context: true,
                    }),
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::QueueFetch,
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        NetEffect::FetchFolderTracksForQueue { folder, recursive } => {
            let key = InFlightKey::FolderTracksForQueue(folder.clone(), recursive);
            run_guarded(in_flight, semaphore, key, events, async move {
                match items::folder_tracks(&client, &folder, recursive).await {
                    Ok(tracks) => Event::Data(DataAction::TracksLoaded {
                        tracks,
                        source: QueueSource::Folder {
                            id: folder,
                            recursive,
                        },
                        full_context: recursive,
                    }),
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::QueueFetch,
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        NetEffect::FetchPlaylistTracksForQueue { playlist } => {
            let key = InFlightKey::PlaylistTracksForQueue(playlist.clone());
            run_guarded(in_flight, semaphore, key, events, async move {
                match playlists::items(&client, &playlist).await {
                    Ok(entries) => {
                        let tracks = entries
                            .into_iter()
                            .map(|entry| {
                                let mut track = entry.track;
                                track.playlist_entry_id = Some(entry.entry_id);
                                track
                            })
                            .collect();
                        Event::Data(DataAction::TracksLoaded {
                            tracks,
                            source: QueueSource::Playlist { id: playlist },
                            full_context: false,
                        })
                    }
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::QueueFetch,
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        NetEffect::InstantMix { seed, limit } => {
            let key = InFlightKey::InstantMix(seed.clone());
            run_guarded(in_flight, semaphore, key, events, async move {
                match instant_mix::instant_mix(&client, &seed, limit).await {
                    Ok(tracks) => Event::Data(DataAction::TracksLoaded {
                        tracks,
                        source: QueueSource::InstantMix { seed },
                        full_context: false,
                    }),
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::QueueFetch,
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        NetEffect::Search { query, limit } => {
            let key = InFlightKey::Search(query.clone());
            run_guarded(in_flight, semaphore, key, events, async move {
                // `search::search` itself never returns `Err` — a per-section failure is folded
                // into `SearchResults`'s own `*_error` fields (`02-07`/`07-01`) rather than
                // failing the whole request, so there is no `LoadFailed` arm here to reach.
                let results = search::search(&client, &query, limit)
                    .await
                    .unwrap_or_default();
                Event::Data(DataAction::SearchResultsLoaded {
                    query,
                    results: to_core_results(results),
                })
            });
        }
        NetEffect::FetchFavourites => {
            let key = InFlightKey::Favourites;
            run_guarded(in_flight, semaphore, key, events, async move {
                // Unlike `search()`, `favorites()` is a single query — it can fail outright
                // (`EmbyError`), which is why this arm (unlike `Search`, above) has a real
                // `LoadFailed` path.
                match favorites::favorites(&client, loxia_emby::query::Page::FIRST).await {
                    Ok(results) => Event::Data(DataAction::FavouritesLoaded {
                        results: to_core_results(results),
                    }),
                    Err(e) => Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::Favourites,
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }),
                }
            });
        }
        // Fire-and-forget like `ReportPlayback`, not `run_guarded`: success needs no reply at all
        // (the reducer's own optimistic update, `07-02`, is already the correct final state), so
        // there is no single `Event` type every path here could produce.
        NetEffect::SetFavorite { id, on } => {
            tokio::spawn(async move {
                if let Err(e) = favorites::set_favorite(&client, &id, on).await {
                    // `AppState::pending_favorite_toggles`'s entry for this id is only ever
                    // cleared by a *failure* reaching here, never by a quiet success — a
                    // harmless, bounded leak (one stale entry per item ever toggled this
                    // session) documented in `docs/12-decisions.md`, not a correctness issue.
                    let _ = events.send(Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::FavoriteToggle(id),
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }));
                }
            });
        }
        // `10-08`: unlike `PlaylistRemove`/`PlaylistMove`/`PlaylistDelete` below, this genuinely
        // needs a *success* reply too — this is a brand-new save, not an already-optimistic update
        // with a known-good state to fall back to, so the reducer's own "saving <n> tracks…"
        // toast needs telling one way or the other, whichever request actually finishes last.
        NetEffect::PlaylistCreate {
            name,
            tracks,
            overview,
        } => {
            tokio::spawn(async move {
                let created = match playlists::create(&client, &name, &tracks).await {
                    Ok(id) => id,
                    Err(e) => {
                        let _ = events.send(Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::PlaylistSave,
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }));
                        return;
                    }
                };
                // `02-08`: "Overview cannot be set at creation... callers that want a description
                // on a new playlist call `create` then `set_overview`" — the second request this
                // task's own spec names, run directly here rather than as a second `Effect`/round
                // trip through the reducer, since nothing else ever needs `set_overview` as its
                // own standalone action.
                if !overview.is_empty()
                    && let Err(e) = playlists::set_overview(&client, &created, &overview).await
                {
                    let _ = events.send(Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::PlaylistSave,
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }));
                    return;
                }
                let _ = events.send(Event::Data(DataAction::PlaylistSaved { name }));
            });
        }
        NetEffect::PlaylistAdd { id, name, tracks } => {
            tokio::spawn(async move {
                match playlists::add(&client, &id, &tracks).await {
                    Ok(()) => {
                        let _ = events.send(Event::Data(DataAction::PlaylistSaved { name }));
                    }
                    Err(e) => {
                        let _ = events.send(Event::Data(DataAction::LoadFailed {
                            target: LoadTarget::PlaylistSave,
                            message: e.to_string(),
                            offline: matches!(e, EmbyError::Offline { .. }),
                        }));
                    }
                }
            });
        }
        // `07-03`: fire-and-forget like `SetFavorite` above — success needs no reply (the
        // reducer's own optimistic update is already correct), only a failure reaching
        // `LoadFailed` triggers a rollback (`reducer::queue::playlist_mutation_failed`).
        NetEffect::PlaylistRemove { id, entries } => {
            tokio::spawn(async move {
                if let Err(e) = playlists::remove(&client, &id, &entries).await {
                    let _ = events.send(Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::PlaylistMutation(id),
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }));
                }
            });
        }
        NetEffect::PlaylistMove {
            id,
            item,
            new_index,
        } => {
            tokio::spawn(async move {
                if let Err(e) = playlists::move_item(&client, &id, &item, new_index).await {
                    let _ = events.send(Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::PlaylistMutation(id),
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }));
                }
            });
        }
        NetEffect::PlaylistDelete { id } => {
            tokio::spawn(async move {
                if let Err(e) = playlists::delete(&client, &id).await {
                    let _ = events.send(Event::Data(DataAction::LoadFailed {
                        target: LoadTarget::PlaylistMutation(id),
                        message: e.to_string(),
                        offline: matches!(e, EmbyError::Offline { .. }),
                    }));
                }
            });
        }
        // `06-07`: fire-and-forget, no reply and no in-flight dedup — each report carries its own
        // distinct payload (position, timestamp-implicit ordering via session id), so there is
        // never a genuinely redundant duplicate to drop the way column fetches have.
        NetEffect::FetchLyrics { track, stream_ref } => {
            // `lyrics::fetch` never actually returns `Err` (a failure degrades to
            // `Lyrics::Unsynced(vec![])` — lyrics are cosmetic, `docs/03-emby-api.md` §7), so
            // there is no `LoadFailed` reply to emit; `run_guarded` isn't used either, since a
            // stale in-flight fetch for a track the user has already skipped past is harmless
            // (`reducer::nav::lyrics_loaded` discards it by comparing against the current track).
            tokio::spawn(async move {
                tracing::info!(track = %track, index = stream_ref.stream_index, "fetching lyrics");
                match lyrics::fetch(&client, &track, &stream_ref).await {
                    Ok(lyrics) => {
                        let (kind, count, first) = match &lyrics {
                            loxia_core::model::Lyrics::Synced(l) => (
                                "synced",
                                l.len(),
                                l.first().map(|x| x.text.clone()).unwrap_or_default(),
                            ),
                            loxia_core::model::Lyrics::Unsynced(l) => {
                                ("unsynced", l.len(), l.first().cloned().unwrap_or_default())
                            }
                        };
                        tracing::info!(
                            track = %track,
                            empty = lyrics.is_empty(),
                            kind,
                            count,
                            first = %first.chars().take(60).collect::<String>(),
                            "lyrics fetched"
                        );
                        let _ =
                            events.send(Event::Data(DataAction::LyricsLoaded { track, lyrics }));
                    }
                    // `lyrics::fetch` degrades every failure to empty lyrics rather than erroring,
                    // so this arm should be unreachable — but if it ever isn't, the pane would sit
                    // on "loading lyrics…" forever waiting for a reply that never comes. Reply with
                    // empty lyrics so the UI always resolves (`docs/12-decisions.md`).
                    Err(error) => {
                        tracing::warn!(track = %track, %error, "lyrics fetch errored");
                        let _ = events.send(Event::Data(DataAction::LyricsLoaded {
                            track,
                            lyrics: loxia_core::model::Lyrics::Unsynced(Vec::new()),
                        }));
                    }
                }
            });
        }
        // `10-01`: same shape as `FetchLyrics` just above — a fire-and-forget fetch with no
        // in-flight dedup, reporting nothing back on either a transport failure or a 404
        // (`images::fetch`'s own `Ok(Bytes::new())`), exactly like a missing lyric stream is
        // cosmetic, not an error. Decoding runs on the blocking-thread pool
        // (`tokio::task::spawn_blocking`), off this worker's own async task — the real substance
        // of `docs/07-ui-spec.md` §11's "decoding happens off the main thread," proven here even
        // though nothing yet consumes the decoded image on the other side of `ImageLoaded` (no
        // task has wired an `ArtCache` into the render path yet — `docs/12-decisions.md`). A
        // persistent decoded-image store is deliberately not kept here either, for the same
        // reason: nothing would ever read it.
        NetEffect::FetchImage { id, size, tag } => {
            tokio::spawn(async move {
                let bytes = match images::fetch(&client, &id, &tag, size).await {
                    Ok(bytes) if !bytes.is_empty() => bytes,
                    _ => return,
                };
                let decoded = tokio::task::spawn_blocking(move || image::load_from_memory(&bytes))
                    .await
                    .ok()
                    .and_then(Result::ok);
                // The decoded pixels go to the render layer's own store, not through `Action` — a
                // `DynamicImage` is render data, and actions stay plain serialisable values. The
                // `ImageLoaded` reply below still goes out, purely to trigger the redraw that will
                // pick it up (`docs/12-decisions.md`).
                if let Some(image) = decoded {
                    if let Ok(mut store) = art_store.lock() {
                        store.insert(id.clone(), image);
                    }
                    let _ = events.send(Event::Data(DataAction::ImageLoaded { id, tag }));
                }
            });
        }
        NetEffect::ReportPlayback(report) => {
            tokio::spawn(async move {
                match playback::report(&client, &report).await {
                    Ok(()) => {}
                    Err(EmbyError::Offline { .. }) => {
                        // `08-07` implements the offline scrobble buffer
                        // (`Effect::Cache(AppendScrobble)`) — stubbed here with just a log line,
                        // per this task's own spec.
                        tracing::warn!(
                            "offline; playback report dropped (08-07 will buffer these)"
                        );
                    }
                    Err(e) => tracing::warn!(error = %e, "playback report failed"),
                }
            });
        }
        // `08-06`: the connectivity probe. A failure is silent — `reducer::connectivity`'s own
        // Tick-driven backoff will simply try again later, exactly like every other worker
        // failure in this file (`docs/12-decisions.md`).
        NetEffect::Reconnect => {
            tokio::spawn(async move {
                if probe::probe(&client).await.is_ok() {
                    let _ = events.send(Event::System(SystemEvent::ConnectivityChanged(
                        Connectivity::Reconnecting,
                    )));
                }
            });
        }
        // `11-03`: the server-profile editor's "Test connection"/"Save" — deliberately ignores
        // `client` entirely (the whole point is testing a *candidate* profile, which may not be
        // the one `client` was built from at all) and builds a fresh, unshared client from
        // `url`/`headers` instead. No `in_flight`/semaphore guard: this is a one-off,
        // user-triggered action, not a repeatable background fetch a fast scroll could duplicate.
        NetEffect::TestServerConnection {
            url,
            headers,
            device_id,
            username,
            password,
        } => {
            tokio::spawn(async move {
                let event = test_server_connection(
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
    }
}

/// Builds an ad-hoc client from raw `url`/`headers` (no `ServerConfig`, no `EmbyClient` — neither
/// exists yet for a profile that might not be saved) and runs `auth::test_connection`, mapping the
/// result onto the two `DataAction` replies the server-profile editor's reducer code expects.
/// `pub(crate)`: also the network *stub*'s own handling of the same effect
/// (`workers::spawn_network_stub`) — a candidate profile's "Test connection" needs neither an
/// active `client` nor `library` (that's this function's whole point), so it must keep working
/// even before the app has ever connected to any server at all, which is exactly the state the
/// plain `spawn_stub` used to leave it stuck in forever (`docs/12-decisions.md`).
pub(crate) async fn test_server_connection(
    url: &str,
    headers: &std::collections::BTreeMap<String, String>,
    device_id: &str,
    username: &str,
    password: &str,
) -> Event {
    let base = match loxia_emby::client::parse_base_url(url) {
        Ok(base) => base,
        Err(e) => {
            tracing::warn!(url, error = ?e, "test connection: invalid server url");
            return Event::Data(DataAction::ServerTestFailed {
                message: e.to_string(),
            });
        }
    };
    let header_map = match loxia_emby::client::build_custom_headers(headers) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(url, error = ?e, "test connection: invalid custom header");
            return Event::Data(DataAction::ServerTestFailed {
                message: e.to_string(),
            });
        }
    };
    match loxia_emby::auth::test_connection(&base, &header_map, device_id, username, password).await
    {
        Ok(result) => Event::Data(DataAction::ServerTestSucceeded {
            user_id: result.user_id.as_str().to_string(),
            access_token: result.access_token,
            server_name: result.server_name,
            version: result.version,
        }),
        Err(e) => {
            // Found while debugging a real "the server reported an error" report: the on-screen
            // message is deliberately short (`EmbyError`'s own `Display`), so the response body
            // — often the one thing that actually explains a non-standard status — only ever
            // went anywhere at all if it landed here. Truncated: a reverse proxy's own error page
            // can be an arbitrarily large HTML document, and a log line isn't the place for one.
            let debug = format!("{e:?}");
            let truncated = if debug.len() > 500 {
                let mut end = 500;
                while !debug.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}… ({} bytes total)", &debug[..end], debug.len())
            } else {
                debug
            };
            tracing::warn!(url, error = %e, detail = %truncated, "test connection failed");
            Event::Data(DataAction::ServerTestFailed {
                message: e.to_string(),
            })
        }
    }
}

/// `08-05`: serves `net` from `offline_index` without ever touching `client`, if `net` is one of
/// the two effects `docs/06-cache-and-offline.md` §6 names (`FetchColumn { Artists }`,
/// `FetchDiscography`) and an index is actually loaded. `None` for anything else — the caller
/// falls through to the normal HTTP path, which is this worker's existing (accepted) behaviour
/// for every other effect while offline: it will simply fail or hang per its own retry/timeout
/// logic, exactly as before this task.
fn try_serve_offline(
    net: &NetEffect,
    offline_index: &Arc<StdMutex<Option<OfflineIndex>>>,
) -> Option<Event> {
    let guard = offline_index
        .lock()
        .expect("offline index lock is never poisoned");
    let index = guard.as_ref()?;
    match net {
        NetEffect::FetchColumn {
            tab,
            depth,
            kind,
            page,
        } if matches!(kind, ColumnKind::Artists | ColumnKind::AlbumArtists) => {
            let artists = index.artists();
            let total = artists.len();
            Some(Event::Data(DataAction::ItemsLoaded {
                tab: *tab,
                depth: *depth,
                kind: kind.clone(),
                items: artists.into_iter().map(MediaItem::Artist).collect(),
                total,
                page: *page,
            }))
        }
        NetEffect::FetchDiscography { tab, depth, artist } => {
            let d = index.discography(artist);
            Some(Event::Data(DataAction::DiscographyLoaded {
                tab: *tab,
                depth: *depth,
                primary: d.primary,
                appears_on: d.appears_on,
            }))
        }
        _ => None,
    }
}

/// Spawns `fut` as its own task, first registering `key` in `in_flight` and dropping the request
/// outright (never touching `semaphore`) if it was already there. Otherwise waits for a permit,
/// runs `fut`, sends the resulting `Event` on `events`, and deregisters `key`.
fn run_guarded<F>(
    in_flight: Arc<Mutex<HashSet<InFlightKey>>>,
    semaphore: Arc<Semaphore>,
    key: InFlightKey,
    events: mpsc::UnboundedSender<Event>,
    fut: F,
) where
    F: std::future::Future<Output = Event> + Send + 'static,
{
    tokio::spawn(async move {
        {
            let mut guard = in_flight.lock().await;
            if !guard.insert(key.clone()) {
                return; // an identical request is already outstanding; drop this one
            }
        }
        let _permit = semaphore
            .acquire()
            .await
            .expect("semaphore is never closed");
        let event = fut.await;
        in_flight.lock().await.remove(&key);
        let _ = events.send(event);
    });
}

/// `loxia-emby::endpoints::search::SearchResults` -> `loxia_core::state::search::SearchResults` —
/// two distinct, identically-shaped types (`loxia-core` cannot depend on `loxia-emby`), used by
/// both `Search` and `FetchFavourites` (`07-01`/`07-02`).
fn to_core_results(r: search::SearchResults) -> loxia_core::state::search::SearchResults {
    loxia_core::state::search::SearchResults {
        artists: r.artists,
        albums: r.albums,
        tracks: r.tracks,
        artists_error: r.artists_error,
        albums_error: r.albums_error,
        tracks_error: r.tracks_error,
        playlists: r.playlists,
        // No section-level failure exists for playlists: they come from `favorites`' single query,
        // which fails as a whole or not at all — unlike search, whose three requests run
        // concurrently and can fail one at a time.
        playlists_error: None,
    }
}

/// A minimal stand-in for the artist `discography()` needs — every field but `id` is empty since
/// nothing downstream of `discography()` reads them.
fn placeholder_artist(id: ItemId) -> Artist {
    Artist {
        id,
        name: String::new(),
        sort_name: String::new(),
        album_count: 0,
        track_count: 0,
        genres: Vec::new(),
        is_favorite: false,
        image: None,
        overview: None,
    }
}

/// A minimal stand-in for the genre `genre_artists()` needs — only `name` is real, since
/// `/Artists?Genres={name}` filters by name, not id (`docs/03-emby-api.md` §3), and nothing
/// downstream of `genre_artists()` reads `id`.
fn placeholder_genre(name: String) -> Genre {
    Genre {
        id: ItemId::from(""),
        name,
    }
}

// --- 10-12: WebSocket remote control ----------------------------------------------------------

/// How often to recheck `offline.offline` while it's set — no need for a tight poll loop; this
/// only governs how promptly a reconnect resumes once the connectivity probe (`08-06`) clears it,
/// not correctness.
const OFFLINE_RECHECK_INTERVAL: Duration = Duration::from_secs(5);

/// `enable_websocket: false` (`ui.enable_websocket`) skips this entirely — no task is spawned at
/// all, so `disabled_by_config_skips_connection` is provable directly from this function's own
/// return value, with no connection ever attempted.
fn spawn_ws_session(
    client: Arc<EmbyClient>,
    events: mpsc::UnboundedSender<Event>,
    offline: OfflineHandle,
    enable_websocket: bool,
) -> Option<JoinHandle<()>> {
    if !enable_websocket {
        return None;
    }
    Some(tokio::spawn(ws_reconnect_loop(
        events,
        offline,
        move || {
            let client = client.clone();
            async move { attempt_connection(&client).await }
        },
    )))
}

async fn attempt_connection(
    client: &EmbyClient,
) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, WsError> {
    let request = ws::handshake_request(client)?;
    let (stream, _response) = tokio_tungstenite::connect_async(request).await?;
    Ok(stream)
}

/// The reconnect loop itself — generic over `connect` purely as an internal test seam (real
/// callers always pass [`attempt_connection`]; nothing outside this module ever calls this
/// directly). Never returns on its own: "the WebSocket is strictly optional... any failure is
/// logged at `warn` once and never blocks browsing or playback" (this task's own spec) means
/// giving up is not an option, only backing off.
///
/// Reconnection is paused entirely while `offline.offline` is set — `08-06`'s own connectivity
/// probe owns deciding when the server is reachable again, and "two independent reconnect loops
/// would fight" (this task's own spec) if this one kept trying on its own schedule too.
async fn ws_reconnect_loop<S, F, Fut>(
    events: mpsc::UnboundedSender<Event>,
    offline: OfflineHandle,
    connect: F,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    F: Fn() -> Fut,
    Fut: Future<Output = Result<WebSocketStream<S>, WsError>>,
{
    let mut attempt: u32 = 0;
    // "Logged at `warn` once" — collapses a run of consecutive failures into a single line, but
    // a *fresh* failure episode (after a successful connection in between) warns again, since
    // that's new information, not a repeat of the same one.
    let mut warned = false;
    loop {
        if offline.offline.load(Ordering::Relaxed) {
            tokio::time::sleep(OFFLINE_RECHECK_INTERVAL).await;
            continue;
        }
        match connect().await {
            Ok(stream) => {
                attempt = 0;
                warned = false;
                run_session(stream, &events, ws::KEEPALIVE_INTERVAL).await;
                // `run_session` only returns once the connection has dropped — loop straight
                // back around to reconnect, at the very first (1s) backoff step.
            }
            Err(error) => {
                if !warned {
                    tracing::warn!(
                        %error,
                        "websocket connection failed; continuing without remote control"
                    );
                    warned = true;
                }
            }
        }
        let backoff = ws::reconnect_backoff(attempt, rand::random());
        attempt = attempt.saturating_add(1);
        tokio::time::sleep(backoff).await;
    }
}

/// Drives one live connection until it closes or errors: sends `KeepAlive` on its own interval
/// (adjustable mid-session by an inbound `ForceKeepAlive`) and translates every inbound text
/// frame via `ws::parse_message`. Generic over the stream type so tests can drive it with a
/// plain loopback `TcpStream` — no TLS, and no real Emby server, involved. `initial_interval` is
/// `ws::KEEPALIVE_INTERVAL` in production; tests pass a short one and use real (not paused) time,
/// so a real local socket's actual I/O never has to race a manually-driven virtual clock.
async fn run_session<S>(
    mut stream: WebSocketStream<S>,
    events: &mpsc::UnboundedSender<Event>,
    initial_interval: Duration,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut keepalive_interval = initial_interval;
    let mut next_keepalive = tokio::time::Instant::now() + keepalive_interval;
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(next_keepalive) => {
                if stream.send(Message::text(ws::keepalive_message())).await.is_err() {
                    return; // the connection is gone; the outer loop will reconnect
                }
                next_keepalive = tokio::time::Instant::now() + keepalive_interval;
            }
            frame = stream.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        for ws_event in ws::parse_message(text.as_str()) {
                            match ws_event {
                                WsEvent::Player(action) => {
                                    let _ = events.send(Event::Player(action));
                                }
                                // `10-13`: `SystemEvent::Toast` now carries bare `message`/`level`
                                // — `AppState::toast` (the reducer) is the sole assigner of
                                // `id`/`created_at`, so there is no `Toast`/`Timestamp` to build
                                // here at all any more.
                                WsEvent::Toast(message) => {
                                    let _ = events.send(Event::System(SystemEvent::Toast {
                                        message,
                                        level: ToastLevel::Info,
                                    }));
                                }
                                WsEvent::LibraryChanged => {
                                    let _ = events.send(Event::Data(DataAction::LibraryChanged));
                                }
                                WsEvent::UserDataChanged { id, is_favorite, play_count } => {
                                    let _ = events.send(Event::Data(DataAction::UserDataChanged {
                                        id,
                                        is_favorite,
                                        play_count,
                                    }));
                                }
                                WsEvent::ForceKeepAlive(interval) => {
                                    keepalive_interval = interval;
                                    next_keepalive = tokio::time::Instant::now() + keepalive_interval;
                                }
                            }
                        }
                    }
                    // `tokio-tungstenite` answers `Ping`/`Pong` automatically; a `Binary` frame is
                    // not part of this protocol at all — both are simply not this task's own
                    // message table, so neither is acted on.
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => return,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use loxia_core::config::ServerConfig;
    use loxia_core::state::nav::Tab;
    use wiremock::matchers::{method, path, path_regex, query_param};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    use super::*;

    fn cfg(url: &str) -> ServerConfig {
        ServerConfig {
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

    fn artists_page_body(total: usize) -> String {
        serde_json::json!({
            "TotalRecordCount": total,
            "StartIndex": 0,
            "Items": (0..total).map(|i| serde_json::json!({"Id": format!("a{i}"), "Name": format!("Artist {i}")})).collect::<Vec<_>>(),
        })
        .to_string()
    }

    async fn spawn_worker(
        server_uri: &str,
    ) -> (
        mpsc::UnboundedSender<Effect>,
        mpsc::UnboundedReceiver<Event>,
        JoinHandle<()>,
    ) {
        spawn_worker_with_offline(
            server_uri,
            OfflineHandle {
                offline: Arc::new(AtomicBool::new(false)),
                index: Arc::new(StdMutex::new(None)),
            },
        )
        .await
    }

    async fn spawn_worker_with_offline(
        server_uri: &str,
        offline: OfflineHandle,
    ) -> (
        mpsc::UnboundedSender<Effect>,
        mpsc::UnboundedReceiver<Event>,
        JoinHandle<()>,
    ) {
        let client = Arc::new(EmbyClient::new(&cfg(server_uri)).unwrap());
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let handle = spawn(
            client,
            Some(ItemId::from("lib-1")),
            effects_rx,
            events_tx,
            offline,
            false, // `10-12`: none of these pre-existing tests exercise the WebSocket
            Default::default(),
        );
        (effects_tx, events_rx, handle)
    }

    /// `wiremock`'s `MockServer` runs its own request handling on a **dedicated
    /// single-threaded** runtime (`new_current_thread`, its own OS thread — see
    /// `wiremock::mock_server::bare_server::BareMockServer::start`), so a blocking
    /// `std::thread::sleep` inside a `Respond` impl serializes every request there regardless of
    /// how many worker threads *this* test's own runtime has — there is no way to observe real
    /// overlap through it. This exercises `run_guarded`'s semaphore gating directly instead, with
    /// synthetic futures that `tokio::time::sleep` (cooperative, not blocking) — the exact
    /// mechanism `worker_limits_concurrency_to_four` needs to prove, independent of any mock
    /// server's own threading model. See `docs/12-decisions.md`.
    #[tokio::test]
    async fn worker_limits_concurrency_to_four() {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
        let in_flight: Arc<Mutex<HashSet<InFlightKey>>> = Arc::new(Mutex::new(HashSet::new()));
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        for page in 0..12 {
            let key = InFlightKey::Column(ColumnKind::Artists, page);
            let current = current.clone();
            let peak = peak.clone();
            run_guarded(
                in_flight.clone(),
                semaphore.clone(),
                key,
                events_tx.clone(),
                async move {
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Event::System(loxia_core::action::SystemEvent::Refresh)
                },
            );
        }
        drop(events_tx);

        for _ in 0..12 {
            events_rx.recv().await;
        }

        assert!(
            peak.load(Ordering::SeqCst) <= 4,
            "peak concurrent tasks was {}, expected at most 4",
            peak.load(Ordering::SeqCst)
        );
        assert!(
            peak.load(Ordering::SeqCst) > 1,
            "the test is meaningless if tasks never overlapped at all"
        );
    }

    /// `07-04`: `FetchColumn` for `ColumnKind::Genres` hits `/MusicGenres` and replies with
    /// `MediaItem::Genre` rows, the same shape `ColumnKind::Artists`'s own worker test already
    /// exercises for `/Artists`.
    #[tokio::test]
    async fn fetch_column_genres_hits_music_genres_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/MusicGenres"))
            .respond_with(ResponseTemplate::new(200).set_body_string(artists_page_body(2)))
            .mount(&server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Genres,
                depth: 0,
                kind: ColumnKind::Genres,
                page: 0,
            }))
            .unwrap();
        drop(effects_tx);

        let event = events_rx.recv().await.unwrap();
        match event {
            Event::Data(DataAction::ItemsLoaded { items, total, .. }) => {
                assert_eq!(total, 2);
                assert!(items.iter().all(|i| matches!(i, MediaItem::Genre(_))));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    /// A live user found the top-level Albums tab (`ColumnKind::Albums { of_artist: None }`) spun
    /// forever — there was no `FetchColumn` arm for it at all, only the `_ => warn!` catch-all
    /// (`docs/12-decisions.md`). This confirms the new arm actually reaches `/Users/{uid}/Items`
    /// and replies with `MediaItem::Album` rows, the way `ColumnKind::Artists`'s own worker test
    /// confirms `/Artists`.
    #[tokio::test]
    async fn fetch_column_albums_of_artist_none_hits_items_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "MusicAlbum"))
            .respond_with(ResponseTemplate::new(200).set_body_string(artists_page_body(3)))
            .mount(&server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Albums,
                depth: 0,
                kind: ColumnKind::Albums { of_artist: None },
                page: 0,
            }))
            .unwrap();
        drop(effects_tx);

        let event = events_rx.recv().await.unwrap();
        match event {
            Event::Data(DataAction::ItemsLoaded { items, total, .. }) => {
                assert_eq!(total, 3);
                assert!(items.iter().all(|i| matches!(i, MediaItem::Album(_))));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    /// A live user with several music libraries only ever saw one under Folders — the root was
    /// wired to the first library's children. It now lists *every* music library as a folder
    /// (`docs/12-decisions.md`).
    #[tokio::test]
    async fn folders_root_lists_every_music_library() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Views"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Items": [
                    {"Id": "lib-a", "Name": "Music", "CollectionType": "music"},
                    {"Id": "lib-b", "Name": "Soundtracks", "CollectionType": "music"},
                    {"Id": "lib-c", "Name": "Movies", "CollectionType": "movies"},
                ],
            })))
            .mount(&server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Folders,
                depth: 0,
                kind: ColumnKind::Folders { of_parent: None },
                page: 0,
            }))
            .unwrap();
        drop(effects_tx);

        let event = events_rx.recv().await.unwrap();
        match event {
            Event::Data(DataAction::ItemsLoaded { items, total, .. }) => {
                assert_eq!(total, 2, "both music libraries, not the movies one");
                let names: Vec<&str> = items
                    .iter()
                    .filter_map(|i| match i {
                        MediaItem::Folder(f) => Some(f.name.as_str()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(names, vec!["Music", "Soundtracks"]);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    /// `07-04`: `FetchColumn` for `ColumnKind::GenreArtists` filters `/Artists` by the genre's
    /// name, not id — confirming the placeholder-`Genre` plumbing actually reaches the endpoint.
    #[tokio::test]
    async fn fetch_column_genre_artists_filters_by_name() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Artists"))
            .and(query_param("Genres", "Darkwave"))
            .respond_with(ResponseTemplate::new(200).set_body_string(artists_page_body(1)))
            .mount(&server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Genres,
                depth: 1,
                kind: ColumnKind::GenreArtists {
                    of_genre: "Darkwave".to_string(),
                },
                page: 0,
            }))
            .unwrap();
        drop(effects_tx);

        let event = events_rx.recv().await.unwrap();
        match event {
            Event::Data(DataAction::ItemsLoaded { items, total, .. }) => {
                assert_eq!(total, 1);
                assert!(items.iter().all(|i| matches!(i, MediaItem::Artist(_))));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn duplicate_page_request_is_coalesced() {
        let server = MockServer::start().await;
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_clone = hits.clone();
        Mock::given(method("GET"))
            .and(path("/emby/Artists"))
            .respond_with(move |_req: &Request| {
                hits_clone.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(100));
                ResponseTemplate::new(200).set_body_string(artists_page_body(1))
            })
            .mount(&server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        let make = || {
            Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Artists,
                depth: 0,
                kind: ColumnKind::Artists,
                page: 0,
            })
        };
        effects_tx.send(make()).unwrap();
        effects_tx.send(make()).unwrap();
        effects_tx.send(make()).unwrap();
        drop(effects_tx);

        let event = tokio::time::timeout(Duration::from_secs(2), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, Event::Data(DataAction::ItemsLoaded { .. })));
        // Give the (dropped) duplicates a chance to have wrongly fired a second request, if the
        // coalescing were broken.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    /// The Album Artists column must hit `/Artists/AlbumArtists`, not `/Artists` — the whole point
    /// of the tab is that it is the shorter, album-credited list. Only the mocked endpoint is
    /// registered, so a request to the wrong one fails the test rather than quietly returning the
    /// same rows.
    #[tokio::test]
    async fn album_artists_column_uses_the_album_artists_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Artists/AlbumArtists"))
            .respond_with(ResponseTemplate::new(200).set_body_string(artists_page_body(2)))
            .expect(1)
            .mount(&server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::FetchColumn {
                tab: Tab::AlbumArtists,
                depth: 0,
                kind: ColumnKind::AlbumArtists,
                page: 0,
            }))
            .unwrap();
        drop(effects_tx);

        match events_rx.recv().await.unwrap() {
            Event::Data(DataAction::ItemsLoaded { kind, items, .. }) => {
                assert_eq!(kind, ColumnKind::AlbumArtists);
                assert_eq!(items.len(), 2);
                assert!(items.iter().all(|i| matches!(i, MediaItem::Artist(_))));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn reply_for_popped_column_is_ignored_by_reducer() {
        // The worker's own contract is just "echo tab/depth/kind back unchanged" — this confirms
        // that contract, not the reducer (covered by `reducer::nav`'s own tests): the emitted
        // event's `tab`/`depth`/`kind` exactly match what was requested, which is all the reducer
        // needs to discard a reply for a column that no longer exists.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Artists"))
            .respond_with(ResponseTemplate::new(200).set_body_string(artists_page_body(3)))
            .mount(&server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Artists,
                depth: 2,
                kind: ColumnKind::Artists,
                page: 0,
            }))
            .unwrap();
        drop(effects_tx);

        let event = events_rx.recv().await.unwrap();
        match event {
            Event::Data(DataAction::ItemsLoaded {
                tab, depth, kind, ..
            }) => {
                assert_eq!(tab, Tab::Artists);
                assert_eq!(depth, 2);
                assert_eq!(kind, ColumnKind::Artists);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn unhandled_column_kind_emits_nothing() {
        let server = MockServer::start().await;
        let (effects_tx, mut events_rx, handle) = spawn_worker(&server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Search,
                depth: 0,
                kind: ColumnKind::SearchResults,
                page: 0,
            }))
            .unwrap();
        drop(effects_tx);
        handle.await.unwrap(); // worker loop exits once the effects channel closes

        // Every sender clone is gone once the worker task above has ended, so a fully drained
        // channel now yields `None` immediately rather than blocking — no timeout needed.
        assert_eq!(
            events_rx.recv().await,
            None,
            "an unhandled ColumnKind must not emit an event"
        );
    }

    /// A one-artist, one-track offline library, built the same way `08-04`'s downloads would
    /// leave one: a single `.loxia.json` sidecar under a fresh temp directory.
    fn build_test_offline_index() -> (tempfile::TempDir, OfflineIndex) {
        let dir = tempfile::tempdir().unwrap();
        let artist = loxia_core::test_support::fixtures::artist("Boy Harsher");
        let album = loxia_core::test_support::fixtures::album("Care", 2019, &artist);
        let track = loxia_core::test_support::fixtures::track("Motion", 1, &album, &[&artist]);
        std::fs::write(
            dir.path().join("motion.loxia.json"),
            serde_json::to_vec(&track).unwrap(),
        )
        .unwrap();
        let index = OfflineIndex::build(dir.path()).unwrap();
        (dir, index)
    }

    /// `08-05`: with `offline` set and an index loaded, `FetchColumn { Artists }` never reaches
    /// `client` at all — the mock server here answers every request with a 500, and the reply
    /// still carries the downloaded artist, proving the HTTP path was never taken.
    #[tokio::test]
    async fn offline_worker_serves_from_index() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let (_dir, index) = build_test_offline_index();
        let offline = OfflineHandle {
            offline: Arc::new(AtomicBool::new(true)),
            index: Arc::new(StdMutex::new(Some(index))),
        };
        let (effects_tx, mut events_rx, _handle) =
            spawn_worker_with_offline(&server.uri(), offline).await;

        effects_tx
            .send(Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Artists,
                depth: 0,
                kind: ColumnKind::Artists,
                page: 0,
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .expect("offline reply must be immediate, never wait on the network")
            .unwrap();
        match event {
            Event::Data(DataAction::ItemsLoaded { items, total, .. }) => {
                assert_eq!(total, 1);
                assert_eq!(items.len(), 1);
            }
            other => panic!("expected an offline ItemsLoaded reply, got {other:?}"),
        }
    }

    fn load_discography_fixture(name: &str) -> String {
        // `crates/loxia-emby`'s own fixtures, reused rather than copied — this crate is the only
        // one depending on both `loxia-emby` and `loxia-cache`, which is exactly why this parity
        // test has to live here rather than in either of them (`docs/12-decisions.md`).
        std::fs::read_to_string(format!(
            "{}/../loxia-emby/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    /// `08-05`'s own acceptance list: "the same fixture data through both paths yields identical
    /// primary and appears-on sets." Runs the *real* online `discography()` (HTTP-mocked with
    /// `loxia-emby`'s own fixtures: artist `Sync24`/`66665`, 2 primary albums, 4 appears-on) and a
    /// hand-built offline library describing the identical artist/album/relation shape, then
    /// compares both results by album id set — not full `Album` struct equality, since the
    /// server's rich per-album fields (genres, run time, image tags, ...) and what an offline
    /// index can synthesise purely from downloaded tracks are never going to match byte-for-byte;
    /// the thing this task's own wording asks to prove is the *classification*, i.e. which album
    /// ids land in which section.
    #[tokio::test]
    async fn offline_discography_split_matches_online() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("AlbumArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(load_discography_fixture("discography_primary.json")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("ArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(load_discography_fixture("discography_all.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let sync24 = placeholder_artist(ItemId::from("66665"));
        let online = discography::discography(&client, &sync24).await.unwrap();

        // The identical shape the fixtures describe (verified against
        // `discography.rs`'s own `splits_primary_and_appears_on` test): `Sync24` (`66665`) is the
        // album artist of `66666`/`66667`, and merely a contributor on four "Various Artists"
        // compilations.
        let dir = tempfile::tempdir().unwrap();
        let a = loxia_core::test_support::fixtures::artist("Sync24");
        let a = loxia_core::model::Artist {
            id: ItemId::from("66665"),
            ..a
        };
        let primary_albums = [("66666", "Comfortable Void"), ("66667", "Source")];
        let appears_on_albums = [
            ("124929", "Fahrenheit Project, Part Six"),
            ("111142", "Albedo"),
            ("71824", "Chillogram"),
            ("124919", "Fahrenheit Project, Part Five"),
        ];
        for (id, name) in primary_albums {
            let mut alb = loxia_core::test_support::fixtures::album(name, 2010, &a);
            alb.id = ItemId::from(id);
            let t = loxia_core::test_support::fixtures::track("Track", 1, &alb, &[&a]);
            std::fs::write(
                dir.path().join(format!("{id}.loxia.json")),
                serde_json::to_vec(&t).unwrap(),
            )
            .unwrap();
        }
        for (id, name) in appears_on_albums {
            let mut alb = loxia_core::test_support::fixtures::appears_on_album(name, 2005, &a);
            alb.id = ItemId::from(id);
            let t = loxia_core::test_support::fixtures::track("Track", 1, &alb, &[&a]);
            std::fs::write(
                dir.path().join(format!("{id}.loxia.json")),
                serde_json::to_vec(&t).unwrap(),
            )
            .unwrap();
        }

        let index = OfflineIndex::build(dir.path()).unwrap();
        let offline = index.discography(&ItemId::from("66665"));

        let online_primary: std::collections::HashSet<_> =
            online.primary.iter().map(|a| a.id.clone()).collect();
        let offline_primary: std::collections::HashSet<_> =
            offline.primary.iter().map(|a| a.id.clone()).collect();
        assert_eq!(online_primary, offline_primary);

        let online_appears_on: std::collections::HashSet<_> =
            online.appears_on.iter().map(|a| a.id.clone()).collect();
        let offline_appears_on: std::collections::HashSet<_> =
            offline.appears_on.iter().map(|a| a.id.clone()).collect();
        assert_eq!(online_appears_on, offline_appears_on);
    }

    /// `08-06`: `NetEffect::Reconnect` (the connectivity probe) replies with
    /// `ConnectivityChanged(Reconnecting)` the moment the mocked server answers at all.
    #[tokio::test]
    async fn reconnect_effect_reports_reconnecting_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx.send(Effect::Net(NetEffect::Reconnect)).unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            event,
            Event::System(SystemEvent::ConnectivityChanged(Connectivity::Reconnecting))
        );
    }

    /// A probe that can't reach the server at all emits nothing — `reducer::connectivity`'s own
    /// backoff schedule is what retries later, not this worker.
    #[tokio::test]
    async fn reconnect_effect_is_silent_on_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx.send(Effect::Net(NetEffect::Reconnect)).unwrap();

        // Bounded wait rather than relying on the channel closing: the probe's own
        // `tokio::spawn`ed sub-task (not the worker's main loop `_handle`) still holds a live
        // `events` sender for a little while after this send, so `recv()` alone could hang.
        let result = tokio::time::timeout(Duration::from_millis(300), events_rx.recv()).await;
        assert!(result.is_err(), "no event should arrive on a failed probe");
    }

    /// `11-03`: `TestServerConnection` must build its own ad-hoc client from the raw `url` it's
    /// given, entirely ignoring the worker's own already-connected `client` — proven here by
    /// pointing the worker at a *different* server than the one it's testing, and mocking only
    /// the one being tested.
    #[tokio::test]
    async fn test_server_connection_ignores_the_workers_own_client() {
        let connected_server = MockServer::start().await; // never mocked/hit for this effect
        let tested_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/AuthenticateByName"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "User": { "Id": "user-9" },
                "AccessToken": "fresh-token",
                "ServerId": "server-1",
            })))
            .mount(&tested_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ServerName": "Home Library",
                "Version": "4.8.0.80",
            })))
            .mount(&tested_server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&connected_server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::TestServerConnection {
                url: tested_server.uri(),
                headers: BTreeMap::new(),
                device_id: "dev".to_string(),
                username: "alice".to_string(),
                password: loxia_core::effect::RedactedSecret::new("s3cr3t-pw"),
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            event,
            Event::Data(DataAction::ServerTestSucceeded {
                user_id: "user-9".to_string(),
                access_token: "fresh-token".to_string(),
                server_name: "Home Library".to_string(),
                version: "4.8.0.80".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn test_server_connection_maps_a_failure_to_server_test_failed() {
        let connected_server = MockServer::start().await;
        let tested_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/AuthenticateByName"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&tested_server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&connected_server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::TestServerConnection {
                url: tested_server.uri(),
                headers: BTreeMap::new(),
                device_id: "dev".to_string(),
                username: "alice".to_string(),
                password: loxia_core::effect::RedactedSecret::new("wrong-pw"),
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            event,
            Event::Data(DataAction::ServerTestFailed { .. })
        ));
    }

    /// Found while debugging a real "the server reported an error" report: a status outside the
    /// classified set (401/403/404/429/5xx) reaches the user only as a bare, generic phrase unless
    /// the status code itself is included — this is the end-to-end proof it actually is.
    #[tokio::test]
    async fn test_server_connection_failure_message_includes_the_status_code() {
        let connected_server = MockServer::start().await;
        let tested_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/AuthenticateByName"))
            .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
            .mount(&tested_server)
            .await;

        let (effects_tx, mut events_rx, _handle) = spawn_worker(&connected_server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::TestServerConnection {
                url: tested_server.uri(),
                headers: BTreeMap::new(),
                device_id: "dev".to_string(),
                username: "alice".to_string(),
                password: loxia_core::effect::RedactedSecret::new("s3cr3t-pw"),
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        match event {
            Event::Data(DataAction::ServerTestFailed { message }) => {
                assert!(message.contains("400"), "{message}");
            }
            other => panic!("expected ServerTestFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_server_connection_rejects_an_invalid_url_without_any_request() {
        let connected_server = MockServer::start().await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&connected_server.uri()).await;
        effects_tx
            .send(Effect::Net(NetEffect::TestServerConnection {
                url: "not a url".to_string(),
                headers: BTreeMap::new(),
                device_id: "dev".to_string(),
                username: "alice".to_string(),
                password: loxia_core::effect::RedactedSecret::new("s3cr3t-pw"),
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            event,
            Event::Data(DataAction::ServerTestFailed { .. })
        ));
    }

    /// `08-07`'s own integration acceptance test: with the server unreachable, playing three
    /// tracks to completion buffers three `Played` reports instead of losing them; once the
    /// server is reachable again, draining the buffer delivers exactly those three, in order.
    #[tokio::test]
    async fn scrobbles_survive_a_disconnect_and_replay_on_reconnect() {
        use loxia_cache::scrobble::ScrobbleBuffer;
        use loxia_core::model::{PlaybackReport, ServerId};
        use loxia_emby::endpoints::playback;

        let dir = tempfile::tempdir().unwrap();
        let mut buffer = ScrobbleBuffer::open(dir.path(), ServerId::from("srv1")).unwrap();

        // Phase 1: the server is genuinely unreachable (nothing listens on this port) — three
        // completed tracks each fail to report and get buffered instead of lost.
        let unreachable = EmbyClient::new(&cfg("http://127.0.0.1:1")).unwrap();
        for id in ["track-1", "track-2", "track-3"] {
            let played = PlaybackReport::Played {
                item: ItemId::from(id),
            };
            assert!(playback::report(&unreachable, &played).await.is_err());
            buffer.append(played, loxia_core::Timestamp::now()).unwrap();
        }
        assert_eq!(buffer.pending(), 3);

        // Phase 2: the server is reachable again — draining replays all three, in the order they
        // were buffered.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/emby/Users/.*/PlayedItems/.*"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();

        let sent = buffer
            .drain(|report| {
                let client = &client;
                async move { playback::report(client, &report).await.is_ok() }
            })
            .await
            .unwrap();

        assert_eq!(sent, 3);
        assert_eq!(buffer.pending(), 0);
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }

    fn png_bytes() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]));
        let mut bytes = Vec::new();
        image::DynamicImage::from(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    // `10-01`
    #[tokio::test]
    async fn fetch_image_reports_image_loaded_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/track-1/Images/Primary"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes()))
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx
            .send(Effect::Net(NetEffect::FetchImage {
                id: ItemId::from("track-1"),
                size: loxia_core::model::ImageSize::Thumb,
                tag: "tag-1".to_string(),
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            event,
            Event::Data(DataAction::ImageLoaded {
                id: ItemId::from("track-1"),
                tag: "tag-1".to_string(),
            })
        );
    }

    /// A 404 is `images::fetch`'s own `Ok(Bytes::new())` — cosmetic, not an error — and reports
    /// nothing back, matching `FetchLyrics`'s identical treatment of a missing lyric stream.
    #[tokio::test]
    async fn fetch_image_reports_nothing_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/track-1/Images/Primary"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx
            .send(Effect::Net(NetEffect::FetchImage {
                id: ItemId::from("track-1"),
                size: loxia_core::model::ImageSize::Thumb,
                tag: "tag-1".to_string(),
            }))
            .unwrap();

        let result = tokio::time::timeout(Duration::from_millis(300), events_rx.recv()).await;
        assert!(
            result.is_err(),
            "a 404 must report nothing, not even an empty event"
        );
    }

    // --- 10-08: save playlist -------------------------------------------------------------------

    #[tokio::test]
    async fn playlist_create_without_overview_reports_saved() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"Id": "pl-new", "ItemAddedCount": 1})),
            )
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx
            .send(Effect::Net(NetEffect::PlaylistCreate {
                name: "My Mix".to_string(),
                tracks: vec![ItemId::from("t1")],
                overview: String::new(),
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            event,
            Event::Data(DataAction::PlaylistSaved {
                name: "My Mix".to_string()
            })
        );
    }

    #[tokio::test]
    async fn playlist_create_with_overview_makes_two_requests_then_reports_saved() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"Id": "pl-new", "ItemAddedCount": 1})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items/pl-new"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Id": "pl-new", "Name": "My Mix", "Overview": null,
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/emby/Items/pl-new"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx
            .send(Effect::Net(NetEffect::PlaylistCreate {
                name: "My Mix".to_string(),
                tracks: vec![ItemId::from("t1")],
                overview: "A great mix".to_string(),
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            event,
            Event::Data(DataAction::PlaylistSaved {
                name: "My Mix".to_string()
            })
        );
    }

    #[tokio::test]
    async fn playlist_create_failure_reports_load_failed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx
            .send(Effect::Net(NetEffect::PlaylistCreate {
                name: "My Mix".to_string(),
                tracks: vec![],
                overview: String::new(),
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            event,
            Event::Data(DataAction::LoadFailed {
                target: loxia_core::action::LoadTarget::PlaylistSave,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn playlist_add_reports_saved_with_the_existing_playlists_own_name() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists/pl1/Items"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx
            .send(Effect::Net(NetEffect::PlaylistAdd {
                id: loxia_core::model::PlaylistId::from("pl1"),
                name: "Late Night Darkwave".to_string(),
                tracks: vec![ItemId::from("t1")],
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            event,
            Event::Data(DataAction::PlaylistSaved {
                name: "Late Night Darkwave".to_string()
            })
        );
    }

    #[tokio::test]
    async fn playlist_add_failure_reports_load_failed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists/pl1/Items"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let (effects_tx, mut events_rx, _handle) = spawn_worker(&server.uri()).await;

        effects_tx
            .send(Effect::Net(NetEffect::PlaylistAdd {
                id: loxia_core::model::PlaylistId::from("pl1"),
                name: "Late Night Darkwave".to_string(),
                tracks: vec![ItemId::from("t1")],
            }))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            event,
            Event::Data(DataAction::LoadFailed {
                target: loxia_core::action::LoadTarget::PlaylistSave,
                ..
            })
        ));
    }

    // --- 10-12: WebSocket remote control -------------------------------------------------

    fn ws_test_client() -> Arc<EmbyClient> {
        Arc::new(EmbyClient::new(&cfg("http://192.0.2.1:8096")).unwrap())
    }

    fn inert_offline_handle(offline: bool) -> OfflineHandle {
        OfflineHandle {
            offline: Arc::new(AtomicBool::new(offline)),
            index: Arc::new(StdMutex::new(None)),
        }
    }

    /// `10-12`: `disabled_by_config_skips_connection` — provable directly from `spawn_ws_session`'s
    /// own return value: `enable_websocket: false` spawns no task at all, so no connection is
    /// ever attempted.
    #[test]
    fn disabled_by_config_skips_connection() {
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let handle = spawn_ws_session(
            ws_test_client(),
            events_tx,
            inert_offline_handle(false),
            false,
        );
        assert!(handle.is_none());
    }

    /// `10-12`: `no_reconnect_while_offline` — a fake `connect` that counts its own calls proves
    /// the loop never even attempts a connection while `offline.offline` is set, across a large
    /// span of virtual time.
    #[tokio::test(start_paused = true)]
    async fn no_reconnect_while_offline() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_closure = attempts.clone();
        let offline = inert_offline_handle(true);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(ws_reconnect_loop::<tokio::net::TcpStream, _, _>(
            events_tx,
            offline,
            move || {
                attempts_for_closure.fetch_add(1, Ordering::SeqCst);
                async move { Err(WsError::AlreadyClosed) }
            },
        ));

        for _ in 0..20 {
            tokio::time::advance(OFFLINE_RECHECK_INTERVAL).await;
            tokio::task::yield_now().await;
        }
        handle.abort();

        assert_eq!(
            attempts.load(Ordering::SeqCst),
            0,
            "must never attempt a connection while offline"
        );
    }

    /// `10-12`: `handshake_failure_is_non_fatal_and_warns_once` — a fake `connect` that always
    /// fails proves the loop keeps running (and keeps retrying, with backoff) rather than exiting
    /// or panicking; asserting on the log line itself isn't attempted here (no other test in this
    /// codebase captures `tracing` output either), only the behaviour the spec actually cares
    /// about — non-fatal, keeps trying.
    #[tokio::test(start_paused = true)]
    async fn handshake_failure_is_non_fatal_and_warns_once() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_closure = attempts.clone();
        let offline = inert_offline_handle(false);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(ws_reconnect_loop::<tokio::net::TcpStream, _, _>(
            events_tx,
            offline,
            move || {
                attempts_for_closure.fetch_add(1, Ordering::SeqCst);
                async move { Err(WsError::AlreadyClosed) }
            },
        ));

        // Longer than the 30s backoff cap, several times over.
        for _ in 0..6 {
            tokio::time::advance(Duration::from_secs(35)).await;
            tokio::task::yield_now().await;
        }
        assert!(!handle.is_finished(), "must not exit after a failure");
        handle.abort();

        assert!(
            attempts.load(Ordering::SeqCst) >= 3,
            "must keep retrying after a handshake failure, non-fatally; got {}",
            attempts.load(Ordering::SeqCst)
        );
    }

    /// `10-12`: `reconnect_backoff_is_jittered_and_capped` is already covered directly against
    /// `loxia_emby::ws::reconnect_backoff` in that crate's own test module — nothing here
    /// duplicates it.

    #[test]
    fn keepalive_default_interval_is_30s() {
        assert_eq!(ws::KEEPALIVE_INTERVAL, Duration::from_secs(30));
    }

    /// `10-12`: `keepalive_sent_every_30s` — a real local loopback WebSocket server (no TLS, no
    /// real Emby server involved), so `run_session` is exercised end-to-end against an actual
    /// `tokio_tungstenite` connection rather than a mock. Uses a short `initial_interval` and real
    /// (unpaused) time rather than the literal 30s constant (separately asserted by
    /// `keepalive_default_interval_is_30s` above) — mixing a manually-driven virtual clock with a
    /// real socket's actual I/O has no reliable ordering guarantee, so real, short waits are both
    /// simpler and more robust here.
    #[tokio::test]
    async fn keepalive_sent_every_30s() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            ws.next().await.unwrap().unwrap()
        });

        let (client_stream, _response) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/embywebsocket"))
                .await
                .unwrap();
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let session = tokio::spawn(async move {
            run_session(client_stream, &events_tx, Duration::from_millis(20)).await
        });

        let received = tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("server task must receive the keepalive promptly")
            .unwrap();
        assert_eq!(received, Message::text(ws::keepalive_message()));

        session.abort();
    }

    /// `10-12`: an inbound `ForceKeepAlive` reschedules the *next* send at its own requested
    /// interval rather than whatever the session started with.
    #[tokio::test]
    async fn force_keep_alive_reschedules_the_next_send() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            ws.send(Message::text(
                r#"{"MessageType":"ForceKeepAlive","Data":0}"#,
            ))
            .await
            .unwrap();
            // The rescheduled send (interval 0 -> fires essentially immediately) should now
            // arrive well before the session's own 60s starting interval ever would.
            ws.next().await.unwrap().unwrap()
        });

        let (client_stream, _response) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/embywebsocket"))
                .await
                .unwrap();
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let session = tokio::spawn(async move {
            run_session(client_stream, &events_tx, Duration::from_secs(60)).await
        });

        let received = tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("server task must receive the rescheduled keepalive promptly")
            .unwrap();
        assert_eq!(received, Message::text(ws::keepalive_message()));

        session.abort();
    }
}
