# 10-09 · Sort profile modal

**Phase:** 10 — Polish · **Agent:** D · **Size:** S
**Prerequisites:** `06-04`, `03-07`
**Reference:** `docs/02-data-model.md` §8

## Goal
The `o` modal for picking a sort profile and applying it to the queue or the focused column.

## Files
- `crates/loxia-tui/src/modals/sort_profile.rs`

## Specification

```
┌─ SORT PROFILE ────────────────────────────────────────────────────────┐
│  Apply to: (•) Active Queue   ( ) Current Column                       │
│                                                                       │
│  ▶ chronological_discog                                               │
│      album_artist ↑ · year ↑ · album ↑ · track_number ↑               │
│    release_chronology                                                 │
│      year ↓ · album ↑ · track_number ↑                                │
│    name_only                                                          │
│      name ↑                                                           │
│                                                                       │
│  [ Enter ] Apply    [ e ] Edit profiles    [ Esc ] Cancel             │
└───────────────────────────────────────────────────────────────────────┘
```

Each profile shows its name and, beneath in `Dim`, its rule chain with direction arrows. Showing the
rules inline is what makes the profile names meaningful — a list of bare names tells a user nothing
about what they do.

**Target.** A radio pair at the top: the active queue, or the focused browse column. Defaults to the
queue when something is playing, otherwise to the column.

**`Enter`** emits `Action::Queue(ApplySortProfile)` for the queue target, or a column sort for the
column target, and closes. Applying to the queue clears shuffle (task `06-04`).

**`e`** closes this modal and opens Settings → Sorting (task `11-04`). Editing profiles is a
configuration activity and does not belong in a quick-apply modal.

The currently active profile is marked `•`. When no profile is active, the marker is absent and the
footer notes `no profile active`.

**Empty profile list** — possible if a user deletes them all — shows
`no sort profiles — press {e} to create one`.

## Acceptance
- `profiles_show_rule_chain_with_directions`
- `target_defaults_to_queue_when_playing`
- `target_defaults_to_column_when_stopped`
- `enter_applies_to_selected_target`
- `applying_to_queue_clears_shuffle`
- `active_profile_marked`
- `e_opens_settings_sorting`
- `empty_profile_list_state`
- `sort_profile_snapshot`, `_empty`

## Done when
The global DoD in `tasks/README.md` is satisfied.
