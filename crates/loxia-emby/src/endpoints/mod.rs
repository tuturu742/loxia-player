//! Emby REST endpoint modules.

pub mod discography;
pub mod download;
pub mod favorites;
pub mod images;
pub mod instant_mix;
pub mod items;
pub mod lyrics;
pub mod playback;
pub mod playlists;
pub mod probe;
pub mod search;

use reqwest::RequestBuilder;

use loxia_core::model::ItemId;

use crate::client::{EmbyClient, parse_retry_after};
use crate::dto::envelope::ItemsResponse;
use crate::dto::item::BaseItemDto;
use crate::error::{EmbyError, classify_status, classify_transport};
use crate::query::{ItemQuery, PAGE_SIZE, Page};
use crate::retry::with_retry;

/// Shared GET + retry + decode for every `/Items`-shaped endpoint (`/Users/{uid}/Items`,
/// `/Artists`, `/Artists/AlbumArtists`, `/MusicGenres` all return the same envelope). `pairs` is
/// already fully built by the caller — this only owns the transport and error classification.
pub(crate) async fn fetch_items(
    client: &EmbyClient,
    path: &str,
    pairs: &[(String, String)],
) -> Result<ItemsResponse, EmbyError> {
    with_retry(|_attempt| async {
        let response = client
            .get(path)
            .query(pairs)
            .send()
            .await
            .map_err(classify_transport)?;

        let status = response.status();
        let retry_after = parse_retry_after(response.headers());
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(classify_status(status.as_u16(), retry_after, None, message));
        }

        let text = response.text().await.map_err(classify_transport)?;
        serde_json::from_str(&text).map_err(|source| EmbyError::Decode {
            endpoint: path.to_string(),
            source,
        })
    })
    .await
}

/// Pages `query` against `path` until `TotalRecordCount` is exhausted, converting every item
/// along the way. Used wherever a full collection must be diffed or queued client-side rather
/// than browsed page by page (discography's two album-level queries, `artist_tracks`).
pub(crate) async fn fetch_all_items<T>(
    client: &EmbyClient,
    path: &str,
    query: &ItemQuery,
) -> Result<Vec<T>, EmbyError>
where
    T: TryFrom<BaseItemDto, Error = EmbyError>,
{
    let mut items = Vec::new();
    let mut page = Page::FIRST;
    loop {
        let paged = query.clone().page(page.start(), PAGE_SIZE);
        let response = fetch_items(client, path, &paged.to_query_pairs()).await?;
        let total = response.total_record_count;
        let got = response.items.len();
        for dto in response.items {
            items.push(T::try_from(dto)?);
        }
        if items.len() >= total || got == 0 {
            break;
        }
        page = Page {
            index: page.index + 1,
        };
    }
    Ok(items)
}

/// A single un-paginated fetch, converting every item. For endpoints that always request their
/// own explicit `Limit` and never page further (search, instant mix).
pub(crate) async fn fetch_typed<T>(
    client: &EmbyClient,
    path: &str,
    pairs: &[(String, String)],
) -> Result<Vec<T>, EmbyError>
where
    T: TryFrom<BaseItemDto, Error = EmbyError>,
{
    let response = fetch_items(client, path, pairs).await?;
    response.items.into_iter().map(T::try_from).collect()
}

/// Sends an already-built mutation request (query params or JSON body already attached) and
/// classifies the result. Deliberately **not** wrapped in `with_retry`: every caller of this is a
/// mutation, and the reducer needs a prompt failure to roll back an optimistic update rather than
/// stalling behind a multi-second retry loop.
pub(crate) async fn send_mutation(
    request: RequestBuilder,
    item: Option<ItemId>,
) -> Result<(), EmbyError> {
    let response = request.send().await.map_err(classify_transport)?;
    let status = response.status();
    if !status.is_success() {
        let retry_after = parse_retry_after(response.headers());
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(status.as_u16(), retry_after, item, message));
    }
    Ok(())
}
