//! Shared item-query builder for /Users/{uid}/Items and the sibling ArtistsService/
//! MusicGenresService endpoints. Every endpoint composes this instead of hand-formatting query
//! strings.

use loxia_core::model::ItemId;

/// Page size is fixed at 200 everywhere; `Page` carries only the page index so a caller cannot
/// forget the size or drift it out of sync between endpoints.
pub const PAGE_SIZE: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub index: usize,
}

impl Page {
    pub const FIRST: Page = Page { index: 0 };

    pub fn start(&self) -> usize {
        self.index * PAGE_SIZE
    }
}

/// A page of results plus the envelope's pagination totals, so a column knows when it has
/// exhausted the library and can stop requesting further pages.
#[derive(Debug, Clone, PartialEq)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    MusicArtist,
    MusicAlbum,
    Audio,
    Playlist,
    Folder,
    MusicGenre,
}

impl ItemType {
    fn as_str(self) -> &'static str {
        match self {
            ItemType::MusicArtist => "MusicArtist",
            ItemType::MusicAlbum => "MusicAlbum",
            ItemType::Audio => "Audio",
            ItemType::Playlist => "Playlist",
            ItemType::Folder => "Folder",
            ItemType::MusicGenre => "MusicGenre",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    IsFavorite,
}

impl Filter {
    fn as_str(self) -> &'static str {
        match self {
            Filter::IsFavorite => "IsFavorite",
        }
    }
}

/// Emby `SortBy` tokens. Scoped to what the confirmed endpoints in `03-emby-api.md` §3–4 need,
/// plus the full set `config::SortField` (`02-data-model.md` §7) will eventually map onto:
/// `Name -> SortName`, `Artist -> Artist`, `AlbumArtist -> AlbumArtist`, `Album -> Album`,
/// `Year -> ProductionYear`, `TrackNumber -> ParentIndexNumber,IndexNumber`, `Genre -> Genres`,
/// `DateAdded -> DateCreated`. That mapping itself is not this task's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbySort {
    SortName,
    Name,
    Artist,
    AlbumArtist,
    Album,
    ProductionYear,
    ParentIndexNumber,
    IndexNumber,
    Genres,
    DateCreated,
    Random,
    /// Emby's `Path` sort — orders items by their on-disk file path, i.e. the library's own
    /// directory structure. Used for folder-tree queueing so playback follows filenames.
    Path,
}

impl EmbySort {
    fn as_str(self) -> &'static str {
        match self {
            EmbySort::SortName => "SortName",
            EmbySort::Name => "Name",
            EmbySort::Artist => "Artist",
            EmbySort::AlbumArtist => "AlbumArtist",
            EmbySort::Album => "Album",
            EmbySort::ProductionYear => "ProductionYear",
            EmbySort::ParentIndexNumber => "ParentIndexNumber",
            EmbySort::IndexNumber => "IndexNumber",
            EmbySort::Genres => "Genres",
            EmbySort::DateCreated => "DateCreated",
            EmbySort::Random => "Random",
            EmbySort::Path => "Path",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

impl SortOrder {
    fn as_str(self) -> &'static str {
        match self {
            SortOrder::Ascending => "Ascending",
            SortOrder::Descending => "Descending",
        }
    }
}

/// The `Fields` parameter. `FieldSet::default()` is exactly the list in `03-emby-api.md` §3 —
/// requesting it once per query is what prevents N+1 fetches later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSet(Vec<&'static str>);

impl FieldSet {
    pub fn custom(fields: &[&'static str]) -> Self {
        FieldSet(fields.to_vec())
    }
}

impl Default for FieldSet {
    fn default() -> Self {
        FieldSet(vec![
            "Genres",
            "DateCreated",
            "MediaSources",
            "UserData",
            "ProductionYear",
            "PremiereDate",
            "Overview",
            "ParentId",
            "ArtistItems",
            "AlbumArtists",
            "ChildCount",
            "RunTimeTicks",
        ])
    }
}

#[derive(Debug, Default, Clone)]
pub struct ItemQuery {
    parent: Option<ItemId>,
    item_types: Vec<ItemType>,
    recursive: Option<bool>,
    artist_ids: Vec<ItemId>,
    album_artist_ids: Vec<ItemId>,
    genres: Vec<String>,
    filters: Vec<Filter>,
    search_term: Option<String>,
    sort_by: Vec<EmbySort>,
    sort_order: Option<SortOrder>,
    start: Option<usize>,
    limit: Option<usize>,
    fields: FieldSet,
}

impl ItemQuery {
    pub fn parent(mut self, id: &ItemId) -> Self {
        self.parent = Some(id.clone());
        self
    }

