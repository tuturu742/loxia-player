//! Artist, Album, Track, Genre, Folder, Playlist, MediaItem, SectionHeader, AlbumRelation.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::audio_meta::{AudioFormat, ReplayGainInfo};
use super::ids::{ItemId, MediaSourceId, PlaylistEntryId};
use crate::Timestamp;

/// The unit a navigation column holds.
///
/// `Track` (`07-03`'s `playlist_entry_id` tipped it over clippy's size-difference threshold
/// against `Album`/`Playlist`) is intentionally not boxed: every column, the queue, search
/// results, and favourites all match `MediaItem::Track(t)` directly and read its fields by
/// reference (`docs/12-decisions.md`); boxing would ripple `Box::new`/deref through dozens of
/// call sites for a lint about stack-copy cost this app never pays — nothing here clones
/// `Vec<MediaItem>` in a hot loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum MediaItem {
    Artist(Artist),
    Album(Album),
    Track(Track),
    Genre(Genre),
    Folder(Folder),
    Playlist(Playlist),
    SectionHeader(SectionHeader),
}

impl MediaItem {
    /// `None` for `SectionHeader` — headers are not addressable items.
    pub fn id(&self) -> Option<&ItemId> {
        match self {
            MediaItem::Artist(a) => Some(&a.id),
            MediaItem::Album(a) => Some(&a.id),
            MediaItem::Track(t) => Some(&t.id),
            MediaItem::Genre(g) => Some(&g.id),
            MediaItem::Folder(f) => Some(&f.id),
            MediaItem::Playlist(p) => Some(&p.id),
            MediaItem::SectionHeader(_) => None,
        }
    }

    /// `false` for `SectionHeader` — this is what the navigation reducer uses to skip headers
    /// during cursor movement and selection, so every consumer skips them consistently.
    pub fn is_selectable(&self) -> bool {
        !matches!(self, MediaItem::SectionHeader(_))
    }

    pub fn display_name(&self) -> &str {
        match self {
            MediaItem::Artist(a) => &a.name,
            MediaItem::Album(a) => &a.name,
            MediaItem::Track(t) => &t.name,
            MediaItem::Genre(g) => &g.name,
            MediaItem::Folder(f) => &f.name,
            MediaItem::Playlist(p) => &p.name,
            MediaItem::SectionHeader(s) => &s.label,
        }
    }
}

/// A non-selectable divider row, e.g. `── ALBUMS (2) ──` / `── APPEARS ON (2) ──`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionHeader {
    pub label: String,
    pub count: usize,
    pub kind: SectionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionKind {
    Albums,
    AppearsOn,
    Custom,
}

/// Where an item's primary artwork actually lives — the item holding the image, and the tag that
/// versions it.
///
/// Usually the item itself, but Emby routinely reports an album with an empty `ImageTags` and a
/// `PrimaryImageItemId`/`PrimaryImageTag` pointing at whichever child actually holds the cover. On
/// a real library two thirds of albums are like that, which is why the inspector showed a
/// placeholder for most albums while tracks — which nearly always carry their own tag — worked
/// (`docs/12-decisions.md`). Keeping the id and the tag together makes it impossible to build a
/// URL for the item being *displayed* when the image lives somewhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRef {
    pub item: ItemId,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artist {
    pub id: ItemId,
    pub name: String,
    pub sort_name: String,
    /// Zero means **unknown**, not "this artist has no albums": Emby reports no album count on an
    /// artist item at all, so this stays zero until a discography fetch
    /// (`endpoints::discography`) fills it in. Render via [`Artist::counts_summary`] rather than
    /// formatting it directly.
    pub album_count: u32,
    /// From Emby's `ChildCount`, which counts an artist's tracks. Zero means unknown here too —
    /// an artist in a music library always has at least one track.
    pub track_count: u32,
    pub genres: Vec<String>,
    pub is_favorite: bool,
    pub image: Option<ImageRef>,
    pub overview: Option<String>,
}

