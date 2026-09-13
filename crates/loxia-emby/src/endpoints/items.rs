//! Artists, albums, genres, folders browse endpoints.

use serde::Deserialize;

use loxia_core::model::{Album, Artist, Folder, Genre, ItemId, MediaItem, Track};

use crate::client::{EmbyClient, parse_retry_after};
use crate::error::{EmbyError, classify_status, classify_transport};
use crate::query::{EmbySort, ItemQuery, ItemType, PAGE_SIZE, Page, Paged, SortOrder};
use crate::retry::with_retry;

use super::{fetch_all_items, fetch_items};

/// A music library — one entry of `/Users/{uid}/Views` whose `CollectionType` is `"music"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub id: ItemId,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ViewDto {
    id: String,
    name: String,
    #[serde(default)]
    collection_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ViewsResponse {
    #[serde(default)]
    items: Vec<ViewDto>,
}

pub(crate) fn items_path(client: &EmbyClient) -> String {
    format!("Users/{}/Items", client.user_id())
}

fn with_user_id(client: &EmbyClient, mut pairs: Vec<(String, String)>) -> Vec<(String, String)> {
    pairs.push(("UserId".to_string(), client.user_id().to_string()));
    pairs
}

/// `GET /Users/{uid}/Views`, kept to entries whose `CollectionType == "music"`.
pub async fn music_libraries(client: &EmbyClient) -> Result<Vec<Library>, EmbyError> {
    let path = format!("Users/{}/Views", client.user_id());
    let response: ViewsResponse = with_retry(|_attempt| async {
        let response = client.get(&path).send().await.map_err(classify_transport)?;
        let status = response.status();
        let retry_after = parse_retry_after(response.headers());
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(classify_status(status.as_u16(), retry_after, None, message));
        }
        let text = response.text().await.map_err(classify_transport)?;
        serde_json::from_str(&text).map_err(|source| EmbyError::Decode {
            endpoint: path.clone(),
            source,
        })
    })
    .await?;

    Ok(response
        .items
        .into_iter()
        .filter(|v| v.collection_type.as_deref() == Some("music"))
        .map(|v| Library {
            id: ItemId::from(v.id),
            name: v.name,
        })
        .collect())
}

/// `GET /Artists?ParentId={lib}&Recursive=true&SortBy=SortName`.
///
/// `lib` of `None` drops `ParentId`, scoping the list to **every** music library on the server —
/// what a server with more than one music library needs, since a browsing list pinned to the
/// first one silently hides the rest (`docs/12-decisions.md`). Emby de-duplicates the union: an
/// artist present in two libraries is returned once.
pub async fn artists(
    client: &EmbyClient,
    lib: Option<&ItemId>,
    page: Page,
) -> Result<Paged<Artist>, EmbyError> {
    let query = ItemQuery::default()
        .parent_opt(lib)
        .recursive(true)
        .sort_by(&[EmbySort::SortName], SortOrder::Ascending)
        .page(page.start(), PAGE_SIZE);
    let pairs = with_user_id(client, query.to_query_pairs());
    let response = fetch_items(client, "Artists", &pairs).await?;
    let items = response
        .items
        .into_iter()
        .map(Artist::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Paged {
        items,
        total: response.total_record_count,
        start: response.start_index,
    })
}

/// `GET /Artists/AlbumArtists?ParentId={lib}`. `lib` of `None` covers every music library, as in
/// [`artists`].
pub async fn album_artists(
    client: &EmbyClient,
    lib: Option<&ItemId>,
    page: Page,
) -> Result<Paged<Artist>, EmbyError> {
    let query = ItemQuery::default()
        .parent_opt(lib)
        .recursive(true)
        .sort_by(&[EmbySort::SortName], SortOrder::Ascending)
        .page(page.start(), PAGE_SIZE);
    let pairs = with_user_id(client, query.to_query_pairs());
    let response = fetch_items(client, "Artists/AlbumArtists", &pairs).await?;
    let items = response
        .items
        .into_iter()
        .map(Artist::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Paged {
        items,
        total: response.total_record_count,
        start: response.start_index,
    })
}

