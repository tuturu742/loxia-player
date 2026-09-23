//! Regression coverage for the "stale gapless preload after a queue edit" investigation.
//! Full finding, with quoted code and a CONFIRMED verdict: see
//! `tasks/phase-06-queue/06-09-retract-stale-preload.md`.

#![cfg(feature = "test-support")]

use loxia_core::action::{Action, QueueAction};
use loxia_core::effect::{AudioEffect, Effect};
use loxia_core::event::Event;
use loxia_core::model::QueueEntryId;
use loxia_core::reducer;
use loxia_core::state::AppState;
use loxia_core::test_support::fixtures;
use loxia_core::test_support::scenario::Scenario;

/// The entry `reducer::queue`'s own preload logic currently targets: one past `position` in
/// `play_order`, per `PlayerState.last_preloaded`'s own doc (`tasks/phase-06-queue/
/// 06-09-retract-stale-preload.md`).
fn next_entry_id(state: &AppState) -> QueueEntryId {
    state.queue.play_order[state.queue.position + 1]
}

/// Sets up a two-entry queue (`track-a` current, `track-b` next) where `track-b` is already
/// recorded as gaplessly preloaded (`PlayerState.last_preloaded == Some(track-b)`), matching the
/// bug report's own precondition ("after a preload was sent"). Runs insert-next to put `track-c`
/// directly ahead of `track-b`, then:
///
/// 1. asserts the reducer retargets `play_order[position + 1]` to the newly-inserted entry, not
///    the stale one — the reducer itself is not frozen by `last_preloaded`;
/// 2. asserts insert-next's own emitted effects include a fresh `Preload` for that retargeted
///    entry;
/// 3. simulates the audio backend's natural track-ended event and asserts the retargeted entry —
///    not the stale, already-appended-to-mpv one — becomes current.
///
/// Per the linked finding, mpv itself has no way to have the stale entry retracted from its
/// playlist (no `AudioCommand` variant expresses it, and `Preload` only ever appends), so this is
/// expected to fail until `06-09-retract-stale-preload` lands.
#[test]
#[ignore = "fixed by 06-09-retract-stale-preload"]
fn insert_next_after_preload_retargets_next_track() {
    let mut state: AppState = Scenario::new()
        .with_queue(vec![fixtures::track("track-a"), fixtures::track("track-b")])
        .with_position(0)
        .build();

    let stale_next = next_entry_id(&state);
    state.player.last_preloaded = Some(stale_next);

    let effects = reducer::reduce(
        &mut state,
        Action::Queue(QueueAction::InsertNext(fixtures::track("track-c"))),
    );

    let retargeted_next = next_entry_id(&state);
    assert_ne!(
        retargeted_next, stale_next,
        "insert-next must move play_order[position + 1] off the stale preloaded entry"
    );

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Audio(AudioEffect::Preload { .. }))),
        "insert-next must (re-)send Preload for the retargeted next entry: {effects:?}"
    );

    reducer::reduce_event(&mut state, Event::TrackEnded { natural: true });

    assert_eq!(
        state.queue.current_id(),
        Some(retargeted_next),
        "a natural track-ended event must advance current to the retargeted entry, not whatever \
         mpv had already gaplessly queued from the stale Preload"
    );
}
