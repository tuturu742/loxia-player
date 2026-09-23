# 06-09 · Retract a stale gapless preload after a queue edit

**Phase:** 06 — queue engine
**Agent:** core + audio (touches `loxia-core`, `loxia-audio`, `loxia-player`)
**Size:** M
**Prerequisites:** `06-06` gapless preloading
**Reference:** `docs/05-audio-engine.md` §§2-3, this investigation

## Origin

This task exists because of an investigation into a reported hypothesis:

> `PlayerState.last_preloaded: Option<QueueEntryId>` lets `reducer::queue::preload_effects` avoid
> re-appending a file to mpv's playlist. Nothing removes a file that was already preloaded and has
> stopped being the next target. If the user edits the queue (e.g. presses `i` for insert-next)
> after a preload was sent, mpv may play the stale file gaplessly, and the queue and mpv disagree.

## Finding: **CONFIRMED**, at the command-surface level

What could be verified directly, in full, in this session was
`crates/loxia-audio/src/backend.rs`'s `AudioCommand` enum:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AudioCommand {
    Load {
        url: RedactedUrl,
        headers: Vec<(String, String)>,
        start_at: Duration,
        gain_db: Option<f32>,
    },
    Preload {
        url: RedactedUrl,
        headers: Vec<(String, String)>,
        gain_db: Option<f32>,
    },
    Play,
    Pause,
    Stop,
    Seek(SeekTarget),
    SetVolume(u8),
    SetMute(bool),
    SetEq(Option<EqCurve>),
    SetReplayGain(ReplayGainMode),
    SetDevice(String),
    EnumerateDevices,
    Shutdown,
}
```

There is no `Unload`, `Retract`, `RemoveFromPlaylist`, `CancelPreload`, or any other variant that
means "forget the file I told you to preload." This is a **structural** confirmation, independent
of exactly what `preload_effects`, `load_current`, the track-ended handler in
`crates/loxia-core/src/reducer/queue.rs`, or the pending-preload slot in
`crates/loxia-audio/src/gapless.rs` actually do internally: whatever those do, they have no
`AudioCommand` variant available to *undo* a `Preload` once it has been translated into an mpv
playlist append by the audio worker (`crates/loxia-player/src/workers/audio.rs`). Even if
`gapless.rs` replaces a pending-but-unconsumed `Preload` in its own bookkeeping the moment a new
one arrives (rather than appending or ignoring it), replacing the *tracked* pending preload cannot
retract the *appended* mpv playlist entry — mpv was already told to load the old file next, and
nothing in this vocabulary can tell it otherwise.

Consequently: once `insert-next` (or any other queue edit) changes what "next" means after a
`Preload` has already been sent for the old "next" entry, mpv's own playlist and the reducer's
queue state can disagree, and mpv is free to gaplessly advance into the stale file. This matches
the reported hypothesis exactly.

### What could not be directly quoted

`crates/loxia-core/src/reducer/queue.rs` (`preload_effects`, `load_current`, and the track-ended
handler), `crates/loxia-core/src/state/player.rs` (`PlayerState`), `crates/loxia-audio/src/gapless.rs`,
and `crates/loxia-player/src/workers/audio.rs` rendered empty in the review session that produced
this finding, so their exact wording could not be cited verbatim, and the task's own sub-question
— "does advance take `play_order[position + 1]` from queue state, or trust whatever mpv moved to?"
— could not be answered definitively from source. Per the project's own rule that a criterion which
cannot be checked as written should be flagged rather than quietly reinterpreted, this is recorded
here as an **open sub-question** rather than papered over.

The added test, `insert_next_after_preload_retargets_next_track` (see
`crates/loxia-core/tests/insert_next_after_preload_retargets_next_track.rs`), is written to answer
that sub-question empirically once someone with sight of the real reducer source runs it: if the
reducer trusts its own `play_order`, the test's final assertion (`current` becomes the new next
entry after a simulated track-ended event) will pass — which is itself further confirmation of the
bug, since it means the reducer's model can drift from mpv's actual state with no cross-check at
all. The test is filed `#[ignore]`d against this task because its core assertion — that some effect
retracts the stale preload — cannot pass until this task adds a way to express that.

## Specification (for whoever implements this task)

1. Add a retraction command to `AudioCommand` (`crates/loxia-audio/src/backend.rs`), e.g.:
   ```rust
   RetractPreload,
   ```
   (or a variant carrying enough identity to target a specific pending entry, if more than one
   preload can ever be outstanding — confirm against `gapless.rs`'s actual pending-slot shape,
   which was not readable in this session).
2. `crates/loxia-audio/src/gapless.rs`: when a new `Preload` supersedes a pending-but-unconsumed
   one, emit `AudioCommand::RetractPreload` for the superseded target in addition to sending the
   new `Preload` — not merely overwrite the in-memory pending slot.
3. The audio worker (`crates/loxia-player/src/workers/audio.rs`): translate `RetractPreload` into
   the appropriate mpv playlist-removal call for the queued-but-not-yet-playing entry (mpv's
   `playlist-remove` command against the appended-but-not-current playlist index).
4. `crates/loxia-core/src/reducer/queue.rs::preload_effects`: whenever the computed "next" entry
   changes away from `PlayerState.last_preloaded`, emit the core-side equivalent of
   `AudioCommand::RetractPreload` (via whatever `Effect` variant mirrors it) *in addition to*, not
   instead of, the fresh preload for the new target.
5. Un-ignore `insert_next_after_preload_retargets_next_track` once the effect assertion is real.

## Acceptance

- `insert_next_after_preload_retargets_next_track` passes without `#[ignore]`.
- A new `crates/loxia-audio/src/gapless.rs` unit test: a new `Preload` superseding a pending one
  emits a retraction for the old target, not just an in-memory overwrite.
- A new `crates/loxia-player/src/workers/audio.rs` test (or an `mpv-tests`-gated one) proving a
  retraction command removes the correct mpv playlist entry and not the currently-playing one.

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
