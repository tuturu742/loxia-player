# 10-14 · Fix `play_order` reads in the MPRIS worker

**Phase / Agent / Size / Prerequisites / Reference**
Phase 10 — Polish · single-crate (`loxia-player`) · S · prerequisites: `10-11`, `06-03` ·
Reference: `docs/15-play-order-audit.md` row 9.

## Goal

`crates/loxia-player/src/workers/mpris.rs` reports the current track, the exposed track list, and
`CanGoNext`/`CanGoPrevious` by indexing `QueueState::entries` and comparing against
`entries.len()` directly, instead of going through `QueueState::play_order`. Under a shuffled
queue this shows the wrong "now playing" metadata to the OS media-key/notification integration,
exposes the track list to MPRIS clients in the wrong order, and can report `CanGoNext`/
`CanGoPrevious` incorrectly (e.g. `true` when `play_order` is actually exhausted in that
direction, or vice versa). When this task is done, all three read through `play_order`.

## Files

- `crates/loxia-player/src/workers/mpris.rs`

## Specification

- The `Metadata` (`xesam:title`, `xesam:artist`, `mpris:trackid`, etc.) sent for the current track
  must be built from `QueueState::current()` (or equivalently, from `entries[play_order[position]]`),
  never `entries[position]`.
- Any exposed ordered track list (e.g. `org.mpris.MediaPlayer2.TrackList`, if implemented) must be
  built by mapping `play_order` to `entries`, in `play_order`'s order — never `entries` in storage
  order.
- `CanGoNext` must be true iff there is a next position in `play_order` (i.e.
  `position + 1 < play_order.len()`, mirroring `QueueState::peek_next()`'s own bound), not
  `position + 1 < entries.len()`.
- `CanGoPrevious` must be true iff there is a previous position in `play_order` (i.e.
  `position > 0` against `play_order`, mirroring `QueueState::peek_previous()`), not compared
  against `entries` directly.
- No behavioural change when shuffle is off.

## Acceptance

- A new test with a shuffled `QueueState` fixture asserting:
  - `mpris_metadata_matches_play_order_under_shuffle` — the emitted `Metadata` matches
    `QueueState::current()`'s item, not `entries[position]`'s.
  - `mpris_can_go_next_previous_match_play_order_under_shuffle` — `CanGoNext`/`CanGoPrevious`
    match `QueueState::peek_next()`/`peek_previous()` being `Some`/`None`, including at both ends
    of a shuffled `play_order` where `entries[position]` would have given a different answer than
    `play_order[position]`.

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
