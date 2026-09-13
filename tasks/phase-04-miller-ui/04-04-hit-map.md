# 04-04 · Hit map

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** S
**Prerequisites:** `04-02`
**Reference:** `docs/04-state-and-input.md` §8

## Goal
The mouse hit-testing registry. Widgets register their regions during `draw`; the runtime resolves
mouse events against the previous frame. Landing it now means every widget can register from the
start, instead of being retrofitted in phase 10.

## Files
- `crates/loxia-tui/src/hit.rs`

## Specification

```
pub enum HitTarget {
    SidebarTab(Tab),
    ColumnItem { column: usize, index: usize },
    QueueEntry(QueueEntryId),
    SeekBar,
    VolumeBar,
    Transport(TransportButton),      // Prev | PlayPause | Next | Shuffle | Repeat
    ModalField(usize),
    EqBand(usize),
    InspectorAction(ActionId),
}

pub struct HitMap { regions: Vec<(Rect, HitTarget)> }
impl HitMap {
    pub fn clear(&mut self);
    pub fn push(&mut self, area: Rect, target: HitTarget);
    pub fn hit(&self, col: u16, row: u16) -> Option<&HitTarget>;
}
```

`hit` returns the **last** matching region, because widgets are drawn back to front and a modal
region registered later must win over the column beneath it. Iterate in reverse and take the first
match.

`clear` is called by the runtime at the start of each frame. The map is reused rather than
reallocated — it is touched every frame and allocation churn here is measurable.

A `Rect` of zero width or height is never pushed; a widget that is not visible must not be
clickable.

Consumption lives in the runtime (task `10-04`); this task provides only the data structure and the
registration calls added by widgets in `04-05` through `04-09`.

## Acceptance
- `hit_returns_last_registered_overlapping_region`
- `hit_outside_all_regions_is_none`
- `zero_sized_regions_are_not_registered`
- `clear_empties_without_reallocating` — capacity is retained after `clear`.
- `hit_is_inclusive_of_edges` — a click on a region's first and last cell both hit.
- `hit_map_survives_ten_thousand_pushes` — a smoke test for the reuse pattern.

## Done when
The global DoD in `tasks/README.md` is satisfied.
