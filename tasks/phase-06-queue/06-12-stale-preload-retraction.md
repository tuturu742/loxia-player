# 06-12 — Stale-preload retraction

## Prerequisites

- `06-11` — Stale-preload audit

## Crate-boundary authorisation

CONTRIBUTING.md rule 2 ("Never cross a crate boundary in a single task unless the task explicitly
says to") is explicitly overridden **for this task only**. Closing every scenario `06-11` found
requires, in one coherent change:

- **`loxia-core`** — the reducer must detect, in `crates/loxia-core/src/reducer/queue.rs`, that an
  action just invalidated an outstanding preload, and emit a new `Effect` saying so.
- **`loxia-audio`** — `crates/loxia-audio/src/backend.rs` needs a new `AudioCommand` variant (e.g.
  `AudioCommand::RetractPreload`) that the mpv backend (`crates/loxia-audio/src/mpv/handle.rs`) and
  the mock backend (`crates/loxia-audio/src/mock.rs`) both implement, to remove the stale entry
  from mpv's internal playlist (or mark it ignorable) before it can play.
- **`loxia-player`** — the audio worker (`crates/loxia-player/src/workers/audio.rs`) is what
  actually turns the new `Effect` into the new `AudioCommand`; it is the only place both types are
  in scope simultaneously.

Splitting this into three single-crate tasks would leave the middle two states unmergeable (a new
`Effect` with nothing to turn it into a command, or a new `AudioCommand` nothing ever sends) and
individually untestable end-to-end. This paragraph is the task file's explicit authorisation
required by CONTRIBUTING.md; no further sign-off is needed to touch all three crates in this one
PR.

## Goal

Close every scenario documented in `docs/audits/06-11-stale-preload-findings.md`: whenever a queue
mutation invalidates an already-issued gapless preload, retract it before it can play, and issue a
fresh `Preload` for whatever is now actually next (if anything).

## Files to touch

- `crates/loxia-core/src/reducer/queue.rs` — detect invalidation, emit the new effect.
- `crates/loxia-core/src/effect.rs` — new `Effect::Audio` variant for the retraction (name it to
  match the existing `Effect::Audio(AudioCommand-shaped-but-core-side)` convention already used for
  `SetEq`/similar, per `crates/loxia-audio/src/backend.rs`'s own doc comments on that boundary).
- `crates/loxia-audio/src/backend.rs` — new `AudioCommand::RetractPreload` variant (or equivalent
  name — pick one and use it consistently across all three crates).
- `crates/loxia-audio/src/mpv/handle.rs` — implementation against mpv's real playlist (e.g.
  `playlist-remove` for the appended-but-not-yet-playing entry).
- `crates/loxia-audio/src/mock.rs` — `MockEngine`/`MockControl` support so tests can assert a
  retraction actually happened without a real mpv instance.
- `crates/loxia-player/src/workers/audio.rs` — wiring from the new `Effect` to the new
  `AudioCommand`.

## Specification

- Every scenario in the `06-11` findings document must have a corresponding un-ignored, passing
  test that was previously `#[ignore]`d.
- Retraction must be idempotent: retracting when nothing is actually preloaded (e.g. the
  invalidating action arrived after the preloaded track already started naturally) must be a safe
  no-op, not an error — `AudioError` must not gain a new variant for this case unless the audit
  found a real failure mode requiring one.
- After a retraction, if there is a new "actually next" track, the existing `06-06` preload path
  must fire for it in the same reducer pass — a retraction must never leave the queue in a state
  with no preload pending when one should be.
- `crates/loxia-audio/src/gapless.rs`'s own real-mpv test module (`#[cfg(all(test,
  feature = "mpv-tests"))]`) gets one new test exercising retract-then-repreload against a real
  mpv instance, following the existing `silent_wav_bytes`/`collect_transition` pattern already in
  that file.

## Acceptance

- Every `#[ignore]` marker added by `06-11` in `crates/loxia-core/src/reducer/queue.rs` is removed
  and its test passes.
- `mock_engine_retract_preload_is_recorded` (`loxia-audio`, `mock.rs`).
- `retract_preload_then_repreload_next_in_same_pass` (`loxia-core`, `reducer/queue.rs`).
- `retract_preload_when_nothing_preloaded_is_a_noop` (`loxia-audio` or `loxia-core`, wherever the
  idempotency check naturally lives).
- New `mpv-tests`-gated test in `crates/loxia-audio/src/gapless.rs` demonstrating a retracted
  preload never plays, against a real mpv instance (`cargo test --features mpv-tests`, not run in
  CI per that feature's own existing gating).
- `cargo clippy --workspace --all-targets -- -D warnings` clean across all three touched crates.