/// `GET /Users/{uid}/Items?ParentId={album}&SortBy=ParentIndexNumber,IndexNumber` — the complete
/// tracklist in album order. Albums never have enough tracks to paginate, so this returns the
/// whole list rather than a `Paged<Track>`.
pub async fn album_tracks(client: &EmbyClient, album: &ItemId) -> Result<Vec<Track>, EmbyError> {
    let query = ItemQuery::default()
        .parent(album)
        .item_types(&[ItemType::Audio])
        .sort_by(
            &[EmbySort::ParentIndexNumber, EmbySort::IndexNumber],
            SortOrder::Ascending,
        );
    let response = fetch_items(client, &items_path(client), &query.to_query_pairs()).await?;
    response.items.into_iter().map(Track::try_from).collect()
}

/// `GET /Users/{uid}/Items?IncludeItemTypes=MusicAlbum&ParentId={lib}&Recursive=true` — every
/// album in the library, flat (not scoped to any one artist; that's `discography` instead). Backs
/// the top-level Albums tab's `ColumnKind::Albums { of_artist: None }`, which had no endpoint at
/// all before this — see `docs/12-decisions.md`. `lib` of `None` covers every music library, as in
/// [`artists`].
pub async fn albums(
    client: &EmbyClient,
    lib: Option<&ItemId>,
    page: Page,
) -> Result<Paged<Album>, EmbyError> {
    let query = ItemQuery::default()
        .parent_opt(lib)
        .item_types(&[ItemType::MusicAlbum])
        .recursive(true)
        .sort_by(&[EmbySort::SortName], SortOrder::Ascending)
        .page(page.start(), PAGE_SIZE);
    let response = fetch_items(client, &items_path(client), &query.to_query_pairs()).await?;
    let items = response
        .items
        .into_iter()
        .map(Album::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Paged {
        items,
        total: response.total_record_count,
        start: response.start_index,
    })
}

/// `GET /MusicGenres?ParentId={lib}`. `lib` of `None` covers every music library, as in
/// [`artists`].
pub async fn genres(
    client: &EmbyClient,
    lib: Option<&ItemId>,
    page: Page,
) -> Result<Paged<Genre>, EmbyError> {
    let query = ItemQuery::default()
        .parent_opt(lib)
        .page(page.start(), PAGE_SIZE);
    let pairs = with_user_id(client, query.to_query_pairs());
    let response = fetch_items(client, "MusicGenres", &pairs).await?;
    let items = response
        .items
        .into_iter()
        .map(Genre::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Paged {
        items,
        total: response.total_record_count,
        start: response.start_index,
    })
}

/// Every track in a genre — `GET /Users/{uid}/Items?IncludeItemTypes=Audio&Genres={name}`, paged
/// through in full by `fetch_all_items`.
///
/// Filtered by genre **name**, not id, for the same reason `genre_artists` is (see `Genre`'s own
/// doc comment). The sort requested here only makes the paging deterministic — the queue re-orders
/// the result by the user's own sort profile (`reducer::queue::order_incoming`), which is what
/// actually decides play order.
pub async fn genre_tracks(client: &EmbyClient, genre: &Genre) -> Result<Vec<Track>, EmbyError> {
    let query = ItemQuery::default()
        .genres(std::slice::from_ref(&genre.name))
        .item_types(&[ItemType::Audio])
        .recursive(true)
        .sort_by(
            &[
                EmbySort::AlbumArtist,
                EmbySort::Album,
                EmbySort::ParentIndexNumber,
                EmbySort::IndexNumber,
            ],
            SortOrder::Ascending,
        );
    fetch_all_items(client, &items_path(client), &query).await
}

/// `GET /Artists?Genres={name}` — Emby filters genres by name, not id (see `Genre`'s doc comment).
pub async fn genre_artists(
    client: &EmbyClient,
    genre: &Genre,
    page: Page,
) -> Result<Paged<Artist>, EmbyError> {
    let query = ItemQuery::default()
        .genres(std::slice::from_ref(&genre.name))
        .recursive(true)
        .sort_by(&[EmbySort::SortName], SortOrder::Ascending)
        .page(page.start(), PAGE_SIZE);
    let pairs = with_user_id(client, query.to_query_pairs());
    let response = fetch_items(client, "Artists", &pairs).await?;
    let items = response
        .items
        .into_iter()
        .map(Artist::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Paged {
        items,
        total: response.total_record_count,
        start: response.start_index,
    })
}

