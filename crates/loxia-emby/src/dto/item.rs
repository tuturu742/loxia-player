//! BaseItemDto and its conversion to loxia_core::model types. This is the boundary where the
//! server's shape stops and ours begins — nothing above this layer should ever see a
//! `BaseItemDto`.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Deserialize;
use serde::de::Error as _;

use loxia_core::Timestamp;
use loxia_core::model::{
    Album, AlbumRelation, Artist, AudioFormat, Codec, Folder, Genre, ImageRef, ItemId,
    MediaSourceId, Playlist, Track,
};

use crate::error::EmbyError;

use super::media::MediaSourceDto;
use super::ticks::ticks_to_duration;

/// A `{ Name, Id }` pair as Emby represents `ArtistItems`/`AlbumArtists` entries. Left as raw
/// strings at the DTO layer — callers wrap `id` into an `ItemId` at the conversion boundary.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NameIdPair {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

/// Emby's `GenreItems` entries. `Id` is deliberately not declared here: in live captures
/// (`02-01`) it came back as a bare JSON number (e.g. `"Id": 66614`) rather than the string every
/// other id field uses, and nothing downstream needs it — `Track`/`Album` genre lists are built
/// from the plain `Genres` string array instead. See `12-decisions.md`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GenreItemDto {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserDataDto {
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub play_count: u32,
}

/// Mirrors the subset of Emby's `BaseItemDto` this app uses. Every field is `Option`/has
/// `#[serde(default)]` — Emby omits fields freely, so a missing field is normal, not an error.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDto {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub sort_name: Option<String>,
    #[serde(rename = "Type", default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub is_folder: bool,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub album_id: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub album_artist: Option<String>,
    #[serde(default)]
    pub album_artists: Vec<NameIdPair>,
    #[serde(default)]
    pub artist_items: Vec<NameIdPair>,
    #[serde(default)]
    pub artists: Vec<String>,
    #[serde(default)]
    pub index_number: Option<u32>,
    #[serde(default)]
    pub parent_index_number: Option<u32>,
    #[serde(default)]
    pub production_year: Option<u16>,
    #[serde(default)]
    pub premiere_date: Option<String>,
    #[serde(default)]
    pub run_time_ticks: Option<i64>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub genre_items: Vec<GenreItemDto>,
    #[serde(default)]
    pub user_data: Option<UserDataDto>,
    #[serde(default)]
    pub media_sources: Vec<MediaSourceDto>,
    #[serde(default)]
    pub image_tags: BTreeMap<String, String>,
    #[serde(default)]
    pub child_count: Option<u32>,
    /// Emby's "my primary image actually lives on that item" pointer, set when `image_tags` has no
    /// `Primary` of its own. Both halves are needed together — see [`image_ref`].
    #[serde(default)]
    pub primary_image_item_id: Option<String>,
    #[serde(default)]
    pub primary_image_tag: Option<String>,
    #[serde(default)]
    pub date_created: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    /// Not in the confirmed field set (`03-emby-api.md` §3) — no live fixture captured a bare
    /// Playlist-type item during `02-01`. Needed for `Playlist::can_edit`; verify the field name
    /// against a real server response when `02-08` lists/creates playlists.
    #[serde(default)]
    pub can_edit: Option<bool>,
    /// Confirmed live (`02-01` row 9, `playlist_items.json`) — Emby's per-row playlist entry id.
    /// Only present on items returned from `GET /Playlists/{id}/Items`; `02-08` reads it directly
    /// rather than through a `Track` conversion, since `Track` has no reason to carry it.
    #[serde(default)]
    pub playlist_item_id: Option<String>,
}

pub(crate) fn missing_field_error(dto: &BaseItemDto, field: &str) -> EmbyError {
    let context = dto.item_type.as_deref().unwrap_or("unknown");
    EmbyError::Decode {
        endpoint: format!("BaseItemDto({context})"),
        source: serde_json::Error::custom(format!("missing required field: {field}")),
    }
}

fn require_id(dto: &BaseItemDto) -> Result<String, EmbyError> {
    dto.id.clone().ok_or_else(|| missing_field_error(dto, "Id"))
}

fn require_name(dto: &BaseItemDto) -> Result<String, EmbyError> {
    dto.name
        .clone()
        .ok_or_else(|| missing_field_error(dto, "Name"))
}

