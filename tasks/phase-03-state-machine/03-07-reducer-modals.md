# 03-07 · Reducer: modals

**Phase:** 03 — State machine · **Agent:** A · **Size:** M
**Prerequisites:** `03-06`
**Reference:** `docs/02-data-model.md` §6, `docs/04-state-and-input.md` §4

## Goal
Modal lifecycle and field editing. Modals own the keyboard while open, so this also defines how
input is intercepted before it reaches the navigation reducer.

## Files
- `crates/loxia-core/src/reducer/modal.rs`

## Specification

**Lifecycle.** `Modal::Open(kind)` **replaces** any existing modal — there is never a modal stack.
The reducer constructs the modal, populating it from current state:

| Kind | Population |
| :-- | :-- |
| `Help` | current `InputContext`, scroll 0 |
| `Equalizer` | `draft_gains` and `gains_at_open` from `player.eq.gains`; band 0 |
| `DevicePicker` | empty list, `LoadState::Loading`; emits `Effect::Audio(EnumerateDevices)` |
| `SleepTimer` | current `player.sleep_timer`, or defaults |
| `SavePlaylist` | source = current selection when non-empty, else the queue; name empty |
| `SortProfile` | `config.sorting.profiles`, cursor at the active one |
| `KeymapEditor` | cursor 0, not capturing |
| `Confirm` | prompt and boxed action supplied by the caller |

`Modal::Close` clears it. Closing the equalizer via `Cancel` restores `gains_at_open` and emits
`Effect::Audio(SetEq)` to undo the live preview; closing via `Submit` commits and emits
`Effect::Sys(WriteConfig)`.

**Field editing.** `FieldNext`/`FieldPrev` cycle focusable fields with wraparound.
`FieldInput(char)` appends to the focused text field; `FieldBackspace` removes a grapheme, not a
byte — truncating a multi-byte character mid-sequence would panic on the next render.

**Interception.** `apply()` checks for an open modal **first**. When one is open, only
`Action::Modal(..)`, `Action::View(ToggleHelp)`, and `Action::System(..)` are processed; everything
else is dropped. Without this, `j` would move both the modal cursor and the column behind it.

**`Submit`** per modal:
| Modal | Effects |
| :-- | :-- |
| `Equalizer` | `Effect::Audio(SetEq)`, `Effect::Sys(WriteConfig)`, close |
| `DevicePicker` | `Effect::Audio(SetDevice)`, close |
| `SleepTimer` | set `player.sleep_timer`, close |
| `SavePlaylist` | `Effect::Net(PlaylistCreate)` or `PlaylistAdd`, close, toast pending |
| `SortProfile` | `Action::Queue(ApplySortProfile)`, close |
| `KeymapEditor` | validate the capture; **refuse** on conflict, keeping the modal open with `conflict: Some(incumbent)`; otherwise commit and `WriteConfig` |
| `Confirm` | dispatch the boxed action, then close |

`Confirm`'s boxed action is dispatched **after** the modal is cleared, so a confirmed action that
opens another modal works.

## Acceptance
- `modal_open_replaces_existing_modal`
- `modal_intercepts_navigation_actions`
- `modal_allows_system_and_help_actions_through`
- `equalizer_cancel_restores_gains_and_emits_set_eq`
- `equalizer_submit_commits_and_writes_config`
- `device_picker_open_emits_enumerate`
- `field_navigation_wraps`
- `field_backspace_removes_grapheme_not_byte` — a field containing an emoji is truncated cleanly.
- `save_playlist_defaults_to_selection_then_queue`
- `keymap_editor_refuses_conflicting_capture` — the modal stays open and `conflict` names the
  incumbent action.
- `confirm_dispatches_boxed_action_after_close`

## Done when
The global DoD in `tasks/README.md` is satisfied.
