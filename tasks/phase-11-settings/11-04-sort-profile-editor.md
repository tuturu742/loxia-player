# 11-04 · Sort profile editor

**Phase:** 11 — Settings · **Agent:** A · **Size:** M
**Prerequisites:** `11-01`, `06-04`
**Reference:** `docs/02-data-model.md` §8

## Goal
Create and edit sort profiles — up to four stacked rules each — from the Sorting settings section.

## Files
- `crates/loxia-tui/src/views/settings.rs` (extend)

## Specification

```
  Default queue profile:  [ chronological_discog        ▼ ]

  Profiles
  ▶ chronological_discog
      1. Album Artist   ↑
      2. Year           ↑
      3. Album          ↑
      4. Track Number   ↑
    release_chronology
      1. Year           ↓
      ...
  [n] New   [r] Rename   [x] Delete   [a] Add rule   [d] Delete rule   [↕] Reorder
```

**Rules.** Each is a `SortField` select and a direction toggle. `a` appends a rule, `d` removes the
focused one, `Ctrl+↑`/`Ctrl+↓` reorder. **The fourth rule disables `a`** with the reason shown
inline: `maximum 4 rules` — the limit is a documented product decision
(`docs/02-data-model.md` §8), not an arbitrary cap, so the UI states it rather than silently
ignoring the key.

**Names** must be unique and non-empty; a duplicate shows `a profile with that name exists`.
Renaming a profile referenced by `default_queue_profile` updates the reference.

**Deleting** the profile currently set as default clears the default to `None` and warns. Deleting
the last profile is allowed — the queue simply has no active profile, which is a legitimate state
and the one the sort modal's empty state (task `10-09`) already handles.

**Live preview.** With a queue loaded, the section shows the first five tracks as they would be
ordered under the focused profile, updating as rules change. Sort rules are abstract; seeing the
result is what makes them comprehensible.

**Apply** is a separate `Action` control — editing a profile does not reorder the user's queue
underneath them.

Changes persist through the standard 1-second debounce.

## Acceptance
- `add_rule_up_to_four`
- `fifth_rule_refused_with_inline_reason`
- `delete_rule`
- `reorder_rules`
- `direction_toggle`
- `duplicate_name_rejected`
- `empty_name_rejected`
- `rename_updates_default_reference`
- `deleting_default_profile_clears_default_and_warns`
- `deleting_last_profile_allowed`
- `live_preview_reflects_rule_changes`
- `editing_does_not_reorder_queue`
- `sorting_section_snapshot`, `_editing`, `_max_rules`

## Done when
The global DoD in `tasks/README.md` is satisfied.