/// The item's own `ImageTags.Primary` when it has one, else Emby's
/// `PrimaryImageItemId`/`PrimaryImageTag` pointer at whichever item does.
///
/// The fallback is not an edge case: on a real library only 81 of 300 albums carry their own tag
/// while 195 carry the pointer, so ignoring it left most albums showing a placeholder — while
/// tracks, 287 of 300 of which have their own tag, looked fine. Verified against the live server,
/// including that the pointed-at URL actually serves the image (`docs/12-decisions.md`).
fn image_ref(dto: &BaseItemDto) -> Option<ImageRef> {
    if let Some(tag) = dto.image_tags.get("Primary") {
        let id = dto.id.clone()?;
        return Some(ImageRef {
            item: ItemId::from(id),
            tag: tag.clone(),
        });
    }
    Some(ImageRef {
        item: ItemId::from(dto.primary_image_item_id.clone()?),
        tag: dto.primary_image_tag.clone()?,
    })
}

fn album_artist_names(dto: &BaseItemDto) -> Vec<String> {
    if !dto.album_artists.is_empty() {
        dto.album_artists.iter().map(|p| p.name.clone()).collect()
    } else if let Some(name) = &dto.album_artist {
        vec![name.clone()]
    } else {
        Vec::new()
    }
}

fn audio_format(media_sources: &[MediaSourceDto]) -> AudioFormat {
    let stream = media_sources
        .iter()
        .flat_map(|ms| ms.media_streams.iter())
        .find(|s| s.stream_type.as_deref() == Some("Audio"));

    match stream {
        Some(s) => AudioFormat {
            codec: s
                .codec
                .as_deref()
                .map(Codec::from_str_lossy)
                .unwrap_or(Codec::Other(String::new())),
            sample_rate_hz: s.sample_rate.unwrap_or(0),
            bit_depth: s.bit_depth,
            channels: s.channels.unwrap_or(0),
            bitrate_bps: s.bit_rate,
        },
        None => AudioFormat {
            codec: Codec::Other(String::new()),
            sample_rate_hz: 0,
            bit_depth: None,
            channels: 0,
            bitrate_bps: None,
        },
    }
}

impl TryFrom<BaseItemDto> for Artist {
    type Error = EmbyError;

    fn try_from(dto: BaseItemDto) -> Result<Self, Self::Error> {
        let id = require_id(&dto)?;
        let name = require_name(&dto)?;
        let user_data = dto.user_data.clone().unwrap_or_default();
        Ok(Artist {
            id: ItemId::from(id),
            sort_name: dto.sort_name.clone().unwrap_or_else(|| name.clone()),
            name,
            // Emby reports **no album count** for an artist item, so this stays unknown until a
            // discography fetch (`02-06`) fills it in from its own two album queries.
            album_count: 0,
            // `ChildCount` *is* populated — it was only ever missing because this conversion threw
            // it away, which is why the Artists column read `0 albums` for every artist in the
            // library. It counts the artist's **tracks**, not albums: verified against the live
            // server, where an artist with 28 albums reports 289 and has 288 tracks
            // (`docs/12-decisions.md`). `FieldSet::default()` has always requested it.
            track_count: dto.child_count.unwrap_or(0),
            genres: dto.genres.clone(),
            is_favorite: user_data.is_favorite,
            image: image_ref(&dto),
            overview: dto.overview.clone(),
        })
    }
}

impl TryFrom<BaseItemDto> for Album {
    type Error = EmbyError;

    fn try_from(dto: BaseItemDto) -> Result<Self, Self::Error> {
        let id = require_id(&dto)?;
        let name = require_name(&dto)?;
        let user_data = dto.user_data.clone().unwrap_or_default();
        let album_artist_ids = dto
            .album_artists
            .iter()
            .map(|p| ItemId::from(p.id.clone()))
            .collect();
        Ok(Album {
            id: ItemId::from(id),
            sort_name: dto.sort_name.clone().unwrap_or_else(|| name.clone()),
            album_artist_names: album_artist_names(&dto),
            album_artist_ids,
            year: dto.production_year,
            track_count: dto.child_count.unwrap_or(0),
            total_duration: dto
                .run_time_ticks
                .map(ticks_to_duration)
                .unwrap_or(Duration::ZERO),
            genres: dto.genres.clone(),
            is_favorite: user_data.is_favorite,
            image: image_ref(&dto),
            // Only `02-06`'s discography algorithm has the context to know whether an album is a
            // primary release or a compilation appearance — a bare conversion always says Primary.
            relation: AlbumRelation::Primary,
            name,
        })
    }
}

impl TryFrom<BaseItemDto> for Track {
    type Error = EmbyError;

