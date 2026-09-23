# 06-10 · MPRIS worker must read through `play_order`

**Phase:** 06 — Queue engine · **Agent:** loxia-player · **Size:** S
**Prerequisites:** `06-03` (shuffle), the MPRIS background worker in `crates/loxia-player/src/workers/mpris.rs`
**Reference:** `docs/15-play-order-audit.md` (rows 11–13)

## Goal

`loxia-player`'s MPRIS worker (`workers::mpris`, backed by `souvlaki`) currently reports the
current track and the track list by indexing `QueueState::entries` directly in insertion order
rather than playback order. Under shuffle this reports the wrong now-playing metadata to the
desktop environment and exposes the track list to MPRIS clients in the wrong order. When finished,
both read through `play_order`, matching what the audio worker actually plays next.

`CanGoNext`/`CanGoPrevious` are out of scope for this task: they are already correct as written.
`play_order` is always a permutation of `0..entries.len()`, so `play_order.len() == entries.len()`
holds at all times, and `cursor` is a position within `play_order`. That makes
`cursor + 1 < entries.len()` exactly equivalent to `play_order.get(cursor + 1).is_some()`, and
`cursor > 0` is already the correct "has previous" rule. There is no shuffled state in which these
two formulations disagree — see `docs/15-play-order-audit.md` row 13.

## Files

- `crates/loxia-player/src/workers/mpris.rs`

## Specification

- The `MediaControls::set_metadata` call must source title/artist/album/duration from
  `QueueState::current()` (i.e. `entries[play_order[cursor]]`), never `entries[cursor]` directly.
- Any track list surfaced to MPRIS (`org.mpris.MediaPlayer2.TrackList`, if implemented via
  `souvlaki` or a raw D-Bus extension) must be built by mapping over `play_order`, so its order
  matches playback order rather than insertion order.
- Do not change how `CanGoNext`/`CanGoPrevious` (or the equivalent souvlaki calls on
  Windows/macOS) are computed — leave the existing `entries.len()`/`cursor` bounds checks as they
  are.

## Acceptance

- `mpris_reports_shuffled_current_track` (new): with a fixture queue whose `play_order` is a
  non-identity permutation, the metadata sent to `MediaControls::set_metadata` names the track at
  `entries[play_order[cursor]]`, not `entries[cursor]`.
- `mpris_track_list_matches_play_order` (new): the track list handed to MPRIS is in `play_order`
  order, not insertion order.

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
