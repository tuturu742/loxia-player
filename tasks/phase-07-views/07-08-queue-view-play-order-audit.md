# 07-08 — loxia-tui: audit and fix queue-view play-order reads

## Why

Raised by the workspace-wide play-order consumer audit (`docs/15-play-order-consumer-audit.md`).
The play sequence is `entries[play_order[i]]`; any consumer that instead indexes
`entries[position]` directly shows the wrong track whenever shuffle is on. Two `loxia-tui` sites
were named as untraced by that audit and are explicitly out of scope for it (it touches only
`loxia-core`):

1. The queue list rendered by the now-playing view, `crates/loxia-tui/src/views/now_playing.rs`.
2. The current-track line rendered by `crates/loxia-tui/src/widgets/player_bar.rs`.

## Work

- Grep both files for any direct index into `AppState`'s queue entries (`entries[`,
  `.entries.get(`, or a manual loop over `entries` combined with a raw position/index variable).
- For each hit, confirm whether it should instead be calling one of `QueueState`'s own
  play-order-aware accessors (`current()`, `peek_next()`, `peek_previous()`, or an equivalent
  "queue in play order" iterator/helper already exposed by `loxia-core`).
- If a direct, order-unaware index is found, replace it with the correct `loxia-core` accessor.
  Do not duplicate `play_order` resolution logic in `loxia-tui` — call into `loxia-core`.
- If both sites already resolve correctly, this task is a no-op audit: record that finding and
  close it without a code change.

## Acceptance

- A test (snapshot or unit, whichever this view already uses) demonstrating that with shuffle
  enabled and a `play_order` that differs from insertion order, both the queue list and the
  player bar show the track at the *shuffled* position, not the track at the same numeric index
  in insertion order.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace` all clean.
- This task touches only `crates/loxia-tui` (and, if a shared test fixture is missing, its own
  test-support code) — no `loxia-core` change.