    fn try_from(dto: BaseItemDto) -> Result<Self, Self::Error> {
        let id = require_id(&dto)?;
        let name = require_name(&dto)?;
        let user_data = dto.user_data.clone().unwrap_or_default();
        let format = audio_format(&dto.media_sources);
        let media_source_id = dto
            .media_sources
            .first()
            .and_then(|m| m.id.clone())
            .map(MediaSourceId::from);
        let date_created = dto
            .date_created
            .as_deref()
            .and_then(|s| s.parse::<Timestamp>().ok());
        let artist_ids = dto
            .artist_items
            .iter()
            .map(|p| ItemId::from(p.id.clone()))
            .collect();

        Ok(Track {
            id: ItemId::from(id),
            album_id: dto.album_id.clone().map(ItemId::from),
            album_name: dto.album.clone().unwrap_or_default(),
            album_artist_names: album_artist_names(&dto),
            // Never parsed out of the `Artists` display strings — `ArtistItems` is the only
            // source, since it is what the appears-on queue filter tests against.
            artist_ids,
            artist_names: dto.artists.clone(),
            track_number: dto.index_number,
            disc_number: dto.parent_index_number,
            year: dto.production_year,
            duration: dto
                .run_time_ticks
                .map(ticks_to_duration)
                .unwrap_or(Duration::ZERO),
            genres: dto.genres.clone(),
            is_favorite: user_data.is_favorite,
            play_count: user_data.play_count,
            format,
            // No live fixture (`02-01`) surfaced a ReplayGain tag via ItemsService — the
            // NormalizationGain field stays on MediaSourceDto for 09-04's own PlaybackInfo fetch.
            replay_gain: None,
            image: image_ref(&dto),
            media_source_id,
            // Emby exposes `.lrc` sidecars as a Subtitle-type stream on the item's own
            // `MediaSources`, which the default `Fields` set already requests — so this is
            // discovered right here, from data already in hand. It used to be hardcoded `None`
            // ("discovered lazily"), and the only code that ever *did* discover it
            // (`endpoints::playback::playback_info`) is never called for a queued track — so
            // `lyric_stream` was always `None`, `load_current` never emitted `FetchLyrics`, and the
            // lyrics pane could never appear at all (`docs/12-decisions.md`).
            lyric_stream: dto
                .media_sources
                .iter()
                .find_map(crate::endpoints::lyrics::find_lyric_stream),
            date_created,
            name,
            // Only `playlists::items()` (07-03) knows this — it's stamped on afterwards, since
            // the generic item DTO carries no `PlaylistItemId` field at all outside that one
            // endpoint's own response shape.
            playlist_entry_id: None,
        })
    }
}

impl TryFrom<BaseItemDto> for Genre {
    type Error = EmbyError;

    fn try_from(dto: BaseItemDto) -> Result<Self, Self::Error> {
        Ok(Genre {
            id: ItemId::from(require_id(&dto)?),
            name: require_name(&dto)?,
        })
    }
}

impl TryFrom<BaseItemDto> for Folder {
    type Error = EmbyError;

    fn try_from(dto: BaseItemDto) -> Result<Self, Self::Error> {
        Ok(Folder {
            id: ItemId::from(require_id(&dto)?),
            name: require_name(&dto)?,
        })
    }
}

impl TryFrom<BaseItemDto> for Playlist {
    type Error = EmbyError;

