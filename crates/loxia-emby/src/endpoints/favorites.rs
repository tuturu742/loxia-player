//! Favourites browse and toggle.

use loxia_core::model::{Album, Artist, ItemId, Track};

use crate::client::{EmbyClient, parse_retry_after};
use crate::error::{EmbyError, classify_status, classify_transport};
use crate::query::{Filter, ItemQuery, ItemType, PAGE_SIZE, Page};

use super::fetch_items;
use super::items::items_path;
use super::search::SearchResults;

/// `Filters=IsFavorite` across every item type this app shows, split by `Type`.
///
/// Playlists are in the list because Emby favourites any item type and a user asked where theirs
/// had gone — verified against a live server, which records `UserData.IsFavorite` on a playlist and
/// returns it from this very query (`docs/12-decisions.md`).
pub async fn favorites(client: &EmbyClient, page: Page) -> Result<SearchResults, EmbyError> {
    let query = ItemQuery::default()
        .filters(&[Filter::IsFavorite])
        .item_types(&[
            ItemType::MusicArtist,
            ItemType::MusicAlbum,
            ItemType::Audio,
            ItemType::Playlist,
        ])
        .recursive(true)
        .page(page.start(), PAGE_SIZE);
    let response = fetch_items(client, &items_path(client), &query.to_query_pairs()).await?;

    let mut results = SearchResults::default();
    for dto in response.items {
        match dto.item_type.as_deref() {
            Some("MusicArtist") => results.artists.push(Artist::try_from(dto)?),
            Some("MusicAlbum") => results.albums.push(Album::try_from(dto)?),
            Some("Audio") => results.tracks.push(Track::try_from(dto)?),
            Some("Playlist") => results
                .playlists
                .push(loxia_core::model::Playlist::try_from(dto)?),
            _ => {}
        }
    }
    Ok(results)
}

/// `POST`/`DELETE /Users/{uid}/FavoriteItems/{id}`. A **mutation** — deliberately does not go
/// through `with_retry`. The reducer already applied the change optimistically and needs a prompt
/// failure to roll back, not a multi-second retry loop stalling the UI.
pub async fn set_favorite(client: &EmbyClient, id: &ItemId, on: bool) -> Result<(), EmbyError> {
    let path = format!("Users/{}/FavoriteItems/{id}", client.user_id());
    let request = if on {
        client.post(&path)
    } else {
        client.delete(&path)
    };
    let response = request.send().await.map_err(classify_transport)?;

    let status = response.status();
    if !status.is_success() {
        let retry_after = parse_retry_after(response.headers());
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(
            status.as_u16(),
            retry_after,
            Some(id.clone()),
            message,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wiremock::matchers::{method, path};
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

    #[tokio::test]
    async fn favorites_splits_by_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "TotalRecordCount": 3,
                "StartIndex": 0,
                "Items": [
                    {"Id": "a1", "Name": "An Artist", "Type": "MusicArtist"},
                    {"Id": "al1", "Name": "An Album", "Type": "MusicAlbum"},
                    {"Id": "t1", "Name": "A Track", "Type": "Audio"},
                ],
            })))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = favorites(&client, Page::FIRST).await.unwrap();
        assert_eq!(result.artists.len(), 1);
        assert_eq!(result.albums.len(), 1);
        assert_eq!(result.tracks.len(), 1);
    }

    #[tokio::test]
    async fn set_favorite_sends_post_and_delete() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/user-1/FavoriteItems/t1"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/emby/Users/user-1/FavoriteItems/t1"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        set_favorite(&client, &ItemId::from("t1"), true)
            .await
            .unwrap();
        set_favorite(&client, &ItemId::from("t1"), false)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn set_favorite_does_not_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/user-1/FavoriteItems/t1"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let err = set_favorite(&client, &ItemId::from("t1"), true)
            .await
            .unwrap_err();
        assert!(matches!(err, EmbyError::Transient { .. }));
    }
}
