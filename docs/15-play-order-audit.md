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
| 2 | `crates/loxia-core/src/state/queue.rs` | `QueueState::peek_next()` | Yes — a plain bound-check-and-resolve against `play_order[position + 1]`; it does not skip entries (there is no "unplayable item" concept in the queue model), so it is equivalent to checking `position + 1 < play_order.len()` and then resolving | **OK** |
| 3 | `crates/loxia-core/src/state/queue.rs` | `QueueState::peek_previous()` | Yes — the mirror bound-check-and-resolve against `play_order[position - 1]`, same non-skipping behaviour as `peek_next()` | **OK** |
| 4 | `crates/loxia-core/src/reducer/queue.rs` | the handler that builds `PlaybackReport` before emitting `Effect::Net::ReportPlayback` | Yes — calls `queue.current()` and reads the item off the returned entry; never re-indexes `entries` itself | **OK** — closes the "probably in `crates/loxia-core/src/reducer/`, but nobody has confirmed" open question from the task brief. `PlaybackReport` construction is correct under shuffle. |
| 5 | `crates/loxia-core/src/model/playback.rs` | `PlaybackReport` (struct definition) | N/A — plain data holder, not a reader | **N/A** |
| 6 | `crates/loxia-emby/src/endpoints/playback.rs` | `report()` | N/A — pure HTTP transport; posts whatever `PlaybackReport` it is handed, has no queue access | **N/A** |
| 7 | `crates/loxia-tui/src/views/now_playing.rs` | the "up next" queue list body | **No** — renders the visible window by indexing `queue.entries` directly (`entries[i]` / `entries.iter().enumerate()`) instead of walking `queue.play_order` starting from `position` | **WRONG** |
| 8 | `crates/loxia-tui/src/widgets/player_bar.rs` | now-playing title / artist / art lookup | **No** — indexes `queue.entries[queue.position]` directly; `position` is an index into `play_order`, not into `entries` | **WRONG** |
| 9 | `crates/loxia-player/src/workers/mpris.rs` | current-track `Metadata`, the MPRIS track list, `CanGoNext` / `CanGoPrevious` | Metadata and the track list: **No** — both read/iterate `entries` directly in storage order and never resolve through `play_order`. `CanGoNext`/`CanGoPrevious`: **Yes, effectively** — this worker derives them from `position` against a length, and `play_order` is always a permutation of `entries`' indices, so `play_order.len() == entries.len()` unconditionally; combined with `peek_next()`/`peek_previous()` (rows 2–3) being plain bound-checks rather than entry-skipping walks, "`position + 1 < entries.len()`" and "`position + 1 < play_order.len()`" are the identical check, and likewise for `position > 0`. There is no shuffled-queue input on which the two computations diverge. | **WRONG** (Metadata, track list) — **OK** (`CanGoNext`/`CanGoPrevious`) |

## Verdict summary

- **`loxia-core` helpers are correct.** `QueueState::current()`, `peek_next()`, `peek_previous()`,
  and the `PlaybackReport`-building reducer code all resolve through `play_order`, and neither
  `peek_next()` nor `peek_previous()` skips entries — they are bound-checks against `position`, not
  walks that can diverge from a length-based check. No core fix is required by this task's brief:
  fixing a core helper was optional here, conditioned on "if a core helper is wrong", and none is,
  so no core code changes and no `report_item_matches_play_order_under_shuffle`-style core test are
  added by this task.
- **Two consumers outside core are wrong**, and are registered as separate, single-crate follow-up
  tasks rather than fixed here, per the brief's instruction that this task changes nothing outside
  `loxia-core`:
  - `crates/loxia-tui/src/views/now_playing.rs` (the queue list body, row 7) and
    `crates/loxia-tui/src/widgets/player_bar.rs` (the now-playing title/artist/art lookup, row 8)
    both index `queue.entries` directly instead of walking or resolving through
    `queue.play_order`. Tracked as
    `tasks/phase-07-views/07-08-fix-play-order-in-tui-queue-and-player-bar.md`
    (Phase 07 — Remaining views), which cites rows 7 and 8 above and names concrete
    shuffled-queue snapshot tests.
  - `crates/loxia-player/src/workers/mpris.rs`'s current-track `Metadata` and reported track list
    (row 9) both index `entries` directly rather than resolving through `play_order`; only its
    `CanGoNext`/`CanGoPrevious` computation happens to already be correct under shuffle, for the
    reason given in the table. Tracked as
    `tasks/phase-10-polish/10-14-fix-mpris-play-order.md` (Phase 10 — Polish & integrations),
    which cites row 9 above and names a `report_item_matches_play_order_under_shuffle`-style
    regression test plus a `CanGoNext`/`CanGoPrevious` regression test to lock in that they stay
    correct while the `Metadata`/track-list bug is fixed.

Neither follow-up task is done as part of this audit — each is scoped to the single crate its bug
lives in, so it can be picked up independently once its listed prerequisites are ticked.
