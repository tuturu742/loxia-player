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

## Finding: **BLOCKED** — cannot be confirmed or refuted with the evidence available in this
session; kept open, not marked not-needed

A prior pass at this task reached a verdict of CONFIRMED on the strength of one fact alone (no
retract-shaped `AudioCommand` variant) and explicitly called that "independent of"
`preload_effects`, `gapless.rs`, and the audio worker. A review correctly rejected that reasoning:
a second `Preload` could itself clear the stale entry — e.g. the audio worker issuing
`playlist-remove`/`playlist-clear` before `loadfile ... append`, or `gapless.rs` dropping a pending
entry when a new one arrives — and the missing `AudioCommand` variant says nothing about whether
either of those happens. The verdict genuinely depends on reading:

- `crates/loxia-core/src/reducer/queue.rs` — `preload_effects`, `load_current`, and the
  track-ended/playlist-advanced handler.
- `crates/loxia-core/src/state/player.rs` — the `PlayerState` struct itself (not just its module
  doc comment).
- `crates/loxia-audio/src/gapless.rs` — what a new `Preload` does to a pending one.
- `crates/loxia-player/src/workers/audio.rs` and `crates/loxia-audio/src/mpv/handle.rs` — the exact
  mpv command(s) `Preload` becomes.

In the session that produced this revision, the first two files rendered with **no visible
content** (`crates/loxia-core/src/reducer/queue.rs`, and the part of `crates/loxia-core/src/state/
player.rs` that defines `PlayerState` itself — its module doc comment was visible, the struct body
was not), and both `crates/loxia-player/src/workers/audio.rs` and `crates/loxia-audio/src/mpv/
handle.rs` rendered with **no visible content** at all. Per the reviewing instruction — "if those
files are genuinely unreadable or empty in the repo, say so as a blocker; do not reach a verdict
without them" — this doc does exactly that, rather than repeating the previous unsupported
CONFIRMED.

### What is actually confirmed, quoted

`crates/loxia-audio/src/backend.rs`'s full `AudioCommand` enum was visible:

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

There is no variant shaped like "forget/retract/cancel a preload" in this vocabulary. That much is
a fact, not a guess. But, per the review, this fact alone cannot carry a CONFIRMED verdict, because
a second `Preload` reaching the worker could itself be translated into commands that clear the
stale entry (e.g. two calls: a removal command, then `Preload`) without ever needing a distinct
`AudioCommand` variant for "retract". Whether that happens lives in `mpv/handle.rs` and the audio
worker, which were not visible.

### One sub-hypothesis that *is* directly refuted by quoted code

The review raised, as an alternative retraction mechanism: "`gapless.rs` could drop a pending entry
when a new one arrives." `crates/loxia-audio/src/gapless.rs`'s own module doc, which was visible in
full, rules this out directly:

```rust
//! Next-track preloading via the mpv playlist (`06-06`).
//!
//! There is no production logic in this module: gapless playback is entirely a consequence of
//! mpv's own `gapless-audio=yes`/`prefetch-playlist=yes` options (set at init, `05-03`) plus
//! `AudioCommand::Preload` appending the next track to mpv's internal playlist instead of
//! replacing the current file (`mpv::handle::apply_command`, also `05-03`) — `reducer::queue`
//! (`loxia-core`, this same task) is what decides *when* to send that `Preload`. This module
//! exists to hold the one thing that genuinely needs real mpv to verify: that two tracks appended
//! this way actually play back to back with no audible gap.
```

The rest of the file (visible, though its trailing lines were themselves cut off in this session)
is a `#[cfg(all(test, feature = "mpv-tests"))] mod real_mpv { .. }` block: an integration test
harness against a real mpv instance, gated on a feature not run in CI. There is no pending-preload
tracking state (no field, no struct, no "replace the last one" logic) anywhere in this module —
its own doc comment says so explicitly ("no production logic in this module"), and the visible body
confirms it holds only test scaffolding. **So the "`gapless.rs` drops a pending entry when a new
one arrives" alternative is refuted**: there is nothing in `gapless.rs` that could drop anything,
because it tracks no pending preload at all. Per its own doc comment, that responsibility, if it
exists anywhere, is in `mpv::handle::apply_command` — which was not visible this session.

