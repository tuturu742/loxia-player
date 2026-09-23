# 15 — Play-order consumer audit

## Why

`QueueState`'s play sequence is `entries[play_order[i]]`, not `entries[i]`. Any consumer that
indexes `entries` directly, or iterates it in storage order, shows the wrong "current" or "next"
track the moment shuffle is on. This document is the full audit: every site in the workspace that
could plausibly read "the current track" or "the track list", checked against that rule.

This is a **read-only audit**. No production code changed anywhere in the workspace as a result of
it, because every site was found to already go through `play_order` — with one exception in
`loxia-player`, which is registered as a follow-up task (`tasks/phase-10-polish/
10-14-mpris-play-order-audit.md`) rather than fixed here, since fixing it would cross the
`loxia-player` crate boundary and this work item is scoped to `loxia-core`.

## Method

Four greps were run over the whole workspace (`rg -n` from the repo root):

```
rg -n 'play_order'
rg -n 'entries\['
rg -n '\.entries\.get\('
rg -n 'PlaybackReport::'
rg -n -i 'mpris'
```

Notes on the greps themselves:

- `PlaybackReport::` (the associated-function/path syntax) has **no hits at all** outside
  `loxia-core`. `PlaybackReport` is always built as a plain struct literal
  (`PlaybackReport { .. }`), never via an associated function, so this grep alone would have
  under-reported the construction site. The construction site (`reducer::player::
  build_playback_report`) is listed below under its own row, found instead via `rg -n
  'PlaybackReport \{'` and by reading `crates/loxia-core/src/reducer/player.rs` directly.
- `.entries.get(` has exactly one family of hits: `QueueState::current` / `peek_next` /
  `peek_previous` in `crates/loxia-core/src/state/queue.rs`. No hits anywhere outside that file.
- `entries[` has hits in `loxia-core` (the same three helpers, using `[]` in an earlier line
  before the index is checked, plus test assertions) and in `loxia-tui`'s queue-view renderer
  (see row 8), always with an index that itself came from `play_order`. No unguarded
  `entries[position]` or `entries[i]` (raw loop counter) production hit was found anywhere.
- `mpris` resolves to exactly one production file: `crates/loxia-player/src/workers/mpris.rs`,
  plus its two wiring mentions in `crates/loxia-player/src/workers/mod.rs` (`pub mod mpris;`) and
  `crates/loxia-player/src/runtime.rs` (spawns the worker). No MPRIS code exists anywhere else in
  the workspace — the "background worker" the repo overview refers to is this file.

## Per-site table

### `loxia-core` — the four consumers/producers audited by this task

| # | File : Symbol (line) | Grep hit(s) | Reads via `play_order`? | Verdict |
|---|---|---|---|---|
| 1 | `state/queue.rs` : `QueueState::current()` (L54) | `play_order`, `.entries.get(` | Yes — `self.entries.get(*self.play_order.get(self.position)?)?` | **Correct.** This is the canonical helper; every other core site goes through it. |
| 2 | `state/queue.rs` : `QueueState::peek_next()` (L63) | `play_order`, `.entries.get(` | Yes — same pattern at `position + 1` | **Correct.** |
| 3 | `state/queue.rs` : `QueueState::peek_previous()` (L72) | `play_order`, `.entries.get(` | Yes — same pattern at `position - 1` | **Correct.** |
| 4 | `reducer/queue.rs` : `advance_to_next(state: &mut AppState)` (L118) and `handle_track_ended(state: &mut AppState, natural: bool)` (L142) | `play_order` (via calls, not the literal field) | Yes — both call `queue.peek_next()` to decide the next track and `queue.current()` to re-derive the now-playing item after advancing; neither indexes `entries` or `play_order` directly | **Correct.** |
| 5 | `reducer/player.rs` : `build_playback_report(state: &AppState) -> Option<PlaybackReport>` (L86) | `PlaybackReport {` (struct literal, not `PlaybackReport::`) | Yes — the `Track` it reads fields from comes from `state.queue.current()`, never from `state.queue.entries[state.queue.position]` | **Correct.** This answers the work item's open question directly: `PlaybackReport` construction lives in `reducer/player.rs`, and it is shuffle-safe. |

### `loxia-core` — producer and type-definition sites (not consumers, listed for completeness per the grep)

| # | File : Symbol (line) | Grep hit(s) | Note |
|---|---|---|---|
| 6 | `queue/shuffle.rs` : `reshuffle(queue: &mut QueueState)` (L22) | `play_order` | Writes `play_order`, doesn't read `entries` by index at all. Not a consumer; included because it's a `play_order` grep hit. |
| 7 | `model/playback.rs` : `struct PlaybackReport` (L9) | — | Type definition only, matched by neither `PlaybackReport::` nor `PlaybackReport {`. Not a read site. |

### `loxia-core` — test-code hits (per review item 6, listed explicitly rather than omitted)

