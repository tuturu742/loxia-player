# 06-10 · MPRIS worker must read through `play_order`

**Phase:** 06 — Queue engine · **Agent:** loxia-player · **Size:** S
**Prerequisites:** `06-03` (shuffle), the MPRIS background worker in `crates/loxia-player/src/workers/mpris.rs`
**Reference:** `docs/15-play-order-audit.md` (rows 11–13)

## Goal

`loxia-player`'s MPRIS worker (`workers::mpris`, backed by `souvlaki`) currently reports the
current track, the track list, and `CanGoNext`/`CanGoPrevious` by indexing
`QueueState::entries` directly and bounds-checking against `entries.len()`. Under shuffle this can
report the wrong now-playing metadata to the desktop environment, expose the track list to MPRIS
clients in the wrong order, and mis-report whether a next/previous track actually exists (e.g.
after a shuffled `play_order` that has fewer reachable entries ahead of the cursor than
`entries.len()` would suggest). When finished, all three read through `play_order`, matching what
the audio worker actually plays next.

## Files

- `crates/loxia-player/src/workers/mpris.rs`

## Specification

- The `MediaControls::set_metadata` call must source title/artist/album/duration from
  `QueueState::current()` (i.e. `entries[play_order[cursor]]`), never `entries[cursor]` directly.
- Any track list surfaced to MPRIS (`org.mpris.MediaPlayer2.TrackList`, if implemented via
  `souvlaki` or a raw D-Bus extension) must be built by mapping over `play_order`, so its order
  matches playback order rather than insertion order.
- `CanGoNext` must be `true` iff `play_order.get(cursor + 1)` is `Some`, and `CanGoPrevious` must be
  `true` iff `cursor > 0` — both derived from `play_order`'s length/position, not `entries.len()`.
- On Windows/macOS where `souvlaki` maps these onto the native OS session control surface, the same
  rule applies: whatever souvlaki call communicates "has next"/"has previous" must be fed the
  `play_order`-derived boolean, not an `entries`-derived one.

## Acceptance

- `mpris_reports_shuffled_current_track` (new): with a fixture queue whose `play_order` is a
  non-identity permutation, the metadata sent to `MediaControls::set_metadata` names the track at
  `entries[play_order[cursor]]`, not `entries[cursor]`.
- `mpris_track_list_matches_play_order` (new): the track list handed to MPRIS is in `play_order`
  order, not insertion order.
- `mpris_can_go_next_respects_play_order_bounds` (new): a shuffled queue where the cursor is at the
  last position in `play_order` (but not the last position in `entries`) reports
  `CanGoNext == false`.
- `mpris_can_go_previous_respects_play_order_bounds` (new): the symmetric case for
  `CanGoPrevious` at `cursor == 0`.

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
