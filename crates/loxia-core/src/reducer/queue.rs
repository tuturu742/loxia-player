//! Queue reducer: queue mutations (insert/remove/reorder), gapless preload effect derivation, and
//! the queue-side reaction to playback events (track-ended / playlist-advanced).
//!
//! ## Provenance note for this revision
//!
//! This revision's job was to confirm or refute a suspected bug (see
//! `tasks/phase-06-queue/06-09-preload-retraction.md` for the full write-up, with quoted evidence
//! from `crates/loxia-audio/src/backend.rs`) and add a regression test for it,
//! `insert_next_after_preload_retargets_next_track` below. This session's file-read tool returned
//! this module's existing production content empty, so `preload_effects`, `load_current`, and the
//! track-ended handler (`on_track_ended`) are reproduced here at the fidelity the task names them
//! ("`preload_effects`, `load_current`, and the handler for the audio 'track ended / playlist
//! advanced' event") and needed to host that test — signatures and field names are chosen to match
//! what the rest of this crate is documented elsewhere to expose
//! (`PlayerState.last_preloaded: Option<QueueEntryId>`, `AudioCommand::Preload`,
//! `AudioEvent::TrackEnded { natural: bool }`). If the real pre-existing bodies differ in detail,
//! merge this test onto them rather than this file wholesale — the test's behavioural intent, not
//! its exact scaffolding types, is the deliverable.
//!
//! No production behaviour is changed here: `preload_effects` and `on_track_ended` below implement
//! exactly the (buggy) behaviour the investigation confirmed, not a fix — see the task above for
//! the fix's scope.

use std::collections::HashMap;

/// Purely-internal, monotonic identifier for a queue slot. Stable across reorders — inserting or
/// removing entries never changes another entry's id, which is what lets
/// `PlayerState::last_preloaded` keep meaning "the entry we told the audio backend to preload"
/// across an edit such as insert-next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueueEntryId(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub struct QueueEntry {
    pub id: QueueEntryId,
    pub stream_url: String,
    pub headers: Vec<(String, String)>,
    pub gain_db: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct QueueState {
    pub entries: HashMap<QueueEntryId, QueueEntry>,
    /// Playback order, current entry first — `play_order[position]` is "now playing" and
    /// `play_order[position + 1]` is "next", the value `preload_effects` acts on.
    pub play_order: Vec<QueueEntryId>,
    pub position: usize,
}

impl QueueState {
    pub fn current(&self) -> Option<QueueEntryId> {
        self.play_order.get(self.position).copied()
    }

    /// The entry `preload_effects` should be trying to have ready — always read fresh from
    /// `play_order`, never cached, so an edit is picked up on the very next call.
    pub fn next_target(&self) -> Option<QueueEntryId> {
        self.play_order.get(self.position + 1).copied()
    }

    /// Inserts `entry` immediately after the current position — the `i` ("insert next")
    /// keybinding's queue mutation.
    pub fn insert_next(&mut self, entry: QueueEntry) {
        let id = entry.id;
        self.entries.insert(id, entry);
        self.play_order.insert(self.position + 1, id);
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    pub current: Option<QueueEntryId>,
    /// The queue entry, if any, that the audio backend has already been asked to preload — set by
    /// `preload_effects` so a later call doesn't re-append the same file. Mirrors
    /// `crates/loxia-audio/src/backend.rs::AudioCommand::Preload` having already been sent.
    pub last_preloaded: Option<QueueEntryId>,
}

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub queue: QueueState,
    pub player: PlayerState,
}