    /// [`Self::parent`] for a scope that may be "the whole server": `None` leaves `ParentId` off
    /// the query entirely, which is how a browsing list covers **every** music library rather than
    /// one (`endpoints::items::artists` and friends — see `docs/12-decisions.md`).
    pub fn parent_opt(self, id: Option<&ItemId>) -> Self {
        match id {
            Some(id) => self.parent(id),
            None => self,
        }
    }

    pub fn item_types(mut self, types: &[ItemType]) -> Self {
        self.item_types = types.to_vec();
        self
    }

    pub fn recursive(mut self, yes: bool) -> Self {
        self.recursive = Some(yes);
        self
    }

    pub fn artist_ids(mut self, ids: &[ItemId]) -> Self {
        self.artist_ids = ids.to_vec();
        self
    }

    /// Real and load-bearing — this is what `02-06`'s Appears-On discography split is built on.
    /// Verified live against a real server (`12-decisions.md` §10 item 1); do not remove.
    pub fn album_artist_ids(mut self, ids: &[ItemId]) -> Self {
        self.album_artist_ids = ids.to_vec();
        self
    }

    pub fn genres(mut self, names: &[String]) -> Self {
        self.genres = names.to_vec();
        self
    }

    pub fn filters(mut self, f: &[Filter]) -> Self {
        self.filters = f.to_vec();
        self
    }

    pub fn search_term(mut self, q: &str) -> Self {
        self.search_term = Some(q.to_string());
        self
    }

    pub fn sort_by(mut self, fields: &[EmbySort], dir: SortOrder) -> Self {
        self.sort_by = fields.to_vec();
        self.sort_order = Some(dir);
        self
    }

    /// Takes the page index and the limit explicitly — callers cannot forget the size, since
    /// there is no hidden default; `endpoints/items.rs` always passes `PAGE_SIZE`.
    pub fn page(mut self, start: usize, limit: usize) -> Self {
        self.start = Some(start);
        self.limit = Some(limit);
        self
    }

    pub fn fields(mut self, f: FieldSet) -> Self {
        self.fields = f;
        self
    }

    /// Multi-valued parameters are comma-joined once here and handed to `reqwest` as a single
    /// plain string; `reqwest::RequestBuilder::query` percent-encodes the whole pair exactly once
    /// when it serialises the URL, so nothing here pre-encodes anything.
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();

        if let Some(id) = &self.parent {
            pairs.push(("ParentId".to_string(), id.as_str().to_string()));
        }
        if !self.item_types.is_empty() {
            pairs.push((
                "IncludeItemTypes".to_string(),
                join(&self.item_types, ItemType::as_str),
            ));
        }
        if let Some(r) = self.recursive {
            pairs.push(("Recursive".to_string(), r.to_string()));
        }
        if !self.artist_ids.is_empty() {
            pairs.push(("ArtistIds".to_string(), join_ids(&self.artist_ids)));
        }
        if !self.album_artist_ids.is_empty() {
            pairs.push((
                "AlbumArtistIds".to_string(),
                join_ids(&self.album_artist_ids),
            ));
        }
        if !self.genres.is_empty() {
            pairs.push(("Genres".to_string(), self.genres.join(",")));
        }
        if !self.filters.is_empty() {
            pairs.push(("Filters".to_string(), join(&self.filters, Filter::as_str)));
        }
        if let Some(q) = &self.search_term {
            pairs.push(("SearchTerm".to_string(), q.clone()));
        }
        if !self.sort_by.is_empty() {
            pairs.push(("SortBy".to_string(), join(&self.sort_by, EmbySort::as_str)));
            let order = self.sort_order.unwrap_or(SortOrder::Ascending);
            pairs.push(("SortOrder".to_string(), order.as_str().to_string()));
        }
        if let Some(start) = self.start {
            pairs.push(("StartIndex".to_string(), start.to_string()));
        }
        if let Some(limit) = self.limit {
            pairs.push(("Limit".to_string(), limit.to_string()));
        }
        pairs.push(("Fields".to_string(), self.fields.0.join(",")));

