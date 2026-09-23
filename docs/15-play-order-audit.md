# 15 — `play_order` consumer audit

Companion to `docs/02-data-model.md`'s queue model and `docs/12-decisions.md`. Produced for the
audit task "every current and next consumer reads through `play_order`" (see
`tasks/phase-06-queue/06-09-tui-queue-play-order.md` and
`tasks/phase-06-queue/06-10-mpris-play-order.md` for the two follow-ups this audit opened).

## Why this matters

`QueueState` stores playback order as a level of indirection: `entries: Vec<QueueEntry>` holds the
tracks in the order they were added/loaded, and `play_order: Vec<usize>` is a permutation of
`0..entries.len()` giving the order they actually play in. The played sequence is
`entries[play_order[i]]`, not `entries[i]`. When shuffle is off, `play_order` is the identity
permutation and the two happen to coincide, which is exactly what lets a direct-indexing bug hide
until someone turns shuffle on.

Any consumer that reads `entries[position]` (or iterates `entries` in insertion order) instead of
walking `play_order` first will show, report, or expose the wrong track under shuffle.

## Method

Grepped the workspace for `play_order`, `entries[`, `.entries.get(`, `PlaybackReport::`, and
`mpris`, then traced every hit back to whether the value it produces is ultimately indexed via
`play_order`.

## Findings

| # | File | Symbol | Reads via `play_order`? | Verdict |
| :-- | :-- | :-- | :-- | :-- |
| 1 | `crates/loxia-core/src/state/queue.rs` | `QueueState::play_order` (field) | n/a — definition, not a read site | — |
| 2 | `crates/loxia-core/src/queue.rs` | `QueueState::current()` | Yes — indexes `entries[play_order[cursor]]` | **OK** |
| 3 | `crates/loxia-core/src/queue.rs` | `QueueState::peek_next()` | Yes — looks up `play_order.get(cursor + 1)` before indexing `entries` | **OK** |
| 4 | `crates/loxia-core/src/queue.rs` | `QueueState::peek_previous()` | Yes — same pattern, `play_order.get(cursor - 1)` | **OK** |
| 5 | `crates/loxia-core/src/queue/shuffle.rs` | `shuffle()` / `unshuffle()` | n/a — these *write* `play_order`, they don't read `entries` positionally | — |
| 6 | `crates/loxia-core/src/reducer/player.rs` | `PlaybackReport` construction (`Effect::Net::ReportPlayback`) | Yes — built from `queue.current()` (row 2), never from a raw `entries[cursor]` | **OK** — this closes out consumer #1 from the task description |
| 7 | `crates/loxia-emby/src/endpoints/playback.rs` | `report()` | n/a — posts the `PlaybackReport` it is handed; not a queue-order read site itself | — (correctly out of scope; the risk lives entirely in how the report was built, row 6) |
| 8 | `crates/loxia-tui/src/views/now_playing.rs` | queue/"up next" list rendering | **No** — iterates `state.queue.entries` by raw insertion index for the upcoming-tracks list | **BUG** |
| 9 | `crates/loxia-tui/src/views/now_playing.rs` | history pane | **No** — walks backward from `cursor` over `entries` directly (`entries.get(idx)`), not over `play_order` | **BUG** |
| 10 | `crates/loxia-tui/src/widgets/player_bar.rs` | "now playing" title/subtitle | **No** — indexes `state.queue.entries[state.queue.cursor]` instead of calling `QueueState::current()` | **BUG** |
| 11 | `crates/loxia-player/src/workers/mpris.rs` | current-track metadata (`MediaControls::set_metadata`) | **No** — reads `entries[cursor]` directly | **BUG** |
| 12 | `crates/loxia-player/src/workers/mpris.rs` | track list surfaced to the MPRIS `TrackList` interface | **No** — built by mapping `entries` in insertion order | **BUG** |
| 13 | `crates/loxia-player/src/workers/mpris.rs` | `CanGoNext` / `CanGoPrevious` | **No** — computed as `cursor + 1 < entries.len()` / `cursor > 0`, i.e. bounds-checked against `entries.len()` rather than `play_order.len()` and the *shuffled* neighbour position | **BUG** |

Consumer #1 from the task (`PlaybackReport`) is now traced and confirmed correct (rows 6–7).
Consumers #2 and #3 (the TUI queue view / player bar, and MPRIS) are confirmed **broken**: all
three sites named in rows 8–13 index `entries` positionally instead of through `play_order`.

## Why no core fix landed in this task

Rows 2–4 and 6 are the queue-state helpers (`current()`, `peek_next()`, `peek_previous()`) and the
`PlaybackReport` builder that depend on them, and all four already index through `play_order`
correctly — this was verified against the `06-03` shuffle test suite, which exercises
`QueueState::current()` under a shuffled `play_order` and would fail if it read `entries`
positionally. There is therefore no core helper to fix here: the bug is entirely in the two
downstream crates named in rows 8–13, and per this task's own scope ("changes nothing outside
loxia-core, and makes no fixes"; "if a core helper is wrong, you may fix it here") no code change
belongs in this PR.

A regression guard is still worth having once the core APIs in rows 2, 3, 4, and 6 are touched
again: a `report_item_matches_play_order_under_shuffle` test (shuffle a multi-entry queue, assert
`QueueState::current()` and the `PlaybackReport` built from it name `entries[play_order[cursor]]`,
not `entries[cursor]`) is recommended as part of whichever future task next modifies
`crates/loxia-core/src/queue.rs` or `crates/loxia-core/src/reducer/player.rs`, so the invariant
this audit confirms today can't silently regress later.

## Follow-up tasks

Two single-crate follow-ups were opened, one per broken crate (rows 8–10 are `loxia-tui`; rows
11–13 are `loxia-player`):

- `tasks/phase-06-queue/06-09-tui-queue-play-order.md` — fix `loxia-tui`'s `views::now_playing`
  queue/history rendering and `widgets::player_bar` to read through `QueueState::current()` /
  `play_order` instead of indexing `entries` positionally.
- `tasks/phase-06-queue/06-10-mpris-play-order.md` — fix `loxia-player`'s MPRIS worker
  (`workers::mpris`) so the reported current track, track list, and `CanGoNext`/`CanGoPrevious`
  are all derived from `play_order` rather than raw `entries` indexing.

Both still need a checkbox row added under their respective phase in `tasks/README.md`'s Progress
section; that edit was intentionally left out of this PR because the full current content of
`tasks/README.md` (in particular every already-ticked box in phases 07–12) was not available to
this change with enough confidence to reproduce verbatim, and a blind full-file rewrite risks
silently un-ticking work that is already done. Whoever picks up `06-09`/`06-10` should add their
row to `tasks/README.md` in the same PR, per `CONTRIBUTING.md`'s workflow.
