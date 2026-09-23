# Play-order consumer audit

Tracks the audit required to answer one question for every place in the workspace that reads
"what track is logically at position N in the queue": does it resolve that through
`entries[play_order[i]]`, or does it index `entries[i]` directly and therefore show the wrong
track whenever shuffle is on?

This document is the artefact required by that audit task. It changes nothing outside
`loxia-core`, and it does not itself contain a code fix — see "Follow-ups registered" below for
the two consumers whose correctness could not be confirmed from within a core-scoped task and
which therefore got their own single-crate follow-up tasks.

## Method

Grepped the workspace for:

- `play_order`
- `entries[`
- `.entries.get(`
- `PlaybackReport::`
- `mpris`
- queue-state helper method names (`fn current`, `fn peek_next`, `fn peek_previous`, `fn peek_next_n`)

For each hit, the table below records the file, the symbol, whether the read resolves through
`play_order`, and a verdict.

## Findings

| # | File | Symbol | Goes through `play_order`? | Verdict |
|---|------|--------|------------------------------|---------|
| 1 | `crates/loxia-core/src/state/queue.rs` | `QueueState::current()` | Yes — resolves the logical position through `play_order` before indexing `entries` | OK — core |
| 2 | `crates/loxia-core/src/state/queue.rs` | `QueueState::peek_next()` | Yes — same resolution pattern, offset by one logical position | OK — core |
| 3 | `crates/loxia-core/src/state/queue.rs` | `QueueState::peek_previous()` | Yes — same resolution pattern, offset by minus one | OK — core |
| 4 | `crates/loxia-core/src/reducer/queue.rs` | track-advance / track-ended handling | Delegates to `QueueState::current()` / the advance helper rather than indexing `entries` itself | OK — core |
| 5 | `crates/loxia-core/src/reducer/player.rs` | the site that builds `PlaybackReport` for `Effect::Net::ReportPlayback` | **Newly traced by this audit.** Reads the item to report via `QueueState::current()`, not a raw `entries[position]` index | OK — core (previously unconfirmed, now confirmed) |
| 6 | `crates/loxia-core/src/model/playback.rs` | `PlaybackReport` | Plain data type; carries whatever item it is constructed with, does no queue lookup of its own | N/A |
| 7 | `crates/loxia-emby/src/endpoints/playback.rs` | `report` | Posts the `PlaybackReport` it is given verbatim; performs no queue indexing at all | N/A — no queue access, nothing to get wrong here |
| 8 | `crates/loxia-tui/src/views/now_playing.rs` | queue list rendering | **Unconfirmed.** Not verified whether the visible queue rows are built by walking `play_order` (correct) or by iterating `entries` in storage order (wrong under shuffle) | Follow-up task registered (`07-08`) |
| 9 | `crates/loxia-tui/src/widgets/player_bar.rs` | now-playing track render | **Unconfirmed.** Not verified whether the "now playing" line reads `QueueState::current()` or a raw entries index | Follow-up task registered (`07-08`) |
| 10 | `crates/loxia-player/src/workers/mpris.rs` | current track / track list / `CanGoNext` / `CanGoPrevious` | **Unconfirmed and unlocated with certainty.** The MPRIS worker's exact reporting logic was not confirmed to resolve current/next/previous through `play_order` | Follow-up task registered (`10-14`) |

## Verdict on the core helpers

`QueueState::current()`, `peek_next()`, and `peek_previous()` in
`crates/loxia-core/src/state/queue.rs` already resolve every logical position through
`play_order` before touching `entries`, and `reducer::player`'s construction of `PlaybackReport`
for `Effect::Net::ReportPlayback` calls `QueueState::current()` rather than indexing `entries`
directly — this was the specific gap this task named as unconfirmed, and it is now traced and
confirmed correct.

**No core fix is made by this task.** Per the task's own instructions, a core fix (and an
accompanying `report_item_matches_play_order_under_shuffle`-style test) is only warranted if a
core helper is found to be wrong. None was.

## Follow-ups registered

Two consumers named directly in this task's brief could not be confirmed correct or incorrect
without crate-local investigation beyond this core-scoped task's remit, per the task's own
instruction to register a separate single-crate task for any such site rather than fix it here:

- **`tasks/phase-07-views/07-08-queue-view-play-order-audit.md`** (`loxia-tui`) — audit and, if
  needed, fix `views/now_playing.rs`'s queue list and `widgets::player_bar` so both read the
  current/next/previous track through `QueueState::current()` / `peek_next()` /
  `peek_previous()`, never a raw `entries[position]` index.
- **`tasks/phase-10-polish/10-14-mpris-play-order-audit.md`** (`loxia-player`) — locate the MPRIS
  background worker, and audit and, if needed, fix what it reports as the current track, the
  track list, and `CanGoNext` / `CanGoPrevious` so all four resolve through `play_order`.
