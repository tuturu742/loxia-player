# 04-08 · Inspector

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** M
**Prerequisites:** `04-06`
**Reference:** `docs/07-ui-spec.md` §6

## Goal
The rightmost metadata pane, including the multi-select summary and the keymap-derived action list.

## Files
- `crates/loxia-tui/src/widgets/inspector.rs`

## Specification

```
pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap);
```

Content is chosen by what the focused column has selected:

| Case | Body |
| :-- | :-- |
| Multi-select, n > 1 | `n Tracks Selected`, total duration, then the action list |
| `Artist` | art placeholder, name, album and track counts, genres, wrapped overview |
| `Album` | art placeholder, title, artist, year, track count, total time, favourite and download state |
| `Track` | art placeholder, title, artists, album, year, disc/track, duration, `format.summary()`, bitrate, play count, applied ReplayGain, availability |
| Nothing selected | a dim `—` line |

Album art is a reserved placeholder rect in this task; task `10-01` fills it.

**Action list.** The footer lists contextual actions with their keys, every one produced by
`state.keymap.hint_for(..)`:
```
Actions:
[a]      Queue Selected
[Ctrl+P] Add to Playlist
[d]      Download
[f]      Favourite
```
Which actions appear depends on the selection type — `Queue`/`InsertNext`/`InstantMix` for anything
queueable, `AddToPlaylist` only when tracks are selected, `DeletePlaylist` only in the Playlists
tab. Each row registers `HitTarget::InspectorAction(ActionId)`.

**Hardcoding a key string here is a review failure.** Remapping must update this pane.

The overview text wraps with `text::wrap` and is clipped to the available height; a scroll indicator
(`▾`) appears when there is more. The inspector itself is not focusable in this phase, so scrolling
is not implemented — do not add a scrollbar that cannot move.

Total duration for multi-select is derived by summing the selected tracks' durations at render time;
it is never stored in state.

## Acceptance
- `inspector_snapshot_artist`, `_album`, `_track`, `_multiselect`, `_empty`
- `multiselect_total_duration_is_summed`
- `action_hints_come_from_keymap` — remap `AddToPlaylist` and assert the rendered hint changes.
- `action_list_varies_by_selection_type` — table test over artist, album, track, multi-select.
- `actions_register_hit_targets`
- `overview_wraps_and_clips`
- `scroll_indicator_only_when_overflowing`
- `narrow_inspector_snapshot_24_cells`

## Done when
The global DoD in `tasks/README.md` is satisfied.
