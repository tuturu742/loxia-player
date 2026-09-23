# 15 — `play_order` read-site audit

Scope: every place in the workspace that needs "the track currently/next/previously playing"
must resolve it as `entries[play_order[position]]` (or via a helper that already does so), never
as `entries[position]` directly. `position` is an index into `play_order`, not into `entries` —
indexing `entries` with it directly is only correct when shuffle is off (`play_order` happens to
be the identity permutation), and silently wrong the moment shuffle is on.

Method: grepped the workspace for `play_order`, `entries[`, `.entries.get(`, `PlaybackReport::`,
and `mpris`, then read every hit plus its call sites.

## Table

| # | File | Symbol | Goes through `play_order`? | Verdict |
| :-- | :-- | :-- | :-- | :-- |
| 1 | `crates/loxia-core/src/state/queue.rs` | `QueueState::current()` | Yes — resolves `play_order[position]` to an `entries` index first, then indexes `entries` with *that* index | **OK** |
| 2 | `crates/loxia-core/src/state/queue.rs` | `QueueState::peek_next()` | Yes — walks `play_order[position + 1..]` | **OK** |
| 3 | `crates/loxia-core/src/state/queue.rs` | `QueueState::peek_previous()` | Yes — walks `play_order[..position]` in reverse | **OK** |
| 4 | `crates/loxia-core/src/reducer/queue.rs` | the handler that builds `PlaybackReport` before emitting `Effect::Net::ReportPlayback` | Yes — calls `queue.current()` and reads the item off the returned entry; never re-indexes `entries` itself | **OK** — closes the "probably in `crates/loxia-core/src/reducer/`, but nobody has confirmed" open question from the task brief. `PlaybackReport` construction is correct under shuffle. |
| 5 | `crates/loxia-core/src/model/playback.rs` | `PlaybackReport` (struct definition) | N/A — plain data holder, not a reader | **N/A** |
| 6 | `crates/loxia-emby/src/endpoints/playback.rs` | `report()` | N/A — pure HTTP transport; posts whatever `PlaybackReport` it is handed, has no queue access | **N/A** |
| 7 | `crates/loxia-tui/src/views/now_playing.rs` | the "up next" queue list body | **No** — renders the visible window by indexing `queue.entries` directly (`entries[i]` / `entries.iter().enumerate()`) instead of walking `queue.play_order` starting from `position` | **WRONG** |
| 8 | `crates/loxia-tui/src/widgets/player_bar.rs` | now-playing title / artist / art lookup | **No** — indexes `queue.entries[queue.position]` directly; `position` is an index into `play_order`, not into `entries` | **WRONG** |
| 9 | `crates/loxia-player/src/workers/mpris.rs` | current-track `Metadata`, the MPRIS track list, `CanGoNext` / `CanGoPrevious` | **No** — reads `entries[position]` for "now playing", builds the exposed MPRIS track list from `entries` in storage order (not playback order), and derives `CanGoNext`/`CanGoPrevious` from `position` against `entries.len()` instead of from `play_order` position/length | **WRONG** |

## Verdict summary

- **`loxia-core` helpers are correct.** `QueueState::current()`, `peek_next()`, `peek_previous()`,
  and the `PlaybackReport`-building reducer code all resolve through `play_order`. No core fix is
  needed, so no core diff and no `report_item_matches_play_order_under_shuffle` test is added by
  this task — there is nothing in `loxia-core` for that test to guard against regressing that
  isn't already covered by the existing shuffle/queue tests exercising `current()` /
  `peek_next()` / `peek_previous()`.
- **Three sites outside `loxia-core` are wrong**, all in the two crates named in the task brief:
  - `loxia-tui`: the queue view (`views/now_playing.rs`) and `widgets/player_bar.rs`.
  - `loxia-player`: the MPRIS worker (`workers/mpris.rs`), covering current-track metadata, the
    exposed track list, and `CanGoNext`/`CanGoPrevious`.

This task changes nothing outside `loxia-core` and makes no fixes itself, per its own scope.
Follow-up single-crate fix tasks for the three wrong sites are registered below.

## Follow-up tasks registered

- `tasks/phase-07-views/07-08-fix-play-order-in-tui-queue-and-player-bar.md` (crate: `loxia-tui`)
  — fixes rows 7 and 8 above.
- `tasks/phase-10-polish/10-14-fix-mpris-play-order.md` (crate: `loxia-player`)
  — fixes row 9 above.

`tasks/README.md`'s Progress section should gain the following two unticked rows once this audit
lands (not added here to avoid corrupting the rest of that file's content, which lies outside
this change's visible context):

```
### Phase 07 — Views
- [ ] `07-08` fix play_order reads in the TUI queue view and player bar

### Phase 10 — Polish
- [ ] `10-14` fix play_order reads in the MPRIS worker
```
