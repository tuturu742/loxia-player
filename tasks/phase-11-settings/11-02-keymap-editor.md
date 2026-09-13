# 11-02 · Keymap editor

**Phase:** 11 — Settings · **Agent:** A · **Size:** L
**Prerequisites:** `11-01`, `03-05`
**Reference:** `docs/04-state-and-input.md` §7, `docs/12-decisions.md` §4

## Goal
Remap any action from within the UI, with conflict detection that refuses to create a broken keymap.

## Files
- `crates/loxia-tui/src/modals/keymap_editor.rs`
- `crates/loxia-core/src/reducer/settings.rs` (extend)

## Specification

The Keybindings section lists every `ActionId` grouped by `HelpCategory`:
```
  Playback
    Play / Pause              Space
    Next Track                n
    Previous Track            p
  ⚠ Queue
    Queue (artist only)       a          ⚠ also bound to: Instant Mix
```

**Capture flow.** `Enter` on a row enters capture mode: the row shows `press a key…` and the next
key event is captured verbatim rather than resolved. `Esc` cancels capture. Two-chord sequences are
captured by pressing a prefix and then the second key, with a 2-second window.

**Conflict handling on commit** — this is the point of the task:
1. Run `KeyMap::validate` against the prospective map.
2. On conflict, **refuse the binding.** Keep the modal open, show
   `{chord} is already bound to {incumbent}` in `Error` style, and offer two choices:
   `[Enter] rebind anyway (unbinds {incumbent})` or `[Esc] cancel`.
3. Choosing to rebind unbinds the incumbent explicitly, so the user knows what they gave up. There
   is never a silent overwrite.

**Reserved chords** cannot be rebound: `Ctrl+C` (always quits) and `Esc` (always cancels). Attempting
either shows `{chord} is reserved`.

**Per-row actions:** `d` resets a row to its default, `x` unbinds it entirely (an unbound action
shows `—` and remains reachable through Settings).
A footer action resets **all** bindings to defaults, behind a `Confirm`.

**Persistence.** On commit, write the friendly rendering via `render_binding` into
`config.keybindings` and emit `WriteConfig`. The round-trip from task `03-04` guarantees it reads
back identically.

**Live effect.** The new binding works immediately — the `KeyMap` in `AppState` is replaced, and
every UI hint drawn through `hint_for` updates on the next frame. Verifying this is what proves the
"no hardcoded keys" rule held across the whole codebase.

## Acceptance
- `capture_takes_key_verbatim_not_resolved`
- `two_chord_capture_within_window`
- `capture_cancels_on_esc`
- `keymap_conflict_is_rejected_by_editor`
- `conflict_message_names_incumbent`
- `rebind_anyway_unbinds_incumbent_explicitly`
- `reserved_chords_cannot_be_rebound` — `Ctrl+C` and `Esc`.
- `reset_row_restores_default`
- `unbind_leaves_action_listed_with_dash`
- `reset_all_requires_confirmation`
- `binding_persists_in_friendly_syntax`
- `binding_roundtrips_through_config`
- `remapped_key_works_immediately`
- `all_ui_hints_update_after_remap` — remap `AddToPlaylist` and assert the inspector, the help
  modal, and the save-playlist footer all show the new key.
- `keymap_editor_snapshot`, `_capturing`, `_conflict`

## Done when
The global DoD in `tasks/README.md` is satisfied.
