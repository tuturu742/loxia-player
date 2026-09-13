# 10-04 · Mouse support

**Phase:** 10 — Polish · **Agent:** D · **Size:** M
**Prerequisites:** `04-04`, `07-06`
**Reference:** `docs/04-state-and-input.md` §8

## Goal
Click, double-click, drag, and scroll, resolved against the hit map that widgets have been
populating since phase 04.

## Files
- `crates/loxia-player/src/input.rs` (extend)
- `crates/loxia-player/src/runtime.rs` (extend)

## Specification

The runtime retains the **previous frame's** `HitMap` and resolves mouse events against it. Using
the current frame's map is impossible — the event arrives before the frame is drawn.

| Event | Target | Result |
| :-- | :-- | :-- |
| `Down(Left)` | `ColumnItem` | focus that column, set the cursor |
| `Down(Left)` ×2 within 400 ms on the same target | `ColumnItem` | `NavRight` (drill in) |
| `Down(Left)` | `SidebarTab` | `SetTab` |
| `Down(Left)` or `Drag` | `SeekBar` | `Seek(Fraction(x_rel))` |
| `Down(Left)` or `Drag` | `VolumeBar` | `SetVolume` |
| `Down(Left)` | `Transport(b)` | the matching playback action |
| `Down(Left)` | `QueueEntry(id)` | focus; double-click → `JumpTo(id)` |
| `Down(Left)` | `InspectorAction(a)` | dispatch that action |
| `Down(Left)` or `Drag` | `EqBand(n)` | select the band, set the gain from the y position |
| `ScrollUp` / `ScrollDown` | any | `MoveUp{3}` / `MoveDown{3}` on the column **under the pointer** |

**Scroll targets the column under the pointer, not the focused one.** Scrolling a list you are
pointing at without first clicking it is what every other application does, and requiring a click
first would feel broken.

**Drag** requires tracking the target captured on mouse-down; a drag that leaves the widget's rect
continues to control it until release. Releasing outside must not leave the drag stuck on.

**Double-click detection** compares the target and timestamp of the previous click. Two clicks on
*different* targets are never a double-click regardless of timing.

**Gating.** All of this is skipped when `ui.enable_mouse` is false, and crossterm mouse capture is
never enabled in that case (task `01-05`), so terminal text selection keeps working.

**Modals** capture the mouse: a click outside an open modal's rect closes it; clicks inside route to
`ModalField` and `EqBand` only.

## Acceptance
- `click_focuses_column_and_sets_cursor`
- `double_click_drills_in`
- `two_clicks_on_different_targets_is_not_double`
- `double_click_window_is_400ms`
- `seek_bar_click_seeks_to_fraction`
- `drag_continues_outside_widget_rect`
- `drag_released_outside_clears_capture`
- `scroll_targets_column_under_pointer_not_focused`
- `transport_buttons_dispatch_actions` — table test over all five.
- `eq_band_drag_sets_gain_from_y`
- `click_outside_modal_closes_it`
- `mouse_ignored_when_disabled`
- `hit_map_is_from_previous_frame`
- Manual, pasted into the PR: click through tabs and columns, seek by clicking and dragging the
  progress bar, and scroll a column with a trackpad.

## Done when
The global DoD in `tasks/README.md` is satisfied.
