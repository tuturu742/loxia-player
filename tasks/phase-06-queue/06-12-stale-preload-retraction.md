# 06-12 · Stale-preload retraction

**Phase:** 06 — Queue engine · **Agent:** `loxia-core` + `loxia-audio` + `loxia-player` (crate-boundary exception granted below) · **Size:** L · **Prerequisites:** `06-11` · **Reference:** `docs/05-audio-engine.md` §2, `docs/12-decisions.md` §9

## Crate-boundary authorisation

**This task explicitly authorises crossing the `loxia-core`, `loxia-audio`, and `loxia-player`
crate boundaries in a single PR**, per `CONTRIBUTING.md`'s rule "never cross a crate boundary in a
single task unless the task explicitly says to." The fix is only correct if all three change
together: `loxia-core`'s reducer must emit a new/changed effect when the "next" track changes
after a preload was already issued (the exact cases `06-11` inventoried); `loxia-audio` must gain
a command that lets a pending preload be retracted rather than only ever appended; `loxia-player`'s
audio worker must wire the new effect through to the new command. Splitting this across three
separate task PRs would leave the system in a broken intermediate state (an effect with nothing to
receive it, or a command nothing ever sends) at every point except the last. Do not use this
authorisation for anything outside the stale-preload fix itself.

## Goal

When a queue mutation changes what track is "next" after a preload has already been issued for
the old "next" track, retract that stale preload and issue the correct one, so the audio engine
never gaplessly transitions into the wrong track.

## Files

- `crates/loxia-core/src/reducer/queue.rs`
- `crates/loxia-core/src/effect.rs`
- `crates/loxia-audio/src/backend.rs`
- `crates/loxia-audio/src/mpv/handle.rs`
- `crates/loxia-audio/src/gapless.rs`
- `crates/loxia-player/src/workers/audio.rs`
- `docs/12-decisions.md` (§9 row, required by `CONTRIBUTING.md` workflow item 4)

## Specification

- Add `AudioCommand::CancelPreload` to `loxia-audio::backend` — retracts a previously issued
  `Preload` from mpv's internal playlist without affecting the currently-playing track. If no
  preload is pending, it is a no-op, not an error.
- `reducer::queue` emits `Effect::Audio(AudioCommand::CancelPreload)` immediately followed by a
  corrected `Effect::Audio(AudioCommand::Preload(..))` whenever a queue mutation changes the
  "next" track after a preload was already issued for the previous "next" track — the exact set
  of mutations `06-11` inventoried as leaving the preload stale (insert-next, remove, shuffle
  toggle, sort change).
- `loxia-player`'s audio worker (`workers/audio.rs`) forwards `CancelPreload` to the backend
  exactly as it already forwards `Preload`.
- `MpvEngine`'s handling of `CancelPreload` removes the pending playlist entry mpv would otherwise
  gaplessly transition into.

## Acceptance

- `cancel_preload_command_exists_and_round_trips_through_mock_engine`
- `queue_emits_cancel_then_reissue_preload_on_insert_next_after_preload_issued`
- `queue_emits_cancel_then_reissue_preload_on_remove_after_preload_issued`
- `queue_emits_cancel_then_reissue_preload_on_shuffle_toggle_after_preload_issued`
- `mpv_cancel_preload_is_noop_when_nothing_pending` (behind the `mpv-tests` feature, per the
  existing convention in `gapless.rs`)

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] `docs/12-decisions.md` §9 updated in the same PR
- [ ] Public items documented; the crate's `lib.rs` module list updated in every crate touched
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
