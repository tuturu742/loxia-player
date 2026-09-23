//! Regression coverage for the "stale gapless preload after a queue edit" investigation.
//! See `tasks/phase-06-queue/06-09-retract-stale-preload.md` for the full finding (currently
//! **BLOCKED**, not CONFIRMED — read that file before trusting this one's framing).
//!
//! # Which symbols below are verified, and which are still guesses
//!
//! This file was written without sight of `crates/loxia-core/src/reducer/queue.rs`,
//! `crates/loxia-core/src/action.rs`, `crates/loxia-core/src/event.rs`, or
//! `crates/loxia-core/src/state/queue.rs` — all four rendered with no visible content in the
//! session that produced this revision (see the finding doc's "blocker" section). A previous
//! version of this file presented its guessed names as settled and reasoned from an unsupported
//! verdict; this revision does neither. Every name used here falls into one of three buckets:
//!
//! - **Confirmed directly, by citation:**
//!   - `loxia_core::model::QueueEntryId` — `crates/loxia-core/src/state/player.rs` imports it by
//!     exact name: `use crate::model::{AudioDevice, AudioFormat, PlaySessionId, QueueEntryId,
//!     ReplayGainInfo};`.
//!   - `loxia_core::state::player::PlayerState` exists as a type in that module (its own doc
//!     comment: "PlayerState mirror of the audio engine").
//!   - The `test-support` feature is real on `loxia-core`, not assumed: `crates/loxia-audio/
//!     Cargo.toml` declares `test-support = ["loxia-core/test-support"]`, and Cargo refuses to
//!     build a manifest naming a feature that does not exist on a path dependency — so
//!     `loxia-core` genuinely defines this feature, and gating this file on it is not a silent
//!     "the test never exists" mistake.
//!   - `AppState` as the top-level state type's real name is inferred from the task-library
//!     filename `tasks/phase-03-state-machine/03-01-appstate-and-substates.md`, not invented here.
//!   - `PlayerState.last_preloaded: Option<QueueEntryId>` is quoted verbatim from the work item's
//!     own hypothesis text, not derived independently.
//! - **Still unverified guesses**, because the files that would confirm them rendered empty in
//!   this session: `Action::Queue(QueueAction::InsertNext(..))`, `Event::TrackEnded { natural }`,
//!   `reducer::reduce` / `reducer::reduce_event` as the two dispatch entry points,
//!   `test_support::scenario::Scenario`, `test_support::fixtures::track`, and the exact shape of
//!   `queue.play_order` / `queue.current_id()`. If the real source spells any of these
//!   differently, only this file needs renaming to match — the *intent* below (assert on the
//!   effects `insert-next` emits, then on `current` after a simulated track-ended event) does not
//!   change.
//! - To avoid *also* guessing the exact shape of `Effect`/`AudioCommand` as re-wrapped inside
//!   `loxia-core`'s own `Effect` enum (`crates/loxia-core/src/effect.rs` was likewise empty in
//!   this session), the assertions below match on `Debug` output rather than a specific enum path.
//!   That is deliberate, not sloppy: it is the one technique here robust to not knowing the exact
//!   wrapping type, while still checking something real about what the reducer emits.

#![cfg(feature = "test-support")]

use loxia_core::action::{Action, QueueAction};
use loxia_core::event::Event;
use loxia_core::model::QueueEntryId;
use loxia_core::reducer;
use loxia_core::state::AppState;
use loxia_core::test_support::fixtures;
use loxia_core::test_support::scenario::Scenario;

/// Sets up a two-entry queue (A current, B next) where B is already recorded as gaplessly
/// preloaded (`PlayerState.last_preloaded == Some(B)`), matching the bug report's own
/// precondition ("after a preload was sent"). Runs insert-next to put C directly ahead of B, and
/// checks three things:
///
/// 1. Insert-next does retarget the next slot to C — the reducer is not simply frozen by the
///    stale `last_preloaded` value.
/// 2. Insert-next's own emitted effects include something that (re)preloads C.
/// 3. Insert-next's own emitted effects include something that retracts/removes the stale
///    preload it already sent for B. Per the finding doc, `AudioCommand`
///    (`crates/loxia-audio/src/backend.rs`) has no variant shaped like that at all today, so this
///    assertion is expected to fail until `06-09-retract-stale-preload` adds one and wires
///    `preload_effects` to emit it.
/// 4. After simulating mpv's own track-ended event, `current` becomes C — checking, empirically,
///    the finding doc's open sub-question of whether the reducer's advance logic trusts its own
///    `play_order` rather than whatever mpv reports.
#[test]
#[ignore = "symbol names unverified against real reducer source (see module doc); also the \
            intended regression test for 06-09-retract-stale-preload once that task lands"]
fn insert_next_after_preload_retargets_next_track() {
    let mut state: AppState = Scenario::new()
        .with_queue(["Track A", "Track B"])
        .with_position(0)
        .build();

    let stale_next_id: QueueEntryId = state.queue.play_order[1];
    state.player.last_preloaded = Some(stale_next_id);

    let effects = reducer::reduce(
        &mut state,
        Action::Queue(QueueAction::InsertNext(vec![fixtures::track("Track C")])),
    );

    let new_next_id = state.queue.play_order[1];
    assert_ne!(
        new_next_id, stale_next_id,
        "insert-next should retarget the next slot to the newly inserted track (C)"
    );

    assert!(
        effects
            .iter()
            .any(|effect| format!("{effect:?}").contains("Preload")),
        "insert-next should emit something that (re)preloads the new next track: {effects:?}"
    );

    let retracts_stale_preload = effects.iter().any(|effect| {
        let text = format!("{effect:?}");
        text.contains("Retract") || text.contains("Unload") || text.contains("PlaylistRemove")
    });
    assert!(
        retracts_stale_preload,
        "insert-next should retract the stale preload already sent for the old next track (B), \
         but no such effect variant exists yet — see the finding doc's `AudioCommand` quote: \
         {effects:?}"
    );

    reducer::reduce_event(&mut state, Event::TrackEnded { natural: true });
    assert_eq!(
        state.queue.current_id(),
        Some(new_next_id),
        "after track-ended, current should advance to the reducer's own next entry (C), not \
         whatever mpv itself may have gaplessly continued into"
    );
}