impl Artist {
    /// `"12 albums, 143 tracks"` — or just `"143 tracks"` when only the track count is known,
    /// which is the usual case in a browsing list, or `None` when neither is.
    ///
    /// The Artists column used to read `0 albums` beside every artist in the library: the album
    /// count is genuinely never reported for an artist item, and the track count was being
    /// discarded by the DTO conversion despite the server sending it (`docs/12-decisions.md`).
    /// Zero therefore means "not known", never "none" — an artist with no tracks cannot appear in
    /// a music library's artist list in the first place — and each count is shown only once it is
    /// real.
    pub fn counts_summary(&self) -> Option<String> {
        match (self.album_count, self.track_count) {
            (0, 0) => None,
            (0, tracks) => Some(format!("{tracks} tracks")),
            (albums, 0) => Some(format!("{albums} albums")),
            (albums, tracks) => Some(format!("{albums} albums, {tracks} tracks")),
        }
    }
}

/// Whether an album is a primary release by the artist in the active discography query, or a
/// compilation/soundtrack the artist merely appears on. Assigned by
/// `loxia-emby::endpoints::discography` (see `docs/03-emby-api.md` §4) — never guessed in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlbumRelation {
    Primary,
    AppearsOn { context_artist: ItemId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: ItemId,
    pub name: String,
    pub sort_name: String,
    pub album_artist_names: Vec<String>,
    pub album_artist_ids: Vec<ItemId>,
    pub year: Option<u16>,
    pub track_count: u32,
    pub total_duration: Duration,
    pub genres: Vec<String>,
    pub is_favorite: bool,
    pub image: Option<ImageRef>,
    pub relation: AlbumRelation,
}

/// The lyric format discovered on a track's subtitle-type media stream (`docs/03-emby-api.md` §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricFormat {
    Lrc,
    Srt,
    Txt,
}

/// A reference to a track's lyric subtitle stream — the discovery result, not the lyrics
/// themselves (those are fetched lazily and cached separately, see `docs/03-emby-api.md` §7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricStreamRef {
    pub media_source_id: MediaSourceId,
    pub stream_index: u32,
    pub format: LyricFormat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: ItemId,
    pub name: String,
    pub album_id: Option<ItemId>,
    pub album_name: String,
    pub album_artist_names: Vec<String>,
    /// What the Appears-On queue filter tests against. Populated from Emby's `ArtistItems`
    /// field — never parsed out of a display string, which is ambiguous.
    pub artist_ids: Vec<ItemId>,
    pub artist_names: Vec<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u16>,
    pub duration: Duration,
    pub genres: Vec<String>,
    pub is_favorite: bool,
    pub play_count: u32,
    pub format: AudioFormat,
    pub replay_gain: Option<ReplayGainInfo>,
    pub image: Option<ImageRef>,
    pub media_source_id: Option<MediaSourceId>,
    pub lyric_stream: Option<LyricStreamRef>,
    pub date_created: Option<Timestamp>,
    /// Set only on a row inside a `ColumnKind::PlaylistTracks` column (`07-03`) — Emby's
    /// `PlaylistRemove`/`PlaylistMove` need this per-row id, not `id` itself, since the same
    /// track can appear in a playlist more than once. `None` everywhere else a `Track` appears
    /// (search results, favourites, a generic Tracks column, the queue, ...).
    pub playlist_entry_id: Option<PlaylistEntryId>,
}

/// Deliberately minimal: every consumer (`items::genres`/`genre_artists`, the Genres tab) only
/// ever needs the id for the fetch and the name for the row label and the genre-name filter Emby
/// itself requires (`docs/03-emby-api.md` §3 — Emby filters genres by name, not id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Genre {
    pub id: ItemId,
    pub name: String,
}

