# 02-04 · DTOs and model conversion

**Phase:** 02 — Emby client · **Agent:** B · **Size:** L
**Prerequisites:** `02-03`, `01-07`
**Reference:** `docs/03-emby-api.md` §3, `docs/02-data-model.md` §2

## Goal
Mirror Emby's JSON as DTOs and convert them into `loxia-core` domain types. This is the boundary
where the server's shape stops and ours begins; nothing above this layer should ever see a
`BaseItemDto`.

## Files
- `crates/loxia-emby/src/dto/mod.rs`, `item.rs`, `envelope.rs`, `media.rs`, `ticks.rs`

## Specification

**Envelope:**
```
pub struct ItemsResponse { pub items: Vec<BaseItemDto>, pub total_record_count: usize, pub start_index: usize }
```
`#[serde(rename_all = "PascalCase")]` on every DTO. Emby omits fields freely, so **every** DTO field
is `Option<T>` or has `#[serde(default)]`. A missing field is normal, not an error.

**`BaseItemDto`** — the fields the app actually uses:
`Id, Name, SortName, Type, IsFolder, ParentId, AlbumId, Album, AlbumArtist, AlbumArtists,
ArtistItems, Artists, IndexNumber, ParentIndexNumber, ProductionYear, PremiereDate, RunTimeTicks,
Genres, GenreItems, UserData, MediaSources, ImageTags, ChildCount, DateCreated, Overview`

`AlbumArtists` and `ArtistItems` are `Vec<NameIdPair { Id, Name }>`. **Both are required for the
appears-on split and the track artist filter** — do not drop them to simplify the DTO.

**`MediaSourceDto`:** `Id, Container, Bitrate, Size, SupportsDirectStream, SupportsTranscoding,
MediaStreams, NormalizationGain`.
**`MediaStreamDto`:** `Index, Type, Codec, Language, IsDefault, BitRate, SampleRate, BitDepth,
Channels`.

**`ticks.rs`** — the single conversion point:
```
pub const TICKS_PER_SECOND: i64 = 10_000_000;
pub fn ticks_to_duration(t: i64) -> Duration;    // saturating; negative -> ZERO
pub fn duration_to_ticks(d: Duration) -> i64;    // saturating at i64::MAX
```
Nothing else in the workspace may multiply or divide by 10,000,000.

**Conversions** — `impl TryFrom<BaseItemDto>` for `Artist`, `Album`, `Track`, `Genre`, `Folder`,
`Playlist`, each returning `EmbyError::Decode` when `Id` or `Name` is missing. Rules:

- `Track.artist_ids` ← `ArtistItems[].Id`. **Never parse the `Artists` display strings** — names
  are ambiguous and this field drives the appears-on filter.
- `Track.duration` ← `RunTimeTicks` via the helper; absent → `Duration::ZERO`.
- `Track.format` ← the first `MediaStreams` entry with `Type == "Audio"`, mapping `Codec` into
  `Codec` (unrecognised → `Codec::Other`).
- `Track.media_source_id` ← `MediaSources[0].Id`.
- `Track.replay_gain` ← ReplayGain tags when present; `NormalizationGain` is kept separately on the
  media source for the fallback in `09-04`.
- `Track.is_favorite`, `play_count` ← `UserData`.
- `Album.relation` is **always `Primary` here.** Only `02-06` may set `AppearsOn` — it is the sole
  place with the context to decide.
- `Album.total_duration` ← sum of child ticks when available, else `Duration::ZERO`.
- `image_tag` ← `ImageTags["Primary"]`.

## Acceptance
Tests against the fixtures from `02-01`:
- `parses_artists_fixture`, `parses_album_tracks_fixture`, `parses_favorites_fixture`
- `handles_empty_library_envelope`
- `missing_optional_fields_do_not_error` — a DTO with only `Id` and `Name` converts.
- `missing_id_is_a_decode_error`
- `track_artist_ids_come_from_artist_items`
- `unknown_codec_maps_to_other`
- `ticks_roundtrip` (proptest) — within one tick over `0..=i64::MAX/2`.
- `negative_ticks_clamp_to_zero`
- `album_relation_defaults_to_primary`

## Done when
The global DoD in `tasks/README.md` is satisfied.