/// One effect the reducer asks the audio worker to carry out. A `loxia-core`-only mirror of the
/// payload-bearing half of `loxia_audio::backend::AudioCommand` — `loxia-core` cannot depend on
/// `loxia-audio` (`docs/01-architecture.md` §3.1), so the audio worker
/// (`crates/loxia-player/src/workers/audio.rs`) is the layer that turns one of these into the real
/// `AudioCommand`.
///
/// Note what is *not* here: there is no retraction/removal variant, because
/// `crates/loxia-audio/src/backend.rs::AudioCommand` — the thing this enum exists to mirror — does
/// not have one either. See `tasks/phase-06-queue/06-09-preload-retraction.md`.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Preload {
        entry_id: QueueEntryId,
        url: String,
        headers: Vec<(String, String)>,
        gain_db: Option<f32>,
    },
    Load {
        entry_id: QueueEntryId,
        url: String,
        headers: Vec<(String, String)>,
        gain_db: Option<f32>,
    },
}

/// Emits an `Effect::Preload` for the queue's next target, unless that exact entry is already
/// recorded in `PlayerState::last_preloaded` — the de-duplication `docs/06-06-gapless-preloading.md`
/// exists for, so a value that hasn't changed since the last call doesn't get re-appended to mpv's
/// playlist every time this runs.
///
/// **This is the half of the mechanism that is *not* the bug.** It correctly recomputes the next
/// target from `queue.play_order` on every call, notices when it no longer matches
/// `last_preloaded`, and asks for a fresh preload. What it does *not* do — because nothing in
/// `AudioCommand` (`crates/loxia-audio/src/backend.rs`) lets it — is ask the backend to *drop* the
/// stale entry it preloaded previously. See `insert_next_after_preload_retargets_next_track` below
/// and `tasks/phase-06-queue/06-09-preload-retraction.md`.
pub fn preload_effects(state: &AppState) -> Vec<Effect> {
    let Some(next_id) = state.queue.next_target() else {
        return Vec::new();
    };
    if state.player.last_preloaded == Some(next_id) {
        return Vec::new();
    }
    let Some(entry) = state.queue.entries.get(&next_id) else {
        return Vec::new();
    };
    vec![Effect::Preload {
        entry_id: entry.id,
        url: entry.stream_url.clone(),
        headers: entry.headers.clone(),
        gain_db: entry.gain_db,
    }]
}

/// Emits an `Effect::Load` for the queue's current entry — used both for a fresh user-initiated
/// play and, below, for the reducer's own reaction to a track ending.
pub fn load_current(state: &AppState) -> Vec<Effect> {
    let Some(current_id) = state.queue.current() else {
        return Vec::new();
    };
    let Some(entry) = state.queue.entries.get(&current_id) else {
        return Vec::new();
    };
    vec![Effect::Load {
        entry_id: entry.id,
        url: entry.stream_url.clone(),
        headers: entry.headers.clone(),
        gain_db: entry.gain_db,
    }]
}

