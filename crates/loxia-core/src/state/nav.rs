//! NavState, Column, ColumnKind, SelectionState, sliding-window navigation.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::{Deserialize, Serialize};

use crate::model::{ItemId, MediaItem, SectionHeader};

/// The 9 sidebar tabs, in `Alt+1`..`9` order (`design_overview` §2.1); `F2`..`F9` alias the last
/// eight, `F1` being reserved for help.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Tab {
    #[default]
    NowPlaying,
    Favourites,
    Search,
    Playlists,
    Artists,
    /// Emby's `/Artists/AlbumArtists` — only the artists credited as an *album* artist, i.e. the
    /// ones a library is actually organised by. `Artists` lists every performer credited on any
    /// track, which on a library with many compilations or featured guests is a far longer and
    /// much less navigable list.
    AlbumArtists,
    Albums,
    Genres,
    Folders,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NavFocus {
    #[default]
    Sidebar,
    Column(usize),
    Inspector,
}

/// `Albums { of_artist: None }` and `Folders { of_parent: None }` are the library-root listing —
/// the `Albums`/`Folders` sidebar tabs seed a column this way (`design_overview` §2.1's "Albums:
/// Miller Column view starting at the Album level" has no artist to scope to yet). `Some(id)` is
/// the drilled-in case. `docs/02-data-model.md`'s original non-optional `{ of_artist: ItemId }`
/// spec had no way to express this — corrected while implementing `03-06`'s tab-seeding logic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColumnKind {
    Artists,
    /// As [`ColumnKind::Artists`], but from `/Artists/AlbumArtists` — see [`Tab::AlbumArtists`].
    /// A separate kind rather than a flag on `Artists` because the two are distinct columns with
    /// distinct fetches, and pagination/dedup key off the kind.
    AlbumArtists,
    Albums {
        of_artist: Option<ItemId>,
    },
    Tracks {
        of_album: ItemId,
    },
    ArtistTracks {
        of_artist: ItemId,
    },
    Genres,
    /// Emby's `/Artists?Genres={name}` filters by genre **name**, not id (`docs/03-emby-api.md`
    /// §3) — unlike every other `of_X` field in this enum, `of_genre` deliberately carries the
    /// name, not an `ItemId`, so `07-04`'s fetch has what it needs without a second lookup.
    GenreArtists {
        of_genre: String,
    },
    Folders {
        of_parent: Option<ItemId>,
    },
    Playlists,
    PlaylistTracks {
        of_playlist: ItemId,
    },
    SearchResults,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LoadState {
    #[default]
    Idle,
    Loading,
    Loaded {
        total: usize,
    },
    Error(String),
}

/// Keyed by **id, not index** — an index would silently point at the wrong row the moment a
/// filter changes the visible order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SelectionState {
    pub visual_mode: bool,
    pub selected: BTreeSet<ItemId>,
    pub anchor: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub kind: ColumnKind,
    pub items: Vec<MediaItem>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub selection: SelectionState,
    /// The inline `/` search. `Some(_)` — even `Some(String::new())` — means the column is
    /// actively narrowed; only `Cancel`'s ladder (`reducer::nav::cancel`) ever puts this back to
    /// `None`. Distinct from `filter_editing`: text can be committed (`Enter`) and the column
    /// stays narrowed while ordinary navigation resumes.
    pub filter: Option<String>,
    /// Whether the inline filter is currently capturing keystrokes (`04-11`) — this, not
    /// `filter.is_some()`, is what `InputContext::TextInput` is keyed on, since a committed filter
    /// (`Enter` pressed) keeps `filter` set but must not keep swallowing `j`/`k`/etc.
    pub filter_editing: bool,
    pub load: LoadState,
    pub page_loaded: usize,
    pub title: String,
}

impl Column {
    pub fn new(kind: ColumnKind, title: impl Into<String>) -> Self {
        Column {
            kind,
            items: Vec::new(),
            cursor: 0,
            scroll_offset: 0,
            selection: SelectionState::default(),
            filter: None,
            filter_editing: false,
            load: LoadState::default(),
            page_loaded: 0,
            title: title.into(),
        }
    }

