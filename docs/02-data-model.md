# Data model and configuration

`loxia-core` owns the data that represents the application independently of I/O and rendering.

## Domain model

The `model` module contains strongly typed identifiers and media values for artists, albums, tracks,
playlists, images, lyrics, audio devices, audio formats, and playback metadata. `ItemId` and related
identifier types avoid passing unclassified Emby strings through the application.

`state` holds the current application state. Its modules cover navigation, player state, queues,
search, favourites, settings, modals, and toast notifications. Reducers own transitions between
these values.

## Configuration

`loxia_core::config` parses and validates the user configuration. The schema includes server
profiles, interface preferences, cache settings, audio settings, transcoding settings, key bindings,
sorting profiles, equalizer presets, and ReplayGain settings. Configuration migration lives in
`config::migrate`; validation and defaults live in `config::schema`.

The application resolves platform-specific configuration and data directories through
`loxia_core::paths`. The repository does not contain a user configuration file. Local test
configuration belongs in the platform location selected by that module.

## Themes

`loxia_core::theme` loads theme definitions from the embedded theme assets. The bundled theme names
are:

- `amber_crt`
- `cyberpunk_neon`
- `darcula`
- `default_terminal`
- `far_blue`
- `green_crt`
- `oled_black`

Theme roles include foreground, background, borders, selection, status, and progress colours.
`default_terminal` uses terminal palette values and preserves a reset background.

## Persistence

`loxia-cache` persists cache metadata, downloads, offline browsing data, queued scrobbles, and
session data. The persisted representations are implementation details of that crate; callers use
its layout, manifest, session, and index APIs rather than constructing filesystem paths themselves.

See [`06-cache-and-offline.md`](06-cache-and-offline.md) for cache ownership and offline behaviour.
