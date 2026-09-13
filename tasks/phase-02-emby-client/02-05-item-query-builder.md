# 02-05 · Item query builder

**Phase:** 02 — Emby client · **Agent:** B · **Size:** M
**Prerequisites:** `02-04`
**Reference:** `docs/03-emby-api.md` §3

## Goal
One typed builder for the `/Users/{uid}/Items` query surface, plus the basic browse endpoints. Every
later endpoint composes it instead of hand-formatting query strings.

## Files
- `crates/loxia-emby/src/query.rs`
- `crates/loxia-emby/src/endpoints/items.rs`

## Specification

```
#[derive(Default, Clone)]
pub struct ItemQuery { .. }

impl ItemQuery {
    pub fn parent(self, id: &ItemId) -> Self;
    pub fn item_types(self, types: &[ItemType]) -> Self;
    pub fn recursive(self, yes: bool) -> Self;
    pub fn artist_ids(self, ids: &[ItemId]) -> Self;
    pub fn album_artist_ids(self, ids: &[ItemId]) -> Self;
    pub fn genres(self, names: &[String]) -> Self;
    pub fn filters(self, f: &[Filter]) -> Self;          // Filter::IsFavorite, ...
    pub fn search_term(self, q: &str) -> Self;
    pub fn sort_by(self, fields: &[EmbySort], dir: SortOrder) -> Self;
    pub fn page(self, start: usize, limit: usize) -> Self;
    pub fn fields(self, f: FieldSet) -> Self;            // defaults to FieldSet::DEFAULT
    pub fn to_query_pairs(&self) -> Vec<(String, String)>;
}
```

`FieldSet::DEFAULT` is exactly the list in `docs/03-emby-api.md` §3. Requesting it once per query is
what prevents N+1 fetches later.

**`album_artist_ids` is real and load-bearing** — it is what `02-06`'s discography split is built
on. (An earlier draft of this task claimed `AlbumArtistIds` didn't exist and banned it from this
builder; that was wrong — verified live against a real server in task `02-01`, see
`docs/12-decisions.md` §10 item 1. Do not remove this method.)

`ItemType` = `MusicArtist | MusicAlbum | Audio | Playlist | Folder | MusicGenre`, serialised to the
exact Emby names. Multi-valued parameters are comma-joined and percent-encoded once.

Page size is **200**; `page()` takes the index and limit explicitly so callers cannot forget.

**`endpoints/items.rs`:**
```
pub async fn music_libraries(c: &EmbyClient) -> Result<Vec<Library>, EmbyError>;
pub async fn artists(c: &EmbyClient, lib: &ItemId, page: Page) -> Result<Paged<Artist>, EmbyError>;
pub async fn album_artists(c: &EmbyClient, lib: &ItemId, page: Page) -> Result<Paged<Artist>, EmbyError>;
pub async fn album_tracks(c: &EmbyClient, album: &ItemId) -> Result<Vec<Track>, EmbyError>;
pub async fn genres(c: &EmbyClient, lib: &ItemId, page: Page) -> Result<Paged<Genre>, EmbyError>;
pub async fn genre_artists(c: &EmbyClient, genre: &Genre, page: Page) -> Result<Paged<Artist>, EmbyError>;
pub async fn folder_children(c: &EmbyClient, parent: &ItemId, page: Page) -> Result<Paged<MediaItem>, EmbyError>;
```
`Paged<T> { items: Vec<T>, total: usize, start: usize }`.

`music_libraries` reads `/Users/{uid}/Views` and keeps entries whose `CollectionType == "music"`.
`album_tracks` sorts by `ParentIndexNumber,IndexNumber` so disc and track order is correct.
`folder_children` is **not** recursive and returns `MediaItem::Folder` or `MediaItem::Track`
according to `IsFolder`.

All reads go through `with_retry` from `02-03`.

## Acceptance
- `query_snapshot` — an `insta` snapshot of `to_query_pairs()` for: artists list, album tracks,
  favourites, search, and a paged folder listing.
- `default_fieldset_matches_doc`
- `multi_value_params_are_comma_joined_and_encoded_once`
- `page_size_is_200`
- `music_libraries_filters_to_music_collection_type` (fixture `views.json`)
- `album_tracks_sorted_by_disc_then_track` (fixture `album_tracks.json`)
- `folder_children_splits_folders_and_tracks`
- `album_artist_ids_emits_the_correct_query_key` — `to_query_pairs()` for a builder configured with
  `album_artist_ids` contains a key literally named `AlbumArtistIds`.

## Done when
The global DoD in `tasks/README.md` is satisfied.
