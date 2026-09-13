# 02-01 · API audit against the live server

**Phase:** 02 — Emby client · **Agent:** B · **Size:** M
**Prerequisites:** `01-06`
**Reference:** `docs/03-emby-api.md` (all), `docs/12-decisions.md` §7

## Goal
Confirm every route and parameter in `docs/03-emby-api.md` against the project's live Emby server,
and capture the fixture set that the rest of phase 02 is tested against. This is a gate: no other
phase-02 task starts until it is signed off.

## Files
- `crates/loxia-emby/tests/fixtures/*.json`, `lyrics.lrc`
- `docs/12-decisions.md` (append findings to §10)
- `scripts/capture-fixtures.py`

## Specification

Write `scripts/capture-fixtures.py` — a small script using only the standard library
(`urllib.request`, `json`) so it runs with no extra dependencies. It reads credentials from a local
session file (`{"user_id", "access_token"}`, written once by a separate login step with `0600`
permissions) rather than accepting them as arguments or reading a password from an environment
variable — both would risk landing in shell history or a process listing. **This is what was
actually used**: the originally-specified `curl` + `jq` approach was abandoned once it turned out
`jq` is not reliably available in every execution environment; a dependency-free script is more
portable and just as auditable.

For each row below, issue the request, record the HTTP status, save the response, and mark the doc
row ✅ (as documented) or ✏️ (differs — describe how).

| # | Check | Fixture file |
| :-- | :-- | :-- |
| 1 | `POST /Users/AuthenticateByName` returns `AccessToken` and `User.Id` | not saved (contains credentials) |
| 2 | `GET /Users/{uid}/Views` includes a `CollectionType == "music"` entry | `views.json` |
| 3 | `GET /Artists?ParentId={lib}&Recursive=true` | `artists.json` |
| 4 | `GET /Artists/AlbumArtists?ParentId={lib}` | `album_artists.json` |
| 5 | **Discography — two-query diff, not a single track query** (see `docs/12-decisions.md` §10 item 1: `AlbumArtistIds` turned out to be real). For an artist with both primary and compilation albums: `/Users/{uid}/Items?IncludeItemTypes=MusicAlbum&AlbumArtistIds={id}&Recursive=true&Fields=<default set>` (primary) and the same with `ArtistIds` instead (all). Confirm the `AlbumArtistIds` result is a strict subset of the `ArtistIds` result by id. | `discography_primary.json`, `discography_all.json` |
| 6 | `GET /Users/{uid}/Items?ParentId={album}&IncludeItemTypes=Audio` in album order | `album_tracks.json` |
| 7 | `GET /MusicGenres?ParentId={lib}` | `genres.json` |
| 8 | `GET /Users/{uid}/Items?Filters=IsFavorite&...` | `favorites.json` |
| 9 | `GET /Playlists/{id}/Items?UserId={uid}` — **confirm the per-row playlist entry id field name** | `playlist_items.json` |
| 10 | `DELETE /Playlists/{id}/Items?EntryIds=...` — confirm it accepts that id, not `ItemId` | — |
| 11 | `GET /Users/{uid}/Items?SearchTerm=...` per type | `search_*.json` |
| 12 | `GET /Items/{id}/InstantMix?UserId={uid}` | `instant_mix.json` |
| 13 | `POST /Items/{id}/PlaybackInfo?UserId={uid}` — confirm `MediaSources[].Id` and `MediaStreams` | `playback_info.json` |
| 14 | **Lyrics** — find a track with a `MediaStreams` entry where `Type == "Subtitle"`, then fetch `GET /Items/{id}/{msid}/Subtitles/{idx}/Stream.{fmt}`. Confirm the format extension must match the stream's own codec (`.lrc` for an `lrc`-codec stream, `.srt` otherwise) — the wrong extension returns `200` with an **empty** body, not an error, which is easy to mistake for "no lyrics" | `track_with_lyrics.json`, `lyrics.lrc` |
| 15 | Same for a track **without** lyrics — confirm no such stream | `track_no_lyrics.json` |
| 16 | `POST` / `DELETE /Users/{uid}/FavoriteItems/{id}` | — |
| 17 | `GET /Items/{id}/Images/Primary?tag=...&maxHeight=64` | — |
| 18 | WebSocket handshake at `/embywebsocket?api_key=...&deviceId=...` | — |
| 19 | Confirm whether `AlbumArtistIds` is honoured, rejected, or silently ignored on `/Users/{uid}/Items` — **do not assume any of the three**; test with a bogus id and compare the returned count to the unfiltered total | — |

Two things worth testing that weren't in the original 19-row list, found in practice while
exercising row 10 (playlist mutation) with a throwaway playlist: whether `POST /Playlists` accepts
its parameters as a JSON body or as query parameters, and whether an `Overview` passed at creation
time actually persists. Test both — create a temporary playlist, verify, then delete it.

**Redaction is mandatory.** Before committing, strip from every fixture: `AccessToken`, any
`api_key` query value, `ServerId`, private IP addresses, and real usernames. The CI grep from task
`00-04` enforces this; run it locally first.

Also capture two synthetic fixtures by hand: `empty_library.json` (a valid envelope with
`Items: []` and `TotalRecordCount: 0`) and `malformed.json` (a truncated body), for error-path tests.

## Acceptance
- All 19 rows checked, each marked ✅ or ✏️ in a table appended to `docs/12-decisions.md` §10 with
  today's date.
- Every ✏️ row has a corresponding correction applied to `docs/03-emby-api.md` in this PR.
- All listed fixture files exist and pass the CI secret grep.
- If row 14 fails — no subtitle stream appears for a track that has an `.lrc` — **stop and report**
  rather than improvising. Lyrics scope depends on this answer.

## Done when
The global DoD in `tasks/README.md` is satisfied, and the fixture set is committed.
