# 10-14 — Fix: MPRIS track list must follow `play_order`

**Crate:** `loxia-player` only. Do not touch `loxia-core`, `loxia-tui`, or any other crate in this
task — see `docs/15-play-order-consumer-audit.md` for why this is split out from the core audit
that found it.

## Background

`docs/15-play-order-consumer-audit.md` audited every reader of `QueueState` for whether it
resolves the current/next track via `entries[play_order[i]]` (correct under shuffle) or via
`entries[i]` / storage-order iteration (wrong under shuffle — shows the wrong track). Three of the
four MPRIS-related reads in `crates/loxia-player/src/workers/mpris.rs` are correct. One is not.

## The bug

`crates/loxia-player/src/workers/mpris.rs`, `build_track_list(queue: &QueueState) -> Vec<TrackId>`,
around line 121:

```rust
fn build_track_list(queue: &QueueState) -> Vec<TrackId> {
    let mut ids = Vec::with_capacity(queue.entries.len());
    for track in &queue.entries {
        ids.push(TrackId::from(&track.id));
    }
    ids
}
```

This iterates `queue.entries` in storage (library-add) order. It should iterate `queue.play_order`
and resolve each index into `queue.entries`, matching the pattern already used correctly by
`update_metadata`, `can_go_next`, and `can_go_previous` in the same file.

**Effect:** any MPRIS client (desktop media widget, lock-screen media controls, `playerctl`, etc.)
that reads the `org.mpris.MediaPlayer2.TrackList` interface sees tracks in library order while
shuffle is on. "Jump to track N" from such a client jumps to the wrong track — it uses the
`TrackId` at that position, but that position doesn't correspond to loxia's actual play order, so
the track a user picks from the list is not the one that starts playing next in the shuffled
sequence they're looking at in-app.

## The fix

```rust
fn build_track_list(queue: &QueueState) -> Vec<TrackId> {
    queue
        .play_order
        .iter()
        .filter_map(|&idx| queue.entries.get(idx))
        .map(|track| TrackId::from(&track.id))
        .collect()
}
```

`filter_map`/`.get(idx)` rather than `entries[idx]`, matching `QueueState::current`'s own
defensive style (`state/queue.rs`), so an out-of-range index (which should never happen, but see
`06-03-shuffle.md`'s own invariants) degrades to "track omitted from the list" rather than a panic
in a background worker.

## Acceptance

- [ ] `build_track_list` iterates `queue.play_order`, not `queue.entries`, directly.
- [ ] New test `mpris_track_list_follows_play_order_under_shuffle` in
      `crates/loxia-player/src/workers/mpris.rs`'s own test module: build a `QueueState` with a
      non-identity `play_order` (e.g. reversed relative to `entries`), call `build_track_list`,
      assert the returned `Vec<TrackId>` order matches `play_order`, not `entries`' storage order.
- [ ] `update_metadata`, `can_go_next`, `can_go_previous` are unchanged (they were already
      correct — see `docs/15-play-order-consumer-audit.md`).
- [ ] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and
      `cargo test --workspace` all clean.

## Out of scope

Do not modify `loxia-core`, `loxia-tui`, or any other crate. Do not re-run the full consumer audit
— it is already done and lives in `docs/15-play-order-consumer-audit.md`.