    fn try_from(dto: BaseItemDto) -> Result<Self, Self::Error> {
        let id = require_id(&dto)?;
        let name = require_name(&dto)?;
        Ok(Playlist {
            id: ItemId::from(id),
            overview: dto.overview.clone(),
            track_count: dto.child_count.unwrap_or(0),
            total_duration: dto
                .run_time_ticks
                .map(ticks_to_duration)
                .unwrap_or(Duration::ZERO),
            can_edit: dto.can_edit.unwrap_or(false),
            is_favorite: dto.user_data.as_ref().is_some_and(|u| u.is_favorite),
            name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture(name: &str) -> super::super::envelope::ItemsResponse {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// A real gap: `lyric_stream` was hardcoded `None`, so `load_current` never emitted
    /// `FetchLyrics` and the lyrics pane could never appear. It is discovered here from the
    /// `MediaSources` the default `Fields` set already requests (`docs/12-decisions.md`).
    #[test]
    fn a_subtitle_stream_becomes_the_tracks_lyric_stream() {
        let dto: BaseItemDto = serde_json::from_value(serde_json::json!({
            "Id": "t1",
            "Name": "Motion",
            "MediaSources": [{
                "Id": "src-1",
                "MediaStreams": [
                    { "Type": "Audio", "Index": 0, "Codec": "flac" },
                    { "Type": "Subtitle", "Index": 1, "Codec": "lrc" },
                ],
            }],
        }))
        .unwrap();

        let track = Track::try_from(dto).unwrap();
        let stream = track
            .lyric_stream
            .expect("an .lrc subtitle stream must yield a lyric stream");
        assert_eq!(stream.media_source_id.as_str(), "src-1");
        assert_eq!(stream.stream_index, 1);
        assert_eq!(stream.format, loxia_core::model::LyricFormat::Lrc);
    }

    #[test]
    fn a_track_with_no_subtitle_stream_has_no_lyric_stream() {
        let dto: BaseItemDto = serde_json::from_value(serde_json::json!({
            "Id": "t1",
            "Name": "Motion",
            "MediaSources": [{
                "Id": "src-1",
                "MediaStreams": [{ "Type": "Audio", "Index": 0, "Codec": "flac" }],
            }],
        }))
        .unwrap();
        assert!(Track::try_from(dto).unwrap().lyric_stream.is_none());
    }

    #[test]
    fn parses_artists_fixture() {
        let response = load_fixture("artists.json");
        assert!(!response.items.is_empty());
        let artist = Artist::try_from(response.items.into_iter().next().unwrap()).unwrap();
        assert_eq!(artist.id.as_str(), "127674");
        assert_eq!(artist.name, ":Waijdan:");
        assert!(!artist.is_favorite);
    }

    /// The conversion used to hardcode `track_count: 0` on the belief that Emby never populates
    /// `ChildCount` for an artist. It does — `FieldSet::default()` asks for it and the server
    /// sends it — so every artist in the library rendered as `0 albums`. The fixture was captured
    /// before the field was noticed and has since been given the counts the live server really
    /// returns for these two artists (`docs/12-decisions.md`).
    #[test]
    fn artist_track_count_comes_from_child_count() {
        let response = load_fixture("artists.json");
        let counts: Vec<(String, u32)> = response
            .items
            .into_iter()
            .filter_map(|dto| Artist::try_from(dto).ok())
            .map(|a| (a.name, a.track_count))
            .take(2)
            .collect();

        assert_eq!(
            counts,
            vec![
                (":Waijdan:".to_string(), 3),
                (":Wumpscut:".to_string(), 223)
            ]
        );
    }

    /// Most albums hold no image of their own: Emby answers with an empty `ImageTags` and a
    /// `PrimaryImageItemId`/`PrimaryImageTag` naming the item that does. Reading only `ImageTags`
    /// left two thirds of a real library's albums showing a placeholder while tracks — which
    /// nearly always carry their own tag — looked fine (`docs/12-decisions.md`). The shape here is
    /// copied from an actual response.
    #[test]
    fn an_album_inherits_its_cover_from_the_item_that_holds_it() {
        let dto: BaseItemDto = serde_json::from_value(serde_json::json!({
            "Id": "17105",
            "Name": "An Album",
            "Type": "MusicAlbum",
            "ImageTags": {},
            "PrimaryImageItemId": "17106",
            "PrimaryImageTag": "851144fe94896988cdbb0618e994be3b",
        }))
        .unwrap();

        let album = Album::try_from(dto).unwrap();
        let image = album.image.expect("the inherited cover must be found");
        assert_eq!(
            image.item,
            ItemId::from("17106"),
            "the URL must address the item holding the image, not the album"
        );
        assert_eq!(image.tag, "851144fe94896988cdbb0618e994be3b");
    }

    /// An item with its own tag keeps addressing itself — the pointer is a fallback, not a
    /// replacement.
    #[test]
    fn an_own_image_tag_still_wins() {
        let dto: BaseItemDto = serde_json::from_value(serde_json::json!({
            "Id": "17105",
            "Name": "An Album",
            "Type": "MusicAlbum",
            "ImageTags": { "Primary": "own-tag" },
            "PrimaryImageItemId": "17106",
            "PrimaryImageTag": "inherited-tag",
        }))
        .unwrap();

        let image = Album::try_from(dto).unwrap().image.expect("has an image");
        assert_eq!(image.item, ItemId::from("17105"));
        assert_eq!(image.tag, "own-tag");
    }

    /// Neither source: no artwork, and the pane draws its placeholder rather than a broken URL.
    #[test]
    fn no_image_anywhere_is_none() {
        let dto: BaseItemDto = serde_json::from_value(serde_json::json!({
            "Id": "17105",
            "Name": "An Album",
            "Type": "MusicAlbum",
            "ImageTags": {},
        }))
        .unwrap();
        assert!(Album::try_from(dto).unwrap().image.is_none());
    }

    /// A half-populated pointer is not usable — both halves are required.
    #[test]
    fn a_partial_pointer_is_ignored() {
        for partial in [
            serde_json::json!({ "PrimaryImageItemId": "17106" }),
            serde_json::json!({ "PrimaryImageTag": "some-tag" }),
        ] {
            let mut value = serde_json::json!({
                "Id": "17105", "Name": "An Album", "Type": "MusicAlbum", "ImageTags": {},
            });
            for (k, v) in partial.as_object().unwrap() {
                value[k] = v.clone();
            }
            let dto: BaseItemDto = serde_json::from_value(value).unwrap();
            assert!(Album::try_from(dto).unwrap().image.is_none(), "{partial}");
        }
    }

    /// The album count genuinely is not reported for an artist item, and must stay at its
    /// "unknown" zero rather than borrowing `ChildCount` — which counts tracks, not albums.
    #[test]
    fn artist_album_count_stays_unknown() {
        let response = load_fixture("artists.json");
        let artist = Artist::try_from(response.items.into_iter().next().unwrap()).unwrap();
        assert_eq!(artist.album_count, 0);
    }

    #[test]
    fn parses_album_tracks_fixture() {
        let response = load_fixture("album_tracks.json");
        let track = Track::try_from(response.items.into_iter().next().unwrap()).unwrap();
        assert_eq!(track.name, "Comfortable Void");
        assert_eq!(track.artist_ids, vec![ItemId::from("66665")]);
        assert_eq!(track.artist_names, vec!["Sync24".to_string()]);
        assert_eq!(track.album_id.as_ref().unwrap().as_str(), "66666");
        assert_eq!(track.album_artist_names, vec!["Sync24".to_string()]);
        assert_eq!(track.track_number, Some(1));
        assert_eq!(track.format.codec, Codec::Mp3);
        assert_eq!(track.format.sample_rate_hz, 44_100);
        assert_eq!(track.format.channels, 2);
        assert_eq!(
            track.media_source_id.as_ref().unwrap().as_str(),
            "mediasource_1517"
        );
    }

    #[test]
    fn parses_favorites_fixture() {
        let response = load_fixture("favorites.json");
        assert_eq!(response.total_record_count, 0);
        assert!(response.items.is_empty());
    }

    #[test]
    fn handles_empty_library_envelope() {
        let response = load_fixture("empty_library.json");
        assert_eq!(response.total_record_count, 0);
        assert!(response.items.is_empty());
    }

    #[test]
    fn missing_optional_fields_do_not_error() {
        let dto: BaseItemDto = serde_json::from_str(r#"{"Id":"x","Name":"y"}"#).unwrap();
        assert!(Artist::try_from(dto.clone()).is_ok());
        assert!(Album::try_from(dto.clone()).is_ok());
        assert!(Track::try_from(dto.clone()).is_ok());
        assert!(Genre::try_from(dto.clone()).is_ok());
        assert!(Folder::try_from(dto.clone()).is_ok());
        assert!(Playlist::try_from(dto).is_ok());
    }

    #[test]
    fn missing_id_is_a_decode_error() {
        let dto: BaseItemDto = serde_json::from_str(r#"{"Name":"y"}"#).unwrap();
        let err = Artist::try_from(dto).unwrap_err();
        assert!(matches!(err, EmbyError::Decode { .. }));
    }

    #[test]
    fn track_artist_ids_come_from_artist_items() {
        let dto: BaseItemDto = serde_json::from_str(
            r#"{
                "Id": "t1", "Name": "Song",
                "ArtistItems": [{"Id": "a1", "Name": "Real Artist"}],
                "Artists": ["A Totally Different Display String"]
            }"#,
        )
        .unwrap();
        let track = Track::try_from(dto).unwrap();
        assert_eq!(track.artist_ids, vec![ItemId::from("a1")]);
    }

    #[test]
    fn unknown_codec_maps_to_other() {
        let dto: BaseItemDto = serde_json::from_str(
            r#"{
                "Id": "t1", "Name": "Song",
                "MediaSources": [{"MediaStreams": [{"Type": "Audio", "Codec": "dts"}]}]
            }"#,
        )
        .unwrap();
        let track = Track::try_from(dto).unwrap();
        assert_eq!(track.format.codec, Codec::Other("dts".to_string()));
    }

    #[test]
    fn album_relation_defaults_to_primary() {
        let dto: BaseItemDto = serde_json::from_str(r#"{"Id":"al1","Name":"An Album"}"#).unwrap();
        let album = Album::try_from(dto).unwrap();
        assert_eq!(album.relation, AlbumRelation::Primary);
    }
}
