//! Offline browse tree built from download sidecars (`docs/06-cache-and-offline.md` §6).
//!
//! Every `.loxia.json` sidecar under `downloads_root` is a complete, serialised `Track` — this
//! walks all of them and reconstructs artists/albums purely from what those tracks carry, since
//! only tracks have sidecars at all (`08-04`). No caching layer: `build` always re-walks disk, so
//! it is correct-by-construction after any `pin`/`unpin` with nothing to invalidate.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use loxia_core::discography::{Discography, classify};
use loxia_core::model::{Album, AlbumRelation, Artist, ItemId, Track};
use loxia_core::state::search::SearchResults;

use crate::error::CacheError;

struct AlbumAgg {
    name: String,
    album_artist_names: Vec<String>,
    year: Option<u16>,
    genres: HashSet<String>,
    image: Option<loxia_core::model::ImageRef>,
    total_duration: Duration,
    track_ids: Vec<ItemId>,
}

struct ArtistAgg {
    name: String,
    genres: HashSet<String>,
    primary_albums: HashSet<ItemId>,
    track_ids: HashSet<ItemId>,
}

/// A browsable library reconstructed entirely from downloaded tracks' own sidecars — no server
/// round trip. `album_count`/`track_count` on synthesised artists, and every count on synthesised
/// albums, reflect **what is downloaded**, never a true server-side total: showing a server count
/// next to a partial offline library would be misleading (`docs/06-cache-and-offline.md` §6).
pub struct OfflineIndex {
    tracks: HashMap<ItemId, Track>,
    tracks_by_album: HashMap<ItemId, Vec<ItemId>>,
    albums: HashMap<ItemId, AlbumAgg>,
    artists: HashMap<ItemId, ArtistAgg>,
}

impl OfflineIndex {
    /// Walks `downloads_root` reading every `.loxia.json` sidecar. A sidecar that fails to parse
    /// (or a file that fails to read at all) is skipped with a `warn!` — one bad file must not make
    /// the whole offline library unavailable.
    pub fn build(downloads_root: &Path) -> Result<OfflineIndex, CacheError> {
        let mut tracks: HashMap<ItemId, Track> = HashMap::new();
        walk_sidecars(downloads_root, &mut tracks)?;

        let mut albums: HashMap<ItemId, AlbumAgg> = HashMap::new();
        let mut artists: HashMap<ItemId, ArtistAgg> = HashMap::new();
        let mut tracks_by_album: HashMap<ItemId, Vec<ItemId>> = HashMap::new();

        for track in tracks.values() {
            if let Some(album_id) = &track.album_id {
                let agg = albums.entry(album_id.clone()).or_insert_with(|| AlbumAgg {
                    name: track.album_name.clone(),
                    album_artist_names: track.album_artist_names.clone(),
                    year: track.year,
                    genres: HashSet::new(),
                    image: track.image.clone(),
                    total_duration: Duration::ZERO,
                    track_ids: Vec::new(),
                });
                agg.genres.extend(track.genres.iter().cloned());
                agg.total_duration += track.duration;
                agg.track_ids.push(track.id.clone());
                tracks_by_album
                    .entry(album_id.clone())
                    .or_default()
                    .push(track.id.clone());
            }

            for (i, artist_id) in track.artist_ids.iter().enumerate() {
                let agg = artists
                    .entry(artist_id.clone())
                    .or_insert_with(|| ArtistAgg {
                        name: track.artist_names.get(i).cloned().unwrap_or_default(),
                        genres: HashSet::new(),
                        primary_albums: HashSet::new(),
                        track_ids: HashSet::new(),
                    });
                agg.genres.extend(track.genres.iter().cloned());
                agg.track_ids.insert(track.id.clone());
                if let Some(album_id) = &track.album_id
                    && classify(track, artist_id) == Some(AlbumRelation::Primary)
                {
                    agg.primary_albums.insert(album_id.clone());
                }
            }
        }

        Ok(OfflineIndex {
            tracks,
            tracks_by_album,
            albums,
            artists,
        })
    }

