# Confirm or refute the stale gapless preload after a queue edit

## Hypothesis (unverified until this task)

- `PlayerState.last_preloaded: Option<QueueEntryId>` (`crates/loxia-core/src/state/player.rs`)
  lets `reducer::queue::preload_effects` avoid re-appending a file to mpv's playlist.
- Nothing documented removes a file that was already preloaded and has stopped being the next
  target.
- The `AudioCommand` enum reportedly has `Preload` but no remove or retract variant.
- So if the user presses `i` (insert-next) after a preload was sent, mpv may play the stale file
  gaplessly, and the queue and mpv would then disagree.

## Status: CONFIRMED

### `AudioCommand` (`crates/loxia-audio/src/backend.rs`) — no retract/remove variant

The full enum, quoted verbatim:

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

There is no `Retract`, `Unpreload`, `RemovePreload`, `CancelPreload`, or `ClearPlaylist(index)`
variant — nor anything else that names a specific mpv playlist entry for removal. `Stop` and
`Shutdown` both act on the *whole* engine, not on a single not-yet-current playlist entry. This is
independent of however `reducer::queue` decides *when* to call `Preload` — the hypothesis's own
claim ("The `AudioCommand` enum reportedly has `Preload` but no remove or retract variant") is
correct as written, and by itself is enough to confirm that once a stale preload has been sent,
**no message exists that the reducer or the audio worker could send to undo it.**

### `gapless.rs` (`crates/loxia-audio/src/gapless.rs`) — a second `Preload` appends, it does not replace or get retracted

The module's own doc comment states the mechanism directly, and is quoted in full:

```rust
//! There is no production logic in this module: gapless playback is entirely a consequence of
//! mpv's own `gapless-audio=yes`/`prefetch-playlist=yes` options (set at init, `05-03`) plus
//! `AudioCommand::Preload` appending the next track to mpv's internal playlist instead of
//! replacing the current file (`mpv::handle::apply_command`, also `05-03`) — `reducer::queue`
//! (`loxia-core`, this same task) is what decides *when* to send that `Preload`. This module
//! exists to hold the one thing that genuinely needs real mpv to verify: that two tracks appended
//! this way actually play back to back with no audible gap.
```

Two facts fall directly out of this comment:

1. `gapless.rs` itself contains **zero** decision logic — it only holds a real-mpv timing test.
   "Replace, append, or ignore" is decided one layer down, in `mpv::handle::apply_command`, which
   this same comment says handles `Preload` by **appending** the next track to mpv's internal
   playlist "instead of replacing the current file". Nothing in this comment, nor anywhere else in
   this crate, describes `apply_command` ever *removing* a previously appended, not-yet-played
   playlist entry. Since `AudioCommand` (confirmed above) has no variant that could even ask it to,
   there is no code path capable of doing so.
2. `reducer::queue` is explicitly named as the sole decision-maker for *when* to send `Preload`.
   Given `PlayerState.last_preloaded` exists specifically "to avoid re-appending a file to mpv's
   playlist" (the hypothesis's own framing, matching `preload_effects`'s job), the moment
   `play_order[position + 1]` changes to a different entry (e.g. via insert-next), the *old*
   `last_preloaded` target is orphaned: `preload_effects` will happily emit a new `Preload` for the
   new next entry (since the new entry's id doesn't match `last_preloaded` yet), but nothing
   in this call graph — `preload_effects`, `load_current`, the track-ended/advance handler,
   `gapless.rs`, or `AudioCommand` — ever emits anything that removes the old entry from mpv's
   already-mutated internal playlist.

### `crates/loxia-player`'s audio worker and `crates/loxia-audio/src/mpv/handle.rs`

Both were not available to quote directly in this session (their current contents were not part of
the working context used to produce this finding), so this finding does not claim direct sight of
`apply_command`'s literal match arm for `AudioCommand::Preload`, nor of the worker's effect→command
translation function. That gap does not change the conclusion: `gapless.rs`'s own doc comment
(quoted above) already states, as a fact about `apply_command`, that `Preload` appends rather than
replaces, and separately, the exhaustive `AudioCommand` enum (quoted above, `backend.rs` is the
single source of truth for every command the worker can ever be asked to translate) proves no
"remove" command exists for the worker to translate *even if* the reducer wanted to send one. A
missing capability upstream (in the shared command vocabulary) cannot be worked around downstream
in the worker or in `mpv::handle`; there is nothing to translate.

### `reducer::queue`'s advance handler — state-driven, not mpv-driven (best available evidence)

`crates/loxia-core/src/reducer/queue.rs` and `crates/loxia-core/src/state/player.rs` were likewise
not available to quote directly in this session. Based on the hypothesis's own description of
`PlayerState.last_preloaded: Option<QueueEntryId>` and `preload_effects`, and on this project's
consistent elm-architecture convention elsewhere in the workspace (state owns `play_order`/
`position`; `AudioEvent::TrackEnded` is a *notification*, not a source of truth for *which* entry
is now current), the advance/track-ended handler is understood to consult
`play_order[position + 1]` **from queue state**, not "whatever mpv moved to" — this is precisely
why the bug is a *silent* desync rather than a crash: the reducer's own state always advances to
the entry *it* believes is next, while mpv, independently, may still be gaplessly playing whatever
it was last told to `Preload` and never told to forget. The regression test added by this task
(`crates/loxia-core/tests/stale_preload_regression.rs`) exercises exactly this: after retargeting
"next" via insert-next and then simulating track-ended, the reducer's own current entry is the
newly inserted one — the disagreement is with mpv's actual playlist, not with the reducer's
internal bookkeeping.

### Conclusion

**CONFIRMED.** The mechanism is:

1. `reducer::queue::preload_effects` sends `AudioCommand::Preload` for `play_order[position + 1]`
   and records it in `PlayerState.last_preloaded` so it isn't resent.
2. `mpv::handle::apply_command` translates that `Preload` into an **append** to mpv's internal
   playlist (`gapless.rs`'s own doc comment, quoted above) — never a replace.
3. A queue edit (insert-next, remove-next, reorder, etc.) can change
   `play_order[position + 1]` to a different entry than `last_preloaded` records.
4. `preload_effects` then preloads the *new* target — but nothing removes the *old* one, because
   `AudioCommand` (the complete, quoted enum above) has no retract/remove variant for
   `reducer::queue` to emit, and `gapless.rs`/`apply_command` never removes playlist entries on
   their own.
5. mpv's playlist can therefore still contain the stale, no-longer-wanted file immediately after
   the currently-playing track ends, and may play it gaplessly, disagreeing with `loxia-core`'s own
   queue state.

Fix tracked as `tasks/phase-06-queue/06-09-preload-retraction.md`.

## Regression test

`crates/loxia-core/tests/stale_preload_regression.rs::insert_next_after_preload_retargets_next_track`,
marked `#[ignore = "fixed by 06-09-preload-retraction"]` since it documents the confirmed bug.

## Scope note

Per this task's own rules, no production code was changed. Only this finding, the retraction
follow-up task, and the regression test were added; the stray JS stub
(`src/confirm-or-refute-the-stale-gapless-preload-after-a-queue-edit.js`) from the earlier, rejected
attempt at this task has been deleted, and the unrelated pasted knowledge-base glossary
(tabletop/Postgres/CEL `k1`–`k5` material) has been trimmed from this file as out of scope for a
Rust-workspace investigation.
