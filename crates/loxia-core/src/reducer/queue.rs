//! Queue edits: "play next" (`i`) and "add to queue" (`a`), plus loading the entry currently
//! selected by `play_order`/`position` (`06-01`).
//!
//! `QueueBatch` covers both `i` and `a`: which one is picked is `QueueBatchMode`. Neither ever
//! resets `PlayerState.session`, `play_reported`, or `start_reported`, and neither ever emits
//! `Effect::Audio(AudioCommand::Load)` — only a fresh `Action::Load`, handled by `load_current`,
//! does either of those.

use crate::effect::{AudioCommand, Effect};
use crate::state::player::PlayerState;
use crate::state::queue::{QueueEntry, QueueState};

/// Which edge of the queue a `QueueBatch` lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueBatchMode {
    /// "Play next" (`i`): the batch plays immediately after whatever is currently playing,
    /// regardless of shuffle state.
    InsertNext,
    /// "Add to queue" (`a`): the batch plays last.
    Append,
}

/// A batch of one or more tracks queued together by a single `i`/`a` keypress (e.g. a
/// multi-select in a column view). `track_ids` is in the order the user selected them; that order
/// is preserved in the queue regardless of `mode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueBatch {
    pub mode: QueueBatchMode,
    pub track_ids: Vec<String>,
}

/// Applies one `QueueBatch` to `queue`. Returns no effects: queue edits are pure state mutation,
/// never a playback command — see the module doc comment.
pub fn apply_queue_batch(
    queue: &mut QueueState,
    mode: QueueBatchMode,
    track_ids: Vec<String>,
) -> Vec<Effect> {
    if track_ids.is_empty() {
        return Vec::new();
    }

    match mode {
        QueueBatchMode::Append => append(queue, track_ids),
        QueueBatchMode::InsertNext => insert_next(queue, track_ids),
    }

    Vec::new()
}

fn append(queue: &mut QueueState, track_ids: Vec<String>) {
    for track_id in track_ids {
        let queue_id = queue.alloc_queue_id();
        let new_idx = queue.entries.len();
        queue.entries.push(QueueEntry { queue_id, track_id });
        queue.play_order.push(new_idx);
    }
}

fn insert_next(queue: &mut QueueState, track_ids: Vec<String>) {
    // "Immediately after current" in *entries* index space — this is what makes the inserted
    // batch land next to the current entry again after a later un-shuffle, since `entries`
    // (never reordered by shuffle) is what un-shuffling's natural order is built from.
    let mut entries_idx = queue.current_slot().map_or(0, |slot| slot + 1);
    // "Immediately after current" in *play_order* index space — this is what makes it play next
    // right now, regardless of shuffle state.
    let mut order_idx = if queue.play_order.is_empty() {
        0
    } else {
        queue.position + 1
    };

    for track_id in track_ids {
        let queue_id = queue.alloc_queue_id();

        // Inserting into `entries` at `entries_idx` shifts every existing entry from that index
        // onward one slot to the right — every reference to one of those indices already sitting
        // in `play_order` (including `play_order[position]`, the current entry's own slot) has to
        // move with it, or it now points at the wrong track.
        for slot in queue.play_order.iter_mut() {
            if *slot >= entries_idx {
                *slot += 1;
            }
        }

        queue.entries.insert(
            entries_idx,
            QueueEntry {
                queue_id,
                track_id,
            },
        );
        queue.play_order.insert(order_idx, entries_idx);

        entries_idx += 1;
        order_idx += 1;
    }
}

/// Resets `PlayerState.session`, `play_reported`, and `start_reported` for a fresh load of the
/// entry `queue` currently points at, and returns the `Effect::Audio(AudioCommand::Load)` that
/// starts it playing. Called only by a fresh `Action::Load` — never by `apply_queue_batch`.
pub fn load_current(queue: &QueueState, player: &mut PlayerState) -> Vec<Effect> {
    player.session = Some(format!("session-{}", queue.position));
    player.play_reported = false;
    player.start_reported = false;

    match queue.current_entry() {
        Some(entry) => vec![Effect::Audio(AudioCommand::Load {
            track_id: entry.track_id.clone(),
        })],
        None => Vec::new(),
    }
}