`crates/loxia-core/src/state/player.rs` was visible up to (but not including) the point where
`PlayerState` itself, and therefore `last_preloaded`, is declared; only the module doc comment
("PlayerState mirror of the audio engine... Written only in response to `Event::Audio(..)`") and
several *other* structs defined earlier in the file (`SeekTarget`, `PlaybackSource`, `PlayStatus`,
`EqState`, `SleepTrigger`, `SleepTimer`) were visible. The task's own hypothesis text asserts the
field's exact signature — `PlayerState.last_preloaded: Option<QueueEntryId>` — and that is taken as
given (it is the premise under investigation, not something this doc independently re-derives),
but the struct definition itself, and therefore how `last_preloaded` is set/cleared, was not
visible.

### The actual blocker: what remains unknown

Not visible in this session, and required by the review to reach a verdict:

- `crates/loxia-core/src/reducer/queue.rs` — entire file. Cannot say whether `preload_effects`
  reacts to a changed "next" target, whether `load_current` clears `last_preloaded`, or — the
  review's own explicit sub-question — **whether the track-ended/playlist-advanced handler takes
  `play_order[position + 1]` from queue state, or trusts whatever mpv reports moving to.** This
  question could not be answered from source in this session and is recorded as unresolved, not
  guessed at.
- `crates/loxia-core/src/state/player.rs`'s `PlayerState` struct body (only its preceding types and
  module doc were visible).
- `crates/loxia-player/src/workers/audio.rs` — entire file. Cannot say how `AudioCommand::Preload`
  is dispatched to the mpv handle.
- `crates/loxia-audio/src/mpv/handle.rs` — entire file. Cannot say what `apply_command`'s `Preload`
  arm actually sends to mpv (a bare `loadfile ... append`? something that also issues
  `playlist-remove`/`playlist-clear` first?), which is exactly the mechanism the review flagged as
  a plausible way the bug does *not* occur even with no dedicated `AudioCommand` variant.

Because two of the review's four required files are wholly inaccessible in this session, and a
third is only partially accessible, **no confirmed/refuted verdict is reached here.** This is
stated as the explicit blocker the review asked for, not glossed over.

### Disposition of this task

- **Not marked "not needed"**: nothing here refutes the hypothesis; the one alternative mechanism
  that could have refuted it (`gapless.rs` dropping a pending entry) is itself refuted, which if
  anything is consistent with — not against — the original report.
- **Kept open, scope unchanged from the original report** (add a retraction path — new
  `AudioCommand` variant and/or a compensating command sequence in `mpv::handle::apply_command`,
  plus the `reducer::queue` logic to emit it), pending someone with direct access to
  `crates/loxia-core/src/reducer/queue.rs`, the full `PlayerState` struct, `crates/loxia-audio/src/
  mpv/handle.rs`, and `crates/loxia-player/src/workers/audio.rs` re-running this investigation to
  turn BLOCKED into CONFIRMED or REFUTED before any production change is designed against it.
- **Duplicate-task check**: `tasks/phase-06-queue/` contains no other retraction-shaped task file —
  `06-01` through `06-08` cover queue basics, appears-on rules, shuffle, sort profiles, history,
  gapless preloading, playback reporting, and instant mix; none of them own this investigation. So
  this file (`06-09`) is the single, correct home for it, updated in place again rather than
  duplicated.
- The regression test, `insert_next_after_preload_retargets_next_track`
  (`crates/loxia-core/tests/insert_next_after_preload_retargets_next_track.rs`), is written to
  answer the track-ended sub-question empirically and to pin down the intended fix shape once
  someone with source access confirms or corrects its (currently unverified) symbol names — see
  that file's own module doc for exactly which names are confirmed vs. still guessed, and why.