        pairs
    }
}

fn join<T: Copy>(items: &[T], to_str: impl Fn(T) -> &'static str) -> String {
    items
        .iter()
        .copied()
        .map(to_str)
        .collect::<Vec<_>>()
        .join(",")
}

fn join_ids(ids: &[ItemId]) -> String {
    ids.iter().map(ItemId::as_str).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_is_200() {
        assert_eq!(PAGE_SIZE, 200);
        assert_eq!(Page { index: 0 }.start(), 0);
        assert_eq!(Page { index: 2 }.start(), 400);
    }

    #[test]
    fn default_fieldset_matches_doc() {
        let expected = [
            "Genres",
            "DateCreated",
            "MediaSources",
            "UserData",
            "ProductionYear",
            "PremiereDate",
            "Overview",
            "ParentId",
            "ArtistItems",
            "AlbumArtists",
            "ChildCount",
            "RunTimeTicks",
        ];
        assert_eq!(FieldSet::default().0, expected.to_vec());
    }

    #[test]
    fn multi_value_params_are_comma_joined_and_encoded_once() {
        let query = ItemQuery::default().artist_ids(&[ItemId::from("a1"), ItemId::from("a2")]);
        let pairs = query.to_query_pairs();
        let (_, value) = pairs.iter().find(|(k, _)| k == "ArtistIds").unwrap();
        assert_eq!(
            value, "a1,a2",
            "comma preserved literally, not pre-percent-encoded"
        );
    }

    #[test]
    fn album_artist_ids_emits_the_correct_query_key() {
        let query = ItemQuery::default().album_artist_ids(&[ItemId::from("ar1")]);
        let pairs = query.to_query_pairs();
        assert!(
            pairs
                .iter()
                .any(|(k, v)| k == "AlbumArtistIds" && v == "ar1")
        );
    }

    /// `ItemQuery` deliberately has no `MediaTypes` builder: the one place that wanted it —
    /// `playlists::list` — got 106,115 unrelated items back from a real server when it sent
    /// `IncludeItemTypes=Playlist&MediaTypes=Audio` (`docs/12-decisions.md`). Nothing should emit
    /// this parameter.
    #[test]
    fn media_types_is_never_emitted() {
        let query = ItemQuery::default()
            .item_types(&[ItemType::Playlist])
            .recursive(true);
        let pairs = query.to_query_pairs();
        assert!(!pairs.iter().any(|(k, _)| k == "MediaTypes"));
    }

    fn snapshot_pairs(query: &ItemQuery) -> String {
        let mut pairs = query.to_query_pairs();
        pairs.sort();
        pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    #[test]
    fn query_snapshot() {
        let artists_list = ItemQuery::default()
            .parent(&ItemId::from("lib1"))
            .recursive(true)
            .sort_by(&[EmbySort::SortName], SortOrder::Ascending)
            .page(0, PAGE_SIZE);
        insta::assert_snapshot!("artists_list", snapshot_pairs(&artists_list));

        let album_tracks = ItemQuery::default()
            .parent(&ItemId::from("album1"))
            .item_types(&[ItemType::Audio])
            .sort_by(
                &[EmbySort::ParentIndexNumber, EmbySort::IndexNumber],
                SortOrder::Ascending,
            );
        insta::assert_snapshot!("album_tracks", snapshot_pairs(&album_tracks));

        let favorites = ItemQuery::default()
            .filters(&[Filter::IsFavorite])
            .item_types(&[ItemType::MusicArtist, ItemType::MusicAlbum, ItemType::Audio])
            .recursive(true);
        insta::assert_snapshot!("favorites", snapshot_pairs(&favorites));

        let search = ItemQuery::default()
            .search_term("boy harsher")
            .item_types(&[ItemType::Audio])
            .recursive(true)
            .page(0, 50);
        insta::assert_snapshot!("search", snapshot_pairs(&search));

        let folder_page = ItemQuery::default()
            .parent(&ItemId::from("folder1"))
            .page(200, PAGE_SIZE);
        insta::assert_snapshot!("folder_page", snapshot_pairs(&folder_page));
    }
}
