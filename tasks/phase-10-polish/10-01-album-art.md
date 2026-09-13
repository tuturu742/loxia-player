# 10-01 · Album art

**Phase:** 10 — Polish · **Agent:** D · **Size:** L
**Prerequisites:** `04-08`, `02-12`
**Reference:** `docs/07-ui-spec.md` §11

## Goal
Render artwork in the inspector and Zen mode, with terminal-protocol detection and a graceful
fallback chain down to no art at all.

## Files
- `crates/loxia-tui/src/widgets/album_art.rs`
- `crates/loxia-player/src/workers/network.rs` (extend)

## Specification

**Protocol detection** at startup, in order: Kitty graphics → Sixel → iTerm2 → half-blocks → off.
`ui.album_art_protocol` forces a choice; `Auto` detects. Detection uses `ratatui-image`'s picker and
must **never block startup for more than 100 ms** — a terminal that does not answer the query
otherwise hangs the app before it draws anything. Fall back to half-blocks on timeout.

**Decoding happens off the main thread.** `Effect::Net(FetchImage { id, size, tag })` → the network
worker fetches bytes, decodes, and resizes to the target cell dimensions → `Event::ImageLoaded`. The
render path only blits. Decoding a 600 px JPEG in `draw()` would drop the frame rate to single
digits.

**Decoded-image cache**: an LRU of 16, keyed by `(ItemId, cell_width, cell_height)`. Resizing per
frame is the single most expensive thing a TUI can do; without this cache the terminal becomes
unusable.

**Fallback chain**, using `images::art_candidates` (task `02-12`): track art → album art → artist
art → a themed placeholder. The placeholder is a bordered box with a centred `♪` in `Dim` — never an
empty hole, which reads as a rendering bug.

**Sizing.** The inspector uses a square region of the pane width, capped at 12 rows. Zen uses up to
20 rows. Cell aspect ratio is roughly 1:2, so square artwork needs twice as many columns as rows;
`ratatui-image` handles this, but the reserved `Rect` must account for it or the image is squashed.

**Placement.** Art draws before text in the same pane so a protocol that writes out of band cannot
overwrite the text.

**Cleanup.** Kitty and iTerm2 images persist until deleted. On resize, tab change, or track change,
issue the protocol's delete before drawing the replacement, or stale images accumulate on screen.

## Acceptance
- `protocol_detection_falls_back_on_timeout` — a stub picker that never answers yields half-blocks
  within 100 ms.
- `forced_protocol_skips_detection`
- `decode_happens_in_worker_not_render` — a render with no cached image draws the placeholder and
  emits a fetch effect.
- `decoded_cache_evicts_at_sixteen`
- `cache_key_includes_cell_size` — the same image at two sizes is two entries.
- `fallback_chain_order`
- `placeholder_drawn_when_no_art_anywhere`
- `zen_and_inspector_use_different_sizes`
- `stale_image_deleted_on_track_change`
- `art_snapshot_halfblocks`, `art_snapshot_placeholder`
- Manual, pasted into the PR: artwork renders correctly in Kitty, in a Sixel-capable terminal, and
  in a terminal with no graphics support; resizing the window does not leave stale images.

## Done when
The global DoD in `tasks/README.md` is satisfied.
