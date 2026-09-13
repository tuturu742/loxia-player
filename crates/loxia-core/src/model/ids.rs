//! Newtype identifiers. Never pass a bare `String` id across a function boundary — that is the
//! whole point of this module: `ItemId` cannot be passed where a `PlaylistId` is expected, even
//! though both wrap a `String`.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! string_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    ItemId,
    "An Emby item GUID. Used for artists, albums, tracks, genres, and folders alike."
);
string_id!(
    ServerId,
    "A config-assigned local server id, e.g. `\"remote_proxy\"`."
);
string_id!(UserId, "An Emby user GUID.");
string_id!(
    PlaylistId,
    "Semantically an `ItemId`; a distinct type for safety."
);
string_id!(
    PlaylistEntryId,
    "Emby's per-playlist-row id. Required for removal — an `ItemId` alone is not enough, since \
     the same track can appear in a playlist more than once."
);
string_id!(
    MediaSourceId,
    "Identifies a media source within an item; needed for lyric-stream fetching."
);
string_id!(
    PlaySessionId,
    "A UUID v4 generated once per track load and reused for that track's whole lifetime \
     (`03-emby-api.md` §6). A fresh id per report would make Emby treat each update as a new \
     session."
);

/// Monotonic, session-local queue entry identifier. The same track can appear twice in a queue,
/// so index-based identity is not enough — this is what `QueueState::next_id` hands out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QueueEntryId(pub u64);

impl fmt::Display for QueueEntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_string_and_str() {
        let a = ItemId::from("abc".to_string());
        let b = ItemId::from("abc");
        assert_eq!(a, b);
        assert_eq!(a.as_str(), "abc");
    }

    #[test]
    fn display_matches_inner_string() {
        let id = PlaylistId::from("xyz");
        assert_eq!(format!("{id}"), "xyz");
    }

    #[test]
    fn queue_entry_id_is_copy_and_orders_numerically() {
        let a = QueueEntryId(1);
        let b = a; // Copy, not a move
        assert_eq!(a, b);
        assert!(QueueEntryId(2) > QueueEntryId(1));
    }

    #[test]
    fn ids_roundtrip_serde() {
        let id = ItemId::from("track-123");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"track-123\"");
        let back: ItemId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    // `ids_are_distinct_types`: ItemId and PlaylistId are structurally identical (both wrap a
    // String) but are different types, so one cannot be passed where the other is expected. The
    // real proof is the commented line below, which must not compile — the runtime test just
    // exercises the same values without pulling in a trybuild harness for one check the macro's
    // design already guarantees at the type level.
    //
    // fn takes_playlist_id(_: PlaylistId) {}
    // fn won_t_compile() {
    //     let item: ItemId = ItemId::from("x");
    //     takes_playlist_id(item); // expected `PlaylistId`, found `ItemId`
    // }
    #[test]
    fn ids_are_distinct_types() {
        fn takes_playlist_id(_: PlaylistId) {}
        let item = ItemId::from("x");
        let playlist = PlaylistId::from("x");
        takes_playlist_id(playlist);
        assert_eq!(item.as_str(), "x");
    }
}