/// Equally minimal: the Folders tab fetches non-recursively by parent id and renders the child's
/// own name; ancestry is tracked by the Miller column stack itself, not by this type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: ItemId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: ItemId,
    pub name: String,
    pub overview: Option<String>,
    pub track_count: u32,
    pub total_duration: Duration,
    pub can_edit: bool,
    /// Emby favourites any item type, playlists included — verified against a live server, which
    /// both records `UserData.IsFavorite` on a playlist and returns it from an `IsFavorite` query.
    /// The field was simply missing here, so `f` on a playlist row had nothing to read and did
    /// nothing (`docs/12-decisions.md`).
    pub is_favorite: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::audio_meta::Codec;

    fn artist_with_counts(albums: u32, tracks: u32) -> Artist {
        Artist {
            id: ItemId::from("ar1"),
            name: "Boy Harsher".to_string(),
            sort_name: "Boy Harsher".to_string(),
            album_count: albums,
            track_count: tracks,
            genres: Vec::new(),
            is_favorite: false,
            image: None,
            overview: None,
        }
    }

    /// Nothing known at all: say nothing, rather than the `0 albums` that used to sit beside every
    /// artist in the library (`docs/12-decisions.md`).
    #[test]
    fn unknown_artist_counts_have_no_summary() {
        assert_eq!(artist_with_counts(0, 0).counts_summary(), None);
    }

    /// Each count is reported only once it is real. A browsing list has the track count (Emby's
    /// `ChildCount`) but never the album count, so that one-sided case is the common one and must
    /// not drag a fabricated `0 albums` along with it.
    #[test]
    fn each_artist_count_is_shown_only_once_it_is_known() {
        assert_eq!(
            artist_with_counts(12, 143).counts_summary().as_deref(),
            Some("12 albums, 143 tracks")
        );
        assert_eq!(
            artist_with_counts(0, 143).counts_summary().as_deref(),
            Some("143 tracks")
        );
        assert_eq!(
            artist_with_counts(12, 0).counts_summary().as_deref(),
            Some("12 albums")
        );
    }

    fn track_fixture(name: &str) -> Track {
        Track {
            id: ItemId::from("t1"),
            name: name.to_string(),
            album_id: Some(ItemId::from("al1")),
            album_name: "Care".to_string(),
            album_artist_names: vec!["Boy Harsher".to_string()],
            artist_ids: vec![ItemId::from("ar1")],
            artist_names: vec!["Boy Harsher".to_string()],
            track_number: Some(3),
            disc_number: Some(1),
            year: Some(2019),
            duration: Duration::from_secs(211),
            genres: vec!["Darkwave".to_string()],
            is_favorite: false,
            play_count: 0,
            format: AudioFormat {
                codec: Codec::Flac,
                sample_rate_hz: 44_100,
                bit_depth: Some(16),
                channels: 2,
                bitrate_bps: Some(1_012_000),
            },
            replay_gain: None,
            image: None,
            media_source_id: None,
            lyric_stream: None,
            date_created: None,
            playlist_entry_id: None,
        }
    }

    #[test]
    fn section_header_is_not_selectable() {
        let h = MediaItem::SectionHeader(SectionHeader {
            label: "ALBUMS".to_string(),
            count: 2,
            kind: SectionKind::Albums,
        });
        assert!(!h.is_selectable());
    }

    #[test]
    fn media_item_id_is_none_for_header() {
        let h = MediaItem::SectionHeader(SectionHeader {
            label: "ALBUMS".to_string(),
            count: 2,
            kind: SectionKind::Albums,
        });
        assert!(h.id().is_none());
    }

    #[test]
    fn media_item_id_is_some_for_every_other_variant() {
        let track = MediaItem::Track(track_fixture("Motion"));
        assert!(track.is_selectable());
        assert_eq!(track.id().unwrap().as_str(), "t1");
        assert_eq!(track.display_name(), "Motion");
    }

    #[test]
    fn model_types_roundtrip_serde() {
        let t = track_fixture("Motion");
        let json = serde_json::to_string(&t).unwrap();
        let back: Track = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    proptest::proptest! {
        #[test]
        fn track_roundtrips_for_arbitrary_names(name in ".{0,40}") {
            let t = track_fixture(&name);
            let json = serde_json::to_string(&t).unwrap();
            let back: Track = serde_json::from_str(&json).unwrap();
            proptest::prop_assert_eq!(t, back);
        }
    }
}

