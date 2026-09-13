//! Discography (ALBUMS / APPEARS ON split) and the whole-artist track queue.
//!
//! The split is two album-level queries diffed by id — see `03-emby-api.md` §4 and
//! `12-decisions.md` §10 item 1 for why this replaced an earlier recursive-track-query draft.

use std::cmp::Ordering;
use std::collections::HashSet;

use loxia_core::discography::Discography;
use loxia_core::model::{Album, AlbumRelation, Artist, Track};

use crate::client::EmbyClient;
use crate::error::EmbyError;
use crate::query::{EmbySort, ItemQuery, ItemType, SortOrder};

use super::fetch_all_items;
use super::items::items_path;

/// Two album-level queries (`AlbumArtistIds` for primary releases, `ArtistIds` for every album the
/// artist appears on) diffed client-side by id. Issues no track-level request.
pub async fn discography(client: &EmbyClient, artist: &Artist) -> Result<Discography, EmbyError> {
    let base = ItemQuery::default()
        .item_types(&[ItemType::MusicAlbum])
        .recursive(true);
    let path = items_path(client);

    let mut primary: Vec<Album> = fetch_all_items(
        client,
        &path,
        &base
            .clone()
            .album_artist_ids(std::slice::from_ref(&artist.id)),
    )
    .await?;
    let all: Vec<Album> = fetch_all_items(
        client,
        &path,
        &base.artist_ids(std::slice::from_ref(&artist.id)),
    )
    .await?;

    let primary_ids: HashSet<_> = primary.iter().map(|a| a.id.clone()).collect();
    let mut appears_on: Vec<Album> = all
        .into_iter()
        .filter(|a| !primary_ids.contains(&a.id))
        .collect();

    for album in &mut primary {
        album.relation = AlbumRelation::Primary;
    }
    for album in &mut appears_on {
        album.relation = AlbumRelation::AppearsOn {
            context_artist: artist.id.clone(),
        };
    }

    sort_albums(&mut primary);
    sort_albums(&mut appears_on);

    tracing::debug!(
        artist = %artist.id,
        primary = primary.len(),
        appears_on = appears_on.len(),
        "discography split"
    );

    Ok(Discography {
        primary,
        appears_on,
    })
}

