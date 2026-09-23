//! Regression coverage for the "stale gapless preload after a queue edit" investigation.
//!
//! See `tasks/confirm-or-refute-the-stale-gapless-preload-after-a-queue-edit.md` for the full
//! write-up (finding: **CONFIRMED**) and `tasks/phase-06-queue/06-09-preload-retraction.md` for
//! the follow-up task that fixes it.
//!
//! ## Why this lives under `tests/`, not inside `reducer::queue`'s own `#[cfg(test)] mod tests`
//!
//! The task instructions ask for this test "alongside the existing reducer::queue tests
//! (`crates/loxia-core/src/reducer/queue.rs` or its test module)". This session's working context
//! did not have `crates/loxia-core/src/reducer/queue.rs`'s (nor `state/player.rs`'s, nor
//! `state/queue.rs`'s) current contents available to append to. Emitting a "complete file" for a
//! source file whose real contents are unknown would silently discard whatever production code
//! and tests already live there — exactly the kind of unreviewed production change this task
//! forbids ("No production code changes should be made"). Landing the same test as a new,
//! self-contained integration test file avoids that risk entirely: nothing existing is touched,
//! and this file should be folded into `reducer::queue`'s own test module in a follow-up once its
//! current contents are back in view.
//!
//! The field/type names below (`PlayerState::last_preloaded`, `QueueState::play_order`,
//! `QueueState::position`) are quoted directly from the task's own hypothesis text and so are
//! used verbatim; the exact `Action`/`Effect` variant names for "insert next" and "preload" are
//! this test's own best-effort reconstruction from the surrounding naming conventions
//! (`preload_effects`, `load_current`, the `i` insert-next keybind) and may need a one-line path
//! fix once checked against the real enums.

use loxia_core::action::{Action, QueueAction};
use loxia_core::event::Event;
use loxia_core::reducer::reduce;
use loxia_core::state::AppState;
use loxia_core::test_support::fixtures;

/// Sets up a queue whose *next* target (`play_order[position + 1]`) has already been recorded in
/// `PlayerState.last_preloaded` — i.e. a `Preload` for it has already been sent to the audio
/// backend, per `reducer::queue::preload_effects`'s own de-duplication check. Then dispatches
/// insert-next (the `i` key) so a brand-new entry becomes `play_order[position + 1]` instead, and
/// finally simulates the audio backend's track-ended event.
///
/// What this checks, per the task's finding:
/// - insert-next *does* emit a fresh preload-shaped effect for the newly inserted entry, and
/// - nothing in the emitted effects *retracts* the now-stale, already-preloaded entry, because
///   (per `crates/loxia-audio/src/backend.rs`'s `AudioCommand` enum, quoted in the finding) there
///   is no variant capable of expressing "forget/remove that preload" for `reducer::queue` to
///   emit in the first place.
/// - after the (simulated) track-ended event, the queue's current entry follows `state.queue`'s
///   own `play_order`/`position` — the newly inserted entry — and not whatever mpv might actually
///   have advanced to internally (which, per the finding, could still be the stale file, since
///   `AudioCommand::Preload` only ever appends to mpv's playlist and nothing ever removes from
///   it). This half of the test is expected to keep passing even before the fix, because the
///   *reducer's* advance handler is state-driven; it is the audio side, not this reducer test,
///   where the drift with mpv actually manifests.
#[test]
#[ignore = "fixed by 06-09-preload-retraction"]
fn insert_next_after_preload_retargets_next_track() {
    let mut state = AppState::default();
    fixtures::seed_queue(&mut state, &["track-a", "track-b", "track-c"]);

    // The queue is sitting on "track-a" (position 0); "track-b" is play_order[position + 1] and
    // is already recorded as preloaded, standing in for mpv already having appended it to its
    // internal playlist via a prior `AudioCommand::Preload`.
    let stale_next = state.queue.play_order[state.queue.position + 1];
    state.player.last_preloaded = Some(stale_next);

    // The user presses `i`: insert a new track directly after the current one, which retargets
    // "next" away from the stale preloaded entry.
    let new_entry = fixtures::sample_queue_entry("track-d-inserted");
    let inserted_id = new_entry.id;
    let effects = reduce(&mut state, Action::Queue(QueueAction::InsertNext(new_entry)));

    let new_next = state.queue.play_order[state.queue.position + 1];
    assert_eq!(
        new_next, inserted_id,
        "insert-next must retarget play_order[position + 1] to the newly inserted entry"
    );
    assert_ne!(
        new_next, stale_next,
        "the newly inserted entry must displace the previously-preloaded one as \"next\""
    );

    // A preload-shaped effect for the *new* next entry should exist — `preload_effects` is
    // documented as deciding "when" to send `Preload`, and the target has just changed.
    let effects_debug = format!("{effects:?}");
    assert!(
        effects_debug.contains("Preload") && effects_debug.contains(&format!("{new_next:?}")),
        "expected a Preload-shaped effect for the new next entry {new_next:?}, got: {effects_debug}"
    );

    // The bug under test: nothing retracts the stale entry. There is (per the finding) no
    // Effect/AudioCommand variant that could even express this, so this assertion is written as
    // "no retraction-shaped effect exists at all" rather than naming a specific missing variant,
    // to avoid asserting against an enum member that does not exist yet.
    let retracts_something = effects_debug.contains("Retract")
        || effects_debug.contains("Unpreload")
        || effects_debug.contains("RemovePreload")
        || effects_debug.contains("CancelPreload");
    assert!(
        !retracts_something,
        "no retraction effect variant exists yet (confirmed bug) — if this now fails, the fix \
         landed and this test (and its #[ignore]) should be updated: {effects_debug}"
    );

    // Simulate the audio backend reporting the track ended naturally. The advance handler must
    // take play_order[position + 1] from queue *state*, not "whatever mpv moved to" — see the
    // finding's discussion of the advance handler.
    let _ = reduce(&mut state, Action::Event(Event::TrackEnded { natural: true }));

    assert_eq!(
        state.queue.play_order.get(state.queue.position).copied(),
        Some(inserted_id),
        "after track-ended, the queue's current entry must be the newly inserted track, per \
         reducer state — not the stale entry mpv may still gaplessly play out of its own \
         internal, never-retracted playlist"
    );
}
