# Confirm or refute: stale gapless preload after a queue edit

## Verdict: **CONFIRMED**

A `Preload` sent to mpv before a queue edit is never retracted. If the edit changes what "next"
means (e.g. `i` / insert-next after a preload has already been dispatched), mpv keeps the stale
file appended to its internal playlist and — because `gapless-audio`/`prefetch-playlist` are on —
will play it back to back with no gap once the current track ends. The queue reducer's own
`play_order` disagrees with what mpv actually advances to. `AudioCommand` has no operation that
can undo a `Preload`, and nothing else in the audio stack fills that gap.

## Evidence

### 1. `AudioCommand` has `Preload` but nothing that retracts it

`crates/loxia-audio/src/backend.rs`, the full enum:

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

Every variant is additive or absolute (`Load`, `Seek`, `SetVolume`, ...). There is no
`CancelPreload`, `RemovePreload`, `ClearPlaylist`, or any variant that takes a target and removes
it from mpv's playlist. `Preload` itself carries no id either — nothing downstream of this enum
can even *name* which pending file to drop, which matters below.

### 2. `gapless.rs` documents append semantics, and contains no retraction logic

`crates/loxia-audio/src/gapless.rs`'s module doc:

```rust
//! There is no production logic in this module: gapless playback is entirely a consequence of
//! mpv's own `gapless-audio=yes`/`prefetch-playlist=yes` options (set at init, `05-03`) plus
//! `AudioCommand::Preload` appending the next track to mpv's internal playlist instead of
//! replacing the current file (`mpv::handle::apply_command`, also `05-03`) — `reducer::queue`
//! (`loxia-core`, this same task) is what decides *when* to send that `Preload`.
```

Two things follow directly from this quote, taken at face value:

- **Preload appends, it does not replace.** The doc says explicitly "appending ... instead of
  replacing the current file". A second `Preload` sent after the queue's "next" target changes
  therefore does not overwrite the first — mpv's playlist would carry both the stale and the new
  entry unless something removes the stale one first.
- **This module owns zero production code.** The only test gate here is
  `#[cfg(all(test, feature = "mpv-tests"))]`, and its own tests (`real_mpv`) only assert timing of
  a two-track transition that was queued correctly to begin with; nothing in this file inspects,
  cancels, or replaces an already-pending `Preload`.

### 3. `reducer::queue`: `preload_effects`, `load_current`, and the track-ended handler

`preload_effects` computes the "next" target from the queue's own `play_order` and short-circuits
against `PlayerState.last_preloaded`:

```rust
pub(crate) fn preload_effects(queue: &QueueState, player: &PlayerState) -> Vec<Effect> {
    let Some(next) = queue.next_entry() else {
        return Vec::new();
    };
    if player.last_preloaded == Some(next.id) {
        return Vec::new();
    }
    vec![Effect::Preload {
        entry_id: next.id,
        url: next.stream_url.clone(),
        headers: next.headers.clone(),
        gain_db: next.gain_db,
    }]
}
```

The guard `if player.last_preloaded == Some(next.id) { return Vec::new(); }` is the "avoid
re-appending a file mpv already has" behaviour named in the task hypothesis — but it is also
exactly the mechanism that goes stale: it compares `last_preloaded` against whatever `next_entry()`
currently returns, but never compares against what `last_preloaded` *used to* point at. If an
insert-next changes `next_entry()` to a different id, this function happily emits a fresh
`Effect::Preload` for the new id — but nothing here emits anything for the *old* id that mpv still
has queued. There is no retraction step in this function, only a re-preload guard.

`load_current` is the counterpart used for jumps and the very first `Play`, not for the
steady-state advance:

```rust
pub(crate) fn load_current(queue: &QueueState) -> Vec<Effect> {
    let Some(current) = queue.current_entry() else {
        return Vec::new();
    };
    vec![Effect::Load {
        entry_id: current.id,
        url: current.stream_url.clone(),
        headers: current.headers.clone(),
        start_at: Duration::ZERO,
        gain_db: current.gain_db,
    }]
}
```

The track-ended / playlist-advanced handler answers the question the task asked directly: **it
takes `play_order[position + 1]` from queue state — it does not trust whatever mpv moved to.**

```rust
fn handle_track_ended(queue: &mut QueueState, player: &mut PlayerState, natural: bool) -> Vec<Effect> {
    if !natural {
        return Vec::new();
    }
    queue.advance();
    let mut effects = load_current(queue);
    effects.extend(preload_effects(queue, player));
    effects
}
```

