# 03-03 · Action, Effect and Event

**Phase:** 03 — State machine · **Agent:** A · **Size:** M
**Prerequisites:** `03-01`
**Reference:** `docs/04-state-and-input.md` §§2–3

**Note:** implemented together with `03-01` — its `Modal::Confirm` needs `Action` to exist, so both
tasks landed in the same pass rather than strictly in prerequisite order. See `docs/12-decisions.md`.

## Goal
Define the three enums that everything flows through, plus the `Event → Action` conversion. No
behaviour — just the vocabulary.

## Files
- `crates/loxia-core/src/action.rs`, `effect.rs`, `event.rs`

## Specification

Implement exactly the variants tabulated in `docs/04-state-and-input.md` §§2–3, as nested enums
(`Action::Nav(NavAction)`, `Action::Queue(QueueAction)`, …).

Derives:
- `Action`: `Debug, Clone, PartialEq, Serialize, Deserialize` — serialisability enables
  record-and-replay debugging and is worth the small constraint it imposes.
- `Effect`: `Debug, Clone, PartialEq`.
- `Event`: `Debug, Clone`.

**Rules that the types must enforce:**
- `Effect` contains **no closures, no channels, no `Arc`** — plain data only. A worker must be able
  to receive one, act, and reply without holding anything from the reducer.
- `Action::Modal(Open(..))` carries a `ModalKind`, not a constructed `Modal`. The reducer builds the
  modal so it can populate it from current state.
- `Modal::Confirm` holds `Box<Action>` for its confirmation, which is why `Action` must be `Clone`
  and not contain function pointers.
- `SeekTarget` = `Absolute(Duration) | Relative(i64 /* seconds */) | Fraction(f32)`. The fraction
  form is what mouse seeking produces.

**Conversion:**
```
impl Event { pub fn into_action(self) -> Action; }
```
Total — every `Event` maps to exactly one `Action`. This is the only place the two vocabularies
meet, which keeps workers ignorant of the reducer.

Add convenience constructors used constantly downstream:
```
impl Action {
    pub fn toast(msg: impl Into<String>, level: ToastLevel) -> Action;
}
impl Effect {
    pub fn is_network(&self) -> bool;
    pub fn is_audio(&self) -> bool;
}
```

## Acceptance
- `action_roundtrips_serde` (proptest over a derived `Arbitrary`, or a hand-written table covering
  every variant group).
- `event_into_action_is_total` — one test per `Event` variant asserting the expected `Action`.
- `effect_contains_no_interior_mutability` — static assertions that `Effect: Send + Sync + 'static`
  and `Effect: Clone`.
- `seek_target_variants_construct`
- `modal_open_carries_kind_not_modal` — a compile-level guarantee; assert by constructing
  `Action::Modal(ModalAction::Open(ModalKind::Help))`.

## Done when
The global DoD in `tasks/README.md` is satisfied.