/// Expands a "now playing" template (`config.ui.now_playing_format`) against `track`.
///
/// Placeholders are `{name}`; an unrecognised one is left **verbatim** so a typo shows up on screen
/// rather than silently blanking part of the line. A placeholder whose data is missing (no year, no
/// genre) expands to an empty string, and any resulting double spacing / empty bracket pair is
/// tidied so `"{title} [{year}]"` doesn't leave a stray `[]` for an untagged track
/// (`docs/12-decisions.md`).
pub fn format_now_playing(template: &str, track: &Track) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // An unclosed brace is literal text, not a broken placeholder.
            out.push_str(&rest[open..]);
            return tidy(&out);
        };
        let key = &after[..close];
        match placeholder_value(key, track) {
            Some(value) => out.push_str(&value),
            None => {
                out.push('{');
                out.push_str(key);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    tidy(&out)
}

fn placeholder_value(key: &str, t: &Track) -> Option<String> {
    Some(match key {
        "title" => t.name.clone(),
        "artist" => t.artist_names.join(", "),
        "album_artist" => t.album_artist_names.join(", "),
        "album" => t.album_name.clone(),
        "year" => t.year.map(|y| y.to_string()).unwrap_or_default(),
        "track_number" => t.track_number.map(|n| n.to_string()).unwrap_or_default(),
        "disc_number" => t.disc_number.map(|n| n.to_string()).unwrap_or_default(),
        "genre" => t.genres.first().cloned().unwrap_or_default(),
        "duration" => {
            let secs = t.duration.as_secs();
            format!("{}:{:02}", secs / 60, secs % 60)
        }
        _ => return None,
    })
}

/// Removes the debris an empty placeholder leaves behind — an empty `()`/`[]` pair, a doubled
/// separator, and leading/trailing whitespace or punctuation.
fn tidy(s: &str) -> String {
    let mut out = s.replace("()", "").replace("[]", "").replace("{}", "");
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    out.trim()
        .trim_end_matches(['-', '\u{2014}', '\u{b7}', ','])
        .trim()
        .to_string()
}

#[cfg(test)]
mod now_playing_format_tests {
    use super::*;
    use crate::test_support::fixtures;

    fn track_with_year(year: Option<u16>) -> Track {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let mut t = fixtures::track("Motion", 3, &alb, &[&a]);
        t.year = year;
        t
    }

    #[test]
    fn default_template_matches_the_previous_hardcoded_line() {
        let t = track_with_year(Some(2019));
        assert_eq!(
            format_now_playing("{title} — {artist} ({album})", &t),
            "Motion — Boy Harsher (Care)"
        );
    }

    #[test]
    fn a_user_can_reorder_and_add_fields() {
        let t = track_with_year(Some(2019));
        assert_eq!(
            format_now_playing("{artist} - {title} [year: {year}] {album}", &t),
            "Boy Harsher - Motion [year: 2019] Care"
        );
        assert_eq!(
            format_now_playing("{track_number}. {title}", &t),
            "3. Motion"
        );
    }

    #[test]
    fn a_missing_value_does_not_leave_empty_brackets_or_double_spaces() {
        let t = track_with_year(None);
        assert_eq!(format_now_playing("{title} ({year})", &t), "Motion");
        assert_eq!(format_now_playing("{title} [{year}]", &t), "Motion");
        assert_eq!(format_now_playing("{title} — {year}", &t), "Motion");
    }

    /// A typo must be visible, not silently blank the line.
    #[test]
    fn an_unknown_placeholder_is_left_verbatim() {
        let t = track_with_year(Some(2019));
        assert_eq!(format_now_playing("{title} {nope}", &t), "Motion {nope}");
    }

    #[test]
    fn an_unclosed_brace_is_literal_text() {
        let t = track_with_year(Some(2019));
        assert_eq!(format_now_playing("{title} {oops", &t), "Motion {oops");
    }
}
