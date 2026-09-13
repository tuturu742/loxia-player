# 04-02 · Root layout

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** S
**Prerequisites:** `04-01`
**Reference:** `docs/07-ui-spec.md` §§1–2

## Goal
Compute the top-level zones and the responsive degradation ladder, and add the `draw()` entry point
that every later widget hangs off.

## Files
- `crates/loxia-tui/src/render.rs`, `layout.rs`

## Specification

```
pub fn draw(f: &mut Frame, state: &AppState, theme: &Theme, hits: &mut HitMap);

pub struct Zones { pub header: Rect, pub sidebar: Option<Rect>, pub canvas: Rect, pub player: Rect }
pub fn zones(area: Rect, state: &AppState) -> Zones;

pub struct CanvasPlan { pub columns: Vec<Rect>, pub inspector: Option<Rect> }
pub fn plan_canvas(canvas: Rect, column_count: usize) -> CanvasPlan;
```

Vertical split: header `Length(1)`, body `Min(0)`, player `Length(3)`.
Body split: sidebar `Length(16)`, canvas `Min(0)`. Zen mode returns `sidebar: None` and gives the
whole body to the canvas.

**Degradation ladder** — implement exactly:
| Width | Data columns | Inspector |
| :-- | :-- | :-- |
| ≥ 160 | 3 | full |
| 120–159 | 3 | 24 cells |
| 100–119 | 2 | full |
| 80–99 | 1 | hidden |

**Too-small guard.** Below 80×24, `draw` renders only a centred single-line message
`terminal too small (needs 80x24)` and returns. It must not attempt the normal layout — `Rect`
arithmetic on a 20-column terminal produces zero-width areas and panicking widgets.

`draw` renders in order: header, sidebar, canvas (view-dispatched on `state.nav.active_tab`, or Zen
when `state.zen_mode`), player bar, toasts, then the modal. Modals last so they overlay everything.

In this task the header, sidebar, canvas, and player bar are placeholder blocks with correct borders
and titles; tasks `04-05` through `04-09` replace them one at a time.

`draw` takes `&AppState` and must not mutate it. The only out-parameter is `hits`.

## Acceptance
- `zones_sum_to_area` (proptest over areas from 80×24 to 400×120) — the four zones tile the area
  with no overlap and no gap.
- `zen_hides_sidebar`
- `degradation_ladder` — table test asserting column count and inspector presence at widths 80, 99,
  100, 119, 120, 159, 160, 240.
- `too_small_renders_message_only` — at 40×10, the buffer contains the message and no borders.
- `too_small_does_not_panic` (proptest over areas from 1×1 to 79×23).
- `layout_snapshot_80x24`, `layout_snapshot_120x30`, `layout_snapshot_200x50`

## Done when
The global DoD in `tasks/README.md` is satisfied.