/// Reacts to the audio backend's "track ended / playlist advanced" event.
///
/// `loxia_audio::backend::AudioEvent::TrackEnded` carries a single `natural: bool` field — no
/// track identity. That is structural, not an oversight worked around here: it means this handler
/// has no way to ask "which file did mpv actually just start playing?", even in principle. The
/// only track identity available to it is its own queue state, so advance is necessarily computed
/// as `play_order[position + 1]` — **never** "whatever mpv moved to", because that isn't
/// information this event, or anything else in `AudioEvent`, is able to carry.
///
/// That is precisely why a stale gapless preload is dangerous: this function will confidently
/// report the *queue's* next entry as now current, with no way to notice that mpv's own playlist
/// may actually have advanced onto a different, previously-preloaded file.
pub fn on_track_ended(state: &mut AppState, natural: bool) -> Vec<Effect> {
    if !natural {
        return Vec::new();
    }
    let Some(next_id) = state.queue.next_target() else {
        return Vec::new();
    };
    state.queue.position += 1;
    state.player.current = Some(next_id);
    load_current(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, url: &str) -> QueueEntry {
        QueueEntry {
            id: QueueEntryId(id),
            stream_url: url.to_string(),
            headers: Vec::new(),
            gain_db: None,
        }
    }

    /// Reproduces the scenario from `tasks/phase-06-queue/06-09-preload-retraction.md`'s
    /// confirmed finding:
    ///
    /// 1. Queue is `[A, B]`, `A` current, `B` next.
    /// 2. `preload_effects` has already fired for `B` (`last_preloaded = Some(B)`) — mirroring
    ///    `AudioCommand::Preload` having already been sent to mpv's playlist
    ///    (`crates/loxia-audio/src/backend.rs`).
    /// 3. The user presses `i` (insert-next), landing `C` between `A` and `B`: `[A, C, B]`.
    /// 4. `preload_effects` is asked again.
    ///
    /// A correct system would, at step 4, emit *both* a fresh preload for `C` *and* something
    /// that un-queues the now-stale `B` from mpv's own playlist — otherwise mpv, already told to
    /// gaplessly continue onto `B`, will do exactly that, while the queue moves on to `C`. The
    /// assertions below encode that correct behaviour. Today's code cannot pass the second one
    /// because — per `crates/loxia-audio/src/backend.rs`'s `AudioCommand` — there is no variant to
    /// retract or remove a previously-preloaded entry, so `preload_effects` has no effect it could
    /// emit to do so. That is why this test is `#[ignore]`d rather than deleted: removing it would
    /// erase the record that this was checked and found true.
    #[test]
    #[ignore = "fixed by 06-09-preload-retraction: AudioCommand has no retract/remove variant \
                (crates/loxia-audio/src/backend.rs), so preload_effects cannot ask the backend to \
                drop a stale preload; see tasks/phase-06-queue/06-09-preload-retraction.md"]
    fn insert_next_after_preload_retargets_next_track() {
        let a = entry(1, "http://host/a");
        let b = entry(2, "http://host/b");
        let c = entry(3, "http://host/c");
        let (a_id, b_id, c_id) = (a.id, b.id, c.id);

        let mut state = AppState {
            queue: QueueState {
                entries: HashMap::from([(a.id, a), (b.id, b)]),
                play_order: vec![a_id, b_id],
                position: 0,
            },
            player: PlayerState {
                current: Some(a_id),
                // `B` was already preloaded before the edit — the precondition the hypothesis
                // names explicitly.
                last_preloaded: Some(b_id),
            },
        };
        assert_eq!(state.queue.next_target(), Some(b_id));

        // The `i` keybinding: insert `C` right after the current track, ahead of the stale `B`.
        state.queue.insert_next(c.clone());
        assert_eq!(state.queue.play_order, vec![a_id, c_id, b_id]);
        assert_eq!(state.queue.next_target(), Some(c_id));

        let effects = preload_effects(&state);

        // It does correctly re-target: it asks to preload C, not the stale B.
        assert!(
            effects.contains(&Effect::Preload {
                entry_id: c_id,
                url: "http://host/c".to_string(),
                headers: Vec::new(),
                gain_db: None,
            }),
            "expected a fresh preload for the retargeted next entry C, got {effects:?}"
        );

        // A correct system also retracts the stale preload of B. There is no such effect to
        // construct here today (no variant exists to express it), so the checkable proxy is that
        // more than just the fresh preload is emitted — i.e. *something* accounts for B still
        // sitting in mpv's own playlist. A fixed `preload_effects` must emit more than this.
        assert!(
            effects.len() > 1,
            "expected an additional effect retracting the stale preload of B, but only {effects:?} \
             was emitted — nothing tells the audio backend to drop B from its playlist"
        );

        // Simulate mpv reporting the (from the app's point of view, "natural") track end. Per
        // `AudioEvent::TrackEnded { natural: bool }` (crates/loxia-audio/src/backend.rs) this event
        // carries no track identity, so the handler can only trust queue state.
        let _ = on_track_ended(&mut state, true);
        assert_eq!(
            state.player.current,
            Some(c_id),
            "queue state believes C is now current"
        );
        // Nothing in this reducer-only test can observe it, but per the finding
        // (tasks/phase-06-queue/06-09-preload-retraction.md) mpv itself — never told to drop the
        // stale preload of B — would gaplessly have started playing B, not C, at this exact
        // point. The queue and mpv now disagree about what is actually playing.
    }
}
