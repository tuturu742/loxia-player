//! Regression coverage for the "stale gapless preload after a queue edit" investigation.
//! See `tasks/phase-06-queue/06-09-retract-stale-preload.md` for the full finding this pins down.
//!
//! # Provenance and assumptions
//!
//! This file was written without sight of `crates/loxia-core/src/reducer/queue.rs`,
//! `crates/loxia-core/src/state/player.rs`, `crates/loxia-core/src/state/queue.rs`, or
//! `crates/loxia-core/src/action.rs` — all four rendered empty in the review session that
//! produced this test (see the finding doc's "What could not be directly quoted" section). What
//! *was* verified in full is `crates/loxia-audio/src/backend.rs`'s `AudioCommand` enum, which has
//! `Load` and `Preload` but no variant that means "retract a preload" — that absence is the
//! structural confirmation of the bug and does not depend on any of the guesses below.
//!
//! The symbol names used here — `AppState`'s public `queue`/`player` fields,
//! `Action::Queue(QueueAction::InsertNext(..))`, `Event::TrackEnded { natural }`, and
//! `reducer::reduce` / `reducer::reduce_event` as the two dispatch entry points — are the most
//! conservative reading of the task's own description and of `reducer/mod.rs`'s module split
//! (one `reducer::<domain>` submodule per `Action`/`Event` family). If the real names differ,
//! only this file needs renaming; the test's *intent*, recorded in the finding doc, does not
//! change.

#![cfg(feature = "test-support")]

use loxia_core::action::{Action, QueueAction};
use loxia_core::effect::Effect;
use loxia_core::event::Event;
use loxia_core::reducer;
use loxia_core::state::AppState;
use loxia_core::test_support::fixtures;
use loxia_core::test_support::scenario::Scenario;

/// Sets up a two-entry queue (A current, B already recorded in
/// `PlayerState::last_preloaded` as the gaplessly-preloaded next track), runs `insert-next` to
/// put C ahead of B, and checks:
///
/// 1. `insert-next` does emit fresh preload-shaped effects for the retargeted next entry (C) —
///    the reducer is not simply frozen by the stale `last_preloaded` value.
/// 2. Nothing in that effect list can retract mpv's already-appended, now-stale entry for B,
///    because `AudioCommand`/`Effect` has no variant for that (see
///    `crates/loxia-audio/src/backend.rs`). This assertion is written to fail until
///    `tasks/phase-06-queue/06-09-retract-stale-preload.md` adds one.
/// 3. Simulating mpv's own track-ended event advances `current` from the reducer's own queue
///    state alone — it has no way to check what mpv actually started playing, so the reducer's
///    bookkeeping looks consistent (`current` becomes C) even though, per point 2, mpv itself
///    would gaplessly continue into the stale B.
#[test]
#[ignore = "fixed by 06-09-retract-stale-preload"]
fn insert_next_after_preload_retargets_next_track() {
    let mut state: AppState = Scenario::new()
        .with_queue(["Track A", "Track B"])
        .with_position(0)
        .build();

    // Record B (currently next) as already gaplessly preloaded, matching the bug report's
    // precondition: "after a preload was sent".
    let stale_next_id = state.queue.play_order[1];
    state.player.last_preloaded = Some(stale_next_id);

    // Run insert-next: C should become the new immediate-next entry, ahead of B.
    let effects = reducer::reduce(
        &mut state,
        Action::Queue(QueueAction::InsertNext(vec![fixtures::track("Track C")])),
    );

    let new_next_id = state.queue.play_order[1];
    assert_ne!(
        new_next_id, stale_next_id,
        "insert-next should retarget the next slot to the newly inserted track"
    );

    // preload_effects should not be stuck on the stale last_preloaded value: it must recognise
    // the next target changed and (re)emit something for it.
    assert!(
        !effects.is_empty(),
        "insert-next should emit at least one effect to (re)preload the new next track"
    );
    assert_eq!(
        state.player.last_preloaded,
        Some(new_next_id),
        "preload_effects should update its own bookkeeping to the new next track"
    );

    // But nothing in that effect list can retract mpv's now-stale preload of the *old* next
    // track: no `Effect`/`AudioCommand` variant means "retract a preload" (see
    // crates/loxia-audio/src/backend.rs). This is written so it always fails today, which is the
    // confirmation this test exists to record, not a false negative in the assertion logic.
    let retracted_stale_preload = effects.iter().any(|effect| match effect {
        Effect::Audio(_) => false, // no variant meaning "retract" exists to match against
        _ => false,
    });
    assert!(
        retracted_stale_preload,
        "expected an effect retracting mpv's stale preload of the old next track ({stale_next_id:?}); \
         no such effect kind exists yet (see AudioCommand in crates/loxia-audio/src/backend.rs); \
         effects were: {effects:?}"
    );

    // Simulate mpv's 'track ended / playlist advanced' event. The reducer can only trust its own
    // `play_order`, not what mpv actually started playing, so `current` silently becomes C here —
    // even though, per the assertion above, the real mpv playlist would still gaplessly continue
    // into the stale B.
    let _ = reducer::reduce_event(&mut state, Event::TrackEnded { natural: true });
    assert_eq!(
        state.queue.current_id(),
        Some(new_next_id),
        "the reducer advances `current` from its own queue state, with no cross-check against \
         what mpv actually started playing — the exact desync this task exists to close"
    );
}
