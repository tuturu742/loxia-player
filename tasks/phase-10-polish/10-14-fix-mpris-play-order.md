# 10-14 · Fix MPRIS play-order reads

**Phase:** 10 — Polish
**Agent:** single-crate (`loxia-player`)
**Size:** S
**Prerequisites:** `10-11` (media keys), `06-03` (shuffle)
**Reference:** `docs/15-play-order-audit.md` row 9

## Goal

`crates/loxia-player/src/workers/mpris.rs` reports the current track and the exposed track list by
reading/iterating `entries` directly rather than resolving through `play_order`. With shuffle on,
MPRIS (and the SMTC/CoreAudio equivalents `souvlaki` maps it to) shows the wrong "now playing"
metadata and the wrong track list. When this task is done, the MPRIS worker's `Metadata` and track
list both resolve through `play_order` the same way `QueueState::current()` already does.

`CanGoNext`/`CanGoPrevious` were audited separately (`docs/15-play-order-audit.md` row 9) and found
already correct: `play_order` is always a permutation of `entries`' indices, so
`play_order.len() == entries.len()` unconditionally, and `QueueState::peek_next()`/
`peek_previous()` are plain bound-checks against `position` rather than entry-skipping walks, so a
length-based check against either vec is identical. Nothing in this task changes
`CanGoNext`/`CanGoPrevious`, and no test for them is added here.

## Files

- `crates/loxia-player/src/workers/mpris.rs`

## Specification

- The `MediaMetadata` built for `souvlaki::MediaControls::set_metadata` must be built from
  `queue.current()` (or an equivalent `play_order`-resolved lookup), never from
  `queue.entries[queue.position]` or any other direct index into `entries`.
- Any track list this worker exposes must be built by walking `queue.play_order` and resolving
  each index into `entries`, in play order, not by iterating `entries` in storage order.
- `CanGoNext`/`CanGoPrevious` are unchanged by this task.

## Acceptance

- `mpris_metadata_matches_play_order_under_shuffle` — a `QueueState` fixture with shuffle on (a
  non-identity `play_order`) produces `MediaMetadata` for `entries[play_order[position]]`, not
  `entries[position]`.
- `mpris_track_list_matches_play_order_under_shuffle` — the exposed track list, for the same
  fixture, is in play order (`play_order`-resolved), not storage order.

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
