# 03-05 · Default keymap and conflict validation

**Phase:** 03 — State machine · **Agent:** A · **Size:** M
**Prerequisites:** `03-04`
**Reference:** `docs/04-state-and-input.md` §§5–7, `docs/12-decisions.md` §4

## Goal
Encode the default keymap, resolve chords to actions, and detect conflicts in user configs. The
defaults are conflict-free by construction; the validator exists for what users write.

## Files
- `crates/loxia-core/src/keymap/defaults.rs`, `resolve.rs`, `validate.rs`

## Specification

**`ActionId`** — a `strum`-derived enum with one variant per bindable action, with stable
snake_case `Display`/`FromStr` (`play_pause`, `queue_artist_only`, …). These strings are the config
file's keys, so renaming one is a breaking change requiring a migration.

**`defaults.rs`** — transcribe the tables in `docs/04-state-and-input.md` §6 **exactly**. Every
chord listed, including the aliases (`↓` alongside `j`, `F1` alongside `Alt+1`, `=` alongside `+`).
That document is normative; if you believe a binding is wrong, change the doc in the same PR rather
than diverging.

```
pub struct KeyMap { bindings: BTreeMap<KeyBinding, ActionId>, hints: BTreeMap<ActionId, String> }
impl KeyMap {
    pub fn defaults() -> KeyMap;
    pub fn from_config(overrides: &BTreeMap<String, String>) -> (KeyMap, Vec<ConfigWarning>);
    pub fn hint_for(&self, a: ActionId) -> String;     // e.g. "Ctrl+P"; "unbound" when none
    pub fn binding_for(&self, a: ActionId) -> Option<&KeyBinding>;
    pub fn validate(&self) -> Vec<KeyConflict>;
}
```

**`hint_for` is required by every UI surface** — inspector action lists, modal footers, the help
overlay. Hardcoding a key in UI text is a review failure. When an action has several bindings,
return the first in the table order so hints are stable.

**`resolve.rs`:**
```
pub enum Resolution { Action(ActionId), Pending, None }
pub fn resolve(map: &KeyMap, ctx: InputContext, pending: Option<&KeyChord>, chord: KeyChord) -> Resolution;
```
- `InputContext` = `Normal | TextInput | Modal(ModalKind)`.
- In `TextInput`, only `Esc`, `Enter`, `Tab`, `Shift+Tab`, and the arrows resolve; everything else
  is `None` so the caller routes it to the buffer.
- In `Modal(kind)`, the modal's own table applies, plus `Esc`, `?`, and `Ctrl+C`.
- `Pending` is returned when the chord is a prefix of a two-chord binding (only `g` in the defaults).
- A `pending` chord that forms no valid sequence returns `None` and the caller clears the prefix.

**`validate.rs`:**
```
pub struct KeyConflict { pub context: InputContext, pub binding: KeyBinding, pub actions: Vec<ActionId> }
pub fn validate(map: &KeyMap) -> Vec<KeyConflict>;
```
`from_config` does two things per entry in `config.keybindings`, in this order:
1. **Parse.** A value that fails `parse_binding` is **dropped** (the action keeps its default
   binding) with a `ConfigWarning` naming the action id and the bad string:
   `keybinding for {action} is invalid ("{value}"); using the default`. This is the check
   `01-02`'s `validate()` explicitly does **not** do, since the parser doesn't exist at that point
   in the crate's build-out — this is its one true home.
2. **Apply and detect conflicts**, in config order, **last wins**, emitting one further
   `ConfigWarning` per conflict:
   `key {binding} is bound to both {a} and {b}; using {b}`.

The app must always start. Both parse failures and conflicts are reported, never fatal — see
`docs/12-decisions.md` §4.

## Acceptance
- **`default_keymap_has_no_conflicts`** — `validate(&KeyMap::defaults())` is empty. This is the
  single most important test in the task.
- `default_keymap_covers_every_action_id` — every `ActionId` except those documented as
  intentionally unbound has at least one binding.
- `defaults_match_documented_table` — an `insta` snapshot of the rendered
  `(binding, action)` list, reviewed against `docs/04-state-and-input.md` §6.
- `user_override_replaces_default`
- `unparseable_keybinding_is_dropped_with_warning` — the action keeps its default binding and the
  warning names both the action and the bad string.
- `conflicting_overrides_last_wins_with_warning`
- `conflict_warning_names_both_actions`
- `text_input_context_swallows_printable_keys`
- `modal_context_uses_modal_table`
- `g_prefix_returns_pending_then_resolves`
- `invalid_sequence_after_prefix_returns_none`
- `hint_for_unbound_action_returns_unbound`
- `hint_for_is_stable_across_calls`

## Done when
The global DoD in `tasks/README.md` is satisfied.