    /// Applies `filter` (case-insensitive `nucleo-matcher` fuzzy matching against
    /// `display_name()`, `docs/07-ui-spec.md` §5) — pairing each surviving item with its original
    /// index in `items` so callers can still address it there. Items are filtered, not reordered:
    /// a Miller column's own order (year, track number, ...) is meaningful, so match score decides
    /// inclusion only, never position.
    ///
    /// A `SectionHeader` is retained only if at least one item in its section (everything up to
    /// the next header, or the end of the list) survives the filter, and its `count` is
    /// recomputed to that filtered total — which is why this returns owned `Cow`s rather than
    /// plain `&MediaItem` references: a retained header's count differs from the one stored in
    /// `self.items`.
    pub fn visible_items(&self) -> Vec<(usize, Cow<'_, MediaItem>)> {
        let Some(filter) = self.filter.as_deref().filter(|f| !f.is_empty()) else {
            return self
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| (i, Cow::Borrowed(item)))
                .collect();
        };

        let atom = Atom::new(
            filter,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
            false,
        );
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf: Vec<char> = Vec::new();

        let mut result: Vec<(usize, Cow<'_, MediaItem>)> = Vec::with_capacity(self.items.len());
        // The position in `result` of the most recently pushed (still-tentative) header, its
        // original data, and how many of its section's items have survived so far.
        let mut pending: Option<(usize, SectionHeader, usize)> = None;

        for (i, item) in self.items.iter().enumerate() {
            match item {
                MediaItem::SectionHeader(header) => {
                    if let Some((pos, hdr, count)) = pending.take() {
                        finalize_header(&mut result, pos, hdr, count);
                    }
                    result.push((i, Cow::Borrowed(item)));
                    pending = Some((result.len() - 1, header.clone(), 0));
                }
                other => {
                    let haystack = Utf32Str::new(other.display_name(), &mut buf);
                    if atom.score(haystack, &mut matcher).is_some() {
                        if let Some((_, _, count)) = &mut pending {
                            *count += 1;
                        }
                        result.push((i, Cow::Borrowed(item)));
                    }
                }
            }
        }
        if let Some((pos, hdr, count)) = pending.take() {
            finalize_header(&mut result, pos, hdr, count);
        }
        result
    }

    /// Indices of every visible, selectable row — `SectionHeader`s are skipped, never selectable.
    pub fn selectable_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.visible_items()
            .into_iter()
            .filter(|(_, item)| item.is_selectable())
            .map(|(i, _)| i)
    }
}

