//! SearchState for the Search tab (`07-01`).

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::model::{Album, Artist, Track};

use super::nav::LoadState;

/// Which of the three result sections is focused. `Tab` cycles through these once the query line
/// itself isn't focused (`SearchState::query_focused`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SearchSection {
    #[default]
    Artists,
    Albums,
    Tracks,
    /// Only ever populated by the Favourites tab — Emby favourites any item type, playlists
    /// included, and a favourited playlist previously had nowhere in this app to appear
    /// (`docs/12-decisions.md`). The Search tab does not query playlists, so this section is always
    /// empty there, and an empty section renders as nothing.
    Playlists,
}

impl SearchSection {
    pub const ALL: [SearchSection; 4] = [
        SearchSection::Artists,
        SearchSection::Albums,
        SearchSection::Tracks,
        SearchSection::Playlists,
    ];

    pub fn index(self) -> usize {
        match self {
            SearchSection::Artists => 0,
            SearchSection::Albums => 1,
            SearchSection::Tracks => 2,
            SearchSection::Playlists => 3,
        }
    }

    /// `Tab` cycles forward, wrapping.
    pub fn next(self) -> SearchSection {
        let i = (self.index() + 1) % SearchSection::ALL.len();
        SearchSection::ALL[i]
    }

    /// `Shift+Tab` cycles backward, wrapping.
    pub fn prev(self) -> SearchSection {
        let i = (self.index() + SearchSection::ALL.len() - 1) % SearchSection::ALL.len();
        SearchSection::ALL[i]
    }
}

/// The three categorised result lists, plus each section's own independent error — `search()`
/// (`loxia-emby`, `02-07`) runs all three queries concurrently and any one can fail without
/// discarding the others, so "empty because nothing matched" and "empty because this section's
/// request failed" must stay distinguishable (`docs/12-decisions.md`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SearchResults {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub playlists: Vec<crate::model::Playlist>,
    pub artists_error: Option<String>,
    pub albums_error: Option<String>,
    pub tracks_error: Option<String>,
    #[serde(default)]
    pub playlists_error: Option<String>,
}

impl SearchResults {
    pub fn count(&self, section: SearchSection) -> usize {
        match section {
            SearchSection::Artists => self.artists.len(),
            SearchSection::Albums => self.albums.len(),
            SearchSection::Tracks => self.tracks.len(),
            SearchSection::Playlists => self.playlists.len(),
        }
    }

    pub fn error(&self, section: SearchSection) -> Option<&str> {
        match section {
            SearchSection::Artists => self.artists_error.as_deref(),
            SearchSection::Albums => self.albums_error.as_deref(),
            SearchSection::Tracks => self.tracks_error.as_deref(),
            SearchSection::Playlists => self.playlists_error.as_deref(),
        }
    }

    pub fn is_empty(&self, section: SearchSection) -> bool {
        self.count(section) == 0
    }

    /// The first section (in `Artists`/`Albums`/`Tracks` order) with at least one result, if any.
    pub fn first_nonempty(&self) -> Option<SearchSection> {
        SearchSection::ALL
            .into_iter()
            .find(|&section| !self.is_empty(section))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchState {
    pub query: String,
    /// Set to `now + 250ms` on every keystroke; `Tick` fires the debounced fetch once this
    /// deadline passes (`docs/07-ui-spec.md` §9). `None` when nothing is pending.
    pub debounce_until: Option<Timestamp>,
    pub results: SearchResults,
    pub load: LoadState,
    /// Whether the query line (rather than a results section) currently has focus — `true` on
    /// tab entry; `Enter` clears it (moving focus into the first non-empty section), `/` sets it
    /// again.
    pub query_focused: bool,
    pub focused_section: SearchSection,
    /// One cursor per section, indexed by `SearchSection::index()`.
    pub cursors: [usize; 4],
    /// The query text the most recently *issued* `Effect::Net(Search)` was for — `Tick` compares
    /// the live, trimmed `query` against this (not just whether `debounce_until` has passed), so
    /// a second tick past the deadline, or a retype that lands back on unchanged text, does not
    /// refire the same search.
    pub last_searched: Option<String>,
}

impl Default for SearchState {
    fn default() -> Self {
        SearchState {
            query: String::new(),
            debounce_until: None,
            results: SearchResults::default(),
            load: LoadState::default(),
            query_focused: true,
            focused_section: SearchSection::default(),
            cursors: [0, 0, 0, 0],
            last_searched: None,
        }
    }
}