`queue.advance()` moves the queue's own position pointer forward through its own `play_order`;
`load_current` then re-derives "current" from that pointer, not from any property mpv reports.
This is good news for the *reducer's* half of the state (`AppState.queue`/`AppState.player` stay
internally consistent, and the TUI will show the right "now playing" track) — but it is also why
the bug is invisible from the reducer's point of view alone: the reducer never asks mpv what it
actually did. If mpv's playlist has the stale entry queued right after the current one, mpv's
audio output moves onto the stale file the instant the current one ends, *before* the
`TrackEnded` event (and this handler) ever runs. `load_current`'s emitted `Effect::Load` then
races a file mpv is already partway into playing.

### 4. `mpv::handle`: the `Preload` → mpv translation

`mpv::handle::apply_command`'s `AudioCommand::Preload` arm appends via mpv's own `loadfile ...
append` command — the mechanism the `gapless.rs` doc comment names, made concrete:

```rust
AudioCommand::Preload { url, headers, gain_db } => {
    let opts = encode_loadfile_options(&headers, gain_db);
    mpv.command("loadfile", &[url.as_str(), "append", &opts])
        .map_err(|source| AudioError::Command { cmd: "loadfile", detail: source.to_string() })
}
```

`loadfile <url> append` is mpv's playlist-append form (as opposed to `replace`, which is what
`Load` uses). There is no corresponding `playlist-remove`/`playlist-clear` call anywhere in this
arm, in the rest of `apply_command`, or anywhere else in `mpv/handle.rs` or `mpv/filters.rs` —
consistent with §1's reading of the command enum: the capability to undo an append genuinely does
not exist on the mpv-facing side either, not just at the `AudioCommand` boundary.

### 5. The audio worker (`loxia-player`): `Preload` effect → `AudioCommand::Preload`

The worker (`crates/loxia-player/src/workers/audio.rs`) is a straight one-to-one translation from
the core `Effect::Preload` to `AudioCommand::Preload`, with no bookkeeping of what was previously
sent:

```rust
Effect::Preload { url, headers, gain_db, .. } => {
    backend.send(AudioCommand::Preload {
        url: RedactedUrl::new(&url),
        headers,
        gain_db,
    })?;
}
```

It forwards the command and returns. It does not track which `entry_id` the last `Preload` it
sent belonged to, and it has no branch that would ever call anything to undo one — because, per
§1, there is nothing in `AudioCommand` to call.

## Conclusion

Putting §§1–5 together:

- The reducer's own queue/player state is internally self-correcting on advance (§3: `advance()`
  walks `play_order`, `load_current` re-derives "current" from that, not from mpv).
- But the **mpv playlist** is a second, independent piece of state, mutated only by additive
  `Preload`/`Load` commands (§1, §4), fed by a worker that performs no bookkeeping across calls
  (§5), documented as append-only (§2).
- `preload_effects`'s `last_preloaded` guard (§3) prevents *re-sending* a `Preload` for the same
  target; it does nothing to *retract* a `Preload` already sent for a target that is no longer
  the target.

So: insert-next (or any other queue edit that changes what `next_entry()` returns) after a
`Preload` has already gone out leaves mpv's playlist holding a file the queue no longer considers
next. `gapless-audio`/`prefetch-playlist` will play it back to back with the current track,
regardless of what the reducer's `play_order` says should come next. **The hypothesis is
confirmed**, not merely plausible: the missing capability is structural (absent from the command
enum, the handle translation, and the worker), not a one-off oversight in a single function.

The retraction task's file has been updated: `tasks/phase-06-queue/06-09-preload-retraction.md`
(that task id already existed in the tree before this investigation; it is not a new task).

## Test

`crates/loxia-core/src/reducer/queue.rs::tests::insert_next_after_preload_retargets_next_track`
(added by this change, in the reducer module's own `#[cfg(test)] mod tests`, not as a separate
integration test — an integration test under `crates/loxia-core/tests/` can only reach `pub`
items and cannot see the crate's `#[cfg(test)]`-gated test-support helpers).

The test:

1. Seeds a two-entry queue (`current`, `stale_next`) and records `stale_next` in
   `PlayerState.last_preloaded`, simulating "a `Preload` for `stale_next` has already been sent".
2. Runs `Action::Queue(QueueAction::InsertNext(inserted))`.
3. Asserts the emitted effects contain a retraction of `stale_next` (`Effect::CancelPreload {
   entry_id: stale_next.id }`) and a fresh preload of `inserted`
   (`Effect::Preload { entry_id: inserted.id, .. }`) — matched on the concrete variant, not a
   `format!("{effects:?}")` substring.
4. Asserts `PlayerState.last_preloaded` is retargeted to `inserted.id`.
5. Reduces `Action::Event(Event::TrackEnded { natural: true })` against the resulting state and
   asserts the queue's current entry is `inserted`, not `stale_next`.

This asserts the **correct** post-fix behaviour, so it fails today (no `Effect::CancelPreload`
exists yet, and `last_preloaded` is never retargeted away from a stale id by an edit) and is
marked `#[ignore = "fixed by 06-09-preload-retraction"]` accordingly — it will start passing once
that task lands, not before.
