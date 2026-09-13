//! Appears-on track filtering for the `a`/`A` queue rules (`06-02`).

use crate::model::{ItemId, Track};

/// `ArtistOnly` matches **by id, never by name** — name matching breaks on "The Beatles" versus
/// "Beatles, The", and on any artist whose name is a substring of another's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackFilter {
    All,
    ArtistOnly(ItemId),
}

pub fn filter_tracks(tracks: &[Track], filter: &TrackFilter) -> Vec<Track> {
    match filter {
        TrackFilter::All => tracks.to_vec(),
        TrackFilter::ArtistOnly(id) => tracks
            .iter()
            .filter(|t| t.artist_ids.contains(id))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixtures;

    #[test]
    fn all_keeps_every_track() {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let tracks = vec![
            fixtures::track("One", 1, &alb, &[&a]),
            fixtures::track("Two", 2, &alb, &[&a]),
        ];
        assert_eq!(filter_tracks(&tracks, &TrackFilter::All), tracks);
    }

    /// Two artists whose names are substrings of each other — a name-based filter would either
    /// match both or neither; id-based matching must pick exactly the right one regardless.
    #[test]
    fn filter_matches_by_id_not_name() {
        let beatles = fixtures::artist("Beatles");
        let fab_beatles = fixtures::artist("Fab Beatles");
        let alb = fixtures::appears_on_album("Various", 1970, &beatles);
        let by_beatles = fixtures::track("Yesterday", 1, &alb, &[&beatles]);
        let by_fab_beatles = fixtures::track("Tomorrow", 2, &alb, &[&fab_beatles]);
        let tracks = vec![by_beatles.clone(), by_fab_beatles];

        let filtered = filter_tracks(&tracks, &TrackFilter::ArtistOnly(beatles.id.clone()));
        assert_eq!(filtered, vec![by_beatles]);
    }

    #[test]
    fn artist_only_excludes_non_matching_tracks() {
        let a = fixtures::artist("A");
        let b = fixtures::artist("B");
        let alb = fixtures::appears_on_album("Comp", 2020, &a);
        let mine = fixtures::track("Mine", 1, &alb, &[&a]);
        let theirs = fixtures::track("Theirs", 2, &alb, &[&b]);
        let tracks = vec![mine.clone(), theirs];

        let filtered = filter_tracks(&tracks, &TrackFilter::ArtistOnly(a.id.clone()));
        assert_eq!(filtered, vec![mine]);
    }
}
