//! Concurrent typed search across artists, albums, tracks.

use loxia_core::model::{Album, Artist, Track};

use crate::client::EmbyClient;
use crate::error::EmbyError;
use crate::query::{ItemQuery, ItemType};

use super::fetch_typed;
use super::items::items_path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchResults {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    /// `07-01`: each section can fail independently (`futures::join!` below, not `try_join!`) —
    /// carried alongside the (empty, on failure) data so the UI can render "this section errored"
    /// distinctly from "this section genuinely has no matches". `None` on success.
    pub artists_error: Option<String>,
    pub albums_error: Option<String>,
    pub tracks_error: Option<String>,
    /// Only ever populated by `favorites` — text search does not query playlists. Kept on the same
    /// struct so favourites and search share one shape all the way through, as they always have.
    pub playlists: Vec<loxia_core::model::Playlist>,
}

/// Runs three typed queries concurrently (`futures::join!`, not `try_join!` — one failing must
/// not discard the other two successes; see the acceptance test and `12-decisions.md`). A blank
/// query issues no request at all.
pub async fn search(
    client: &EmbyClient,
    q: &str,
    limit: usize,
) -> Result<SearchResults, EmbyError> {
    if q.trim().is_empty() {
        return Ok(SearchResults::default());
    }

    let path = items_path(client);
    let query_for = |item_type: ItemType| {
        ItemQuery::default()
            .item_types(&[item_type])
            .search_term(q)
            .recursive(true)
            .page(0, limit)
            .to_query_pairs()
    };
    let artist_pairs = query_for(ItemType::MusicArtist);
    let album_pairs = query_for(ItemType::MusicAlbum);
    let track_pairs = query_for(ItemType::Audio);

    let (artists, albums, tracks) = futures::join!(
        fetch_typed::<Artist>(client, &path, &artist_pairs),
        fetch_typed::<Album>(client, &path, &album_pairs),
        fetch_typed::<Track>(client, &path, &track_pairs),
    );

    let artists_error = artists.as_ref().err().map(ToString::to_string);
    let albums_error = albums.as_ref().err().map(ToString::to_string);
    let tracks_error = tracks.as_ref().err().map(ToString::to_string);

    Ok(SearchResults {
        artists: artists.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "artist search failed, showing partial results");
            Vec::new()
        }),
        albums: albums.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "album search failed, showing partial results");
            Vec::new()
        }),
        tracks: tracks.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "track search failed, showing partial results");
            Vec::new()
        }),
        artists_error,
        albums_error,
        tracks_error,
        playlists: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use loxia_core::config::ServerConfig;

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

    fn load_fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn search_runs_three_queries_concurrently() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "MusicArtist"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("search_artists.json")),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "MusicAlbum"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("search_albums.json")),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "Audio"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("search_tracks.json")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = search(&client, "boy harsher", 50).await.unwrap();
        assert!(
            !result.artists.is_empty() || !result.albums.is_empty() || !result.tracks.is_empty()
        );
    }

    #[tokio::test]
    async fn blank_query_issues_no_request() {
        let server = MockServer::start().await;
        // No mocks registered at all — any request would fail with a connection/404 error.
        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = search(&client, "   ", 50).await.unwrap();
        assert_eq!(result, SearchResults::default());
    }

    #[tokio::test]
    async fn partial_search_failure_returns_others() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "MusicArtist"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("search_artists.json")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "MusicAlbum"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "Audio"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("search_tracks.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = search(&client, "boy harsher", 50).await.unwrap();
        assert!(result.albums.is_empty());
        assert!(!result.artists.is_empty());
        assert!(!result.tracks.is_empty());
    }
}
