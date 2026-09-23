# 07-08 · Fix `play_order` reads in the TUI queue view and player bar

**Phase / Agent / Size / Prerequisites / Reference**
Phase 07 — Views · single-crate (`loxia-tui`) · S · prerequisites: `07-06`, `06-03` · Reference:
`docs/15-play-order-audit.md` row 7 and row 8.

## Goal

`crates/loxia-tui/src/views/now_playing.rs`'s queue list and
`crates/loxia-tui/src/widgets/player_bar.rs`'s now-playing lookup both index
`QueueState::entries` directly instead of walking `QueueState::play_order`. Under a shuffled
queue this renders the wrong track as "now playing" and the wrong upcoming order in the queue
list. When this task is done, both read through `play_order` (ideally via `QueueState::current()`
/ an iterator over `play_order` from `position` onward, rather than re-deriving the mapping
locally).

## Files

- `crates/loxia-tui/src/views/now_playing.rs`
- `crates/loxia-tui/src/widgets/player_bar.rs`

## Specification

- The now-playing widget must resolve the current entry via `QueueState::current()` (or an
  equivalent that resolves `play_order[position]` before indexing `entries`), never
  `entries[position]`.
- The queue view's "up next"/history list must iterate `play_order` (from `position` onward for
  upcoming tracks, and backwards from `position` for history), mapping each `play_order` value to
  its `entries` index, never iterate `entries` in storage order.
- No behavioural change when shuffle is off (`play_order` is the identity permutation in that
  case), so all existing non-shuffle snapshots must be unaffected.

## Acceptance

- A new widget/view-level test with a shuffled `QueueState` fixture (`play_order` a
  non-identity permutation of `entries`' indices) asserting the rendered "now playing" title and
  the rendered queue-list order match `play_order`, not storage order — e.g.
  `player_bar_now_playing_matches_play_order_under_shuffle` and
  `now_playing_queue_list_matches_play_order_under_shuffle`.
- All existing snapshots in `loxia-tui` remain green (or are re-recorded only if their fixtures
  were already using a non-identity `play_order`, which would itself indicate the snapshot was
  wrong before this fix).

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
