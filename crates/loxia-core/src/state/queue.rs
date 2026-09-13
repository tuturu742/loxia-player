//! QueueState, QueueEntry, HistoryEntry.

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::model::{ItemId, PlaylistId, QueueEntryId, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueSource {
    Album {
        id: ItemId,
    },
    Artist {
        id: ItemId,
    },
    Playlist {
        id: PlaylistId,
    },
    /// `07-05`: `recursive` rides along on the source itself, not a side `pending_*` map like
    /// `Album`'s artist-filter — unlike that case, there's nothing else to correlate at reply time,
    /// so the flag set when the fetch was requested (`a` → `false`, `A` → `true`) is simply carried
    /// straight through to the reply and then onto every resulting `QueueEntry`.
    Folder {
        id: ItemId,
        recursive: bool,
    },
    /// A whole genre queued from the Genres tab. Carries the genre **name**, not an id: Emby
    /// filters by name (see `ColumnKind::GenreArtists`), and the name is what the queue rows show.
    Genre {
        name: String,
    },
    Search,
    InstantMix {
        seed: ItemId,
    },
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Availability {
    Remote,
    Cached,
    Downloaded,
    Unavailable,
}

/// A queue request that cannot be answered from state alone — one network fetch per selected row,
/// reassembled once every reply is in.
///
/// Queueing a *single* container row (`Enter`/`a` on one album) needs none of this: it fires one
/// fetch and appends the moment the reply lands. Two things a live user found do need it, and they
/// are the same problem wearing different hats — the reply has to remember the intent of the press,
/// and replies have to be put back in the order the rows were displayed in:
///
/// * `i` (insert-next) on a container. `DataAction::TracksLoaded` says nothing about whether the
///   press was "append" or "insert after the current track", so `i` on an album or artist quietly
///   appended — indistinguishable from doing nothing when the queue is long.
/// * A visual multi-selection containing containers. `resolve_selection_tracks` dropped every
///   non-`Track` row, so `v`-selecting three albums and pressing `a` queued nothing at all. Fetching
///   all three yields three replies in whatever order the server answers them, while the queue must
///   come out in selection order.
///
/// Not persisted: a batch is in-flight state, meaningless across a restart.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueBatch {
    pub mode: QueueBatchMode,
    /// In display order — the order the resulting tracks join the queue in.
    pub slots: Vec<QueueBatchSlot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueBatchMode {
    Append,
    InsertNext,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueBatchSlot {
    /// The reply this slot is waiting for, matched against `DataAction::TracksLoaded`'s own
    /// `source`. `None` once filled — a row that needed no fetch (a plain track already in the
    /// column) starts filled.
    pub awaiting: Option<QueueSource>,
    pub tracks: Vec<(Track, QueueSource)>,
}

impl QueueBatch {
    /// Whether a reply carrying `source` belongs to this batch. Two slots can never await the same
    /// source — a column holds each item id once — so the first match is the only match.
    pub fn awaits(&self, source: &QueueSource) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.awaiting.as_ref() == Some(source))
    }

    pub fn is_complete(&self) -> bool {
        self.slots.iter().all(|slot| slot.awaiting.is_none())
    }

    /// Every resolved track, in slot order. Consumes the batch.
    pub fn into_tracks(self) -> Vec<(Track, QueueSource)> {
        self.slots
            .into_iter()
            .flat_map(|slot| slot.tracks)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueEntry {
    pub entry_id: QueueEntryId,
    pub track: Track,
    pub source: QueueSource,
    pub availability: Availability,
}

/// **[persist]**
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QueueState {
    /// The canonical, unshuffled order. **Never reordered by shuffle** — see `queue/shuffle.rs`'s
    /// non-destructive contract (`docs/02-data-model.md` §4).
    pub entries: Vec<QueueEntry>,
    /// Indices into `entries`; the identity permutation when unshuffled.
    pub play_order: Vec<usize>,
    pub position: usize,
    pub shuffled: bool,
    pub repeat: RepeatMode,
    pub sort_profile: Option<String>,
    pub next_entry_id: u64,
    /// The display name the current instant mix was seeded from, if the queue is one.
    ///
    /// `QueueSource::InstantMix` carries only the seed's `ItemId`, and an id is not something to
    /// put in front of a user. The Now Playing pane titles itself "MIX FOR `<name>`" from this —
    /// which is where "what is this queue?" belongs, rather than repeated on every single row as a
    /// per-entry badge (`docs/12-decisions.md`).
    #[serde(default)]
    pub mix_name: Option<String>,
}

impl QueueState {
    pub fn current(&self) -> Option<&QueueEntry> {
        let index = *self.play_order.get(self.position)?;
        self.entries.get(index)
    }

    pub fn upcoming(&self) -> impl Iterator<Item = &QueueEntry> {
        let start = self.position.saturating_add(1);
        self.play_order
            .get(start..)
            .into_iter()
            .flatten()
            .filter_map(|&i| self.entries.get(i))
    }

    pub fn next_id(&mut self) -> QueueEntryId {
        let id = QueueEntryId(self.next_entry_id);
        self.next_entry_id += 1;
        id
    }
}

/// **[persist: history.json]** A `VecDeque` capped at 50, newest first — stored on `AppState`
/// directly, not inside `QueueState`, so clearing the queue never clears history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub track: Track,
    pub played_at: Timestamp,
    pub completed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_id_is_monotonic() {
        let mut state = QueueState::default();
        let a = state.next_id();
        let b = state.next_id();
        let c = state.next_id();
        assert!(a.0 < b.0 && b.0 < c.0);
    }

    #[test]
    fn queue_state_roundtrips_serde() {
        let mut state = QueueState::default();
        state.next_id();
        state.play_order = vec![0];
        state.position = 0;
        state.shuffled = true;
        state.repeat = RepeatMode::All;
        state.sort_profile = Some("recent".to_string());

        let json = serde_json::to_string(&state).unwrap();
        let back: QueueState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }
}
