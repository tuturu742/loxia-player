# 06-09 · Stale gapless preload retraction

**Phase:** 06 — Queue engine · **Size:** S · **Prerequisites:** `06-06` (gapless preloading) ·
**Reference:** `docs/05-audio-engine.md` §§2-3

## Status

This task exists to fix a bug **confirmed** during investigation (below), not yet fixed. The
regression test that exposes it — `insert_next_after_preload_retargets_next_track` in
`crates/loxia-core/src/reducer/queue.rs` — is checked in and `#[ignore]`d, pointing back at this
task's id. Un-ignore it as part of the acceptance criteria once the fix lands.

## Investigation finding: stale gapless preload after a queue edit — **CONFIRMED**

The hypothesis under test was:

> `PlayerState.last_preloaded: Option<QueueEntryId>` lets `reducer::queue::preload_effects` avoid
> re-appending a file to mpv's playlist. Nothing removes a file that was already preloaded and has
> stopped being the next target. `AudioCommand` has `Preload` but no remove/retract variant. So if
> the user presses `i` after a preload was sent, mpv may play the stale file gaplessly, and the
> queue and mpv would then disagree.

### 1. `AudioCommand` is closed, and has no retraction variant

`crates/loxia-audio/src/backend.rs` defines the entire vocabulary anything above the audio
backend is able to speak. Quoted in full:

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

This is exhaustive — an enum, not a trait object, not an open list. There is no `CancelPreload`,
no `RemoveFromPlaylist`, no `Retract`, nothing parameterised by a playlist index. `Stop` tears down
the whole current playback session, not a single not-yet-current queued file, so it isn't a
substitute. **Whatever `gapless.rs` and the audio worker do internally with a preload, once it has
actually been dispatched as `AudioCommand::Preload` and appended to mpv's own playlist, nothing in
this crate boundary can undo it.** This holds regardless of `gapless.rs`'s own pending-preload
dedup/replace/append policy, or the audio worker's exact mpv playlist call — those only govern
*commands not yet sent*; they cannot reach back into mpv's playlist after the fact, because no
command exists to ask for that.

(`crates/loxia-audio/src/gapless.rs` and the `Preload`-handling arm of the worker in
`crates/loxia-player/src/workers/audio.rs` did not return readable content in this investigation
session, so their exact "replace vs. append a *pending, unsent* preload" behaviour is not quoted
here. It does not change the conclusion above, which is derived entirely from `AudioCommand` being
closed — see `docs/12-decisions.md` note to be added when this task lands, recording that a fix
requires widening that enum, which is itself evidence the bug is structural rather than a call-site
oversight.)

### 2. `AudioEvent::TrackEnded` cannot carry "what mpv actually moved to"

Also quoted in full from the same file:

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

`TrackEnded` carries exactly one field, `natural: bool` — no URL, no queue entry id, nothing that
identifies *which* file mpv actually started playing next. This settles the specific question this
investigation was asked to answer for `crates/loxia-core/src/reducer/queue.rs`'s track-ended
handler: **it cannot possibly "trust whatever mpv moved to", because that information is not part
of the event it receives.** The only track identity available to it is its own queue state, so
advance is necessarily computed as `play_order[position + 1]` — reproduced faithfully in this
revision's `on_track_ended` (see `crates/loxia-core/src/reducer/queue.rs`).

### 3. The two facts combine into the confirmed bug

1. `preload_effects` sends `AudioCommand::Preload` for the queue's current "next" entry (call it
   `B`) and records `PlayerState::last_preloaded = Some(B)`. Per `docs/06-06-gapless-preloading.md`,
   this must happen *before* the current track ends, or it isn't gapless — so by the time anything
   else happens, `B` is already live in mpv's own playlist.
2. The user presses `i` (insert-next), landing a new entry `C` ahead of `B`. `preload_effects` is
   called again, correctly notices `last_preloaded != next_target()` any more, and emits a fresh
   `Preload` for `C` — but has no effect available (per §1) that also retracts `B`.
3. The current track ends naturally. `TrackEnded { natural: true }` arrives with no identifying
   payload (per §2), so the reducer advances the *queue's* position to `C` and reports `C` as now
   current.
4. mpv, having never been told to drop `B`, gaplessly continues into `B` — the file the user just
   asked to *not* play next. **The queue believes `C` is playing; mpv is playing `B`.**

This is the exact failure the hypothesis named, confirmed by code that is closed by construction
(§1) and by an event shape that structurally cannot detect the disagreement (§2), not merely by an
oversight in one call site.

### Regression test

`crates/loxia-core/src/reducer/queue.rs::tests::insert_next_after_preload_retargets_next_track`
reproduces steps 1-3 above at the reducer level and asserts the two things a fix must provide:

- `preload_effects` after the insert-next still correctly re-targets `C` (this part already works).
- `preload_effects` also emits *something* accounting for retracting the now-stale `B` (this part
  fails today — the assertion `effects.len() > 1` fails because only the `C` preload is emitted,
  with nothing addressing `B`). This is why the test is `#[ignore]`d rather than deleted.

## Fix scope (not implemented by this task's own predecessor)

- Add a removal/retraction variant to `AudioCommand` (e.g. `CancelPreload` or
  `RemoveFromPlaylist(...)`), and a matching `Effect` variant in `loxia-core`.
- `preload_effects` must emit that effect whenever `last_preloaded` is `Some` and no longer equals
  `next_target()`, in the same call that emits the replacement preload.
- The audio worker (`crates/loxia-player/src/workers/audio.rs`) must translate it into the
  corresponding mpv playlist-removal call, and `gapless.rs` must track *dispatched* (not just
  pending) preloads so it knows what to remove.
- Extend, don't replace, `insert_next_after_preload_retargets_next_track` — un-ignore it once the
  fix lands, per `tasks/README.md`'s rule that invariant-guarding tests are only ever extended.

## Files

- `crates/loxia-audio/src/backend.rs` — new `AudioCommand`/`AudioEvent` variant(s).
- `crates/loxia-audio/src/gapless.rs` — track dispatched (not just pending) preloads.
- `crates/loxia-player/src/workers/audio.rs` — translate the new command into an mpv playlist
  removal.
- `crates/loxia-core/src/effect.rs`, `crates/loxia-core/src/reducer/queue.rs` — new `Effect`
  variant and `preload_effects` retraction logic.

## Acceptance

- [ ] `insert_next_after_preload_retargets_next_track` (`crates/loxia-core/src/reducer/queue.rs`)
      passes with its `#[ignore]` removed.
- [ ] A `loxia-audio` test proves the new command actually removes an entry from mpv's playlist
      (behind `mpv-tests`, per existing convention).

## Done when

Global Definition of Done (`tasks/README.md`), plus the acceptance tests above.
