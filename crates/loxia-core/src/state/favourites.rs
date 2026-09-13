//! FavouritesState for the Favourites tab (`07-02`).
//!
//! The same three-section shape `07-01`'s Search tab uses (`SearchResults`/`SearchSection` are
//! reused directly, not duplicated) minus the query/debounce machinery Favourites has no need
//! for — there is only ever one favourites list, fetched on tab entry and refreshed by `Ctrl+R`.

use serde::{Deserialize, Serialize};

use super::nav::LoadState;
use super::search::{SearchResults, SearchSection};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FavouritesState {
    pub results: SearchResults,
    pub load: LoadState,
    pub focused_section: SearchSection,
    /// One cursor per section, indexed by `SearchSection::index()`.
    pub cursors: [usize; 4],
}
