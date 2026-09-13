//! The real [`Fetcher`] behind permanent downloads — `loxia-cache`'s side of the trait, backed by
//! `loxia-emby`.
//!
//! `loxia_cache::downloads` was written against this trait so it could be tested with a fake and
//! stay free of any network dependency, and until now the fake was the *only* implementation: the
//! whole download feature was reachable from nowhere, so pressing `d` did nothing at all
//! (`docs/12-decisions.md`).

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use loxia_cache::downloads::{DownloadScope, Fetcher};
use loxia_cache::error::CacheError;
use loxia_core::config::{QualityProfile, TargetCodec};
use loxia_core::model::{ImageSize, ItemId, Track};
use loxia_emby::client::EmbyClient;
use loxia_emby::endpoints::{discography, images, items, playlists};
use loxia_emby::stream::StreamUrl;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct EmbyFetcher {
    client: Arc<EmbyClient>,
    target_codec: TargetCodec,
}

impl EmbyFetcher {
    pub fn new(client: Arc<EmbyClient>, target_codec: TargetCodec) -> Self {
        EmbyFetcher {
            client,
            target_codec,
        }
    }
}

/// `CacheError` has no "the server said no" variant, and adding one would make `loxia-cache` learn
/// about a failure mode it has no other business knowing. A network failure is reported as I/O
/// against the destination path, carrying the server's own message.
fn fetch_error(path: &Path, e: impl std::fmt::Display) -> CacheError {
    CacheError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(e.to_string()),
    }
}

impl Fetcher for EmbyFetcher {
    fn expand<'a>(
        &'a self,
        scope: &'a DownloadScope,
    ) -> BoxFuture<'a, Result<Vec<Track>, CacheError>> {
        Box::pin(async move {
            match scope {
                // `pin` never calls this for a bare track — there is nothing to expand — but
                // answering with the track itself is the only honest thing if it ever does.
                DownloadScope::Track(track) => Ok(vec![track.clone()]),
                DownloadScope::Album(id) => items::album_tracks(&self.client, id)
                    .await
                    .map_err(|e| fetch_error(Path::new("album"), e)),
                DownloadScope::Artist(id) => {
                    // `artist_tracks` needs an `Artist`, but only its id is ever read — the same
                    // placeholder shape `workers::network`'s own `FetchArtistTracksForQueue` uses.
                    let placeholder = loxia_core::model::Artist {
                        id: id.clone(),
                        name: String::new(),
                        sort_name: String::new(),
                        album_count: 0,
                        track_count: 0,
                        genres: Vec::new(),
                        is_favorite: false,
                        image: None,
                        overview: None,
                    };
                    discography::artist_tracks(&self.client, &placeholder)
                        .await
                        .map_err(|e| fetch_error(Path::new("artist"), e))
                }
                DownloadScope::Playlist(id) => {
                    let entries = playlists::items(&self.client, id)
                        .await
                        .map_err(|e| fetch_error(Path::new("playlist"), e))?;
                    Ok(entries.into_iter().map(|e| e.track).collect())
                }
            }
        })
    }

    fn fetch_track<'a>(
        &'a self,
        track: &'a Track,
        profile: QualityProfile,
        dest: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) + Send + Sync),
    ) -> BoxFuture<'a, Result<u64, CacheError>> {
        Box::pin(async move {
            let url = StreamUrl::build(&self.client, &track.id, profile, self.target_codec);
            // `fetch_to_file` reports nothing as it goes, so progress is announced at the two
            // moments actually known: nothing done, and everything done. A byte-accurate bar would
            // mean threading a callback through that endpoint; the download list only needs to
            // distinguish "running" from "finished".
            on_progress(0, 0);
            let cancel = Arc::new(AtomicBool::new(false));
            let bytes =
                loxia_emby::endpoints::download::fetch_to_file(&self.client, &url, dest, &cancel)
                    .await
                    .map_err(|e| fetch_error(dest, e))?;
            on_progress(bytes, bytes);
            Ok(bytes)
        })
    }

    fn fetch_cover<'a>(
        &'a self,
        album: &'a ItemId,
        dest: &'a Path,
    ) -> BoxFuture<'a, Result<(), CacheError>> {
        Box::pin(async move {
            // Best-effort by contract: a missing cover must never fail the pin of the tracks —
            // `images::fetch` already answers a 404 with empty bytes rather than an error.
            //
            // With no image tag to hand (the scope carries only an album id), the URL is built
            // without one; Emby serves the current primary image either way, it simply cannot be
            // cache-busted, which is irrelevant for a one-off write to disk.
            let bytes = images::fetch(&self.client, album, "", ImageSize::Large)
                .await
                .map_err(|e| fetch_error(dest, e))?;
            if bytes.is_empty() {
                return Ok(());
            }
            tokio::fs::write(dest, &bytes)
                .await
                .map_err(|source| CacheError::Io {
                    path: dest.to_path_buf(),
                    source,
                })
        })
    }
}
