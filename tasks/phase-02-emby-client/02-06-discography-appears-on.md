# 02-06 · Discography and the appears-on split

**Phase:** 02 — Emby client · **Agent:** B · **Size:** M
**Prerequisites:** `02-05`
**Reference:** `docs/03-emby-api.md` §4, `docs/12-decisions.md` §10 item 1

## Goal
Implement the ALBUMS / APPEARS ON grouping. This is the feature most likely to be built wrongly, so
the algorithm is fixed and the tests are specified precisely. It was verified live against a real
Emby server in task `02-01` with a genuine mixed-discography artist.

## Files
- `crates/loxia-emby/src/endpoints/discography.rs`

## Specification

```
pub struct Discography {
    pub primary:    Vec<Album>,   // relation = Primary
    pub appears_on: Vec<Album>,   // relation = AppearsOn { context_artist }
}

pub async fn discography(c: &EmbyClient, artist: &Artist) -> Result<Discography, EmbyError>;

/// Every track by `artist` across their whole discography — used only by the "queue this artist"
/// action (`a`/`A` pressed on an Artist row, not an Album row). Browsing an artist's albums never
/// calls this; `discography` above is enough to populate the Albums column.
pub async fn artist_tracks(c: &EmbyClient, artist: &Artist) -> Result<Vec<Track>, EmbyError>;
```

### `discography` — two album-level queries, diffed by id

```
Primary:  /Users/{uid}/Items?IncludeItemTypes=MusicAlbum&AlbumArtistIds={artist.id}
              &Recursive=true&Fields=<FieldSet::DEFAULT>

All:      /Users/{uid}/Items?IncludeItemTypes=MusicAlbum&ArtistIds={artist.id}
              &Recursive=true&Fields=<FieldSet::DEFAULT>
```

Both paged at 200 until `TotalRecordCount` is exhausted. Then, purely client-side:

1. Build `primary` directly from the first query's albums, tagged `relation: Primary`.
2. From the second query's albums, drop every one whose id appears in `primary` (compare **by
   id**). The remainder is `appears_on`, tagged `relation: AppearsOn { context_artist: artist.id }`.
3. Sort both lists by `year` ascending, then `name` ascending. `None` years sort last.

**This replaces an earlier draft of this task**, which — based on the mistaken belief that
`AlbumArtistIds` didn't exist — issued one recursive *track* query and grouped client-side by
`AlbumId`, with a name-based fallback for albums missing `AlbumArtists` data. That approach is
gone: it fetched far more data than needed and required a fallback this one never does, because
`AlbumArtistIds` is a real, server-side, id-based filter (verified live, `02-01`).

Emit a `debug` span logging the artist and the resulting primary/appears-on counts. This is the
first thing to check when a user reports a wrong split.

### Track-level data: fetched lazily, not here

`discography` returns **only albums** — no track lists, no `tracks_by_album` map. Track-level
fetching happens on demand, in `items::album_tracks` (task `02-05`) at the moment the user acts on
a specific album:

- Drilling into any album's Tracks column, or pressing `A` (full context) on an album row:
  `items::album_tracks(album)` — the complete tracklist.
- Pressing `a` (artist-only) on an **`AppearsOn`** album row: the same `album_tracks` call, then
  the reducer filters client-side to tracks where `artist_ids.contains(context_artist)`
  (`queue/appears_on.rs`, task `06-02`) — no separate endpoint needed for this.

### `artist_tracks` — only for "queue this whole artist"

```
/Users/{uid}/Items?IncludeItemTypes=Audio&ArtistIds={artist.id}&Recursive=true
    &Fields=<FieldSet::DEFAULT>&SortBy=Album,ParentIndexNumber,IndexNumber
```

Paged at 200. This is the one place a recursive track-level query is actually needed — pressing
`a`/`A` on an **Artist** row means "queue everything by this artist," which cannot be answered
from album metadata alone.

## Acceptance
Tests against `discography_primary.json` and `discography_all.json` from `02-01` (a real artist
whose live split was 2 primary albums, 6 total — i.e. 4 appears-on), plus hand-built DTO cases:
- `splits_primary_and_appears_on` — the fixture artist yields exactly 2 primary and 4 appears-on
  album ids.
- `appears_on_excludes_every_primary_album_id`
- `albums_sorted_by_year_then_name`
- `none_year_sorts_last`
- `empty_appears_on_when_all_albums_are_primary`
- `empty_primary_when_artist_has_no_primary_albums` — matches the live finding that at least one
  artist on the test server has `AlbumArtistIds` → 0, `ArtistIds` → 10 (a pure guest artist).
- `paginates_past_200_albums` — wiremock returns two pages for one of the two queries; all albums
  are collected.
- `discography_issues_no_track_level_request` — asserts exactly two requests total, both
  `IncludeItemTypes=MusicAlbum`.
- `artist_tracks_issues_one_recursive_query_over_audio`
- `artist_tracks_paginates_past_200`

## Done when
The global DoD in `tasks/README.md` is satisfied.