    pub fn artists(&self) -> Vec<Artist> {
        let mut result: Vec<Artist> = self
            .artists
            .iter()
            .map(|(id, agg)| Artist {
                id: id.clone(),
                name: agg.name.clone(),
                sort_name: agg.name.clone(),
                album_count: agg.primary_albums.len() as u32,
                track_count: agg.track_ids.len() as u32,
                genres: agg.genres.iter().cloned().collect(),
                // Neither is knowable offline: favouriting and overview text both live on a
                // server-side artist record this index never has a sidecar for (only tracks do).
                is_favorite: false,
                image: None,
                overview: None,
            })
            .collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }

    /// The same ALBUMS / APPEARS ON split the online path produces, via
    /// [`loxia_core::discography::classify`] applied to every downloaded track that involves
    /// `artist` (`docs/12-decisions.md`).
    pub fn discography(&self, artist: &ItemId) -> Discography {
        let mut primary_ids: HashSet<ItemId> = HashSet::new();
        let mut appears_on_ids: HashSet<ItemId> = HashSet::new();

        for track in self.tracks.values() {
            let Some(album_id) = &track.album_id else {
                continue;
            };
            match classify(track, artist) {
                Some(AlbumRelation::Primary) => {
                    primary_ids.insert(album_id.clone());
                }
                Some(AlbumRelation::AppearsOn { .. }) => {
                    appears_on_ids.insert(album_id.clone());
                }
                None => {}
            }
        }
        // A downloaded partial set could in principle have one track classify Primary and
        // another AppearsOn for the same album (e.g. only some tracks of a "various artists"
        // release are pinned) — Primary wins, matching "the album's own album-artist(s) include
        // this artist" being a per-album, not per-track, fact.
        appears_on_ids.retain(|id| !primary_ids.contains(id));

        let mut primary: Vec<Album> = primary_ids
            .iter()
            .map(|id| self.build_album(id, AlbumRelation::Primary))
            .collect();
        let mut appears_on: Vec<Album> = appears_on_ids
            .iter()
            .map(|id| {
                self.build_album(
                    id,
                    AlbumRelation::AppearsOn {
                        context_artist: artist.clone(),
                    },
                )
            })
            .collect();
        sort_albums(&mut primary);
        sort_albums(&mut appears_on);
        Discography {
            primary,
            appears_on,
        }
    }

    /// All downloaded tracks of `album`, sorted by disc then track number — never the order
    /// sidecars happened to be read in.
    pub fn album_tracks(&self, album: &ItemId) -> Vec<Track> {
        let mut tracks: Vec<Track> = self
            .tracks_by_album
            .get(album)
            .into_iter()
            .flatten()
            .filter_map(|id| self.tracks.get(id).cloned())
            .collect();
        tracks.sort_by_key(|t| (t.disc_number.unwrap_or(0), t.track_number.unwrap_or(0)));
        tracks
    }

