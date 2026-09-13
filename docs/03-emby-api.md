# 03 — Emby API Contract (`loxia-emby`)

Routes below were confirmed against the official Emby REST reference at `dev.emby.media`
(2026-07-21) and, for the highest-risk ones, directly against a live Emby 4.9.5.0 server (task
`02-01`) — every correction that verification produced is applied inline below and recorded in
`12-decisions.md` §10.

Emby is **not** Jellyfin. Do not substitute Jellyfin documentation — several routes and parameters
differ.

## 1. Base URL

Emby serves under `/emby`. Build every URL as `{server.url}/emby/{path}`. Normalise the configured
URL at load: strip trailing `/`, reject anything that is not `http` or `https`.

## 2. Authentication

### Header sent on every request
```
X-Emby-Authorization: MediaBrowser Client="loxia", Device="<hostname>",
                      DeviceId="<stable-uuid>", Version="<crate version>", Token="<access_token>"
X-Emby-Token: <access_token>
```
`DeviceId` **must be stable across restarts** — generate once, persist to `servers[].device_id`.
Emby uses it for session identity and remote-control targeting; a fresh id per launch creates
orphan sessions on the server.

### Login (`auth.rs`)
`POST /emby/Users/AuthenticateByName` with `{ "Username": ..., "Pw": ... }`
→ `{ User: { Id }, AccessToken, ServerId }`. Persist `user_id` and `access_token` into the server
profile. On startup, validate the stored token with `GET /emby/Users/{user_id}`; a 401 opens the
re-login flow rather than crashing.

### Custom headers
`server.custom_headers` is merged into every REST request **and** the WebSocket handshake. Build a
`HeaderMap` once at client construction and clone per request. Overriding `Authorization`,
`X-Emby-Authorization`, `Host`, or `Content-Length` is refused with a `ConfigWarning`.

## 3. Shared item query (`query.rs`)

Confirmed parameters on `GET /Users/{uid}/Items` (*ItemsService*), verified both against the public
reference and directly against a live server (task `02-01`):
`ParentId, IncludeItemTypes, Recursive, SortBy, SortOrder, StartIndex, Limit, Fields, Filters,
ArtistIds, AlbumArtistIds, Genres, GenreIds, SearchTerm, EnableUserData, EnableImageTypes,
ImageTypeLimit, AlbumArtist, NameStartsWith`.

**`AlbumArtistIds` is real and load-bearing — it is what §4's split is built on.** (An earlier pass
of this document, based only on the public API reference not listing it, claimed the opposite;
verified live: a bogus id returns `TotalRecordCount: 0`, and a real artist with both primary and
compilation albums returns the expected smaller count. See `12-decisions.md` §10.)

Default `Fields` set, requested once to avoid N+1 fetches:
```
Genres,DateCreated,MediaSources,UserData,ProductionYear,PremiereDate,
Overview,ParentId,ArtistItems,AlbumArtists,ChildCount,RunTimeTicks
```

**Pagination:** page size 200, driven by `StartIndex`/`Limit`; read `TotalRecordCount` from the
response envelope. A column loads page 1 eagerly and requests the next page when the cursor enters
the last 50 loaded items.

| Purpose | Request |
| :-- | :-- |
| Music libraries | `GET /Users/{uid}/Views` → keep entries whose `CollectionType == "music"` |
| All artists | `GET /Artists?ParentId={lib}&Recursive=true&SortBy=SortName` (*ArtistsService*) |
| Album artists only | `GET /Artists/AlbumArtists?ParentId={lib}` (*ArtistsService*) |
| Genres | `GET /MusicGenres?ParentId={lib}` (*MusicGenresService*) |
| Folder children | `GET /Users/{uid}/Items?ParentId={folder}&IncludeItemTypes=Folder,Audio` — no `Recursive` (`07-05`: a directory with 50,000 files below it must still open instantly); `IsFolder` distinguishes rows; `IncludeItemTypes` is what makes non-audio files (images, subtitle sidecars, video) never round-trip at all |
| Folder-row queueing | `GET /Users/{uid}/Items?ParentId={folder}&IncludeItemTypes=Audio[&Recursive=true]` — `a` (`Recursive` absent/`false`) queues only direct children, `A` (`Recursive=true`) the whole subtree (`07-05`) |
| Favourites | `GET /Users/{uid}/Items?Filters=IsFavorite&IncludeItemTypes=MusicArtist,MusicAlbum,Audio&Recursive=true` |
| Playlists | `GET /Users/{uid}/Items?IncludeItemTypes=Playlist&Recursive=true` |
| Search | `GET /Users/{uid}/Items?SearchTerm={q}&IncludeItemTypes={type}&Recursive=true&Limit=50`, run once per type concurrently |
| Instant mix | `GET /Items/{id}/InstantMix?UserId={uid}&Limit=100` (*InstantMixService*) |

