# 03-01 · AppState and sub-states

**Phase:** 03 — State machine · **Agent:** A · **Size:** M
**Prerequisites:** `01-07`, `01-08`
**Reference:** `docs/02-data-model.md` §§3–7

**Note:** implemented together with `03-03` despite the one-directional prerequisite shown above.
`state::modal::Modal::Confirm` (part of this task's own `§6` spec) holds `on_confirm: Box<Action>`,
so `03-01` cannot compile without `Action` existing — and `Action` is `03-03`'s job, declared to
depend on `03-01`. See `docs/12-decisions.md` for the resolution (same class of forward-dependency
mistake as `01-02`/`02-02`/`02-03`).

## Goal
Define `AppState` and every sub-state struct. Types only — no reducer logic yet. Once this lands,
every other phase-03 task has somewhere to write.

## Files
- `crates/loxia-core/src/state/mod.rs`, `nav.rs`, `queue.rs`, `player.rs`, `modal.rs`, `search.rs`,
  `toast.rs`

## Specification

Implement exactly the structures in `docs/02-data-model.md` §§3–7. Every type derives
`Debug, Clone, PartialEq`; `QueueState` and `NavState` additionally derive `Serialize, Deserialize`
for session persistence.

Points that carry design intent and must not be "simplified":

- **`NavState.per_tab_stacks: HashMap<Tab, Vec<Column>>`** — each tab keeps its own drill state, so
  switching tabs and back returns the user where they were.
- **`SelectionState.selected: BTreeSet<ItemId>`** — keyed by **id, not index**. Index-keyed
  selection silently corrupts itself when a filter changes the row order.
- **`QueueState.entries` plus `play_order`** — two separate vectors. Shuffle permutes `play_order`
  only; `entries` is never reordered. This is what makes unshuffle exact.
- **`PlayerState`** carries no `Instant`. Position arrives from the engine; nothing is extrapolated.
- **`AppState.modal: Option<Modal>`** — one modal at a time, by construction.

Add these helpers now, since everything downstream uses them:
```
impl AppState {
    pub fn touch(&mut self);                          // sets dirty = true
    pub fn active_column(&self) -> Option<&Column>;
    pub fn active_column_mut(&mut self) -> Option<&mut Column>;
    pub fn selected_item(&self) -> Option<&MediaItem>;
    pub fn current_entry(&self) -> Option<&QueueEntry>;
    pub fn toast(&mut self, msg: impl Into<String>, level: ToastLevel);
}
impl Column {
    pub fn visible_items(&self) -> impl Iterator<Item = (usize, &MediaItem)>;  // applies filter
    pub fn selectable_indices(&self) -> impl Iterator<Item = usize>;           // skips headers
}
impl QueueState {
    pub fn current(&self) -> Option<&QueueEntry>;
    pub fn upcoming(&self) -> impl Iterator<Item = &QueueEntry>;
    pub fn next_id(&mut self) -> QueueEntryId;
}
```

`Default for AppState` must produce a state that renders without panicking: empty columns, no
modal, `Connectivity::Online`, `PlayStatus::Stopped`.

## Acceptance
- `default_appstate_is_coherent` — `AppState::default()` has no modal, an empty queue,
  `active_column()` is `None`, and `selected_item()` is `None`.
- `touch_sets_dirty`
- `selectable_indices_skips_section_headers`
- `visible_items_respects_filter`
- `next_id_is_monotonic`
- `queue_state_roundtrips_serde`
- `state_types_are_send` — a static assertion that `AppState: Send`.

## Done when
The global DoD in `tasks/README.md` is satisfied.
