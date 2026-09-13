//! Multi-criteria sort profiles (`06-04`): up to four stacked rules applied to the queue and to
//! browse columns.

use std::cmp::Ordering;

use crate::config::{Direction, SortField, SortProfile};
use crate::model::{MediaItem, Track};

/// A maximum of 4 rules is enforced at config validation (`01-02`) — a profile arriving here with
/// more is applied by its first 4, never panics.
const MAX_RULES: usize = 4;

/// One chunk of a "natural" sort key: a run of digits compared numerically, or a run of
/// non-digits compared as text — so `"Track 2"` sorts before `"Track 10"` instead of after it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Chunk {
    Num(u64),
    Text(String),
}

/// Lowercases and drops a leading `"The "`/`"A "`/`"An "` — otherwise every band beginning with
/// "The" clusters under T, which is not what anyone means by sorting by artist.
fn normalize(s: &str) -> String {
    let lower = s.to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(stripped) = lower.strip_prefix(article) {
            return stripped.to_string();
        }
    }
    lower
}

fn natural_key(s: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_is_digit = false;
    for c in s.chars() {
        let is_digit = c.is_ascii_digit();
        if !current.is_empty() && is_digit != current_is_digit {
            chunks.push(finish_chunk(&current, current_is_digit));
            current.clear();
        }
        current.push(c);
        current_is_digit = is_digit;
    }
    if !current.is_empty() {
        chunks.push(finish_chunk(&current, current_is_digit));
    }
    chunks
}

fn finish_chunk(s: &str, is_digit: bool) -> Chunk {
    if is_digit {
        Chunk::Num(s.parse().unwrap_or(0))
    } else {
        Chunk::Text(s.to_string())
    }
}

/// The string this field extracts from a track — empty string when the underlying data is
/// missing (per this task's own field table). `Name` falls back to `name` unconditionally:
/// `Track` (unlike `Artist`/`Album`) carries no `sort_name` field to prefer in the first place
/// (`docs/12-decisions.md`).
fn text_key(track: &Track, field: SortField) -> &str {
    match field {
        SortField::Name => &track.name,
        SortField::Artist => track.artist_names.first().map(String::as_str).unwrap_or(""),
        SortField::AlbumArtist => {
            let primary = track
                .album_artist_names
                .first()
                .map(String::as_str)
                .unwrap_or("");
            if primary.is_empty() {
                track.artist_names.first().map(String::as_str).unwrap_or("")
            } else {
                primary
            }
        }
        SortField::Album => &track.album_name,
        SortField::Genre => track.genres.first().map(String::as_str).unwrap_or(""),
        SortField::Year | SortField::TrackNumber | SortField::DateAdded => {
            unreachable!("handled directly in compare_field, never through text_key")
        }
    }
}

fn apply_direction(ord: Ordering, direction: Direction) -> Ordering {
    match direction {
        Direction::Asc => ord,
        Direction::Desc => ord.reverse(),
    }
}

/// `None` sorts **last regardless of direction** — reversing a descending sort must not promote
/// an unknown value to the top; it stays the least informative row either way. Only the
/// `Some`-vs-`Some` comparison is direction-reversed.
fn compare_option_last<T: Ord>(a: Option<T>, b: Option<T>, direction: Direction) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => apply_direction(a.cmp(&b), direction),
    }
}

fn compare_field(a: &Track, b: &Track, field: SortField, direction: Direction) -> Ordering {
    match field {
        SortField::Year => compare_option_last(a.year, b.year, direction),
        SortField::DateAdded => compare_option_last(a.date_created, b.date_created, direction),
        SortField::TrackNumber => {
            let ka = (a.disc_number.unwrap_or(0), a.track_number.unwrap_or(0));
            let kb = (b.disc_number.unwrap_or(0), b.track_number.unwrap_or(0));
            apply_direction(ka.cmp(&kb), direction)
        }
        SortField::Name
        | SortField::Artist
        | SortField::AlbumArtist
        | SortField::Album
        | SortField::Genre => {
            let ka = natural_key(&normalize(text_key(a, field)));
            let kb = natural_key(&normalize(text_key(b, field)));
            apply_direction(ka.cmp(&kb), direction)
        }
    }
}