/// Year ascending, then name ascending; a missing year sorts last.
fn sort_albums(albums: &mut [Album]) {
    albums.sort_by(|a, b| match (a.year, b.year) {
        (Some(ya), Some(yb)) => ya.cmp(&yb).then_with(|| a.name.cmp(&b.name)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });
}

/// Every track by `artist` across their whole discography — for "queue/play this whole artist"
/// (`Enter`/`a`/`A` on an Artist row); browsing albums never needs this, `discography` is enough.
///
/// Tries `ArtistIds` first (matches tracks where the artist is credited at the *track* level, incl.
/// appears-on) and, **only if that comes back empty**, falls back to `AlbumArtistIds`. A live user
/// found `Enter` on an artist queued nothing while browsing their albums worked fine: their library
/// tags only `AlbumArtists`, not per-track `Artists`, so `ArtistIds` on Audio items matched nothing
/// (`docs/12-decisions.md`). The fallback catches that common tagging style without changing the
/// (richer, appears-on-inclusive) primary query for well-tagged libraries.
pub async fn artist_tracks(client: &EmbyClient, artist: &Artist) -> Result<Vec<Track>, EmbyError> {
    let base = ItemQuery::default()
        .item_types(&[ItemType::Audio])
        .recursive(true)
        .sort_by(
            &[
                EmbySort::Album,
                EmbySort::ParentIndexNumber,
                EmbySort::IndexNumber,
            ],
            SortOrder::Ascending,
        );

    let by_artist = fetch_all_items(
        client,
        &items_path(client),
        &base.clone().artist_ids(std::slice::from_ref(&artist.id)),
    )
    .await?;
    if !by_artist.is_empty() {
        return Ok(by_artist);
    }
    fetch_all_items(
        client,
        &items_path(client),
        &base.album_artist_ids(std::slice::from_ref(&artist.id)),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use loxia_core::config::ServerConfig;
    use loxia_core::model::ItemId;

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

    fn artist(id: &str) -> Artist {
        Artist {
            id: ItemId::from(id),
            name: "Sync24".to_string(),
            sort_name: "Sync24".to_string(),
            album_count: 0,
            track_count: 0,
            genres: Vec::new(),
            is_favorite: false,
            image: None,
            overview: None,
        }
    }

    fn load_fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    async fn mock_discography_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("AlbumArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(load_fixture("discography_primary.json")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("ArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("discography_all.json")),
            )
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn splits_primary_and_appears_on() {
        let server = mock_discography_server().await;
        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = discography(&client, &artist("66665")).await.unwrap();
        assert_eq!(result.primary.len(), 2);
        assert_eq!(result.appears_on.len(), 4);
    }

    #[tokio::test]
    async fn appears_on_excludes_every_primary_album_id() {
        let server = mock_discography_server().await;
        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = discography(&client, &artist("66665")).await.unwrap();
        let primary_ids: HashSet<_> = result.primary.iter().map(|a| a.id.clone()).collect();
        assert!(
            result
                .appears_on
                .iter()
                .all(|a| !primary_ids.contains(&a.id))
        );
        for album in &result.appears_on {
            assert_eq!(
                album.relation,
                AlbumRelation::AppearsOn {
                    context_artist: ItemId::from("66665")
                }
            );
        }
        for album in &result.primary {
            assert_eq!(album.relation, AlbumRelation::Primary);
        }
    }

    #[tokio::test]
    async fn albums_sorted_by_year_then_name() {
        let server = mock_discography_server().await;
        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = discography(&client, &artist("66665")).await.unwrap();
        let years: Vec<_> = result.appears_on.iter().map(|a| a.year).collect();
        let mut sorted = years.clone();
        sorted.sort();
        assert_eq!(years, sorted);
    }

    #[test]
    fn none_year_sorts_last() {
        let mut albums = vec![
            Album {
                id: ItemId::from("a"),
                name: "Zebra".to_string(),
                sort_name: "Zebra".to_string(),
                album_artist_names: Vec::new(),
                album_artist_ids: Vec::new(),
                year: None,
                track_count: 0,
                total_duration: Default::default(),
                genres: Vec::new(),
                is_favorite: false,
                image: None,
                relation: AlbumRelation::Primary,
            },
            Album {
                id: ItemId::from("b"),
                name: "Alpha".to_string(),
                sort_name: "Alpha".to_string(),
                album_artist_names: Vec::new(),
                album_artist_ids: Vec::new(),
                year: Some(1999),
                track_count: 0,
                total_duration: Default::default(),
                genres: Vec::new(),
                is_favorite: false,
                image: None,
                relation: AlbumRelation::Primary,
            },
        ];
        sort_albums(&mut albums);
        assert_eq!(albums[0].name, "Alpha");
        assert_eq!(albums[1].name, "Zebra");
    }

    #[tokio::test]
    async fn empty_appears_on_when_all_albums_are_primary() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("AlbumArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(load_fixture("discography_primary.json")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("ArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(load_fixture("discography_primary.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = discography(&client, &artist("66665")).await.unwrap();
        assert_eq!(result.primary.len(), 2);
        assert!(result.appears_on.is_empty());
    }

    #[tokio::test]
    async fn empty_primary_when_artist_has_no_primary_albums() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("AlbumArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("empty_library.json")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("ArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("discography_all.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = discography(&client, &artist("66665")).await.unwrap();
        assert!(result.primary.is_empty());
        assert_eq!(result.appears_on.len(), 6);
    }

    #[tokio::test]
    async fn paginates_past_200_albums() {
        let server = MockServer::start().await;
        let page0 = serde_json::json!({
            "TotalRecordCount": 201,
            "StartIndex": 0,
            "Items": (0..200).map(|i| serde_json::json!({"Id": format!("p{i}"), "Name": format!("Album {i}")})).collect::<Vec<_>>(),
        });
        let page1 = serde_json::json!({
            "TotalRecordCount": 201,
            "StartIndex": 200,
            "Items": [{"Id": "p200", "Name": "Album 200"}],
        });
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("AlbumArtistIds", "66665"))
            .and(query_param("StartIndex", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page0))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("AlbumArtistIds", "66665"))
            .and(query_param("StartIndex", "200"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page1))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("ArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("empty_library.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = discography(&client, &artist("66665")).await.unwrap();
        assert_eq!(result.primary.len(), 201);
    }

    #[tokio::test]
    async fn discography_issues_no_track_level_request() {
        let server = mock_discography_server().await;
        // The mocked server only ever responds to MusicAlbum queries — if the implementation
        // issued a track-level (`IncludeItemTypes=Audio`) request, this would 404 and fail below.
        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = discography(&client, &artist("66665")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn artist_tracks_issues_one_recursive_query_over_audio() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "Audio"))
            .and(query_param("ArtistIds", "66665"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("album_tracks.json")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let tracks = artist_tracks(&client, &artist("66665")).await.unwrap();
        assert!(!tracks.is_empty());
    }

    #[tokio::test]
    async fn artist_tracks_paginates_past_200() {
        let server = MockServer::start().await;
        let page0 = serde_json::json!({
            "TotalRecordCount": 201,
            "StartIndex": 0,
            "Items": (0..200).map(|i| serde_json::json!({"Id": format!("t{i}"), "Name": format!("Track {i}")})).collect::<Vec<_>>(),
        });
        let page1 = serde_json::json!({
            "TotalRecordCount": 201,
            "StartIndex": 200,
            "Items": [{"Id": "t200", "Name": "Track 200"}],
        });
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "Audio"))
            .and(query_param("StartIndex", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page0))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "Audio"))
            .and(query_param("StartIndex", "200"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page1))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let tracks = artist_tracks(&client, &artist("66665")).await.unwrap();
        assert_eq!(tracks.len(), 201);
    }
}
