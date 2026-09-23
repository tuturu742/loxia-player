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

## Finding: **CONFIRMED**

### The evidence, quoted

`crates/loxia-audio/src/backend.rs` gives the full, closed vocabulary between everything above the
audio engine and mpv itself. Its own doc comment states there is nothing wider than this:

> "The whole workspace's only door into the audio engine."

and the enum in full:

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

There is no variant here shaped like "forget a preload", "cancel a preload", "remove a playlist
entry", or "clear the playlist". Twelve variants, none of them retraction.

`crates/loxia-audio/src/gapless.rs`'s module doc comment, in full, is the authoritative
description of what `Preload` actually does once it reaches mpv, and it says two things that
matter here:

```text
//! There is no production logic in this module: gapless playback is entirely a consequence of
//! mpv's own `gapless-audio=yes`/`prefetch-playlist=yes` options (set at init, `05-03`) plus
//! `AudioCommand::Preload` appending the next track to mpv's internal playlist instead of
//! replacing the current file (`mpv::handle::apply_command`, also `05-03`) — `reducer::queue`
//! (`loxia-core`, this same task) is what decides *when* to send that `Preload`. This module
//! exists to hold the one thing that genuinely needs real mpv to verify: that two tracks appended
//! this way actually play back to back with no audible gap.
```

Two direct answers to the review's own question ("does a new `Preload` replace, append, or
ignore a pending one?"): first, **there is no pending-preload tracking structure in this module at
all** ("no production logic in this module") — so there is nothing here that could notice a second
`Preload` and discard the first. Second, whatever `Preload` does inside `mpv::handle::apply_command`
is explicitly described as *appending* to mpv's playlist, "instead of replacing the current file" —
an additive operation, not a corrective one.

`crates/loxia-player/src/workers/audio.rs` confirms the shape of the boundary those two files
describe: the worker's only path from a queue-driven effect to the engine is

```rust
Some(Effect::Audio(effect)) => {
    match &effect {
        AudioEffect::Load { url, start_at, .. } => {
            tracing::info!(url = %url, sta...
```

i.e. `Effect::Audio(AudioEffect)` values are unwrapped and, ultimately, translated 1:1 into
`AudioCommand` values and handed to `backend.send(cmd)`. Nothing upstream of `mpv::handle::apply_command`
widens the vocabulary past the twelve `AudioCommand` variants quoted above.

### The argument

`mpv::handle::apply_command` — wherever its exact body lives — can only ever be reached with one of
the twelve `AudioCommand` values above, because that enum is, in its own crate's words, "the whole
workspace's only door into the audio engine." A `playlist-remove` or `playlist-clear` call to mpv
can only exist inside a match arm on this enum. Inspecting which arm could plausibly carry that
intent:

- No variant is named or shaped for it (no `Retract`, `CancelPreload`, `ClearPlaylist`, or similar).
- `Preload` itself is documented, by the one file whose entire purpose is describing what `Preload`
  does to mpv's playlist, to *append*, explicitly "instead of replacing" — ruling out the one
  remaining plausible place a removal could be hiding (a second `Preload` implicitly clearing the
  first before appending again).
- `Load` clears and restarts playback outright (it targets the *current* file, not the next), which
  is a different mpv playlist operation than surgically removing one already-preloaded, not-yet-current
  entry.

This holds regardless of what `reducer::queue.rs`'s `preload_effects`/`load_current`/track-ended
handler actually do internally, and regardless of whether the track-ended handler advances via
`play_order[position + 1]` from queue state or trusts whatever mpv itself moved to — because
whatever `Effect`s a queue edit like insert-next produces, those effects can only ever become one of
the twelve `AudioCommand`s above once they reach the worker and `mpv::handle::apply_command`. There
is no wider vocabulary anywhere in the chain for a removal to travel through.

Therefore: once `PlayerState.last_preloaded` records a `QueueEntryId` and mpv has been told
`Preload` for it (i.e. that file is now appended to mpv's internal playlist), no subsequent queue
edit that changes the intended next track can cause mpv to drop that specific stale playlist entry.
mpv will still advance into it gaplessly when the current track ends, at which point the queue's
own `current`/`play_order` state and mpv's actual internal playlist genuinely disagree. The
hypothesis is confirmed.

### What is still not directly quoted, and why it doesn't change the verdict

`crates/loxia-core/src/reducer/queue.rs` (the literal bodies of `preload_effects`, `load_current`,
and the track-ended handler) and the `Preload` match arm inside `crates/loxia-audio/src/mpv/handle.rs`
did not render with visible content in the environment available for this pass, even after
re-attempting the read. That is a real gap against two specific asks from review — quoting
`preload_effects`/`load_current` verbatim, and stating plainly whether the track-ended handler reads
`play_order[position + 1]` or trusts mpv's own advance. Escalating that in one line: if a future task
needs those exact bodies (for example, to scope exactly where a fix should live), they still need a
direct read (e.g. `sed -n '1,400p' crates/loxia-core/src/reducer/queue.rs`) that this pass could not
obtain; nothing above should be read as evidence that those files are actually empty in the real
repository — only that their content did not reach this session.

Critically, the CONFIRMED verdict above does not lean on either file: it is derived entirely from
the closed `AudioCommand` enum (fully quoted, from a file that *did* render) and `gapless.rs`'s own
module doc (also fully quoted). Whatever `reducer::queue.rs` does internally, it cannot make a
removal happen that the command vocabulary has no way to express.

### Disposition

This task is confirmed **needed**, not "not needed". The concrete mechanism to build against, for
whoever picks this task up:

- Either add a new `AudioCommand` variant (e.g. `AudioCommand::RetractPreload` or
  `AudioCommand::ClearUpcoming`) that `mpv::handle::apply_command` turns into an actual
  `playlist-remove`/`playlist-clear` against the entries mpv appended past the current one, and have
  `reducer::queue::preload_effects` emit it whenever the retargeted next entry differs from
  `PlayerState.last_preloaded`; or
- Make `Preload` itself idempotent by having `mpv::handle::apply_command` clear any already-appended,
  not-yet-current playlist entries before appending the new one, removing the need for a new command
  at all.

Either way, `PlayerState.last_preloaded` needs to be cleared/updated in the same reducer step that
emits the retraction, so it cannot itself go stale relative to what was actually sent.

## Acceptance

- `crates/loxia-core/tests/insert_next_after_preload_retargets_next_track.rs`:
  `insert_next_after_preload_retargets_next_track`, `#[ignore = "fixed by
  06-09-retract-stale-preload"]` until this task lands, then un-ignored as part of this task's own
  PR.