## 4. Discography: the ALBUMS / APPEARS ON split (`endpoints/discography.rs`)

This is the feature most likely to be implemented wrongly, so the algorithm is fixed here. It was
verified directly against a live server (task `02-01`) with a real mixed-discography artist: an
artist whose `AlbumArtistIds` query returned 2 albums and whose `ArtistIds` query returned 6 —
i.e. 2 primary releases and 4 compilation appearances, exactly the shape this algorithm must split.

**Two album-level queries, both cheap, no track fetch needed to populate the Albums column:**

```
Primary:    GET /Users/{uid}/Items?IncludeItemTypes=MusicAlbum&AlbumArtistIds={artist_id}
                &Recursive=true&Fields=<default set>

All:        GET /Users/{uid}/Items?IncludeItemTypes=MusicAlbum&ArtistIds={artist_id}
                &Recursive=true&Fields=<default set>
```

Then, purely client-side:

1. Build the `Primary` set directly from the first query's results — tag each with
   `relation: Primary`.
2. From the second query's results, drop every album whose id appears in the `Primary` set (by
   id, not name). The remainder is `AppearsOn { context_artist: artist_id }`.
3. Render `Primary` albums under `── ALBUMS (n) ──` and the rest under `── APPEARS ON (n) ──`,
   each sorted by year then name. Omit a section entirely when its count is zero.

This is deliberately **not** the same shape as an earlier draft of this document, which — believing
`AlbumArtistIds` didn't exist — proposed one recursive *track* query with client-side grouping by
`AlbumId`. That approach is unnecessary: it fetched far more data (every track by the artist, just
to derive album-level facts) and still needed a name-based fallback for albums with missing
`AlbumArtists` data. The two-query approach never needs a name fallback, because `AlbumArtistIds`
is a real server-side filter, not something reconstructed from a display string.

**Track-level data is fetched lazily, on drill-in — not eagerly for the whole discography.** This
is the same pattern already used for a `Primary` album: nothing about the tracks is known until the
user acts on that specific album.

- Drilling into any album's Tracks column, **or** pressing `A` (full context) on an album row from
  the Albums column: `GET /Users/{uid}/Items?ParentId={album_id}&IncludeItemTypes=Audio
  &SortBy=ParentIndexNumber,IndexNumber` — the complete tracklist in album order.
- Pressing `a` (artist-only) on an **`AppearsOn`** album row: the same request as above, then
  filter client-side to tracks where `artist_ids.contains(context_artist)`. Track-level
  highlighting in that column uses the identical test.
- Pressing `a`/`A` on an **`Artist`** row (not a specific album): `GET /Users/{uid}/Items
  ?IncludeItemTypes=Audio&ArtistIds={artist_id}&Recursive=true&Fields=<default set>
  &SortBy=Album,ParentIndexNumber,IndexNumber` — every track by that artist across their entire
  discography. This is the one place a recursive track-level query is actually needed, because the
  action is "queue everything by this artist," not "browse this artist's albums."

## 5. Playback: choosing a stream (`stream.rs`)

Ask the server what it can do rather than guessing:

`POST /Items/{Id}/PlaybackInfo?UserId={uid}` (*MediaInfoService*) with the device profile
→ `MediaSources[]`, each with `SupportsDirectStream`, `SupportsTranscoding`, `Container`, `Bitrate`,
and `MediaStreams`. Use it to pick the source and to capture `MediaSourceId` (needed for lyrics).

| Quality profile | URL (*AudioService*) |
| :-- | :-- |
| `Direct` | `GET /Audio/{id}/stream?static=true&api_key={token}` — exact source bytes. |
| `High320` / `Med192` / `Low96` | `GET /Audio/{id}/stream.{ext}?audioCodec={mp3\|aac\|opus}&audioBitRate={320000\|192000\|96000}&maxAudioChannels=2&api_key={token}` |

