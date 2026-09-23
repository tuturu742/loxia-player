# 06-12 · Stale-preload retraction

**Phase:** 06 — Queue engine · **Agent:** cross-crate (`loxia-core`, `loxia-audio`, `loxia-player`) · **Size:** L · **Prerequisites:** `06-11` · **Reference:** `docs/12-decisions.md` §9

## Crate-boundary authorisation

**This task explicitly authorises crossing the `loxia-core`, `loxia-audio`, and `loxia-player`
crate boundaries in a single PR.** `CONTRIBUTING.md` rule 2 ("never cross a crate boundary in a
single task unless the task explicitly says to") forbids this by default — this section is that
explicit statement. It is unavoidable here because retracting a stale preload is structurally a
three-crate feature: `loxia-core`'s reducer is the only place that knows a queue mutation
invalidated the preload target; `loxia-audio` is the only place that holds the actual preloaded
mpv handle; `loxia-player`'s audio worker is the only thing that already relays `Effect::Audio(..)`
to `AudioBackend::send`. No single-crate task can close every gap `06-11` found.

## Goal

Close every gap `06-11`'s audit found: each of the five mutations it audited now emits an effect
that reaches the audio backend and retracts (or replaces) a stale preload before it can be played
by mistake. When this task is done, `06-11`'s five characterization tests are exactly the ones
whose asserted effect set changed, and a new backend command exists for the "cancel, don't
replace" case.

## Files

- `crates/loxia-core/src/effect.rs`
- `crates/loxia-core/src/reducer/queue.rs`
- `crates/loxia-audio/src/backend.rs`
- `crates/loxia-audio/src/gapless.rs`
- `crates/loxia-player/src/workers/audio.rs`
- `docs/12-decisions.md` (§9 — the row for this fix, referencing `06-11`'s findings row)

## Specification

- Add `AudioCommand::CancelPreload` to `loxia-audio::backend`. `clear-queue`, and "remove exactly
  the track that was the preload target" with nothing to replace it with, both want "stop holding
  this preload, don't start a new one," which is what this variant is for — every other existing
  `AudioCommand` variant (`Load`, `Preload`, `Play`, ...) is unaffected.
- For the other three mutations from `06-11` (insert-next, shuffle toggle, sort-profile change),
  the reducer emits a fresh `Effect::Audio(AudioCommand::Preload { .. })` for whatever track is
  now next, superseding the stale one. `loxia-audio`'s gapless module must treat a second
  `Preload` arriving before the first one is consumed as "replace, don't queue both."
- `gapless.rs` gains whatever internal state is needed to drop an in-flight or already-held
  preload handle on `CancelPreload`, or on a superseding `Preload`, without touching current
  playback — `AudioCommand::Play`/`Pause`/`Stop` on the currently-playing track must remain
  unaffected by either.
- `loxia-player::workers::audio` forwards `CancelPreload` to `AudioBackend::send` exactly like
  every other `AudioCommand` variant it already relays — a single new match arm, following the
  worker's existing pattern for every other command, not new dispatch logic.
- Update each of `06-11`'s five characterization tests to assert the corrected effect is now
  emitted. Do not delete or rename them — the diff on those exact tests is the traceable proof
  the gap closed.

## Acceptance

- `insert_next_reissues_preload_target` (replaces `06-11`'s
  `insert_next_does_not_signal_preload_target_change`)
- `remove_from_queue_cancels_or_reissues_preload_target`
- `clear_queue_cancels_preload`
- `shuffle_toggle_reissues_preload_target`
- `sort_profile_change_reissues_preload_target`
- `cancel_preload_drops_pending_handle_without_affecting_playback` (`loxia-audio::gapless`)
- `superseding_preload_replaces_not_queues` (`loxia-audio::gapless`)
- `cancel_preload_effect_forwarded_to_backend` (`loxia-player::workers::audio`)

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] `docs/12-decisions.md` §9 updated in the same PR, referencing `06-11`'s findings row
- [ ] Public items documented; `loxia-audio`'s `lib.rs` module list unchanged (`gapless` is already
      listed; no new module is added)
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
