//! Playlist CRUD. Removal needs Emby's per-row playlist entry id (`PlaylistItemId`), not the
//! track's own item id — the same track can appear twice in a playlist.

use serde::Deserialize;
use serde_json::Value;

use loxia_core::model::{ItemId, Playlist, PlaylistEntryId, PlaylistId, Track};

use crate::client::{EmbyClient, parse_retry_after};
use crate::dto::item::missing_field_error;
use crate::error::{EmbyError, classify_status, classify_transport};
use crate::query::{ItemQuery, ItemType, PAGE_SIZE, Page, Paged};

use super::items::items_path;
use super::{fetch_items, send_mutation};

const CHUNK_SIZE: usize = 100;

pub struct PlaylistEntry {
    pub entry_id: PlaylistEntryId,
    pub track: Track,
}

/// `GET /Users/{uid}/Items?IncludeItemTypes=Playlist&Recursive=true`.
///
/// **No `MediaTypes` filter.** One was added to keep video and photo playlists out of the tab, and
/// it passed against a mock that asserted the parameter was *sent* — but a real Emby answers
/// `IncludeItemTypes=Playlist&MediaTypes=Audio` by ignoring the type filter entirely and returning
/// audio-ish items of every kind: on a live library, 106,115 folders, albums and tracks in place of
/// 3 playlists (`docs/12-decisions.md`). Dropping it returns exactly what the official client
/// shows.
///
/// Non-music playlists are therefore listed again. They are handled where the problem actually is
/// instead: [`items`] skips entries that aren't audio, so opening or queueing one yields no tracks
/// rather than failing outright.
pub async fn list(client: &EmbyClient, page: Page) -> Result<Paged<Playlist>, EmbyError> {
    let query = ItemQuery::default()
        .item_types(&[ItemType::Playlist])
        .recursive(true)
        .page(page.start(), PAGE_SIZE);
    let response = fetch_items(client, &items_path(client), &query.to_query_pairs()).await?;
    let items = response
        .items
        .into_iter()
        .map(Playlist::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Paged {
        items,
        total: response.total_record_count,
        start: response.start_index,
    })
}

