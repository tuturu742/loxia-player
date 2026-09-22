# Data model and configuration

`loxia-core` contains the domain data shared by every workspace crate.

## Domain data

The `model` module defines identifiers, library items, artwork, audio metadata, playback metadata,
and lyrics. Queue ordering and queue transformations live in `queue`; discography grouping lives in
`discography`.

The application state is split into focused submodules under `state`, including navigation, player,
queue, search, favourites, settings, modals, and toasts. Reducers update those substates through
the action and event types described in [`04-state-and-input.md`](04-state-and-input.md).

## Configuration

The `config` module owns the configuration schema, defaults, validation, migration, and file I/O
boundary types. Configuration is TOML and uses typed sections rather than unstructured maps.

The audio configuration includes playback and ReplayGain settings. Equalizer presets use ten gains
whose frequencies are exposed as `EQ_BANDS_HZ`. Factory preset data is embedded from
`assets/eq_presets.toml`.

Theme definitions are TOML files in `assets/themes`. The shipped names are:

- `amber_crt`
- `cyberpunk_neon`
- `darcula`
- `default_terminal`
- `far_blue`
- `green_crt`
- `oled_black`

## Paths

`loxia-core::paths` resolves platform-appropriate configuration, cache, data, and log locations.
Repository-root `config.toml` is not an application configuration location and is ignored to avoid
accidentally committing credentials.

Storage formats and offline data are described in [`06-cache-and-offline.md`](06-cache-and-offline.md).
