# 10-02 · Zen mode

**Phase:** 10 — Polish · **Agent:** D · **Size:** M
**Prerequisites:** `10-01`, `07-07`
**Reference:** `docs/07-ui-spec.md` §9

## Goal
A minimalist full-screen view: artwork, track detail, lyrics, progress, and the format line. No
navigation chrome.

## Files
- `crates/loxia-tui/src/views/zen.rs`

## Specification

Toggled by `z`. Replaces the whole body — `zones()` already returns `sidebar: None` when
`state.zen_mode` (task `04-02`). The header and player bar remain, because losing the transport
context entirely would make the mode less useful than the terminal it replaces.

**Layout** (from `docs/07-ui-spec.md` §9):
- A horizontal split: artwork on the left at up to 20 rows square, detail on the right.
- Right side, top to bottom: title in `Accent` bold, artist, `Album: <name> (<year>)`, a blank line,
  then `LYRICS` and the lyrics pane from task `07-07`.
- Below both: a full-width progress bar with timestamps, then the format line
  (`🎚 FLAC 24-bit / 96.0 kHz │ Bitrate: 2840 kbps │ Output: WASAPI Exclusive`).

The whole block is **vertically centred** in the available space when the content is shorter than
the canvas. Content pinned to the top with a large gap below looks broken rather than minimal.

**Narrow terminals** (< 100 columns): stack artwork above the detail rather than beside it, and
shrink the art to 10 rows. Below 80 columns, drop the artwork entirely — text is the higher-value
content.

**No lyrics:** the lyrics section is omitted and the detail block centres vertically on its own,
rather than leaving a labelled empty region.

**Nothing playing:** a centred dim `nothing playing — press {z} to return`, with the key from the
keymap.

Zen mode is a **view**, not a mode that changes behaviour: every keybinding continues to work, so
`n`, `p`, `[`, `]`, `f`, and the modals are all still available. Only navigation columns are hidden.

`z` persists to state but **not** to config — Zen is a momentary mode, and starting in it after a
restart would hide the library from a user who has forgotten they left it on.

## Acceptance
- `zen_hides_sidebar_and_columns`
- `zen_keeps_header_and_player_bar`
- `content_is_vertically_centred`
- `narrow_stacks_art_above_detail`
- `very_narrow_drops_art`
- `no_lyrics_omits_section`
- `nothing_playing_state_shows_keymap_hint`
- `keybindings_still_work_in_zen` — table test over next, seek, favourite, and opening a modal.
- `zen_state_not_persisted_to_config`
- `zen_snapshot_wide`, `_narrow`, `_no_lyrics`, `_nothing_playing`

## Done when
The global DoD in `tasks/README.md` is satisfied.
