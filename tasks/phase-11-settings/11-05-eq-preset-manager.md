# 11-05 · EQ preset manager

**Phase:** 11 — Settings · **Agent:** A · **Size:** S
**Prerequisites:** `11-01`, `09-03`
**Reference:** `docs/05-audio-engine.md` §5

## Goal
Save, rename, and delete custom equalizer presets from the Equalizer settings section.

## Files
- `crates/loxia-tui/src/views/settings.rs` (extend)

## Specification

The Equalizer section shows: an enabled toggle, the active preset select, the preamp slider, and the
preset list.

```
  Presets
    flat                        (factory)
    darkwave_ebm                (factory)
  ▶ night_listening_warm        +2.0 +2.0 +1.0 0.0 0.0 -1.0 -1.5 -2.0 -3.0 -4.0
  [s] Save current as new   [r] Rename   [x] Delete   [e] Open equalizer
```

**Factory presets are read-only.** Rename and delete are disabled on them with the reason shown
(`factory presets cannot be modified`), not hidden. Hiding a control the user expects to see is more
confusing than showing why it is unavailable.

**`s`** captures the **current live gains** — including any unsaved adjustments made in the
equalizer modal — and prompts for a name. This is the natural workflow: tune by ear in the modal,
then come here to keep it.

**Name collisions with a factory preset** are rejected inline with
`that name is used by a factory preset`. Collisions between two custom presets are also rejected.
Config validation (task `01-02`) renames colliding presets loaded from a file, but the editor should
never create one.

**Deleting the active preset** falls back to `flat` and applies it, so the audio does not keep
applying a curve the user just deleted.

**`e`** opens the equalizer modal (task `10-06`) for visual editing, prefilled with the focused
preset's gains.

Custom presets persist to `config.equalizer.custom_presets` through the standard debounce.

## Acceptance
- `factory_presets_are_readonly_with_reason`
- `save_captures_current_live_gains`
- `save_prompts_for_name`
- `name_collision_with_factory_rejected`
- `name_collision_with_custom_rejected`
- `empty_name_rejected`
- `rename_custom_preset`
- `delete_custom_preset`
- `deleting_active_preset_falls_back_to_flat_and_applies`
- `e_opens_equalizer_prefilled`
- `presets_persist_to_config`
- `equalizer_section_snapshot`

## Done when
The global DoD in `tasks/README.md` is satisfied.