`api_key` must be in the query string because mpv performs its own HTTP request and does not share
our header map. Custom headers **are** passed to mpv separately via `http-header-fields`
(`05-audio-engine.md` §3) — this is required for reverse-proxy deployments.

**Never log a stream URL** — it carries the token. `StreamUrl`'s `Debug` and `Display` impls redact
the `api_key` value, and a test asserts it.

## 6. Playback reporting (*PlaystateService*)

| Event | Request |
| :-- | :-- |
| Start | `POST /Sessions/Playing` `{ ItemId, PlaySessionId, CanSeek: true, PositionTicks: 0 }` |
| Progress | `POST /Sessions/Playing/Progress` `{ ItemId, PlaySessionId, PositionTicks, IsPaused, PlayMethod }` — every 10 s, and on pause, resume, and seek |
| Stop | `POST /Sessions/Playing/Stopped` `{ ItemId, PlaySessionId, PositionTicks }` |
| Mark played | `POST /Users/{uid}/PlayedItems/{id}` |

Ticks are **100-nanosecond units**: `ticks = secs * 10_000_000`. The conversion lives in exactly one
helper, `dto::ticks`, and is property-tested. Getting this wrong is the classic Emby client bug.

**Completion threshold:** mark played at ≥90 % of duration or ≥4 minutes listened, whichever comes
first. The same threshold gates writing a `HistoryEntry`.

When any of these fail with `Offline`, they are appended to the scrobble buffer with their original
timestamps and replayed in order on reconnect (`06-cache-and-offline.md` §6).

## 7. Lyrics (`endpoints/lyrics.rs`) — *SubtitleService*

Emby has no lyrics service. It exposes `.lrc` / `.txt` sidecars as **text MediaStreams on the audio
item's MediaSource**, retrieved through the subtitle routes. The `/Items/` route below — as opposed
to `/Videos/` — is the one that works for audio.

**Discovery.** Tracks are already fetched with `Fields=MediaSources`. For each track, scan
`MediaSources[].MediaStreams` for an entry whose `Type` is `Subtitle` and whose `Codec` is one of
`lrc`, `srt`, `subrip`, or `text`. Record `LyricStreamRef { media_source_id, stream_index, format }`
on the `Track`. No match means the track has no lyrics and the pane stays hidden.

**Fetch.**
```
GET /Items/{ItemId}/{MediaSourceId}/Subtitles/{Index}/Stream.{Format}
```
`Format` is `lrc` when the source codec is `lrc`, otherwise `srt`. The response body is plain text.
Parse it with the LRC parser in `loxia-core::model::lyrics`:
- `.lrc` → `Lyrics::Synced`
- `.srt` / `.txt` → `Lyrics::Unsynced` (SRT cue timings are ignored; the payload lines are kept)
- unparseable → `Lyrics::Unsynced` with the raw lines. **Never an error** — lyrics are cosmetic and
  must never interrupt playback.

Lyrics are fetched lazily when the track becomes current and the pane is visible, and cached in
`AppState.lyrics` keyed by `ItemId` (one track at a time).

## 8. Mutations

| Action | Request | Service |
| :-- | :-- | :-- |
| Favourite on / off | `POST` / `DELETE /Users/{uid}/FavoriteItems/{id}` | UserLibrary |
| Create playlist | `POST /Playlists?Name={name}&Ids={csv}&UserId={uid}&MediaType=Audio` — **query parameters, not a JSON body** (verified live: a JSON body fails `500 Unrecognized Guid format`; the identical call as query params succeeds). **`Overview` is silently ignored here** — passing it produces no error but does not persist (verified live: read-back is `null`). Setting a description requires a second call: `GET /Users/{uid}/Items/{id}` for the full DTO, set `Overview`, then `POST /Items/{id}` with that DTO as the body (confirmed live: `204`, and the value persists on read-back) | Playlist |
| Playlist items | `GET /Playlists/{id}/Items?UserId={uid}` — captures `PlaylistItemId` per row | Playlist |
| Add to playlist | `POST /Playlists/{id}/Items?Ids={csv}&UserId={uid}` | Playlist |
| Remove from playlist | `DELETE /Playlists/{id}/Items?EntryIds={playlistItemIds}` | Playlist |
| Reorder | `POST /Playlists/{id}/Items/{itemId}/Move/{newIndex}` | Playlist |
| Delete playlist | `DELETE /Items/{id}` | Library |

