# Data model and configuration

`loxia-core` owns domain data. Its models identify media, describe playback and
audio metadata, represent lyrics and images, and provide the state consumed by
reducers and the TUI.

## Configuration

The configuration schema lives in `loxia_core::config::schema`; loading,
validation, defaults, and migration live under `loxia_core::config`.
Configuration is TOML and is resolved through `loxia_core::paths`.

The runtime keeps user data outside the repository. `loxia-core` resolves
platform-appropriate configuration, cache, and data directories through the
`dirs`-based path helpers. A repository-root `config.toml` is deliberately
ignored and is not a supported user configuration location.

Configuration includes server profiles, interface and theme choices, keymap
overrides, audio settings, cache settings, transcode settings, sorting
profiles, equalizer presets, and session-related preferences. Schema types and
their serde names are the authoritative list of keys.

## Media and playback data

The principal model modules are:

| Module | Data |
|---|---|
| `model::ids` | Strongly typed Emby and local identifiers. |
| `model::item` | Music-library items and their relationships. |
| `model::audio_meta` | Codec, format, device, ReplayGain, and related audio metadata. |
| `model::playback` | Playback and stream-facing domain data. |
| `model::lyrics` | Plain and timed lyric representations. |
| `model::image` | Image references and image requests. |

Application state is divided into focused modules under `loxia_core::state`,
including navigation, player, queue, search, favourites, settings, modals, and
toasts. Reducers update that state as described in
[`04-state-and-input.md`](04-state-and-input.md).

## Themes

Theme definitions are TOML files in `assets/themes`. The shipped theme names
are:

- `amber_crt`
- `cyberpunk_neon`
- `darcula`
- `default_terminal`
- `far_blue`
- `green_crt`
- `oled_black`

`loxia_core::theme` parses and validates their named colour roles. CRT themes
set `ascii_only = true`; renderers use that signal when selecting terminal
glyphs.
