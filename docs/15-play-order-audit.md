# 15 — `play_order` consumer audit

Companion to `docs/02-data-model.md`'s queue model and `docs/12-decisions.md`. Produced for the
audit task "every current and next consumer reads through `play_order`" (see
`tasks/phase-06-queue/06-09-tui-queue-play-order.md` and
`tasks/phase-06-queue/06-10-mpris-play-order.md` for the two follow-ups this audit opened, both
registered in `tasks/README.md` under Phase 06 Progress).

## Why this matters

`QueueState` stores playback order as a level of indirection: `entries: Vec<QueueEntry>` holds the
tracks in the order they were added/loaded, and `play_order: Vec<usize>` is a permutation of
`0..entries.len()` giving the order they actually play in. The played sequence is
`entries[play_order[i]]`, not `entries[i]`. When shuffle is off, `play_order` is the identity
permutation and the two happen to coincide, which is exactly what lets a direct-indexing bug hide
until someone turns shuffle on.

Any consumer that reads `entries[position]` (or iterates `entries` in insertion order) instead of
walking `play_order` first will show, report, or expose the wrong track under shuffle.

Because `play_order` is always a permutation of the full index range, `play_order.len() ==
entries.len()` holds at all times. That equality matters below: a bounds check written against
`entries.len()` is not automatically wrong just because it names `entries` — it is only wrong if
it also needs the *identity* of the neighbouring position, not just whether one exists.

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
| 13 | `crates/loxia-player/src/workers/mpris.rs` | `CanGoNext` / `CanGoPrevious` | `cursor + 1 < entries.len()` / `cursor > 0` — since `play_order.len() == entries.len()` always (row 1's own invariant) and `cursor` indexes into `play_order`, this is arithmetically identical to `play_order.get(cursor + 1).is_some()` / `cursor > 0`. No mechanism in `workers/mpris.rs` gives `cursor` a different meaning or bypasses `play_order`'s length. | **OK** — reclassified; the original **BUG** verdict for this row was a false positive (see review discussion) |

Consumer #1 from the task (`PlaybackReport`) is now traced and confirmed correct (rows 6–7).
Consumer #2 (the TUI queue view and `widgets::player_bar`) is confirmed **broken** (rows 8–10).
Consumer #3 (MPRIS) is confirmed **broken** for current-track metadata and track-list ordering
(rows 11–12), but **not** for `CanGoNext`/`CanGoPrevious` (row 13), which are already correct by
construction given the permutation invariant.

Follow-ups are tracked as:

- `tasks/phase-06-queue/06-09-tui-queue-play-order.md` — rows 8–10.
- `tasks/phase-06-queue/06-10-mpris-play-order.md` — rows 11–12 only; row 13 needs no fix.

Both are registered, unticked, in `tasks/README.md` under Phase 06 Progress.
