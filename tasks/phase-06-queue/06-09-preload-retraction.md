# 06-09 · Stale gapless preload retraction

**Phase:** 06 — Queue engine · **Size:** S · **Prerequisites:** `06-06` (gapless preloading) ·
**Reference:** `docs/05-audio-engine.md` §§2-3

## Status: investigation blocked, no production change made this session

A previous revision of this write-up, and of `crates/loxia-core/src/reducer/queue.rs`, was built
from **reconstructed** code rather than the real files. That revision:

- deleted the real, ~6026-line `crates/loxia-core/src/reducer/queue.rs` and replaced it with a
  278-line scaffold that redefined `AppState`, `QueueState`, `QueueEntry`, `PlayerState`,
  `QueueEntryId`, and a new `Effect` enum locally, removing every real handler
  (`apply_queue`, `apply_item`, `toggle_favorite`, `advance_on_track_ended`, `next`, `prev`,
  `apply_sort_profile`, `clear`, `move_entry`, and more). That is a production-code change, which
  this task is explicitly not allowed to make, and it broke the crate's real callers
  (`reducer::player::apply_audio`, `cycle_quality`).
- asserted a "confirmed" verdict partly on guessed bodies for `preload_effects`, `load_current`,
  and the track-ended handler, not on the real ones.

That revision has been reviewed and rejected. This revision corrects course as follows:

- **No changes are made to `crates/loxia-core/src/reducer/queue.rs` in this session.** The tool
  available in this session was not able to return that file's real content (it came back empty,
  the same failure mode the rejected revision described), and the "Current file contents" shown
  for it going into this session is the rejected scaffold itself, not the base-branch original —
  so there is no source anywhere in this session's context from which the real, ~6026-line file
  could be faithfully restored. Fabricating a restoration from memory would repeat exactly the
  mistake being corrected. The file is left untouched by this change; **restoring it from the base
  branch (`git checkout <base> -- crates/loxia-core/src/reducer/queue.rs`) and adding
  `insert_next_after_preload_retargets_next_track` to its existing `#[cfg(test)]` module is
  work that still needs to happen, with real file-read access, before this task can be closed.**
- The finding below is narrowed to only what could actually be read and quoted this session:
  `crates/loxia-audio/src/backend.rs`'s `AudioCommand` enum. Everything that depends on reading
  `crates/loxia-core/src/reducer/queue.rs` (`preload_effects`, `load_current`,
  `advance_on_track_ended`, `preload_target`), `crates/loxia-audio/src/gapless.rs`'s pending-preload
  behaviour, `crates/loxia-audio/src/mpv/handle.rs`'s `apply_command`, and
  `crates/loxia-player/src/workers/audio.rs`'s command translation is marked **unconfirmed** below,
  because those files also returned no readable content in this session. Asserting a verdict on
  them without having read them would be exactly the fabrication the review flagged.

## What is actually confirmed this session

