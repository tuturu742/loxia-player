# Data model and configuration

`loxia-core` owns the domain model used by the rest of the workspace. Its `model` module contains
stable identifiers, library items, images, lyrics, playback metadata, audio metadata, and audio
devices. State modules hold the user-visible application state; reducers are the only place that
changes it in response to actions.

## Configuration

Configuration is represented by `loxia_core::config` schema types. The configuration covers server
profiles, interface preferences, keybindings, audio, cache, transcode, sorting, equalizer presets,
and session-related settings. Validation and migration live alongside the schema in
`config::schema` and `config::migrate`.

The application resolves per-user configuration and data directories through
`loxia_core::paths`. It does not write a repository-root `config.toml`.

## State

`AppState` groups focused state for navigation, player, queue, search, favourites, settings,
modals, toasts, and connectivity. The player state records playback status and metadata; the queue
state records ordering and selection; navigation state records the active view and focus.

Configuration is data, not UI policy: the renderer reads it from state and the runtime persists
changes through effects.

## Queue and playback

`loxia_core::queue` provides queue manipulation, shuffle, sorting, and “appears on” support.
Playback data includes `PlayStatus`, seek targets, ReplayGain information, and applied gain
information. Audio-specific commands remain at the `loxia-audio` boundary.

## Themes

Theme parsing and role definitions live in `loxia_core::theme`. The bundled theme names are:

- `amber_crt`
- `cyberpunk_neon`
- `darcula`
- `default_terminal`
- `far_blue`
- `green_crt`
- `oled_black`

The CRT themes set `ascii_only = true`; rendering uses that setting when choosing glyphs.

See [`04-state-and-input.md`](04-state-and-input.md) for state transitions and
[`06-cache-and-offline.md`](06-cache-and-offline.md) for persisted cache data.
