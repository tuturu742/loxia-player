# 01-02 · Config defaults and validation

**Phase:** 01 — Config · **Agent:** A · **Size:** M
**Prerequisites:** `01-01`
**Reference:** `docs/02-data-model.md` §8, `docs/04-state-and-input.md` §7

## Goal
Add non-failing validation that returns warnings instead of errors, plus the schema-migration hook.
A malformed config must never prevent the app from starting.

## Files
- `crates/loxia-core/src/config/mod.rs`
- `crates/loxia-core/src/config/migrate.rs`

## Specification

```
pub fn validate(cfg: &mut Config) -> Vec<ConfigWarning>
```
It **mutates** `cfg` to a usable state and reports what it changed. It never returns `Err`.

```
pub struct ConfigWarning {
    pub field: String,      // dotted path, e.g. "ui.theme"
    pub message: String,    // user-facing, one sentence
    pub severity: Severity, // Info | Warning
}
```

Rules, each producing one warning when it fires:

| Check | Action taken |
| :-- | :-- |
| `active_server` names no entry in `servers` | If exactly one server exists, select it. Otherwise leave empty; the UI will prompt. |
| Duplicate `servers[].id` | Keep the first, drop the rest. |
| `servers[].url` is empty or not `http(s)` | Warning; leave as-is — the connect attempt reports the real error. |
| `servers[].device_id` empty | Generate a UUID v4, set it, and mark the config dirty so it is persisted. |
| `custom_headers` contains a reserved name (`authorization`, `x-emby-authorization`, `host`, `content-length`, case-insensitive) | Remove that header, warn. |
| `ui.theme` is not one of the seven built-ins | Reset to `default_terminal`. |
| `sorting.profiles` contains a profile with more than 4 rules | Truncate to 4. |
| `sorting.default_queue_profile` names no profile | Reset to the first profile, or `None` if the list is empty. |
| `cache.rolling_max_gb <= 0.0` | Reset to 5.0. |
| `equalizer.custom_presets` has a name colliding with a factory preset | Rename to `<name> (custom)`. |
| `equalizer` gains outside ±12 dB | Clamp. |
| `logging.level` unparseable | Reset to `info`. |

**Keybinding validation is not done here at all** — neither conflicts nor unparseable binding
strings. Both need the binding-string parser and the resolved `KeyMap`, which don't exist until
`03-04`/`03-05`, and `validate()` must not reach forward into a module that isn't built yet.
`03-05`'s `KeyMap::from_config` is the single place that both parses each `config.keybindings`
value (dropping and warning on a bad one) and detects conflicts (last-binding-wins, warning) — its
warnings join the same `Vec<ConfigWarning>` this task defines. Do not add a
`keybindings` row to this task's validation table.

`migrate.rs`:
```
pub fn migrate(raw: &mut toml::Value) -> Vec<ConfigWarning>
```
Runs **before** deserialization, reading `schema_version`. Version 1 is current, so the function is
a no-op that stamps `schema_version = 1` when absent. It exists now so the first breaking change has
somewhere to go. A `schema_version` **greater** than the current version warns loudly ("config was
written by a newer loxia") and proceeds — downgrading must not wipe a user's settings.

## Acceptance
Tests in `config/mod.rs`:
- `validate_never_errors` (proptest) — arbitrary `toml::Value` deserialized leniently, then
  validated, always yields a usable `Config`.
- `unknown_theme_falls_back_with_warning`
- `reserved_custom_header_is_stripped`
- `duplicate_server_ids_deduped_keeping_first`
- `sort_profile_truncated_to_four_rules`
- `empty_device_id_is_generated_and_stable` — validating twice does not change the generated id.
- `future_schema_version_warns_but_proceeds`

(`unparseable_keybinding_is_dropped_with_warning` lives on task `03-05`, not here — see the note
above.)

## Done when
The global DoD in `tasks/README.md` is satisfied.
