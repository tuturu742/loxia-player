# 04-07 · Miller view

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** M
**Prerequisites:** `04-06`
**Reference:** `docs/07-ui-spec.md` §5, `docs/02-data-model.md` §3

## Goal
Compose columns into the sliding-window Miller view, including the parent-path indicator. This is
the view behind five of the nine tabs.

## Files
- `crates/loxia-tui/src/views/miller.rs`

## Specification

```
pub fn render(f: &mut Frame, canvas: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap);
```

1. Read `state.nav` for the active tab's column stack and `window_start`.
2. Take the visible slice: `columns[window_start .. window_start + visible_count]`, where
   `visible_count` comes from `plan_canvas` (task `04-02`).
3. Render each with `render_column`, passing `focused = (absolute_index == nav.focus_column)`.
4. Render the inspector in the remaining `CanvasPlan.inspector` rect (task `04-08`).

**Parent-path indicator.** When `window_start > 0`, the **leftmost visible** column's title is
replaced by the elided ancestor path, built from the titles of columns `0..window_start`:
`…/Gothic/Darkwave/` — produced with `text::ellipsize_start` so the tail, which is the informative
part, survives.

The view **reads** `window_start`; it never computes or adjusts it. That is the reducer's job
(task `03-06`), and splitting the logic across both layers is how sliding-window bugs appear.

When the stack is empty — a tab not yet visited — render a single centred dim line naming what will
load, rather than an empty frame.

Column widths are equal, taking the remainder in the leftmost columns so widths stay stable as the
window slides.

## Acceptance
- `miller_snapshot_three_columns`
- `miller_snapshot_sliding_window` — `fixture_miller_5col()`, asserting columns 3–5 are visible and
  the leftmost title shows the elided path.
- `parent_path_indicator_absent_at_window_start_zero`
- `parent_path_is_start_elided` — the tail of the path is preserved, the head is cut.
- `focused_column_gets_focus_border`
- `empty_stack_renders_placeholder`
- `view_does_not_mutate_window_start`
- `miller_snapshot_80x24_single_column`
- `miller_snapshot_appears_on_split` — the full three-column view with the ALBUMS / APPEARS ON
  sections visible, from `fixture_appears_on()`.

## Done when
The global DoD in `tasks/README.md` is satisfied.
