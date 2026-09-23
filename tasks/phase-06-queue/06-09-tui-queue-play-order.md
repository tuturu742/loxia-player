# 06-09 · TUI queue view and player bar must read through `play_order`

**Phase:** 06 — Queue engine · **Agent:** loxia-tui · **Size:** S
**Prerequisites:** `06-03` (shuffle), `04-09` (player bar), `07-06` (now playing view)
**Reference:** `docs/15-play-order-audit.md` (rows 8–10), `docs/02-data-model.md` §queue model

## Goal

`loxia-tui`'s Now Playing queue/history rendering and the player bar currently index
`QueueState::entries` by raw position instead of walking `play_order`, so with shuffle enabled they
show the wrong current track, the wrong "up next" list, and the wrong history. When finished, both
read exclusively through the same queue-state accessors the reducer already uses correctly
(`QueueState::current()`, and `play_order`-driven iteration for the surrounding entries), so what
the UI shows always matches what plays next.

## Files

- `crates/loxia-tui/src/views/now_playing.rs`
- `crates/loxia-tui/src/widgets/player_bar.rs`

## Specification

- `widgets::player_bar` must source the currently-playing entry via `QueueState::current()` (or an
  equivalent helper that itself indexes `entries[play_order[cursor]]`), never
  `entries[cursor]`/`entries.get(cursor)` directly.
- `views::now_playing`'s "up next" list must be built by walking `play_order` forward from the
  cursor (`play_order[cursor + 1..]`, indexing into `entries` for each), not by iterating `entries`
  in insertion order.
- `views::now_playing`'s history pane must walk `play_order` backward from the cursor
  (`play_order[..cursor]`, reversed), not `entries` directly.
- Both must continue to behave identically to today when shuffle is off, since `play_order` is the
  identity permutation in that case — no snapshot change is expected for the existing non-shuffled
  fixtures.

## Acceptance

- `player_bar_snapshot_playing_with_shuffle` (new): a fixture queue with a non-identity
  `play_order` renders the shuffled current track's title in the player bar, not
  `entries[cursor]`'s title.
- `now_playing_snapshot_queue_with_shuffle` (new): the same fixture's "up next" list matches the
  shuffled order, not insertion order.
- `now_playing_snapshot_history_with_shuffle` (new): the same fixture's history pane matches the
  shuffled order.
- All four existing `player_bar_snapshot_*` and `now_playing_snapshot_*` tests remain green
  unmodified.

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
