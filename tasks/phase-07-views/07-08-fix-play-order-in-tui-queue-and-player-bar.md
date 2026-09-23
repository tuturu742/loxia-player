# 07-08 · Fix play-order reads in the TUI queue view and player bar

**Phase:** 07 — Views
**Agent:** single-crate (`loxia-tui`)
**Size:** S
**Prerequisites:** `07-06` (now-playing view), `04-09` (player bar), `06-03` (shuffle)
**Reference:** `docs/15-play-order-audit.md` rows 7–8

## Goal

`loxia-tui/src/views/now_playing.rs`'s "up next" queue list and
`loxia-tui/src/widgets/player_bar.rs`'s now-playing title/artist/art lookup both index
`queue.entries` directly instead of resolving through `queue.play_order`. With shuffle on, both
show the wrong track. When this task is done, both read sites resolve the current/next entries the
same way `QueueState::current()`/`peek_next()`/`peek_previous()` already do, and a shuffled-queue
test proves it.

## Files

- `crates/loxia-tui/src/views/now_playing.rs`
- `crates/loxia-tui/src/widgets/player_bar.rs`

## Specification

- The "up next" list body must walk `queue.play_order` starting at `queue.position + 1`
  (mirroring `QueueState::peek_next()`'s own traversal), resolving each `play_order[i]` to
  `queue.entries[play_order[i]]` — never iterate or index `queue.entries` directly by list
  position.
- `player_bar`'s now-playing title, artist, and album-art lookups must go through
  `queue.current()` (or an equivalent `play_order`-resolved lookup), never
  `queue.entries[queue.position]`.
- No behaviour change when shuffle is off (`play_order` is the identity permutation there), so
  every existing snapshot for both files must still pass unmodified.

## Acceptance

- `now_playing_snapshot_queue_under_shuffle` — a `QueueState` fixture with shuffle on (a
  non-identity `play_order`) renders the "up next" list in play order, not storage order.
- `player_bar_snapshot_playing_under_shuffle` — the same shuffled fixture shows the title/artist
  of `entries[play_order[position]]`, not `entries[position]`.
- Every pre-existing snapshot test in both files still passes.

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