/// The same digit-aware, case-insensitive ordering `compare` uses for text `SortField`s
/// (`normalize` + `natural_key`), exposed standalone for `07-05`'s folder/file row sorting, which
/// has no `Track`/`SortProfile` to hang a comparison off — this is what makes `"track2"` sort
/// before `"track10"` there too.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    natural_key(&normalize(a)).cmp(&natural_key(&normalize(b)))
}

/// Applies `profile`'s rules in order, each breaking the previous one's ties, then falls back to
/// `id` as an implicit final tiebreak — this is what makes the comparator total, and applying a
/// profile twice a no-op.
pub fn compare(a: &Track, b: &Track, profile: &SortProfile) -> Ordering {
    for rule in profile.rules.iter().take(MAX_RULES) {
        let ord = compare_field(a, b, rule.field, rule.direction);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.id.cmp(&b.id)
}

/// The representative year for each album present in `tracks` — the smallest non-`None`
/// `ProductionYear` among that album's own tracks. Chronological-discography sorting keys the
/// `Year` field off this rather than each track's own year: a live user found "artist + year +
/// album + track" scattering albums across the timeline, because individual tracks carried missing
/// or inconsistent year tags and `None` sorts last, yanking those tracks away from their album-mates
/// (`docs/12-decisions.md`).
pub fn album_years<'a>(
    tracks: impl Iterator<Item = &'a Track>,
) -> std::collections::HashMap<crate::model::ItemId, u16> {
    let mut map = std::collections::HashMap::new();
    for t in tracks {
        if let (Some(album_id), Some(year)) = (t.album_id.as_ref(), t.year) {
            map.entry(album_id.clone())
                .and_modify(|y: &mut u16| *y = (*y).min(year))
                .or_insert(year);
        }
    }
    map
}

fn album_year_of(
    t: &Track,
    years: &std::collections::HashMap<crate::model::ItemId, u16>,
) -> Option<u16> {
    t.album_id
        .as_ref()
        .and_then(|id| years.get(id).copied())
        .or(t.year)
}

