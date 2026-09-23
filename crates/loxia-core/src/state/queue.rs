//! Playback queue state (`06-01`).
//!
//! `entries` holds every track that has ever been added to the current queue, in the order it
//! was inserted — shuffling never reorders this vector, only `play_order` (a permutation of
//! indices into `entries`) does. `position` indexes into `play_order`; the entry currently
//! playing is always `entries[play_order[position]]`.

/// One track sitting in the queue. `queue_id` is assigned at insertion time and is unique within
/// a given `QueueState`'s lifetime, even across duplicate `track_id`s (the same track can be
/// queued more than once).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    pub queue_id: u64,
    pub track_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueState {
    pub entries: Vec<QueueEntry>,
    pub play_order: Vec<usize>,
    pub position: usize,
    pub shuffled: bool,
    next_queue_id: u64,
}

impl QueueState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The `entries` index of the entry currently playing, or `None` for an empty queue.
    pub fn current_slot(&self) -> Option<usize> {
        self.play_order.get(self.position).copied()
    }

    /// The entry currently playing, or `None` for an empty queue.
    pub fn current_entry(&self) -> Option<&QueueEntry> {
        self.current_slot().and_then(|idx| self.entries.get(idx))
    }

    /// Allocates the next `queue_id`, unique for the lifetime of this `QueueState`.
    pub fn alloc_queue_id(&mut self) -> u64 {
        let id = self.next_queue_id;
        self.next_queue_id += 1;
        id
    }
}
