# 10-14 · fix MPRIS play order

**Phase:** 10 — Polish & integrations
**Agent:** loxia-player
**Size:** S
**Prerequisites:** `10-11` media keys, `06-03` shuffle
**Reference:** `docs/15-play-order-audit.md` row 9

## Goal

`crates/loxia-player/src/workers/mpris.rs` builds its MPRIS `Metadata` for the current track, and
its reported track list, by reading `QueueState::entries` directly in storage order instead of
resolving through `QueueState::play_order`. Under shuffle this reports the wrong "now playing"
track and the wrong track list to any MPRIS controller (desktop widgets, media-key overlays,
lock-screen displays), even though this same worker's `CanGoNext`/`CanGoPrevious` computation is
already correct — the audit found it reduces to a `position` bound-check that is identical whether
measured against `entries.len()` or `play_order.len()`. When this task is done, the current-track
`Metadata` and the reported track list both resolve through `play_order`, and a shuffled-queue test
proves it.

## Files

- `crates/loxia-player/src/workers/mpris.rs`

## Specification

- The current-track `Metadata` (`xesam:title`, `xesam:artist`, `mpris:trackid`, art URL, and any
  other per-track field) must be built from `queue.current()` (or the equivalent
  `entries[play_order[position]]` resolution), never `entries[position]`.
- The MPRIS track list must be built by walking `queue.play_order` and resolving each visited index
  into `entries`, not by iterating `entries` directly in storage order.
- `CanGoNext`/`CanGoPrevious` are already correct per the audit and must not be changed by this
  task beyond adding a regression test that locks in that they stay correct alongside the
  `Metadata`/track-list fix.

## Acceptance

- `report_item_matches_play_order_under_shuffle`: a `QueueState` built with a non-identity
  `play_order` produces MPRIS `Metadata` for `entries[play_order[position]]`, not
  `entries[position]`.
- A companion test asserts the reported MPRIS track list order matches `play_order`, not `entries`
  storage order.
- A regression test asserts `CanGoNext`/`CanGoPrevious` remain correct against the same shuffled
  `QueueState` used above.

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