/// Like [`compare`], but every `Year` rule reads the track's **album** year (from `years`, keyed by
/// `album_id`) instead of the track's own `ProductionYear`, so an album sorts as one unit even when
/// its tracks are tagged inconsistently. Every other field is compared exactly as [`compare`] does.
pub fn compare_with_album_year(
    a: &Track,
    b: &Track,
    profile: &SortProfile,
    years: &std::collections::HashMap<crate::model::ItemId, u16>,
) -> Ordering {
    for rule in profile.rules.iter().take(MAX_RULES) {
        let ord = if rule.field == SortField::Year {
            compare_option_last(
                album_year_of(a, years),
                album_year_of(b, years),
                rule.direction,
            )
        } else {
            compare_field(a, b, rule.field, rule.direction)
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.id.cmp(&b.id)
}

pub fn sort_tracks(tracks: &mut [Track], profile: &SortProfile) {
    tracks.sort_by(|a, b| compare(a, b, profile));
}

/// Sorts only within sections: a maximal run of consecutive `MediaItem::Track` items is sorted
/// as a group, and every other item (a `SectionHeader`, or any non-`Track` row) is left exactly
/// where it is, acting as an immovable boundary between runs. `SortField`'s field table only ever
/// defines extraction from a `Track` — there is no defined mapping onto `Artist`/`Album`/etc, so a
/// column of non-`Track` items (e.g. a bare discography Albums/Appears-On split, before drilling
/// into a specific album's Tracks) is correctly left untouched rather than guessed at
/// (`docs/12-decisions.md`).
pub fn sort_items(items: &mut [MediaItem], profile: &SortProfile) {
    let mut start = 0;
    while start < items.len() {
        if !matches!(items[start], MediaItem::Track(_)) {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < items.len() && matches!(items[end], MediaItem::Track(_)) {
            end += 1;
        }
        items[start..end].sort_by(|a, b| match (a, b) {
            (MediaItem::Track(ta), MediaItem::Track(tb)) => compare(ta, tb, profile),
            _ => unreachable!("run contains only Track items by construction"),
        });
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Direction, SortField, SortRule};
    use crate::model::{SectionHeader, SectionKind};
    use crate::test_support::fixtures;
    use proptest::prelude::*;

    fn rule(field: SortField, direction: Direction) -> SortRule {
        SortRule { field, direction }
    }

    fn profile(name: &str, rules: Vec<SortRule>) -> SortProfile {
        SortProfile {
            name: name.to_string(),
            rules,
        }
    }

    fn chronological_discog() -> SortProfile {
        use Direction::Asc;
        use SortField::{Album as AlbumField, AlbumArtist, TrackNumber, Year};
        profile(
            "chronological_discog",
            vec![
                rule(AlbumArtist, Asc),
                rule(Year, Asc),
                rule(AlbumField, Asc),
                rule(TrackNumber, Asc),
            ],
        )
    }

    fn release_chronology() -> SortProfile {
        use Direction::{Asc, Desc};
        use SortField::{Album as AlbumField, TrackNumber, Year};
        profile(
            "release_chronology",
            vec![
                rule(Year, Desc),
                rule(AlbumField, Asc),
                rule(TrackNumber, Asc),
            ],
        )
    }

    /// One artist, two albums (one earlier, one later), each with 2 tracks — enough to exercise
    /// every rule in `chronological_discog` deciding at a different level.
    fn discog_fixture() -> Vec<Track> {
        let artist = fixtures::artist("Boy Harsher");
        let early = fixtures::album("Careful", 2017, &artist);
        let late = fixtures::album("Care", 2019, &artist);
        vec![
            fixtures::track("Fate", 2, &late, &[&artist]),
            fixtures::track("Motion", 1, &early, &[&artist]),
            fixtures::track("Come Closer", 3, &late, &[&artist]),
            fixtures::track("Deep Well", 2, &early, &[&artist]),
        ]
    }

    #[test]
    fn sort_profile_applies_all_four_rules_in_order() {
        let mut tracks = discog_fixture();
        sort_tracks(&mut tracks, &chronological_discog());
        let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
        // Same artist throughout; year ascending puts "Careful" (2017) before "Care" (2019);
        // within each album, track number ascending.
        assert_eq!(names, vec!["Motion", "Deep Well", "Fate", "Come Closer"]);
    }

    #[test]
    fn chronological_discog_keeps_an_album_together_despite_a_dirty_track_year() {
        let artist = fixtures::artist("Boy Harsher");
        let early = fixtures::album("Careful", 2017, &artist);
        let late = fixtures::album("Care", 2019, &artist);
        let mut tracks = vec![
            fixtures::track("Motion", 1, &early, &[&artist]),
            fixtures::track("Deep Well", 2, &early, &[&artist]),
            fixtures::track("Fate", 1, &late, &[&artist]),
            fixtures::track("Come Closer", 2, &late, &[&artist]),
        ];
        // A dirty tag: one 2017-album track carries no year. With the per-track comparator, `None`
        // sorts last and rips it away from its album-mate — exactly the reported "years and albums
        // mismatch".
        tracks[1].year = None;

        let mut plain = tracks.clone();
        plain.sort_by(|a, b| compare(a, b, &chronological_discog()));
        assert_eq!(
            plain.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["Motion", "Fate", "Come Closer", "Deep Well"],
            "per-track year scatters the album — this documents the bug"
        );

        // Keyed off the album's year, the 2017 album stays whole and before the 2019 one.
        let years = album_years(tracks.iter());
        tracks.sort_by(|a, b| compare_with_album_year(a, b, &chronological_discog(), &years));
        assert_eq!(
            tracks.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["Motion", "Deep Well", "Fate", "Come Closer"],
        );
    }

    #[test]
    fn release_chronology_profile_matches_expected() {
        let mut tracks = discog_fixture();
        sort_tracks(&mut tracks, &release_chronology());
        let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
        // Year descending: "Care" (2019) before "Careful" (2017); track number ascending within.
        assert_eq!(names, vec!["Fate", "Come Closer", "Motion", "Deep Well"]);
    }

    #[test]
    fn sort_is_stable_for_equal_keys() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let mut t1 = fixtures::track("Same", 1, &alb, &[&a]);
        let mut t2 = fixtures::track("Same", 1, &alb, &[&a]);
        // Deliberately equal ids too, so the comparator (which appends `id` as its own final
        // tiebreak) is genuinely `Equal` for both — a stable sort must then keep original order.
        t1.id = crate::model::ItemId::from("same-id");
        t2.id = crate::model::ItemId::from("same-id");
        let mut tracks = vec![t1.clone(), t2.clone()];
        sort_tracks(
            &mut tracks,
            &profile("by_name", vec![rule(SortField::Name, Direction::Asc)]),
        );
        assert_eq!(tracks, vec![t1, t2]);
    }

    #[test]
    fn none_sorts_last_ascending() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let mut with_year = fixtures::track("Has Year", 1, &alb, &[&a]);
        with_year.year = Some(2000);
        let mut without_year = fixtures::track("No Year", 2, &alb, &[&a]);
        without_year.year = None;

        let mut tracks = vec![without_year.clone(), with_year.clone()];
        sort_tracks(
            &mut tracks,
            &profile("by_year", vec![rule(SortField::Year, Direction::Asc)]),
        );
        assert_eq!(tracks, vec![with_year, without_year]);
    }

    #[test]
    fn none_sorts_last_descending() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let mut with_year = fixtures::track("Has Year", 1, &alb, &[&a]);
        with_year.year = Some(2000);
        let mut without_year = fixtures::track("No Year", 2, &alb, &[&a]);
        without_year.year = None;

        let mut tracks = vec![without_year.clone(), with_year.clone()];
        sort_tracks(
            &mut tracks,
            &profile("by_year", vec![rule(SortField::Year, Direction::Desc)]),
        );
        assert_eq!(
            tracks,
            vec![with_year, without_year],
            "None must stay last even when direction reverses"
        );
    }

    #[test]
    fn leading_article_is_ignored() {
        let a1 = fixtures::artist("The Soft Moon");
        let a2 = fixtures::artist("Boy Harsher");
        let alb1 = fixtures::album("Alb1", 2020, &a1);
        let alb2 = fixtures::album("Alb2", 2020, &a2);
        let t_soft_moon = fixtures::track("Track", 1, &alb1, &[&a1]);
        let t_boy_harsher = fixtures::track("Track", 1, &alb2, &[&a2]);

        let mut tracks = vec![t_soft_moon.clone(), t_boy_harsher.clone()];
        sort_tracks(
            &mut tracks,
            &profile("by_artist", vec![rule(SortField::Artist, Direction::Asc)]),
        );
        // "The Soft Moon" sorts under S, so "Boy Harsher" (B) comes first.
        assert_eq!(tracks, vec![t_boy_harsher, t_soft_moon]);
    }

    #[test]
    fn case_insensitive_comparison() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let upper = fixtures::track("APPLE", 1, &alb, &[&a]);
        let lower = fixtures::track("banana", 2, &alb, &[&a]);
        let mut tracks = vec![lower.clone(), upper.clone()];
        sort_tracks(
            &mut tracks,
            &profile("by_name", vec![rule(SortField::Name, Direction::Asc)]),
        );
        assert_eq!(tracks, vec![upper, lower]);
    }

    #[test]
    fn numeric_aware_comparison() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let t2 = fixtures::track("Track 2", 1, &alb, &[&a]);
        let t10 = fixtures::track("Track 10", 2, &alb, &[&a]);
        let mut tracks = vec![t10.clone(), t2.clone()];
        sort_tracks(
            &mut tracks,
            &profile("by_name", vec![rule(SortField::Name, Direction::Asc)]),
        );
        assert_eq!(tracks, vec![t2, t10]);
    }

    #[test]
    fn track_number_sorts_by_disc_then_track() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let mut disc1_track2 = fixtures::track("A", 2, &alb, &[&a]);
        disc1_track2.disc_number = Some(1);
        let mut disc2_track1 = fixtures::track("B", 1, &alb, &[&a]);
        disc2_track1.disc_number = Some(2);

        let mut tracks = vec![disc2_track1.clone(), disc1_track2.clone()];
        sort_tracks(
            &mut tracks,
            &profile(
                "by_track_number",
                vec![rule(SortField::TrackNumber, Direction::Asc)],
            ),
        );
        assert_eq!(tracks, vec![disc1_track2, disc2_track1]);
    }

    fn section_header(label: &str, count: usize) -> MediaItem {
        MediaItem::SectionHeader(SectionHeader {
            label: label.to_string(),
            count,
            kind: SectionKind::Custom,
        })
    }

    #[test]
    fn sort_items_does_not_move_headers() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let mut items = vec![
            section_header("GROUP 1", 2),
            MediaItem::Track(fixtures::track("Zeta", 1, &alb, &[&a])),
            MediaItem::Track(fixtures::track("Alpha", 2, &alb, &[&a])),
        ];
        sort_items(
            &mut items,
            &profile("by_name", vec![rule(SortField::Name, Direction::Asc)]),
        );
        assert!(matches!(items[0], MediaItem::SectionHeader(_)));
        assert_eq!(items[1].display_name(), "Alpha");
        assert_eq!(items[2].display_name(), "Zeta");
    }

    #[test]
    fn sort_items_does_not_merge_sections() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let mut items = vec![
            section_header("GROUP 1", 1),
            MediaItem::Track(fixtures::track("Zeta", 1, &alb, &[&a])),
            section_header("GROUP 2", 1),
            MediaItem::Track(fixtures::track("Alpha", 2, &alb, &[&a])),
        ];
        let before = items.clone();
        sort_items(
            &mut items,
            &profile("by_name", vec![rule(SortField::Name, Direction::Asc)]),
        );
        // Each section has exactly one track: nothing to reorder within a section, and no track
        // may cross a header into the other section.
        assert_eq!(items, before);
    }

    /// A raw discography column (Albums, not Tracks) has no `Track` rows at all — `sort_items`
    /// must leave it untouched rather than guess at an `Album`-specific field mapping
    /// (`docs/12-decisions.md`).
    #[test]
    fn sort_items_leaves_non_track_columns_untouched() {
        let artist = fixtures::artist("Sync24");
        let zeta = MediaItem::Album(fixtures::album("Zeta", 2020, &artist));
        let alpha = MediaItem::Album(fixtures::album("Alpha", 2019, &artist));
        let mut items = vec![zeta.clone(), alpha.clone()];
        sort_items(
            &mut items,
            &profile("by_name", vec![rule(SortField::Name, Direction::Asc)]),
        );
        assert_eq!(items, vec![zeta, alpha]);
    }

    proptest! {
        #[test]
        fn sort_is_idempotent(seed in any::<u64>(), n in 1usize..15) {
            let a = fixtures::artist("A");
            let alb = fixtures::album("Alb", 2020, &a);
            let mut tracks: Vec<Track> = (0..n)
                .map(|i| {
                    let mut t = fixtures::track(&format!("T{i}"), (i as u32) % 4, &alb, &[&a]);
                    t.year = if i % 3 == 0 {
                        None
                    } else {
                        Some(2000 + ((seed.wrapping_add(i as u64)) % 30) as u16)
                    };
                    t
                })
                .collect();

            sort_tracks(&mut tracks, &chronological_discog());
            let once = tracks.clone();
            sort_tracks(&mut tracks, &chronological_discog());
            prop_assert_eq!(tracks, once);
        }
    }
}