    /// Fuzzy, case-insensitive matching against track/album/artist names (`nucleo-matcher`, the
    /// same engine `04-11`'s inline filter uses) — never against server data this index doesn't
    /// have.
    pub fn search(&self, q: &str) -> SearchResults {
        if q.trim().is_empty() {
            return SearchResults::default();
        }

        let atom = Atom::new(
            q,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
            false,
        );
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf: Vec<char> = Vec::new();

        let artists: Vec<Artist> = self
            .artists()
            .into_iter()
            .filter(|a| {
                atom.score(Utf32Str::new(&a.name, &mut buf), &mut matcher)
                    .is_some()
            })
            .collect();

        let mut album_ids: Vec<&ItemId> = self.albums.keys().collect();
        album_ids.sort();
        let albums: Vec<Album> = album_ids
            .into_iter()
            .filter(|id| {
                let name = &self.albums[*id].name;
                atom.score(Utf32Str::new(name, &mut buf), &mut matcher)
                    .is_some()
            })
            // `relation` is meaningless outside a discography-for-one-artist context; defaulting
            // to `Primary` matches the same placeholder every other bare (non-discography) album
            // fetch already uses (`loxia-emby`'s DTO conversion, `docs/12-decisions.md`).
            .map(|id| self.build_album(id, AlbumRelation::Primary))
            .collect();

        let mut track_ids: Vec<&ItemId> = self.tracks.keys().collect();
        track_ids.sort();
        let tracks: Vec<Track> = track_ids
            .into_iter()
            .filter(|id| {
                let name = &self.tracks[*id].name;
                atom.score(Utf32Str::new(name, &mut buf), &mut matcher)
                    .is_some()
            })
            .map(|id| self.tracks[id].clone())
            .collect();

        SearchResults {
            artists,
            albums,
            tracks,
            artists_error: None,
            albums_error: None,
            tracks_error: None,
            playlists: Vec::new(),
            playlists_error: None,
        }
    }

    /// Whether `id` (a track, album, or artist) is present in this offline library — used to
    /// decide whether an item that isn't cached can still be browsed/queued offline
    /// (`docs/06-cache-and-offline.md` §6).
    pub fn contains(&self, id: &ItemId) -> bool {
        self.tracks.contains_key(id)
            || self.albums.contains_key(id)
            || self.artists.contains_key(id)
    }

    fn build_album(&self, id: &ItemId, relation: AlbumRelation) -> Album {
        let agg = &self.albums[id];
        Album {
            id: id.clone(),
            name: agg.name.clone(),
            sort_name: agg.name.clone(),
            album_artist_names: agg.album_artist_names.clone(),
            album_artist_ids: self.resolve_album_artist_ids(agg),
            year: agg.year,
            track_count: agg.track_ids.len() as u32,
            total_duration: agg.total_duration,
            genres: agg.genres.iter().cloned().collect(),
            is_favorite: false,
            image: agg.image.clone(),
            relation,
        }
    }

    /// Best-effort: `Track` has no `album_artist_ids` field (only the string
    /// `album_artist_names`, `docs/12-decisions.md`), so this recovers ids by scanning the
    /// album's own tracks for a contributor whose name matches one of `album_artist_names` —
    /// works whenever the album artist is also a per-track contributor, true for every real
    /// release. Empty if none match (an album artist absent from every downloaded track's own
    /// contributor list).
    fn resolve_album_artist_ids(&self, agg: &AlbumAgg) -> Vec<ItemId> {
        let mut ids = Vec::new();
        for track_id in &agg.track_ids {
            let Some(track) = self.tracks.get(track_id) else {
                continue;
            };
            for (i, name) in track.artist_names.iter().enumerate() {
                if agg.album_artist_names.contains(name)
                    && let Some(id) = track.artist_ids.get(i)
                    && !ids.contains(id)
                {
                    ids.push(id.clone());
                }
            }
        }
        ids
    }
}

