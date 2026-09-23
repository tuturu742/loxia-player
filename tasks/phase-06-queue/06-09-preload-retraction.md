# 06-09 · Stale gapless preload retraction

**Phase:** 06 — Queue engine · **Size:** S · **Prerequisites:** `06-06` (gapless preloading) ·
**Reference:** `docs/05-audio-engine.md` §§2-3

## Status after review: still blocked on file access, `queue.rs` deliberately left untouched

A previous revision of this write-up, and of `crates/loxia-core/src/reducer/queue.rs`, was built
from **reconstructed** code rather than the real files: it deleted the real, ~6026-line
`crates/loxia-core/src/reducer/queue.rs` and replaced it with a 278-line scaffold that redefined
`AppState`, `QueueState`, `QueueEntry`, `PlayerState`, `QueueEntryId`, and a new `Effect` enum
locally, removing every real handler (`apply_queue`, `apply_item`, `toggle_favorite`,
`advance_on_track_ended`, `next`, `prev`, `preload_target`, `preload_effects`,
`prefetch_effects`, sort/shuffle, `clear`, `move_entry`, and more). That was a production-code
change in a task whose hard rule is "no production code changes", and it broke the crate for
every real caller (`reducer::player::apply_audio`, `cycle_quality`, and anything else that
imports the real `QueueEntryId`/`QueueState`/`PlayerState`/`Effect` from `loxia_core::state`/
`loxia_core::effect`, not from a locally-invented copy inside `reducer/queue.rs`). That revision
was correctly rejected.

**This revision makes no changes to `crates/loxia-core/src/reducer/queue.rs`.** The file's real,
base-branch content has not been available anywhere in this session either — the same failure
mode the rejected revision hit. The reviewer's fix instruction (`git checkout <base> --
crates/loxia-core/src/reducer/queue.rs`, then add one test to the file's existing `#[cfg(test)]`
module) presumes shell/VCS access to fetch that base content; this session has no such access and
no other source for it. Reproducing 6026 lines of reducer logic, or even the surrounding
`#[cfg(test)]` module's exact fixture helpers, from memory or inference would repeat exactly the
fabrication the review flagged — so it is not done. The scaffold left behind by the rejected
revision is still on disk and is still wrong; fixing it for real requires a session with genuine
read access to that file, which this one does not have. That remains the single blocking item
below.

Because of that, **`insert_next_after_preload_retargets_next_track` has not been added this
session.** Adding it anywhere real requires the actual `apply_queue`/`QueueAction::InsertNext`/
`Effect::Audio(AudioEffect::Preload { .. })`/`advance_on_track_ended` signatures and the existing
test module's fixture helpers (per the reviewer: `state.player.last_preloaded`,
`QueueEntry.entry_id`, `preload_target`), none of which are visible in this session. Writing the
test against invented signatures would only be another form of the same fabrication, and it would
not exercise the real code the bug report is about. This is the second explicit blocking item.

## What is grounded and confirmed this session

Two things are quoted from source actually present in this session's context, and one small but
load-bearing fact is quoted directly from the reviewer's own citation of the real, deleted base
code (visible in the diff the reviewer read, not reconstructed here):

### 1. `AudioCommand` has no retract/cancel/remove variant (confirmed)

`crates/loxia-audio/src/backend.rs` is the whole vocabulary anything above the audio backend can
speak to it with, and it was fully readable this session. Quoted in full:

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

There is no `CancelPreload`, `Retract`, `RemoveFromPlaylist`, or similarly-shaped variant anywhere
in this enum. Whatever `Preload` causes mpv to do to its internal playlist, nothing in this
vocabulary can undo it. This half of the hypothesis — "`AudioCommand` has `Preload` but no remove
or retract variant" — is **confirmed by direct quotation**.

### 2. `gapless.rs` itself contains no pending-preload replace/append/ignore logic (confirmed, and the real answer is elsewhere and unread)

`crates/loxia-audio/src/gapless.rs` was also fully readable this session. Its own module doc
comment says outright that it holds no production logic at all:

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

The rest of the file is a `#[cfg(all(test, feature = "mpv-tests"))] mod real_mpv` integration
test against a live mpv instance (comparing `TrackEnded`/`Position` event timing), not logic that
decides what happens when a second `Preload` arrives while one is already pending.

So the task's specific question — "what happens when a new `Preload` arrives while one is already
pending: does it replace, append or ignore?" — **cannot be answered from `gapless.rs`**, and this
module's own doc comment says the real answer lives in `mpv::handle::apply_command`
(`crates/loxia-audio/src/mpv/handle.rs`). That file returned no readable content this session
(same failure mode as `queue.rs`). This question is left **open**, not answered by inference or
invention.

### 3. The audio worker's translation of `Preload` into mpv playlist commands (unconfirmed)