/// `GET /Playlists/{id}/Items?UserId={uid}`. Captures `PlaylistItemId` per row — without it
/// `remove` cannot target the correct row when a playlist holds the same track twice.
pub async fn items(client: &EmbyClient, id: &PlaylistId) -> Result<Vec<PlaylistEntry>, EmbyError> {
    let path = format!("Playlists/{id}/Items");
    let pairs = vec![("UserId".to_string(), client.user_id().to_string())];
    let response = fetch_items(client, &path, &pairs).await?;
    response
        .items
        .into_iter()
        .filter(|dto| {
            // A playlist can hold video, photos or books. Those have no `Track` representation, and
            // converting one used to fail the whole fetch — so a single stray entry made an
            // otherwise-good playlist unopenable. Skipped instead: a playlist of them simply comes
            // back empty (`docs/12-decisions.md`).
            dto.item_type.as_deref() == Some("Audio")
        })
        .map(|dto| {
            let entry_id = dto
                .playlist_item_id
                .clone()
                .ok_or_else(|| missing_field_error(&dto, "PlaylistItemId"))?;
            let track = Track::try_from(dto)?;
            Ok(PlaylistEntry {
                entry_id: PlaylistEntryId::from(entry_id),
                track,
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CreateResponse {
    id: String,
}

/// `POST /Playlists?Name={name}&Ids={csv}&UserId={uid}&MediaType=Audio` — query parameters, not a
/// JSON body (verified live, `12-decisions.md` §10 item 2). `tracks` may be empty: an empty
/// playlist is legitimate (`ItemAddedCount: 0`, no error).
pub async fn create(
    client: &EmbyClient,
    name: &str,
    tracks: &[ItemId],
) -> Result<PlaylistId, EmbyError> {
    let ids_csv = tracks
        .iter()
        .map(ItemId::as_str)
        .collect::<Vec<_>>()
        .join(",");
    let pairs = [
        ("Name".to_string(), name.to_string()),
        ("Ids".to_string(), ids_csv),
        ("UserId".to_string(), client.user_id().to_string()),
        ("MediaType".to_string(), "Audio".to_string()),
    ];

    let response = client
        .post("Playlists")
        .query(&pairs)
        .send()
        .await
        .map_err(classify_transport)?;
    let status = response.status();
    if !status.is_success() {
        let retry_after = parse_retry_after(response.headers());
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(status.as_u16(), retry_after, None, message));
    }

    let text = response.text().await.map_err(classify_transport)?;
    let parsed: CreateResponse =
        serde_json::from_str(&text).map_err(|source| EmbyError::Decode {
            endpoint: "Playlists".to_string(),
            source,
        })?;
    Ok(PlaylistId::from(parsed.id))
}

/// `Overview` cannot be set at creation (verified live: silently fails to persist). This is the
/// separate second step: `GET` the full item DTO, set `Overview` on it, `POST` it back whole —
/// using a raw `serde_json::Value` roundtrip so every other field on the item survives untouched.
pub async fn set_overview(
    client: &EmbyClient,
    id: &PlaylistId,
    overview: &str,
) -> Result<(), EmbyError> {
    let get_path = format!("Users/{}/Items/{id}", client.user_id());
    let response = client
        .get(&get_path)
        .send()
        .await
        .map_err(classify_transport)?;
    let status = response.status();
    if !status.is_success() {
        let retry_after = parse_retry_after(response.headers());
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(status.as_u16(), retry_after, None, message));
    }
    let text = response.text().await.map_err(classify_transport)?;
    let mut value: Value = serde_json::from_str(&text).map_err(|source| EmbyError::Decode {
        endpoint: get_path,
        source,
    })?;
    value["Overview"] = Value::String(overview.to_string());

    let post_path = format!("Items/{id}");
    send_mutation(client.post(&post_path).json(&value), None).await
}

/// `POST /Playlists/{id}/Items?Ids={csv}&UserId={uid}`, chunked at 100 ids per request. Chunks run
/// sequentially so a partial failure leaves a knowable state.
pub async fn add(client: &EmbyClient, id: &PlaylistId, tracks: &[ItemId]) -> Result<(), EmbyError> {
    let path = format!("Playlists/{id}/Items");
    for chunk in tracks.chunks(CHUNK_SIZE) {
        let csv = chunk
            .iter()
            .map(ItemId::as_str)
            .collect::<Vec<_>>()
            .join(",");
        let pairs = [
            ("Ids".to_string(), csv),
            ("UserId".to_string(), client.user_id().to_string()),
        ];
        send_mutation(client.post(&path).query(&pairs), None).await?;
    }
    Ok(())
}

/// `DELETE /Playlists/{id}/Items?EntryIds={csv}`, chunked at 100 ids per request.
pub async fn remove(
    client: &EmbyClient,
    id: &PlaylistId,
    entries: &[PlaylistEntryId],
) -> Result<(), EmbyError> {
    let path = format!("Playlists/{id}/Items");
    for chunk in entries.chunks(CHUNK_SIZE) {
        let csv = chunk
            .iter()
            .map(PlaylistEntryId::as_str)
            .collect::<Vec<_>>()
            .join(",");
        let pairs = [("EntryIds".to_string(), csv)];
        send_mutation(client.delete(&path).query(&pairs), None).await?;
    }
    Ok(())
}

/// `POST /Playlists/{id}/Items/{itemId}/Move/{newIndex}`.
pub async fn move_item(
    client: &EmbyClient,
    id: &PlaylistId,
    item: &ItemId,
    new_index: usize,
) -> Result<(), EmbyError> {
    let path = format!("Playlists/{id}/Items/{item}/Move/{new_index}");
    send_mutation(client.post(&path), Some(item.clone())).await
}

/// `DELETE /Items/{id}` — playlists are deleted through the generic item route, not a
/// playlist-specific one.
pub async fn delete(client: &EmbyClient, id: &PlaylistId) -> Result<(), EmbyError> {
    let path = format!("Items/{id}");
    send_mutation(client.delete(&path), None).await
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

    /// The `MediaTypes=Audio` filter this used to send made a real Emby ignore the type filter
    /// altogether and return 106,115 folders/albums/tracks in place of 3 playlists — while this
    /// very test passed, because a mock that asserts a parameter was *sent* says nothing about what
    /// a server does with it (`docs/12-decisions.md`). It must not come back.
    #[tokio::test]
    async fn list_asks_only_for_playlists() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items"))
            .and(query_param("IncludeItemTypes", "Playlist"))
            .and(wiremock::matchers::query_param_is_missing("MediaTypes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "TotalRecordCount": 1,
                "StartIndex": 0,
                "Items": [{"Id": "pl1", "Name": "Chill"}],
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let result = list(&client, Page::FIRST).await.unwrap();
        assert_eq!(result.items.len(), 1);
    }

    #[tokio::test]
    async fn items_captures_entry_ids() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Playlists/pl1/Items"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("playlist_items.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let entries = items(&client, &PlaylistId::from("pl1")).await.unwrap();
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| !e.entry_id.as_str().is_empty()));
    }

    #[tokio::test]
    async fn items_distinguishes_same_track_twice() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Playlists/pl1/Items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "TotalRecordCount": 2,
                "StartIndex": 0,
                "Items": [
                    {"Id": "t1", "Name": "Repeat", "Type": "Audio", "PlaylistItemId": "row-1"},
                    {"Id": "t1", "Name": "Repeat", "Type": "Audio", "PlaylistItemId": "row-2"},
                ],
            })))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let entries = items(&client, &PlaylistId::from("pl1")).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].entry_id.as_str(), entries[1].entry_id.as_str());
        assert_eq!(entries[0].track.id, entries[1].track.id);
    }

    /// `list` no longer filters by media type, so video and photo playlists are listed again.
    /// Opening one must yield an empty track list, not the hard error that converting a video entry
    /// into a `Track` used to produce — one stray entry would otherwise sink an entire playlist.
    #[tokio::test]
    async fn items_skips_entries_that_are_not_audio() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Playlists/pl1/Items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "TotalRecordCount": 3,
                "StartIndex": 0,
                "Items": [
                    {"Id": "v1", "Name": "Trailer", "Type": "Movie", "PlaylistItemId": "row-1"},
                    {"Id": "t1", "Name": "Song", "Type": "Audio", "PlaylistItemId": "row-2"},
                    {"Id": "p1", "Name": "Snapshot", "Type": "Photo", "PlaylistItemId": "row-3"},
                ],
            })))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let entries = items(&client, &PlaylistId::from("pl1")).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].track.name, "Song");
    }

    #[tokio::test]
    async fn create_sends_query_params_not_json_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists"))
            .and(query_param("Name", "My Mix"))
            .and(query_param("Ids", "t1,t2"))
            .and(query_param("UserId", "user-1"))
            .and(query_param("MediaType", "Audio"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"Id": "pl-new", "ItemAddedCount": 2})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let id = create(&client, "My Mix", &[ItemId::from("t1"), ItemId::from("t2")])
            .await
            .unwrap();
        assert_eq!(id.as_str(), "pl-new");
    }

    #[tokio::test]
    async fn create_with_no_tracks_is_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists"))
            .and(query_param("Ids", ""))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"Id": "pl-empty", "ItemAddedCount": 0})),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let id = create(&client, "Empty", &[]).await.unwrap();
        assert_eq!(id.as_str(), "pl-empty");
    }

    #[tokio::test]
    async fn set_overview_reads_then_posts_full_dto() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1/Items/pl1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Id": "pl1", "Name": "My Mix", "Overview": null, "SomeOtherField": "keep-me",
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/emby/Items/pl1"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        set_overview(&client, &PlaylistId::from("pl1"), "A description")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn add_chunks_at_100() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists/pl1/Items"))
            .respond_with(ResponseTemplate::new(200))
            .expect(2)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let tracks: Vec<ItemId> = (0..150).map(|i| ItemId::from(format!("t{i}"))).collect();
        add(&client, &PlaylistId::from("pl1"), &tracks)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn remove_sends_entry_ids_not_item_ids() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/emby/Playlists/pl1/Items"))
            .and(query_param("EntryIds", "row-1,row-2"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        remove(
            &client,
            &PlaylistId::from("pl1"),
            &[
                PlaylistEntryId::from("row-1"),
                PlaylistEntryId::from("row-2"),
            ],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn mutations_do_not_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists/pl1/Items"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let err = add(&client, &PlaylistId::from("pl1"), &[ItemId::from("t1")])
            .await
            .unwrap_err();
        assert!(matches!(err, EmbyError::Transient { .. }));
    }

    #[tokio::test]
    async fn delete_uses_items_route() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/emby/Items/pl1"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        delete(&client, &PlaylistId::from("pl1")).await.unwrap();
    }

    #[tokio::test]
    async fn move_uses_path_index() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Playlists/pl1/Items/t1/Move/3"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        move_item(&client, &PlaylistId::from("pl1"), &ItemId::from("t1"), 3)
            .await
            .unwrap();
    }
}