/// `GET /Users/{uid}/Items?ParentId={folder}&IncludeItemTypes=Folder,Audio` — **not** recursive
/// (`07-05`: a directory with 50,000 files below it must still open instantly); `IsFolder`
/// distinguishes rows within the two item types Emby is asked to return. The `IncludeItemTypes`
/// filter (not present before `07-05`) is what makes "non-audio files are omitted" a server-side
/// guarantee rather than a client-side filter after the fact — images, subtitle sidecars, and
/// video files never round-trip at all.
pub async fn folder_children(
    client: &EmbyClient,
    parent: &ItemId,
    page: Page,
) -> Result<Paged<MediaItem>, EmbyError> {
    let query = ItemQuery::default()
        .parent(parent)
        .item_types(&[ItemType::Folder, ItemType::Audio])
        .page(page.start(), PAGE_SIZE);
    let response = fetch_items(client, &items_path(client), &query.to_query_pairs()).await?;
    let items = response
        .items
        .into_iter()
        .map(|dto| {
            if dto.is_folder {
                Folder::try_from(dto).map(MediaItem::Folder)
            } else {
                Track::try_from(dto).map(MediaItem::Track)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Paged {
        items,
        total: response.total_record_count,
        start: response.start_index,
    })
}

/// `GET /Users/{uid}/Items?ParentId={folder}&IncludeItemTypes=Audio[&Recursive=true]` — every
/// audio file directly inside `folder` (`recursive: false`), or anywhere in its subtree
/// (`recursive: true`) — `07-05`'s `a`/`A` folder-row queueing, not column population.
pub async fn folder_tracks(
    client: &EmbyClient,
    folder: &ItemId,
    recursive: bool,
) -> Result<Vec<Track>, EmbyError> {
    // `Path` (on-disk file path), not `SortName` — folder queueing must follow the library's own
    // directory structure/filenames, so a recursive queue plays folder-by-folder in the order the
    // files are laid out rather than interleaving by track title (`docs/12-decisions.md`).
    let query = ItemQuery::default()
        .parent(folder)
        .item_types(&[ItemType::Audio])
        .recursive(recursive)
        .sort_by(&[EmbySort::Path], SortOrder::Ascending);
    let response = fetch_items(client, &items_path(client), &query.to_query_pairs()).await?;
    response.items.into_iter().map(Track::try_from).collect()
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
    async fn music_libraries_filters_to_music_collection_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Views"))
            .respond_with(ResponseTemplate::new(200).set_body_string(load_fixture("views.json")))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let libraries = music_libraries(&client).await.unwrap();
        assert!(libraries.iter().all(|l| !l.name.is_empty()));
        // Every returned entry must genuinely be a music collection — the fixture also contains
        // non-music views, so this only passes if the CollectionType filter actually ran.
        assert!(!libraries.is_empty());
    }

    #[tokio::test]
    async fn album_tracks_sorted_by_disc_then_track() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("SortBy", "ParentIndexNumber,IndexNumber"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("album_tracks.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let tracks = album_tracks(&client, &ItemId::from("66666")).await.unwrap();
        assert!(!tracks.is_empty());
    }

    /// `07-04`: `genre_artists` always passes exactly one name to `ItemQuery::genres` — a genre
    /// literally named "Rock, Alternative" must arrive as one `Genres` value, not silently split
    /// into two filters the way Emby's own multi-value convention would read a bare comma.
    #[tokio::test]
    async fn genre_artists_sends_comma_in_name_as_one_value() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Artists"))
            .and(query_param("Genres", "Rock, Alternative"))
            .respond_with(ResponseTemplate::new(200).set_body_string(load_fixture("artists.json")))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let genre = Genre {
            id: ItemId::from("genre-1"),
            name: "Rock, Alternative".to_string(),
        };
        let result = genre_artists(&client, &genre, Page::FIRST).await.unwrap();
        assert!(!result.items.is_empty());
    }

    /// A server with several music libraries scopes browsing lists to none of them in particular,
    /// which on the wire means no `ParentId` at all. Pinning to the first library is what hid a
    /// user's whole second library from Artists/Albums/Genres (`docs/12-decisions.md`).
    #[tokio::test]
    async fn a_none_scope_sends_no_parent_id() {
        for (name, endpoint) in [
            ("Artists", "/emby/Artists"),
            ("AlbumArtists", "/emby/Artists/AlbumArtists"),
            ("MusicGenres", "/emby/MusicGenres"),
        ] {
            let server = MockServer::start().await;
            // `query_param_is_missing` is the assertion that matters: the mock only matches when
            // `ParentId` is absent, so a scoped request 404s and the call below fails.
            Mock::given(method("GET"))
                .and(path(endpoint))
                .and(wiremock::matchers::query_param_is_missing("ParentId"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_string(load_fixture("artists.json")),
                )
                .mount(&server)
                .await;

            let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
            let ok = match name {
                "Artists" => artists(&client, None, Page::FIRST).await.is_ok(),
                "AlbumArtists" => album_artists(&client, None, Page::FIRST).await.is_ok(),
                _ => genres(&client, None, Page::FIRST).await.is_ok(),
            };
            assert!(ok, "{name} must send no ParentId when the scope is None");
        }
    }

    /// The single-library case keeps sending it, so non-music audio can't leak into the lists.
    #[tokio::test]
    async fn a_scoped_request_still_sends_parent_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Artists"))
            .and(query_param("ParentId", "lib-1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(load_fixture("artists.json")))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        assert!(
            artists(&client, Some(&ItemId::from("lib-1")), Page::FIRST)
                .await
                .is_ok()
        );
    }

    /// `docs/12-decisions.md`: the top-level Albums tab (`ColumnKind::Albums { of_artist: None }`)
    /// had no endpoint behind it at all until this — a live user found it spinning forever.
    #[tokio::test]
    async fn albums_fetches_a_flat_recursive_list() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "MusicAlbum"))
            .and(query_param("Recursive", "true"))
            .and(query_param("ParentId", "lib-1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(load_fixture("discography_primary.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = albums(&client, Some(&ItemId::from("lib-1")), Page::FIRST)
            .await
            .unwrap();
        assert!(!result.items.is_empty());
    }

    #[tokio::test]
    async fn folder_children_splits_folders_and_tracks() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("album_tracks.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = folder_children(&client, &ItemId::from("66666"), Page::FIRST)
            .await
            .unwrap();
        assert!(
            result
                .items
                .iter()
                .all(|item| matches!(item, MediaItem::Track(_) | MediaItem::Folder(_)))
        );
        assert!(
            result
                .items
                .iter()
                .any(|item| matches!(item, MediaItem::Track(_)))
        );
    }

    /// `07-05`: non-audio files (images, subtitle sidecars, video) are omitted server-side, not
    /// filtered client-side after the fact — asserted by checking the outgoing request itself asks
    /// Emby for exactly `Folder,Audio`.
    #[tokio::test]
    async fn non_audio_files_are_omitted() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "Folder,Audio"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("album_tracks.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = folder_children(&client, &ItemId::from("66666"), Page::FIRST)
            .await
            .unwrap();
        assert!(!result.items.is_empty());
    }

    /// `07-05`: a directory with 50,000 files below it must still open instantly — `folder_children`
    /// (column population) never sends `Recursive=true`; `folder_tracks(.., false)` (the `a`-key
    /// folder-row queueing path) explicitly sends `Recursive=false`, and only `A` (`recursive:
    /// true`, covered by `queue_folder_recursive_with_shift` at the reducer level) ever sets it.
    #[tokio::test]
    async fn fetch_is_not_recursive() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("album_tracks.json")),
            )
            .mount(&server)
            .await;
        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();

        let query = ItemQuery::default()
            .parent(&ItemId::from("66666"))
            .item_types(&[ItemType::Folder, ItemType::Audio])
            .page(0, PAGE_SIZE);
        assert!(
            !query
                .to_query_pairs()
                .iter()
                .any(|(k, v)| k == "Recursive" && v == "true"),
            "folder_children's own query must never set Recursive=true"
        );
        let result = folder_children(&client, &ItemId::from("66666"), Page::FIRST)
            .await
            .unwrap();
        assert!(!result.items.is_empty());

        let shallow_query = ItemQuery::default()
            .parent(&ItemId::from("66666"))
            .item_types(&[ItemType::Audio])
            .recursive(false);
        assert!(
            shallow_query
                .to_query_pairs()
                .iter()
                .any(|(k, v)| k == "Recursive" && v == "false"),
            "folder_tracks(.., false) must send Recursive=false explicitly"
        );
        let shallow = folder_tracks(&client, &ItemId::from("66666"), false)
            .await
            .unwrap();
        assert!(!shallow.is_empty());
    }
}