/// Drops the header at `result[pos]` if its section had no surviving items (it is guaranteed to
/// still be the last element in that case — nothing else can have been pushed after it without
/// incrementing `count`), otherwise rewrites it in place with `count` recomputed.
fn finalize_header(
    result: &mut Vec<(usize, Cow<'_, MediaItem>)>,
    pos: usize,
    header: SectionHeader,
    count: usize,
) {
    if count == 0 {
        result.truncate(pos);
    } else {
        result[pos].1 = Cow::Owned(MediaItem::SectionHeader(SectionHeader { count, ..header }));
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NavState {
    /// **[persist]**
    pub active_tab: Tab,
    /// Each tab remembers its own drill state, so switching tabs and back returns the user where
    /// they were.
    pub per_tab_stacks: HashMap<Tab, Vec<Column>>,
    /// Index of the leftmost visible column (max 3 visible).
    pub window_start: usize,
    pub focus: NavFocus,
    /// For the non-Miller tabs (`Search`/`NowPlaying`) — whether focus is parked on the tab
    /// sidebar (so `↑`/`↓` switch tabs) rather than on the tab's own content (results / queue).
    /// Miller tabs express the same thing through `focus == NavFocus::Sidebar` instead; these two
    /// have no column stack to hang that off, so they carry it here. `false` (content-focused) is
    /// the resting state on tab entry — `↓`/`←` reach the sidebar, `→` returns to content
    /// (`docs/12-decisions.md`).
    #[serde(default)]
    pub sidebar_focused: bool,
    /// Whether the sidebar's "Quit" row (rendered beneath the tabs) is the focused item — reached by
    /// pressing `↓` past the last tab. `↑` returns to the last tab, `Enter`/`→` quit. Never persisted
    /// or set by anything but sidebar navigation, and cleared by any explicit tab jump/click
    /// (`set_tab`) so it can't linger (`docs/12-decisions.md`).
    #[serde(default)]
    pub sidebar_quit_focused: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Artist, SectionHeader, SectionKind};

    fn artist(id: &str) -> MediaItem {
        MediaItem::Artist(Artist {
            id: ItemId::from(id),
            name: id.to_string(),
            sort_name: id.to_string(),
            album_count: 0,
            track_count: 0,
            genres: Vec::new(),
            is_favorite: false,
            image: None,
            overview: None,
        })
    }

    fn header(label: &str) -> MediaItem {
        header_with_count(label, 0)
    }

    fn header_with_count(label: &str, count: usize) -> MediaItem {
        MediaItem::SectionHeader(SectionHeader {
            label: label.to_string(),
            count,
            kind: SectionKind::Albums,
        })
    }

    #[test]
    fn selectable_indices_skips_section_headers() {
        let mut column = Column::new(ColumnKind::Artists, "Artists");
        column.items = vec![artist("a"), header("── ALBUMS ──"), artist("b")];
        assert_eq!(column.selectable_indices().collect::<Vec<_>>(), vec![0, 2]);
    }

    #[test]
    fn visible_items_respects_filter() {
        let mut column = Column::new(ColumnKind::Artists, "Artists");
        column.items = vec![artist("Boy Harsher"), artist("Sync24")];
        column.filter = Some("harsh".to_string());
        let visible: Vec<_> = column.visible_items().into_iter().map(|(i, _)| i).collect();
        assert_eq!(visible, vec![0]);
    }

    #[test]
    fn filter_narrows_items() {
        let mut column = Column::new(ColumnKind::Artists, "Artists");
        column.items = vec![
            artist("Boy Harsher"),
            artist("Sync24"),
            artist("Molchat Doma"),
        ];
        column.filter = Some("doma".to_string());
        let visible: Vec<_> = column.visible_items().into_iter().map(|(i, _)| i).collect();
        assert_eq!(visible, vec![2]);
    }

    #[test]
    fn filter_is_case_insensitive() {
        let mut column = Column::new(ColumnKind::Artists, "Artists");
        column.items = vec![artist("Sync24")];
        column.filter = Some("SYNC".to_string());
        let visible: Vec<_> = column.visible_items().into_iter().map(|(i, _)| i).collect();
        assert_eq!(visible, vec![0]);
    }

    #[test]
    fn filter_preserves_original_order() {
        let mut column = Column::new(ColumnKind::Artists, "Artists");
        column.items = (0..10)
            .map(|i| {
                artist(if [1, 5, 9].contains(&i) {
                    "Zzztag"
                } else {
                    "Nope"
                })
            })
            .collect();
        column.filter = Some("zzztag".to_string());
        let visible: Vec<_> = column.visible_items().into_iter().map(|(i, _)| i).collect();
        assert_eq!(visible, vec![1, 5, 9]);
    }

    #[test]
    fn filter_retains_headers_with_surviving_items() {
        let mut column = Column::new(ColumnKind::Albums { of_artist: None }, "Albums");
        column.items = vec![
            header_with_count("ALBUMS", 2),
            artist("Wanted Alpha"),
            artist("Wanted Beta"),
        ];
        column.filter = Some("wanted".to_string());
        let visible = column.visible_items();
        assert_eq!(visible.len(), 3);
        assert!(
            matches!(visible[0].1.as_ref(), MediaItem::SectionHeader(h) if h.label == "ALBUMS")
        );
    }

    #[test]
    fn filter_drops_empty_sections() {
        let mut column = Column::new(ColumnKind::Albums { of_artist: None }, "Albums");
        column.items = vec![
            header_with_count("ALBUMS", 2),
            artist("Alpha"),
            artist("Beta"),
        ];
        column.filter = Some("no-such-query".to_string());
        assert!(column.visible_items().is_empty());
    }

    #[test]
    fn filter_recomputes_header_counts() {
        let mut column = Column::new(ColumnKind::Albums { of_artist: None }, "Albums");
        column.items = vec![
            header_with_count("ALBUMS", 3),
            artist("Wanted Alpha"),
            artist("Wanted Beta"),
            artist("Gamma"),
        ];
        column.filter = Some("wanted".to_string());
        let visible = column.visible_items();
        match visible[0].1.as_ref() {
            MediaItem::SectionHeader(h) => assert_eq!(h.count, 2),
            other => panic!("expected a header, got {other:?}"),
        }
    }

    #[test]
    fn filter_between_two_sections_only_retains_the_matching_one() {
        let mut column = Column::new(ColumnKind::Albums { of_artist: None }, "Albums");
        column.items = vec![
            header_with_count("ALBUMS", 2),
            artist("Wanted Alpha"),
            artist("Wanted Beta"),
            header_with_count("APPEARS ON", 2),
            artist("Delta"),
            artist("Epsilon"),
        ];
        column.filter = Some("wanted".to_string());
        let visible = column.visible_items();
        let labels: Vec<&str> = visible
            .iter()
            .filter_map(|(_, item)| match item.as_ref() {
                MediaItem::SectionHeader(h) => Some(h.label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            vec!["ALBUMS"],
            "APPEARS ON must be dropped, not just its items"
        );
        assert_eq!(visible.len(), 3);
    }
}
