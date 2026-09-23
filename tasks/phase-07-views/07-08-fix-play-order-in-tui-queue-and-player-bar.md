# 07-08 · fix play order in the TUI queue view and player bar

**Phase:** 07 — Remaining views
**Agent:** loxia-tui
**Size:** S
**Prerequisites:** `07-06` now playing view, `04-09` player bar, `06-03` shuffle
**Reference:** `docs/15-play-order-audit.md` rows 7–8

## Goal

`crates/loxia-tui/src/views/now_playing.rs`'s "up next" queue list and
`crates/loxia-tui/src/widgets/player_bar.rs`'s now-playing title/artist/art lookup both index
`QueueState::entries` directly instead of resolving through `QueueState::play_order`. When shuffle
is off, `play_order` is the identity permutation, so this has been invisible; the moment shuffle is
on, both widgets show the wrong track. When this task is done, both consumers resolve every
"current", "next", and "up next" track through `play_order` — directly, or via
`QueueState::current()` / `peek_next()` / `peek_previous()` — and a shuffled-queue test proves it.

## Files

- `crates/loxia-tui/src/views/now_playing.rs`
- `crates/loxia-tui/src/widgets/player_bar.rs`

## Specification

- `now_playing.rs`'s "up next" list body must walk `queue.play_order` starting at (or after)
  `queue.position`, resolving each visited `play_order` entry into `queue.entries` to get the item
  to render, rather than iterating `queue.entries` in storage order or indexing it directly by
  list row.
- `player_bar.rs`'s now-playing title, artist, and album-art lookup must resolve the current track
  via `queue.current()` (or the equivalent `entries[play_order[position]]` resolution), never
  `entries[position]`.
- Neither file may derive an `entries` index from `position` without first resolving it through
  `play_order`.

## Acceptance

- `now_playing_snapshot_queue_under_shuffle`: a `QueueState` built with a non-identity `play_order`
  renders the "up next" list in `play_order` order, not `entries` storage order.
- `player_bar_snapshot_playing_under_shuffle`: a `QueueState` built with a non-identity
  `play_order` renders the title/artist of `entries[play_order[position]]`, not
  `entries[position]`.
- Existing snapshot tests for both widgets remain green (identity-permutation queues render
  unchanged).

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