/// Year ascending, then name ascending; a missing year sorts last — the same rule
/// `loxia-emby::endpoints::discography`'s own (private) `sort_albums` uses, so online and offline
/// discography results are order-comparable, not just set-comparable.
fn sort_albums(albums: &mut [Album]) {
    albums.sort_by(|a, b| match (a.year, b.year) {
        (Some(ya), Some(yb)) => ya.cmp(&yb).then_with(|| a.name.cmp(&b.name)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });
}

fn walk_sidecars(dir: &Path, tracks: &mut HashMap<ItemId, Track>) -> Result<(), CacheError> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|source| CacheError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| CacheError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| CacheError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            walk_sidecars(&path, tracks)?;
            continue;
        }
        let is_sidecar = path
            .file_name()
            .map(|n| n.to_string_lossy().ends_with(".loxia.json"))
            .unwrap_or(false);
        if !is_sidecar {
            continue;
        }
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Track>(&text).ok())
        {
            Some(track) => {
                tracks.insert(track.id.clone(), track);
            }
            None => tracing::warn!(?path, "malformed download sidecar; skipped"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::test_support::fixtures;
    use std::time::Instant;
    use tempfile::tempdir;

    fn write_sidecar(dir: &Path, track: &Track) {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{}.loxia.json", track.name));
        std::fs::write(path, serde_json::to_vec_pretty(track).unwrap()).unwrap();
    }

    #[test]
    fn build_reconstructs_artists_and_albums_from_sidecars() {
        let dir = tempdir().unwrap();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::track("Motion", 1, &alb, &[&a]);
        let t2 = fixtures::track("Escape", 2, &alb, &[&a]);
        write_sidecar(dir.path(), &t1);
        write_sidecar(dir.path(), &t2);

        let index = OfflineIndex::build(dir.path()).unwrap();

        let artists = index.artists();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].id, a.id);
        assert_eq!(artists[0].track_count, 2);

        let tracks = index.album_tracks(&alb.id);
        assert_eq!(tracks.len(), 2);
    }

    #[test]
    fn malformed_sidecar_is_skipped_with_warning() {
        let dir = tempdir().unwrap();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let good = fixtures::track("Motion", 1, &alb, &[&a]);
        write_sidecar(dir.path(), &good);
        std::fs::write(dir.path().join("broken.loxia.json"), b"{ not json").unwrap();

        let index = OfflineIndex::build(dir.path()).unwrap();

        assert_eq!(index.artists().len(), 1);
        assert!(index.contains(&good.id));
    }

    #[test]
    fn counts_reflect_downloaded_not_server() {
        let dir = tempdir().unwrap();
        let a = fixtures::artist("Boy Harsher");
        // The real artist has many more albums/tracks server-side; only one track is downloaded.
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        write_sidecar(dir.path(), &t);

        let index = OfflineIndex::build(dir.path()).unwrap();
        let artist = index.artists().into_iter().find(|x| x.id == a.id).unwrap();
        assert_eq!(
            artist.track_count, 1,
            "reflects the one downloaded track, not a server total"
        );
        assert_eq!(artist.album_count, 1);
    }

    /// An internal sanity check on `discography`'s own split logic — the *real* parity test
    /// against the online path (this task's own named acceptance test,
    /// `offline_discography_split_matches_online`) lives in `crates/loxia`, the only crate
    /// depending on both `loxia-emby` and `loxia-cache` (`docs/12-decisions.md`).
    #[test]
    fn discography_splits_primary_and_appears_on() {
        let dir = tempdir().unwrap();
        let a = fixtures::artist("Boy Harsher");
        let primary_alb = fixtures::album("Care", 2019, &a);
        let comp_alb = fixtures::appears_on_album("Darkwave Comp", 2020, &a);
        let t1 = fixtures::track("Motion", 1, &primary_alb, &[&a]);
        let t2 = fixtures::track("Contribution", 1, &comp_alb, &[&a]);
        write_sidecar(dir.path(), &t1);
        write_sidecar(dir.path(), &t2);

        let index = OfflineIndex::build(dir.path()).unwrap();
        let d = index.discography(&a.id);

        assert_eq!(d.primary.len(), 1);
        assert_eq!(d.primary[0].id, primary_alb.id);
        assert_eq!(d.appears_on.len(), 1);
        assert_eq!(d.appears_on[0].id, comp_alb.id);
    }

    /// "Shared, not duplicated" (this task's own acceptance list): the Primary/AppearsOn rule is
    /// `loxia_core::discography::classify`, imported directly (see this file's own top-level
    /// `use`) and independently tested there — this crate holds no second implementation of it.
    #[test]
    fn classification_function_is_shared_not_duplicated() {
        use loxia_core::discography::classify as shared_classify;
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        assert_eq!(shared_classify(&t, &a.id), Some(AlbumRelation::Primary));
    }

    #[test]
    fn search_matches_titles_and_artists() {
        let dir = tempdir().unwrap();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        write_sidecar(dir.path(), &t);

        let index = OfflineIndex::build(dir.path()).unwrap();

        let by_title = index.search("Motion");
        assert_eq!(by_title.tracks.len(), 1);

        let by_artist = index.search("Boy Harsher");
        assert_eq!(by_artist.artists.len(), 1);
    }

    #[test]
    fn rebuild_on_pin_and_unpin() {
        use crate::downloads::{DownloadScope, Downloads};
        use loxia_core::config::QualityProfile;
        use std::sync::Arc;

        // A minimal fetcher good enough for this test's own needs — `downloads.rs`'s own, fuller
        // fake fetcher is `#[cfg(test)]`-private to that module.
        struct NoopFetcher;
        impl crate::downloads::Fetcher for NoopFetcher {
            fn expand<'a>(
                &'a self,
                _scope: &'a DownloadScope,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<Vec<Track>, CacheError>> + Send + 'a>,
            > {
                Box::pin(async { Ok(Vec::new()) })
            }
            fn fetch_track<'a>(
                &'a self,
                _track: &'a Track,
                _profile: QualityProfile,
                dest: &'a Path,
                on_progress: &'a (dyn Fn(u64, u64) + Send + Sync),
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<u64, CacheError>> + Send + 'a>,
            > {
                Box::pin(async move {
                    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
                    std::fs::write(dest, b"x").unwrap();
                    on_progress(1, 1);
                    Ok(1)
                })
            }
            fn fetch_cover<'a>(
                &'a self,
                _album: &'a ItemId,
                _dest: &'a Path,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), CacheError>> + Send + 'a>,
            > {
                Box::pin(async { Ok(()) })
            }
        }

        let dir = tempdir().unwrap();
        let sink: crate::downloads::ProgressSink = Arc::new(|_| {});
        let mut downloads = Downloads::open(
            dir.path(),
            loxia_core::model::ServerId::from("srv1"),
            QualityProfile::Direct,
            Arc::new(NoopFetcher),
            sink,
        )
        .unwrap();

        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);

        assert!(
            OfflineIndex::build(dir.path())
                .unwrap()
                .artists()
                .is_empty()
        );

        block_on(downloads.pin(DownloadScope::Track(t.clone()))).unwrap();
        assert_eq!(OfflineIndex::build(dir.path()).unwrap().artists().len(), 1);

        block_on(downloads.unpin(&t.id)).unwrap();
        assert!(
            OfflineIndex::build(dir.path())
                .unwrap()
                .artists()
                .is_empty()
        );
    }

    /// A tiny current-thread block-on so this one test (needing `Downloads::pin`/`unpin`, real
    /// `async fn`s) can stay a plain `#[test]` like the rest of this file — nothing here needs
    /// real concurrency or timers, just *an* executor to drive the future to completion.
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn build_10k_tracks_under_two_seconds() {
        let dir = tempdir().unwrap();
        let a = fixtures::artist("Prolific Artist");
        for album_n in 0..100 {
            let alb = fixtures::album(&format!("Album {album_n}"), 2000 + album_n as u16, &a);
            let album_dir = dir.path().join(format!("album{album_n}"));
            for track_n in 0..100 {
                let t = fixtures::track(&format!("Track {track_n}"), track_n as u32, &alb, &[&a]);
                write_sidecar(&album_dir, &t);
            }
        }

        let start = Instant::now();
        let index = OfflineIndex::build(dir.path()).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(index.artists().len(), 1);
        assert!(
            elapsed < Duration::from_secs(2),
            "build took {elapsed:?} for 10,000 tracks"
        );
    }
}
