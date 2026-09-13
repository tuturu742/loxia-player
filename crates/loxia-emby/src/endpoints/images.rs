//! Artwork URL building and fetch with fallback chain.

use std::time::Duration;

use bytes::Bytes;
use reqwest::{StatusCode, Url};

use loxia_core::model::{Album, Artist, ImageSize, ItemId, Track};

use crate::client::EmbyClient;
use crate::error::{EmbyError, classify_transport};

/// Shorter than the client's default 30s: artwork is decoration and must never hold up a UI
/// update.
const IMAGE_TIMEOUT: Duration = Duration::from_secs(10);

/// `GET /Items/{id}/Images/Primary?maxHeight={px}&tag={tag}&quality=90`.
pub fn image_url(client: &EmbyClient, id: &ItemId, tag: &str, size: ImageSize) -> Url {
    let mut url = client.url(&format!("Items/{id}/Images/Primary"));
    url.query_pairs_mut()
        .append_pair("maxHeight", &size.max_height().to_string())
        .append_pair("tag", tag)
        .append_pair("quality", "90");
    url
}

/// `{item_id}_{size}_{tag}.jpg` — the disk cache key `loxia-cache` (`08-02`) uses, defined here so
/// both sides agree. Including the tag is what makes a changed cover invalidate without a manual
/// purge.
pub fn cache_key(id: &ItemId, tag: &str, size: ImageSize) -> String {
    format!("{id}_{}_{tag}.jpg", size.cache_suffix())
}

/// A 404 is `Ok(Bytes::new())`, not an error — it means "try the next fallback candidate" (see
/// [`art_candidates`]). Only a transport failure (the request never got a response at all)
/// propagates as `Err`.
pub async fn fetch(
    client: &EmbyClient,
    id: &ItemId,
    tag: &str,
    size: ImageSize,
) -> Result<Bytes, EmbyError> {
    let url = image_url(client, id, tag, size);
    let response = client
        .get_url(url)
        .timeout(IMAGE_TIMEOUT)
        .send()
        .await
        .map_err(classify_transport)?;

    if response.status() == StatusCode::NOT_FOUND || !response.status().is_success() {
        return Ok(Bytes::new());
    }

    response.bytes().await.map_err(classify_transport)
}

/// The track → album → artist fallback chain, resolved by the caller with the ids it already
/// holds. Returns `(id, tag)` in priority order, skipping any of the three that has no artwork —
/// the caller tries each in turn and falls back to a themed placeholder if all fail.
///
/// The id returned is the one that **holds** the image, which is not always the item it belongs to
/// (see [`loxia_core::model::ImageRef`]).
pub fn art_candidates(
    track: &Track,
    album: Option<&Album>,
    artist: Option<&Artist>,
) -> Vec<(ItemId, String)> {
    [
        track.image.as_ref(),
        album.and_then(|a| a.image.as_ref()),
        artist.and_then(|a| a.image.as_ref()),
    ]
    .into_iter()
    .flatten()
    .map(|image| (image.item.clone(), image.tag.clone()))
    .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use loxia_core::config::ServerConfig;
    use loxia_core::model::{AlbumRelation, ImageRef};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    fn track(image_tag: Option<&str>) -> Track {
        Track {
            id: ItemId::from("t1"),
            name: "Song".to_string(),
            album_id: None,
            album_name: "Album".to_string(),
            album_artist_names: Vec::new(),
            artist_ids: Vec::new(),
            artist_names: Vec::new(),
            track_number: None,
            disc_number: None,
            year: None,
            duration: Duration::ZERO,
            genres: Vec::new(),
            is_favorite: false,
            play_count: 0,
            format: loxia_core::model::AudioFormat {
                codec: loxia_core::model::Codec::Mp3,
                sample_rate_hz: 44_100,
                bit_depth: None,
                channels: 2,
                bitrate_bps: None,
            },
            replay_gain: None,
            image: image_tag.map(|t| ImageRef {
                item: ItemId::from("t1"),
                tag: t.to_string(),
            }),
            media_source_id: None,
            lyric_stream: None,
            date_created: None,
            playlist_entry_id: None,
        }
    }

    fn album(image_tag: Option<&str>) -> Album {
        Album {
            id: ItemId::from("al1"),
            name: "Album".to_string(),
            sort_name: "Album".to_string(),
            album_artist_names: Vec::new(),
            album_artist_ids: Vec::new(),
            year: None,
            track_count: 0,
            total_duration: Duration::ZERO,
            genres: Vec::new(),
            is_favorite: false,
            image: image_tag.map(|t| ImageRef {
                item: ItemId::from("al1"),
                tag: t.to_string(),
            }),
            relation: AlbumRelation::Primary,
        }
    }

    fn artist(image_tag: Option<&str>) -> Artist {
        Artist {
            id: ItemId::from("ar1"),
            name: "Artist".to_string(),
            sort_name: "Artist".to_string(),
            album_count: 0,
            track_count: 0,
            genres: Vec::new(),
            is_favorite: false,
            image: image_tag.map(|t| ImageRef {
                item: ItemId::from("ar1"),
                tag: t.to_string(),
            }),
            overview: None,
        }
    }

    #[test]
    fn image_url_snapshot() {
        let client = EmbyClient::new(&cfg("http://192.168.1.1:8096")).unwrap();
        let thumb = image_url(&client, &ItemId::from("t1"), "tag-abc", ImageSize::Thumb);
        insta::assert_snapshot!("thumb", thumb.as_str());
        let large = image_url(&client, &ItemId::from("t1"), "tag-abc", ImageSize::Large);
        insta::assert_snapshot!("large", large.as_str());
    }

    #[test]
    fn cache_key_includes_tag() {
        let id = ItemId::from("t1");
        let a = cache_key(&id, "tag-a", ImageSize::Large);
        let b = cache_key(&id, "tag-b", ImageSize::Large);
        assert_ne!(a, b);
    }

    #[test]
    fn art_candidates_priority_order() {
        let candidates = art_candidates(
            &track(Some("track-tag")),
            Some(&album(Some("album-tag"))),
            Some(&artist(Some("artist-tag"))),
        );
        assert_eq!(
            candidates,
            vec![
                (ItemId::from("t1"), "track-tag".to_string()),
                (ItemId::from("al1"), "album-tag".to_string()),
                (ItemId::from("ar1"), "artist-tag".to_string()),
            ]
        );
    }

    #[test]
    fn art_candidates_skips_missing_tags() {
        let candidates = art_candidates(
            &track(None),
            Some(&album(Some("album-tag"))),
            Some(&artist(None)),
        );
        assert_eq!(
            candidates,
            vec![(ItemId::from("al1"), "album-tag".to_string())]
        );
    }

    #[test]
    fn art_candidates_empty_when_no_tags_anywhere() {
        let candidates = art_candidates(&track(None), Some(&album(None)), Some(&artist(None)));
        assert!(candidates.is_empty());
    }

    #[test]
    fn fetch_uses_short_timeout() {
        assert_eq!(IMAGE_TIMEOUT, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn fetch_404_is_not_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/Images/Primary"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let bytes = fetch(&client, &ItemId::from("t1"), "tag-abc", ImageSize::Large)
            .await
            .unwrap();
        assert!(bytes.is_empty());
    }

    #[tokio::test]
    async fn fetch_returns_bytes_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/Images/Primary"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1, 2, 3, 4]))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let bytes = fetch(&client, &ItemId::from("t1"), "tag-abc", ImageSize::Large)
            .await
            .unwrap();
        assert_eq!(&bytes[..], &[1, 2, 3, 4]);
    }
}
