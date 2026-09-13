# 03-06 · Reducer: navigation

**Phase:** 03 — State machine · **Agent:** A · **Size:** L
**Prerequisites:** `03-02`, `03-03`
**Reference:** `docs/04-state-and-input.md` §4, `docs/07-ui-spec.md` §5

## Goal
Implement `apply()` and the navigation half of the reducer: cursor movement, drilling, the sliding
window, tab switching, and selection. This is the largest single piece of logic in the project.

## Files
- `crates/loxia-core/src/reducer/mod.rs`, `nav.rs`
- `crates/loxia-core/src/test_support/scenario.rs` (complete the stub from `03-02`)

## Specification

```
pub fn apply(state: &mut AppState, action: Action) -> Vec<Effect>;
```
Dispatches by group to the submodules. Must be **total and panic-free** — clamp every index, handle
every `Option`. A panic here crashes the app with the terminal in raw mode.

**Movement.** `MoveUp`/`MoveDown` operate on `Column::selectable_indices()`, so section headers are
skipped in both directions. From the row above a header, `MoveDown` lands **below** it. At the
list's ends, movement clamps — it does not wrap. `HalfPageUp`/`HalfPageDown` move by
`viewport_rows / 2`, which the reducer does not know, so the action carries `n` and the input layer
supplies it from the last render's geometry.

`scroll_offset` is recomputed after every cursor move to keep the cursor visible with a 2-row margin
at the top and bottom.

**Drilling (`NavRight`).** Behaviour depends on the selected item:
| Selected | Result |
| :-- | :-- |
| `Artist` | push an `Albums { of_artist }` column, emit `Effect::Net(FetchDiscography)` |
| `Album` | push `Tracks { of_album }`, emit `Effect::Net(FetchAlbumTracks)` |
| `Genre` | push `GenreArtists`, emit the fetch |
| `Folder` | push `Folders { of_parent }`, emit the fetch |
| `Playlist` | push `PlaylistTracks`, emit the fetch |
| `Track` | no column; no-op (queueing is `Enter`, a separate action) |
| `SectionHeader` | unreachable — headers are never selectable |

The new column starts `LoadState::Loading` with an empty item list. Drilling into a column that is
already present and loaded (the user went right, left, right) **reuses it** with its cursor intact
and emits no effect.

**`NavLeft` / `PopColumn`.** Pop the rightmost column and restore focus to its parent with the
parent's cursor unchanged. Popping the last remaining column is a no-op. `NavLeft` at column 0 moves
focus to the sidebar.

**Sliding window.** After any push or pop, recompute `window_start` so the focused column is the
rightmost visible one, with at most 3 data columns visible:
`window_start = focused_index.saturating_sub(2)`.

**Tabs.** `SetTab`, `NextTab`, `PrevTab` swap `nav.columns` into `per_tab_stacks` and restore the
target tab's stack. A tab visited for the first time gets a seed column and its fetch effect.

**Selection.** `ToggleVisualMode` toggles the flag and clears the set when leaving. `ToggleItem`
adds or removes the focused item's `ItemId`. `SelectAll` adds every **selectable** item's id (never
headers). `ClearSelection` empties the set.

**`Cancel`** (`Esc`) applies a precedence ladder, taking the first that applies: close a modal →
clear an active filter → leave visual mode → clear selection → do nothing.

**`Data::ItemsLoaded`** replaces the target column's items, sets `Loaded { total }`, clamps the
cursor into range, and preserves selection by id. `Data::LoadFailed` sets `Error(msg)` and leaves
existing items intact so the user does not lose their place on a transient failure.

Every state change calls `state.touch()`.

## Acceptance
Using `Scenario`, with the required test names from `docs/04-state-and-input.md` §10:
- `section_headers_are_skipped_by_cursor_movement`
- `move_down_from_above_header_lands_below_it`
- `movement_clamps_at_list_ends`
- `scroll_offset_keeps_cursor_visible_with_margin`
- `drilling_past_three_columns_slides_window`
- `pop_column_restores_previous_cursor`
- `redrilling_reuses_loaded_column_without_effect`
- `drill_into_track_is_a_noop`
- `tab_switch_preserves_per_tab_column_stack`
- `first_visit_to_tab_emits_seed_fetch`
- `visual_selection_survives_filter_change`
- `select_all_excludes_headers`
- `cancel_precedence_ladder` — table test over all five rungs.
- `items_loaded_clamps_out_of_range_cursor`
- `load_failed_preserves_existing_items`
- `reducer_never_panics` (proptest) — apply 100 random action sequences to every fixture; the
  property is that none panic.

## Done when
The global DoD in `tasks/README.md` is satisfied.