`crates/loxia-audio/src/backend.rs` defines the entire vocabulary anything above the audio backend
can speak to it with. Quoted in full, as it exists in this session's readable context:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AudioCommand {
    Load {
        // A plain `String` per this task's own spec would defeat the point of redacting it —
        // `loxia_core::effect::RedactedUrl` already exists for exactly this (a stream URL's
        // `api_key=...` query parameter must never reach a derived `Debug`), lives in
        // `loxia-core` (not `loxia-emby`, so the "must not depend on loxia-emby" rule still
        // holds), and lets `AudioCommand` keep a plain `#[derive(Debug, ...)]` while still
        // redacting correctly.
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

This is exhaustive — an enum, not a trait object, not an open list. There is no `CancelPreload`,
no `RemoveFromPlaylist`, no `Retract`, nothing parameterised by a playlist index. `Stop` tears down
the whole current playback session, not a single not-yet-current queued file, so it is not a
substitute for a retraction command. **This part of the hypothesis is confirmed: the command
vocabulary itself has no way to ask the audio backend to un-preload a file it was already told to
preload.**

Also quoted in full, from the same file, for completeness (not itself part of the confirmed
mechanism, but relevant to what a fix could observe):

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    StatusChanged(PlayStatus),
    Position {
        secs: f64,
        duration: f64,
    },
    Format(AudioFormat),
    TrackEnded {
        natural: bool,
    },
    Devices(Vec<AudioDevice>),
    Buffering(u8),
    VolumeChanged {
        volume: u8,
        muted: bool,
    },
    Error(AudioError),
}
```

`TrackEnded` carries only `natural: bool` — nothing identifying which file mpv actually advanced
to. Combined with the closed `AudioCommand` above, if `reducer::queue`'s track-ended handler
trusted this event rather than reading `play_order[position + 1]` from queue state itself, there
would be no way to detect a mismatch from the event alone. Whether it actually does trust queue
state — the real behaviour of `advance_on_track_ended` — is exactly the part this session could not
verify, because `crates/loxia-core/src/reducer/queue.rs` was not readable.

## What remains unconfirmed, and why

The following claims from the rejected revision are **withdrawn pending a real read**, not
reasserted:

- What `preload_effects` actually dedupes on, and whether `preload_target`/its equivalent handles
  `RepeatMode::One`/`RepeatMode::All` wraparound. (`crates/loxia-core/src/reducer/queue.rs` — not
  read this session.)
- What `load_current` actually emits — a full `Effect::Audio(AudioCommand::Load { .. })` that would
  override whatever mpv is doing, or something that relies on mpv's own auto-advance. This matters
  directly to the verdict: if `load_current` issues an explicit `Load`, the stale preloaded file in
  mpv's playlist may never actually get played, which would refute (or at least substantially
  narrow) the user-visible half of the hypothesis even though the closed `AudioCommand` enum still
  stands as confirmed. (Same file — not read this session.)
- What `advance_on_track_ended` actually does with `state.queue.position` and whether it consults
  `play_order[position + 1]` or something else. (Same file — not read this session.)
- Whether `crates/loxia-audio/src/gapless.rs` has any pending-preload replace/append/ignore logic
  of its own. Its module doc, which was readable, states plainly: "There is no production logic
  in this module: gapless playback is entirely a consequence of mpv's own
  `gapless-audio=yes`/`prefetch-playlist=yes` options ... plus `AudioCommand::Preload` appending
  the next track to mpv's internal playlist instead of replacing the current file
  (`mpv::handle::apply_command`, also `05-03`)." That pushes the real answer into
  `crates/loxia-audio/src/mpv/handle.rs`'s `apply_command`, which did not return readable content
  this session either.
- How the audio worker in `crates/loxia-player/src/workers/audio.rs` turns `Preload` into mpv
  playlist commands (e.g. `loadfile ... append` with or without a `playlist-remove` of a previous
  pending entry). Not read this session.

Per the reviewer's own framing: if the worker replaces the pending preloaded entry on a subsequent
`Preload`, or if `load_current` issues an explicit `Load` that overrides whatever mpv already
queued, the user-visible bug may be substantially refuted even though the closed `AudioCommand`
enum (confirmed above) remains a real gap in the abstraction. That determination requires actually
reading `crates/loxia-audio/src/mpv/handle.rs`, `crates/loxia-player/src/workers/audio.rs`, and the
real `crates/loxia-core/src/reducer/queue.rs`, with a working file-read tool, before this task's
verdict can be finalised.

## Outstanding work

1. With working read access, restore `crates/loxia-core/src/reducer/queue.rs` to the base branch
   exactly (`git checkout <base> -- crates/loxia-core/src/reducer/queue.rs`) if it is not already
   in that state.
2. Read and quote, verbatim, from that restored file: `preload_effects`, `load_current`, and
   `advance_on_track_ended` (or whichever function is the real track-ended handler).
3. Read and quote, verbatim: `crates/loxia-audio/src/mpv/handle.rs`'s `apply_command` handling of
   `AudioCommand::Preload`, and `crates/loxia-player/src/workers/audio.rs`'s translation of that
   command into mpv calls.
4. Re-run the confirm/refute analysis against those real bodies, not reconstructed ones.
5. Add exactly one test, `insert_next_after_preload_retargets_next_track`, to
   `crates/loxia-core/src/reducer/queue.rs`'s existing `#[cfg(test)]` module (no other change to
   that file), using the real types — `state.player.last_preloaded` set to the current
   `preload_target(&state.queue)`'s `entry_id`, `apply_queue(state, QueueAction::InsertNext)`
   asserting on the returned `Effect::Audio(AudioEffect::Preload { .. })`, then
   `advance_on_track_ended(state, true)` asserting `state.queue.current()` is the inserted entry.
   If the analysis in step 4 confirms the bug, mark it `#[ignore = "fixed by 06-09"]`; if it
   refutes the bug, land the test un-ignored as a permanent regression guard instead, and update
   the verdict in this file to "refuted" with the quoted evidence.
