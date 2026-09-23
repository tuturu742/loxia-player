//! Queue reducer: playback-queue mutation, gapless preloading, and the track-ended /
//! playlist-advance handler (`docs/04-state-and-input.md`; phase-06-queue tasks `06-01` through
//! `06-09`).
//!
//! `preload_effects` and `load_current` are the two functions the audio worker's `Effect::Load`/
//! `Effect::Preload` translation (`crates/loxia-player/src/workers/audio.rs`) is fed from; see
//! `tasks/confirm-or-refute-the-stale-gapless-preload-after-a-queue-edit.md` for why an insert-next
//! after a `Preload` has already gone out currently leaves mpv's own playlist stale — the fix
//! (`tasks/phase-06-queue/06-09-preload-retraction.md`) is not implemented here yet; this module
//! only adds the regression test that demonstrates it.

use std::time::Duration;

use crate::action::{Action, QueueAction};
use crate::effect::Effect;
use crate::event::Event;
use crate::state::AppState;
use crate::state::player::PlayerState;
use crate::state::queue::QueueState;

/// Emits a fresh `Effect::Preload` for whatever `QueueState::next_entry` currently reports,
/// unless that same entry id is already recorded in `PlayerState.last_preloaded` — the guard that
/// stops `reduce` from re-appending a file mpv has already been told about on every unrelated
/// action. This guard compares only against the *current* target, never against what
/// `last_preloaded` used to be, which is the crux of the bug under investigation: it cannot detect
/// "the target changed out from under a pending preload" and therefore never asks anything to
/// retract the old one.
pub(crate) fn preload_effects(queue: &QueueState, player: &PlayerState) -> Vec<Effect> {
    let Some(next) = queue.next_entry() else {
        return Vec::new();
    };
    if player.last_preloaded == Some(next.id) {
        return Vec::new();
    }
    vec![Effect::Preload {
        entry_id: next.id,
        url: next.stream_url.clone(),
        headers: next.headers.clone(),
        gain_db: next.gain_db,
    }]
}

/// Loads the entry at the queue's current position — used for the first `Play` and for any
/// user-directed jump, and (see `handle_track_ended` below) for re-asserting "current" after the
/// queue's own `play_order` pointer has advanced.
pub(crate) fn load_current(queue: &QueueState) -> Vec<Effect> {
    let Some(current) = queue.current_entry() else {
        return Vec::new();
    };
    vec![Effect::Load {
        entry_id: current.id,
        url: current.stream_url.clone(),
        headers: current.headers.clone(),
        start_at: Duration::ZERO,
        gain_db: current.gain_db,
    }]
}

/// `Event::TrackEnded { natural: true }`: mpv finished playing whatever file it had loaded.
/// This handler takes `play_order[position + 1]` from the queue's own state — via
/// `QueueState::advance`, which walks the queue's own `play_order` pointer — rather than trusting
/// whatever mpv itself moved to internally (mpv is never asked). That makes the reducer's own
/// `AppState.queue`/`AppState.player` self-consistent, but it does not, on its own, guarantee mpv's
/// *audio output* is playing the same entry: if mpv's playlist holds a stale preloaded file, mpv
/// moves onto it the instant the current file ends, independently of and before this handler runs.
fn handle_track_ended(queue: &mut QueueState, player: &mut PlayerState, natural: bool) -> Vec<Effect> {
    if !natural {
        return Vec::new();
    }
    queue.advance();
    let mut effects = load_current(queue);
    effects.extend(preload_effects(queue, player));
    effects
}

/// Entry point for queue-related actions and the playback-advance event. Other `Action`/`Event`
/// variants are handled by sibling reducers (`reducer::player`, `reducer::nav`, ...) and are not
/// this module's concern.
pub fn reduce(state: &mut AppState, action: Action) -> Vec<Effect> {
    match action {
        Action::Queue(QueueAction::InsertNext(entry)) => {
            state.queue.insert_next(entry);
            preload_effects(&state.queue, &state.player)
        }
        Action::Event(Event::TrackEnded { natural }) => {
            handle_track_ended(&mut state.queue, &mut state.player, natural)
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ids::QueueEntryId;
    use crate::state::queue::QueueEntry;

    fn entry(id: u64, url: &str) -> QueueEntry {
        QueueEntry {
            id: QueueEntryId::new(id),
            stream_url: url.to_string(),
            headers: Vec::new(),
            gain_db: None,
        }
    }

    /// Reproduces the scenario in
    /// `tasks/confirm-or-refute-the-stale-gapless-preload-after-a-queue-edit.md`: a `Preload` for
    /// `stale_next` has already been sent (recorded in `PlayerState.last_preloaded`) when the user
    /// inserts a new track to play next. Correct behaviour is to retract the stale preload and
    /// preload the new target instead; today, `preload_effects` only ever compares against the
    /// *current* next-entry id, so it emits a fresh `Preload` for the new target but nothing that
    /// undoes the old one, and `last_preloaded` is retargeted without anything ever having told
    /// mpv to drop what it already has queued.
    #[test]
    #[ignore = "fixed by 06-09-preload-retraction"]
    fn insert_next_after_preload_retargets_next_track() {
        let mut state = AppState::default();

        let current = entry(1, "http://host/current.flac");
        let stale_next = entry(2, "http://host/stale.flac");
        state.queue.entries = vec![current.clone(), stale_next.clone()];
        state.queue.play_order = vec![current.id, stale_next.id];
        state.queue.position = Some(0);

        // A `Preload` for `stale_next` already went out before this edit.
        state.player.last_preloaded = Some(stale_next.id);

        let inserted = entry(3, "http://host/inserted.flac");
        let effects = reduce(
            &mut state,
            Action::Queue(QueueAction::InsertNext(inserted.clone())),
        );

        let retracts_stale = effects.iter().any(|effect| {
            matches!(effect, Effect::CancelPreload { entry_id } if *entry_id == stale_next.id)
        });
        assert!(
            retracts_stale,
            "insert-next must retract the stale preload for {:?}; effects were {effects:?}",
            stale_next.id,
        );

        let preloads_new_next = effects.iter().any(|effect| {
            matches!(effect, Effect::Preload { entry_id, .. } if *entry_id == inserted.id)
        });
        assert!(
            preloads_new_next,
            "insert-next must preload the newly-inserted entry as the new next track; \
             effects were {effects:?}",
        );

        assert_eq!(
            state.player.last_preloaded,
            Some(inserted.id),
            "last_preloaded must be retargeted to the newly-inserted entry, not left pointing at \
             the retracted stale entry",
        );

        // mpv naturally finishes the current track. Advance must land on the entry the queue
        // itself now considers next (the newly-inserted one) — never on the stale entry that was
        // preloaded before the edit.
        let _ = reduce(&mut state, Action::Event(Event::TrackEnded { natural: true }));
        assert_eq!(
            state.queue.current_entry().map(|e| e.id),
            Some(inserted.id),
            "after a natural track-ended, the queue's own play_order must decide what is \
             current — not whatever mpv happened to have preloaded"
        );
    }
}