**Removal requires `PlaylistItemId`, not `ItemId`.** `Playlist.entries` therefore stores
`PlaylistEntryId` alongside each track; dropping it makes removal impossible without a refetch.

### Ratings are not implemented
Emby's `POST /Users/{uid}/Items/{id}/Rating` accepts a single boolean `Likes` parameter — there is
no numeric star rating in the API. The 1–5 star feature from `design_overview` is therefore cut;
favourites are the only user-item signal. See `12-decisions.md` §2.

## 9. Images (`endpoints/images.rs`)

`GET /Items/{id}/Images/Primary?maxHeight={px}&tag={image_tag}&quality=90`

- Two sizes: `64` for inspector thumbnails, `600` for Zen mode and desktop notifications.
- Cached to `~/.cache/loxia-player/images/{id}_{size}_{tag}.jpg`. Keying on `image_tag` means a server-side
  artwork change invalidates the entry automatically.
- Images have their own budget (`cache.image_cache_mb`, default 200 MB), separate from the track LRU.
- Fallback chain: track art → album art → artist art → themed placeholder glyph.

## 10. WebSocket (`ws.rs`)

`wss://{host}/embywebsocket?api_key={token}&deviceId={device_id}`, with custom headers on the
handshake.

Inbound `MessageType` handling:
- `Playstate` (`PlayPause`, `NextTrack`, `PreviousTrack`, `Stop`, `Seek`) → mapped to `Action`s, so
  other Emby clients can remote-control loxia
- `GeneralCommand` (`SetVolume`, `Mute`, `Unmute`, `DisplayMessage`)
- `LibraryChanged` → mark affected columns `LoadState::Idle`; refetch only if currently visible
- `UserDataChanged` → update favourite and played flags in place

Send `KeepAlive` every 30 s. Reconnect with jittered exponential backoff, 1 s → 30 s.
**The WebSocket is optional.** Its failure is logged at `warn` and never blocks browsing or
playback. It is disabled entirely when `ui.enable_websocket = false`.

## 11. Error mapping (`error.rs`)

| Condition | `EmbyError` | UI behaviour |
| :-- | :-- | :-- |
| DNS / connect / timeout | `Offline` | flip connectivity, toast once, start the reconnect probe |
| 401 / 403 | `Unauthorized` | toast, then open Settings → server profile |
| 404 | `NotFound(ItemId)` | mark the item `Unavailable` and drop it from its column |
| 429 / 5xx | `Transient { retry_after }` | retry ×3 with jittered backoff, then toast |
| Body parse failure | `Decode { endpoint, source }` | log the body at `debug`, show a generic toast |

Retries apply to idempotent GETs only. Mutation failures surface to the user immediately and roll
back the optimistic UI change.

## 12. Test fixtures

`crates/loxia-emby/tests/fixtures/*.json` — captured, token-redacted responses for: artists list,
artist tracks spanning primary and compilation albums, album tracks, playlist items with
`PlaylistItemId`, search results per type, instant mix, favourites, a track with a `Subtitle`
lyric stream, a track without one, an empty library, and a malformed response.

DTO→model conversion is tested against these with `wiremock`. **No test may touch a real network.**
A CI check greps the fixture directory for token-shaped strings and private IPs and fails the build
if either appears.

## Artwork and the inherited primary image

An item's cover is `ImageTags.Primary` **when it has one**. Most albums do not: Emby answers with an
empty `ImageTags` plus `PrimaryImageItemId`/`PrimaryImageTag`, naming whichever item actually holds
the image (usually a child track). Measured on a real library: of 300 albums, 81 carried their own
tag, 195 carried only the pointer, and 24 had neither — while of 300 tracks, 287 carried their own
tag and none used the pointer.

So the URL to fetch is always `/Items/{PrimaryImageItemId or own id}/Images/Primary?tag={tag}`, and
the id and tag must travel together (`loxia_core::model::ImageRef`). Requesting `/Images/Primary`
without a tag for an untagged album does **not** work — it 404s.
