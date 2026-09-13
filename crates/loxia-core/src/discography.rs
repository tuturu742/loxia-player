//! The ALBUMS / APPEARS ON split, shared between the online path (`loxia-emby`'s `02-06`) and the
//! offline path (`loxia-cache`'s `08-05`), so the two never drift apart (`docs/06-cache-and-offline.md`
//! §6, `docs/12-decisions.md`).

use crate::model::{Album, AlbumRelation, ItemId, Track};

/// An artist's discography, split into their own primary releases and everything they merely
/// appear on. `loxia-emby::endpoints::discography::discography` builds this from two server-side
/// album-level queries diffed by id; `loxia_cache::offline_index::OfflineIndex::discography`
/// builds it from downloaded tracks' own sidecars via [`classify`] — both return this same type,
/// so a caller (and `offline_discography_split_matches_online`, `08-05`) never has to reconcile
/// two shapes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Discography {
    pub primary: Vec<Album>,
    pub appears_on: Vec<Album>,
}

/// Whether `track` is `artist`'s own primary release or one they merely appear on, derived purely
/// from `track`'s own fields — `docs/03-emby-api.md` §4's rule ("primary iff the album's own
/// album-artist(s) include this artist") expressed against data a single downloaded track
/// actually carries. This is the offline path's *only* option: unlike the online path (which diffs
/// two id-filtered album-level queries), there is no `AlbumArtistIds` server query to run against a
/// local sidecar, and `Track` itself has no `album_artist_ids` field, only
/// `album_artist_names: Vec<String>` (`docs/12-decisions.md`) — so this locates `artist` among
/// `track.artist_ids` first (id-based, never ambiguous), reads *that* contributor's own name back
/// out of the parallel `track.artist_names`, and checks whether *that* name appears in
/// `track.album_artist_names`. `None` if `artist` is not a contributor to `track` at all.
pub fn classify(track: &Track, artist: &ItemId) -> Option<AlbumRelation> {
    let idx = track.artist_ids.iter().position(|id| id == artist)?;
    let name = track.artist_names.get(idx)?;
    if track.album_artist_names.iter().any(|n| n == name) {
        Some(AlbumRelation::Primary)
    } else {
        Some(AlbumRelation::AppearsOn {
            context_artist: artist.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixtures;

    #[test]
    fn classify_finds_primary_via_shared_name() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        assert_eq!(classify(&t, &a.id), Some(AlbumRelation::Primary));
    }

    #[test]
    fn classify_finds_appears_on_when_name_not_in_album_artists() {
        let main = fixtures::artist("Various Artists");
        let contributor = fixtures::artist("Boy Harsher");
        let alb = fixtures::appears_on_album("Compilation", 2020, &main);
        let t = fixtures::track("Track", 1, &alb, &[&contributor]);
        assert_eq!(
            classify(&t, &contributor.id),
            Some(AlbumRelation::AppearsOn {
                context_artist: contributor.id.clone()
            })
        );
    }

    #[test]
    fn classify_is_none_for_a_non_contributor() {
        let a = fixtures::artist("Boy Harsher");
        let stranger = fixtures::artist("Someone Else");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        assert_eq!(classify(&t, &stranger.id), None);
    }

    #[test]
    fn classify_matches_by_exact_name_not_substring() {
        // The same substring-name trap `queue::appears_on`'s own tests guard against: two artists
        // whose names are substrings of each other must never be confused. `album_artist_names`
        // comparison is exact-string (`==`), never `.contains()`, so "Fab Beatles" contributing to
        // a "Beatles" album is not mistaken for the album artist themselves.
        let beatles = fixtures::artist("Beatles");
        let fab_beatles = fixtures::artist("Fab Beatles");
        let alb = fixtures::album("Revolver", 1966, &beatles);
        let t = fixtures::track("Yesterday", 1, &alb, &[&fab_beatles]);
        assert_eq!(
            classify(&t, &fab_beatles.id),
            Some(AlbumRelation::AppearsOn {
                context_artist: fab_beatles.id.clone()
            })
        );
    }
}