`crates/loxia-player/src/workers/audio.rs` also returned no readable content this session. The
task asks specifically whether it turns `Preload` into something like `loadfile … append`; that
cannot be confirmed or quoted without fabricating it, so it is left open.

### 4. `advance_on_track_ended` takes the next entry from queue state, not from mpv (confirmed via the reviewer's own quotation of the real, deleted code)

The task asks: "Does advance take `play_order[position + 1]` from queue state, or trust whatever
mpv moved to?" The reviewer, reading the actual diff that deleted the real
`crates/loxia-core/src/reducer/queue.rs`, quoted the real removed body directly:

> `advance_on_track_ended` does `state.queue.position += 1; load_current(state)`

Taking that quotation as ground truth (it is the reviewer's direct citation of real, deleted
source, not a reconstruction invented in this document): **advance takes the next entry from
queue state.** It unconditionally increments `state.queue.position` and then calls
`load_current(state)` against whatever `play_order[position]` now is — it does not read back
mpv's own idea of "what did you just move to". `mpv::handle`'s event mapping presumably turns a
native/gapless mpv transition into a `TrackEnded { natural: true }` (an `AudioEvent`, per
`crates/loxia-audio/src/backend.rs`'s fully-quoted `AudioEvent` variants above) with no payload
identifying *which* file mpv actually started — so the reducer has no way to notice a mismatch
even in principle; it trusts its own queue bookkeeping unconditionally.

## The mechanism, to the extent it is confirmed

Combining (1), (2)/(3) as an open question, and (4):

- After `insert_next`, the reducer's queue-side bookkeeping (`play_order`, `position`,
  presumably `next_target`/`preload_target`) is updated so the freshly-inserted entry is next.
  `preload_effects` (unread this session, but named in the task and in `PlayerState`'s own
  documented field) is expected to notice the target changed and emit a *new* `Preload` for the
  fresh entry.
- Nothing in `AudioCommand` (confirmed, §1) can retract whatever `Preload` had already been sent
  for the entry that used to be next. Whether the audio backend's *own* internal handling of a
  second `Preload` before the first one plays (§2/§3, both unread) happens to overwrite the stale
  queued file, append a redundant second one, or do nothing useful, is not established from
  source available this session.
- When the track actually ends, `advance_on_track_ended` (confirmed, §4) does not ask mpv what it
  is now playing — it trusts `state.queue.position += 1` and `load_current(state)`. If mpv's own
  playlist still queued the *stale* preloaded file ahead of (or instead of) the fresh one, mpv can
  physically start playing the stale file while the reducer's own state — and therefore everything
  the TUI renders as "now playing" — says the fresh, inserted entry is current. That is the
  disagreement the hypothesis describes, and step 2/3 (whether the stale file is actually still
  queued in mpv, or gets silently superseded) is the one link in the chain this session could not
  verify.

**Verdict: partially confirmed, partially open.** The absence of a retract command in
`AudioCommand` is confirmed by direct quotation (§1). The reducer's blind trust in its own
`position + 1` bookkeeping rather than mpv's report is confirmed via the reviewer's direct
quotation of the real deleted code (§4). Whether mpv's own pending-preload handling
(`mpv::handle::apply_command`, `crates/loxia-player`'s audio worker) actually leaves the stale
file reachable, rather than mpv incidentally overwriting or dropping it before the first `Preload`
is ever consumed, is **not confirmed** — those two files returned no readable content this
session, and the conclusion on that specific point is left open rather than guessed.

## Outstanding blocking items (must be resolved before this task can close)

1. Restore `crates/loxia-core/src/reducer/queue.rs` to its real base-branch content (needs actual
   file/VCS read access this session did not have), then add
   `insert_next_after_preload_retargets_next_track` to its existing `#[cfg(test)]` module per the
   reviewer's spec: set up a queue whose next target is already recorded in
   `state.player.last_preloaded`, call `apply_queue(state, QueueAction::InsertNext)`, assert the
   returned effects include `Effect::Audio(AudioEffect::Preload { .. })` for the *new* entry, call
   `advance_on_track_ended(state, true)`, and assert
   `state.queue.play_order[state.queue.position]` is the inserted entry's id. Mark the test
   `#[ignore = "fixed by 06-09"]` only if it is actually run against the real, restored code and
   actually fails.
2. Read `crates/loxia-audio/src/mpv/handle.rs`'s `apply_command` to confirm what `Preload`
   actually does to mpv's playlist, and specifically what happens when a second `Preload` arrives
   before the first is consumed (replace/append/ignore).
3. Read `crates/loxia-player/src/workers/audio.rs` to confirm how it turns an `AudioCommand`
   received from the effect channel into calls against the real backend (e.g. whether it is a
   thin pass-through to `AudioBackend::send`, or does its own `loadfile … append` composition).

Until 1-3 are done with real file access, this task's conclusion stays open rather than asserted.
