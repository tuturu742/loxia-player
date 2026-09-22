# Data model and configuration

`loxia-core` owns the domain model used by the rest of the workspace. Its `model` module includes
`audio_meta`, `ids`, `image`, `item`, `lyrics`, and `playback`; in particular,
`model::image` and `model::playback` are current public model modules.

## Application state

`state::AppState` groups navigation, player, queue, search, favourites, settings, modal, and toast
state. Reducers update that state in response to `Action` and `Event` values and return `Effect`
values for work performed outside `loxia-core`.

Identifiers and server-facing items stay in the model layer so the UI, cache, audio, and Emby
crates share the same values rather than translating private copies.

## Configuration

Configuration schemas, defaults, migration, validation, and file handling live in
`loxia_core::config`. The configuration covers server profiles, interface preferences, keybindings,
audio and equalizer settings, caching, sorting, transcoding, and session-related preferences.

The per-platform configuration path comes from `loxia_core::paths`. A `config.toml` at the
repository root is deliberately ignored and is not a supported user configuration location; the
root-level ignore rule prevents local credentials from being committed accidentally.

## Themes

The bundled theme names match the TOML files in `assets/themes/`:

- `amber_crt`
- `cyberpunk_neon`
- `darcula`
- `default_terminal`
- `far_blue`
- `green_crt`
- `oled_black`

`amber_crt` and `green_crt` set `ascii_only = true`. The other bundled themes set it to `false`.
Theme parsing and role definitions live in `loxia_core::theme`; TUI style conversion lives in
`loxia-tui`.

## Persistent data

The cache crate owns on-disk cache data, downloads, offline indexes, session data, and deferred
scrobbles. Configuration is distinct from this operational data. `06-cache-and-offline.md`
describes the storage responsibilities and failure boundaries.
