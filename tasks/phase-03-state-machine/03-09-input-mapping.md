# 03-09 · Input mapping

**Phase:** 03 — State machine · **Agent:** A · **Size:** S
**Prerequisites:** `03-05`, `03-08`
**Reference:** `docs/04-state-and-input.md` §§5, 7

## Goal
Convert crossterm events into actions through the keymap, including the two-chord prefix and the
text-input mode. This is the bridge between `crossterm`'s types and `loxia-core`'s local ones.

## Files
- `crates/loxia-player/src/input.rs`

## Specification

```
pub fn to_action(state: &AppState, ev: CrosstermEvent, viewport: Viewport) -> Option<Action>;
```

**Conversion.** Map `crossterm::event::KeyCode` and `KeyModifiers` to the local types from `03-04`.
Normalise before lookup:
- A `Char` with `SHIFT` where the char is already uppercase drops the `SHIFT` bit, so `A` and
  `shift+a` produce the same chord — matching the parser's normalisation.
- `KeyEventKind::Release` and `Repeat` are **ignored**; only `Press` maps. Windows terminals send
  release events that would otherwise double every keystroke.

**Context** comes from the state: a focused text field → `TextInput`; an open modal →
`Modal(kind)`; otherwise `Normal`.

**Resolution:**
1. Call `keymap::resolve(map, ctx, state.pending_chord, chord)`.
2. `Resolution::Action(id)` → convert the `ActionId` to a concrete `Action`, filling in parameters
   the keymap cannot know — `HalfPageUp/Down` take `viewport.rows / 2`, `VolumeUp/Down` take the
   step of 5.
3. `Resolution::Pending` → `Action::System(SetPendingChord(chord))`.
4. `Resolution::None` with a pending chord set → clear the prefix and return `None`.
5. In `TextInput`, an unresolved printable character becomes `Action::Modal(FieldInput(c))`.

**Always-on:** `Ctrl+C` maps to `Quit` before any other lookup, in every context including text
input. A user must always be able to leave.

**Ignore** `Event::FocusGained`/`FocusLost`, `Event::Paste` (until a task needs it), and
`Event::Mouse` when `ui.enable_mouse` is false. `Event::Resize` becomes
`Action::System(Resize { w, h })`.

Mouse handling is stubbed here and completed in task `10-04`, once the `HitMap` exists.

## Acceptance
- `key_release_events_are_ignored`
- `uppercase_char_normalises_shift` — `A` and `Shift+a` resolve to the same action.
- `ctrl_c_quits_from_every_context` — table test over `Normal`, `TextInput`, and a modal.
- `g_prefix_then_a_resolves_to_go_to_artist`
- `g_prefix_then_invalid_clears_pending`
- `half_page_uses_viewport_rows`
- `text_input_routes_printable_to_field`
- `text_input_still_resolves_esc_and_enter`
- `resize_maps_to_system_action`
- `mouse_ignored_when_disabled`

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 03's exit criteria in
`docs/08-roadmap.md` are met.
