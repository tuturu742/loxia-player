# 10-03 · Help modal

**Phase:** 10 — Polish · **Agent:** D · **Size:** M
**Prerequisites:** `03-05`, `03-07`
**Reference:** `design_overview` §3.3, `docs/04-state-and-input.md` §6

## Goal
The `?` cheat sheet, generated entirely from the live keymap so it can never drift from what the
keys actually do.

## Files
- `crates/loxia-tui/src/modals/help.rs`

## Specification

**Every row is generated from `KeyMap`.** There is no hardcoded key table in this file. That is the
whole point: a user who remaps `d` sees their binding here, and a developer who adds an action
cannot forget to document it.

**Grouping.** Actions carry a `HelpCategory` (add it to `ActionId`'s metadata in `keymap/mod.rs` if
it is not there): `Navigation`, `Playback`, `Queue`, `Selection`, `Audio`, `Items`, `Views`,
`System`. Render as three columns of category blocks, matching `design_overview` §3.3:

```
┌─ LOXIA KEYBOARD CHEAT SHEET ────────────────────────────────────────────┐
│  NAVIGATION              PLAYBACK & QUEUE           AUDIO               │
│  ──────────              ────────────────           ─────               │
│  h / ←   Column Left     Space   Play / Pause       e  Equalizer        │
│  ...                                                                    │
│  [ Press ? or Esc to close ]                                            │
└─────────────────────────────────────────────────────────────────────────┘
```

Column count drops to two below 120 columns and one below 90, with the content scrolling.

**Context awareness.** The title shows the current context — `[Context: Miller Columns]`,
`[Context: Now Playing]`. When a modal is open, `?` shows **that modal's** bindings instead. A help
key that shows global bindings while the user is stuck in the equalizer is not help.

**Unbound actions** are listed with `—` in the key column, so a user can see the action exists and
go bind it.

**Conflicts.** When `AppState.config_warnings` contains keybinding conflicts, the header shows
`⚠ n conflicts — see Settings` in `Warning` style. This is the third of the three conflict
notification surfaces required by `docs/04-state-and-input.md` §7.

Scrolling with `j`/`k` and `Ctrl+U`/`Ctrl+D`; the scroll position resets on close.

## Acceptance
- `help_rows_generated_from_keymap` — remap three actions and assert all three rendered rows change.
- `no_hardcoded_key_strings` — a grep test asserting the file contains no bracketed key literals.
- `every_action_id_appears_exactly_once`
- `unbound_actions_show_dash`
- `context_shown_in_title` — table test over three contexts.
- `modal_context_shows_modal_bindings`
- `conflict_warning_shown_in_header`
- `column_count_by_width` — 130, 100, 80.
- `scrolls_when_content_overflows`
- `scroll_resets_on_close`
- `help_snapshot_wide`, `_narrow`, `_with_conflicts`, `_modal_context`

## Done when
The global DoD in `tasks/README.md` is satisfied.
