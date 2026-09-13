# 06-06 · Gapless preloading

**Phase:** 06 — Queue engine · **Agent:** A/C · **Size:** M
**Prerequisites:** `06-03`, `06-04`
**Reference:** `docs/05-audio-engine.md` §7

## Goal
Preload the next queue entry so tracks run together with no gap. Gapless is the only transition mode
— crossfade was cut (`docs/12-decisions.md` §3).

## Files
- `crates/loxia-core/src/reducer/queue.rs` (extend)
- `crates/loxia-audio/src/gapless.rs`

## Specification

**Mechanism.** mpv is configured with `gapless-audio=yes` and `prefetch-playlist=yes` (task
`05-03`). Preloading appends the next entry's URL to mpv's playlist; mpv opens and buffers it before
the current file ends. Advancement is observed through `playlist-pos`, which the pump translates to
`TrackEnded { natural: true }`.

```
pub fn preload_target(q: &QueueState) -> Option<&QueueEntry>;   // the entry at position + 1
```
Honours repeat mode: with `Repeat::One` the target is the current entry; with `Repeat::All` at the
end it wraps to index 0; with `Repeat::Off` at the end it is `None`.

**The critical requirement: emit `Effect::Audio(Preload)` after every mutation that can change the
entry at `position + 1`.** Forgetting one is how gapless silently stops working in a way nobody
notices until a user reports "it only gaps sometimes". The complete list, each with its own test:

| Mutation | Why the target can change |
| :-- | :-- |
| `QueueSelection` (append) | appending to a 1-entry queue creates a next entry |
| `InsertNext` | directly replaces the next entry |
| `RemoveEntry` | removing the next entry promotes the one after |
| `MoveEntry` | reordering can move a different entry into the slot |
| `ToggleShuffle` | permutes everything after the current entry |
| `ApplySortProfile` | rebuilds the order entirely |
| `CycleRepeat` | changes the target at the queue's end |
| `Next` / `Prev` / `JumpTo` | the position moved |
| auto-advance | the position moved |

**Deduplication.** The reducer records the last preloaded `QueueEntryId` in `PlayerState`. If the
recomputed target is unchanged, emit nothing — re-appending the same file on every queue touch would
thrash mpv's cache.

**Availability.** Do not preload an entry that is `Unavailable`, or a remote entry while offline.

**Bit-perfect.** Preloading stays enabled; mpv falls back to a brief gap when consecutive tracks
have different sample rates and the device is locked to one. That is inherent to exclusive-mode
output, not a bug to work around.

## Acceptance
- `preload_target_respects_repeat_mode` — table test over Off/All/One, mid-queue and at the end.
- One test per row of the mutation table above, named
  `preload_emitted_after_<mutation>`.
- `preload_deduplicated_when_target_unchanged`
- `no_preload_for_unavailable_entry`
- `no_preload_when_offline_and_remote`
- `preload_target_none_at_end_with_repeat_off`
- Behind `mpv-tests`: `gapless_gap_under_20ms` — play two generated FLACs of identical format
  back to back through `null` output and measure the inter-track gap from the position events.
- Manual, pasted into the PR: play a gapless album (a live recording or a DJ mix) and confirm no
  audible gap.

## Done when
The global DoD in `tasks/README.md` is satisfied.