| # | File : Symbol (line) | Grep hit(s) | Verdict |
|---|---|---|---|
| 8 | `state/queue.rs` tests : `current_follows_play_order_when_shuffled()` (L211) | `entries[` | **N/A — not a consumer.** Uses `entries[expected_idx]` only to build the *expected* value the assertion compares `current()`'s output against. This is the test that already covers the invariant the work item asks for (see "On `report_item_matches_play_order_under_shuffle`" below). |
| 9 | `reducer/queue.rs` tests : `advance_wraps_play_order_not_storage_order()` (L340) | `entries[`, `play_order` | **N/A — not a consumer.** Same pattern: builds a shuffled `play_order`, asserts `handle_track_ended` advances to `entries[play_order[next_position]]`. |
| 10 | `reducer/player.rs` tests : `playback_report_tracks_shuffled_play_order()` (L204) | `PlaybackReport {` | **N/A — not a consumer, but is the regression coverage.** Builds a queue with a non-identity `play_order`, calls `build_playback_report`, asserts `report.item_id` matches `entries[play_order[position]].id`, not `entries[position].id`. |

#### On `report_item_matches_play_order_under_shuffle`

The work item invites adding a test with (approximately) this name if a core helper turns out to
be wrong. None was — row 5 is correct, and row 10 above (`playback_report_tracks_shuffled_play_order`)
already asserts exactly that invariant. No new test was added: `loxia-core` had no bug to fix this
round, and duplicating an existing green test under a new name would add nothing. If a future
change to `build_playback_report` regresses this, that existing test fails.

### `loxia-tui` — queue view and player bar (round-1 "Unconfirmed", resolved here)

| # | File : Symbol (line) | Grep hit(s) | Reads via `play_order`? | Verdict |
|---|---|---|---|---|
| 8t | `views/now_playing.rs` : `queue_rows(queue: &QueueState) -> Vec<QueueRow>` (L212) | `play_order`, `entries[` | Yes — `queue.play_order.iter().enumerate().map(\|(rank, &idx)\| queue_row(&queue.entries[idx], rank == queue.position))`. The index passed into `entries[..]` is `idx`, sourced from iterating `play_order`, not the raw enumeration counter `rank` and not `queue.position` directly. | **Correct.** The `entries[` hit here is the safe pattern (index derived from `play_order`), not the bug pattern (index derived from storage position). |
| 9t | `widgets/player_bar.rs` : `render(frame: &mut Frame, area: Rect, state: &PlayerBarState)` (L48) | none (no `entries[`, `.entries.get(`, or `play_order` token in this file) | N/A — doesn't touch `QueueState` at all | **Correct, trivially.** `player_bar` never sees `QueueState`; it renders `PlayerBarState`, which the reducer populates from `queue.current()` (row 1) before the view layer ever runs. There is no `entries`-indexing path for this file to get wrong. |

Because both `loxia-tui` sites are correct, `tasks/phase-07-views/07-08-queue-view-play-order-audit.md`
— which only handed the question off rather than answering it — has been **deleted**, not
rewritten. There is nothing to fix in `loxia-tui`.

### `loxia-player` — the located MPRIS worker (round-1 "code not located", resolved here)

The MPRIS background worker is `crates/loxia-player/src/workers/mpris.rs`, wired in from
`crates/loxia-player/src/workers/mod.rs` (`pub mod mpris;`) and spawned by
`crates/loxia-player/src/runtime.rs`. Four separate verdicts, as requested:

| Aspect | Symbol (line) | Reads via `play_order`? | Verdict |
|---|---|---|---|
| Current track (MPRIS `Metadata`/`PlaybackStatus`) | `update_metadata(queue: &QueueState) -> Metadata` (L74) | Yes — reads `queue.current()` | **Correct.** |
| Track list (MPRIS `Tracks`/`TrackList` interface) | `build_track_list(queue: &QueueState) -> Vec<TrackId>` (L121) | **No** — `for track in &queue.entries { .. }`, i.e. iterates storage order directly, never touching `queue.play_order` | **Wrong.** With shuffle on, the exposed track list (and therefore "jump to track" from an OS media widget / lock screen) is in library order, not play order. |
| `CanGoNext` | `can_go_next(queue: &QueueState) -> bool` (L138) | Yes — `queue.position + 1 < queue.play_order.len()` | **Correct.** Compares against `play_order`'s length, not `entries`'. |
| `CanGoPrevious` | `can_go_previous(queue: &QueueState) -> bool` (L142) | Yes — `queue.position > 0` | **Correct.** Doesn't index `entries` at all; `position` is already an index into `play_order`. |

This is a genuine, single-crate (`loxia-player`) bug. Per the work item's own instruction ("If a
site outside core is wrong, register a separate single-crate task for it in `tasks/README.md`"),
it is **not** fixed here — that would cross the `loxia-core` boundary this task is scoped to. It is
registered as `tasks/phase-10-polish/10-14-mpris-play-order-audit.md` (a fix task, citing this
exact function and line) and listed in `tasks/README.md` under "Follow-up / audit tasks".

## Summary

| Consumer | Verdict |
|---|---|
| `PlaybackReport` construction (`reducer/player.rs::build_playback_report`) | Correct |
| `loxia-tui` queue view (`views/now_playing.rs::queue_rows`) | Correct |
| `loxia-tui` player bar (`widgets/player_bar.rs::render`) | Correct (doesn't touch `QueueState`) |
| MPRIS current track / `CanGoNext` / `CanGoPrevious` | Correct |
| MPRIS track list (`workers/mpris.rs::build_track_list`) | **Wrong — follow-up task registered** |

No `loxia-core` fix was needed. One follow-up task was registered for `loxia-player`.
