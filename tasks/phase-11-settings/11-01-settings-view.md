# 11-01 · Settings view

**Phase:** 11 — Settings · **Agent:** A · **Size:** L
**Prerequisites:** `10-13`
**Reference:** `docs/07-ui-spec.md` §9, `docs/02-data-model.md` §8

## Goal
A sectioned settings form covering every configuration field, so the app is fully configurable
without hand-editing `config.toml`.

## Files
- `crates/loxia-tui/src/views/settings.rs`
- `crates/loxia-core/src/reducer/settings.rs`

## Specification

**Two panes:** a section list on the left (16 cells), the focused section's controls on the right.
Sections: Servers, Audio, Cache, Transcode, Interface, Sorting, Equalizer, Keybindings, About.

**Typed controls**, each a row with a label, the current value, and a `Dim` one-line description:
```
pub enum Control {
    Toggle { get, set },
    Select { options: Vec<String>, get, set },
    Slider { min, max, step, unit, get, set },
    Text   { get, set, secret: bool },
    Number { min, max, get, set },
    Action { label, action: ActionId },     // e.g. "Clear cache", "Test connection"
}
```
Build the row list with `strum`'s enum iteration where the value is an enum, so adding a variant
cannot leave a settings row behind.

**Every field in `Config`** must appear. A test enumerates the config's fields and asserts each has
a control — that is how this stays true as the schema grows.

**Editing.** `↑`/`↓` move rows, `←`/`→` or `Space` change toggles and selects, `Enter` enters text
edit for a `Text` row, `Esc` leaves it. Changes apply **live** where possible — theme, mouse,
notifications, lyrics — and emit `Action::System(ConfigChanged)`.

**Persistence is debounced 1 second** (task `03-08`'s tick responsibilities), so dragging a slider
does not write the file thirty times.

**Secret fields** (`access_token`) render as `••••••••` with a `[Ctrl+E] reveal` hint, and are never
included in a snapshot test.

**Validation feedback.** A value rejected by `config::validate` shows inline in `Error` style beneath
the row and the previous value is retained. The row is not left in an invalid state.

**Warnings.** `AppState.config_warnings` renders at the top of the relevant section, and the
Keybindings section shows the conflict badge — the second of the three required conflict surfaces
(`docs/04-state-and-input.md` §7).

Sections 11-02 through 11-07 fill in the specialised editors; this task provides the frame and the
simple controls.

## Acceptance
- `every_config_field_has_a_control` — a reflective or hand-maintained exhaustive test over
  `Config`'s fields.
- `enum_valued_selects_cover_all_variants`
- `live_apply_for_theme_mouse_notifications_lyrics`
- `persistence_debounced_one_second`
- `secret_field_masked_by_default`
- `secret_field_reveal_toggle`
- `secret_never_in_snapshot` — the snapshot test asserts the token string is absent.
- `invalid_value_shows_inline_error_and_retains_previous`
- `config_warnings_render_in_section`
- `keybinding_conflict_badge_shown`
- `settings_snapshot_per_section` — one per section.

## Done when
The global DoD in `tasks/README.md` is satisfied.
