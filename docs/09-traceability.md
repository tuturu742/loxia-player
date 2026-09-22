# Traceability

This map routes a contributor from a system concern to the owning implementation area.

| Concern | Primary location |
| :-- | :-- |
| Actions, effects, events, reducers, and application state | `crates/loxia-core/src` |
| Configuration schema, migration, validation, and paths | `crates/loxia-core/src/config`, `crates/loxia-core/src/paths.rs` |
| Key chords and default bindings | `crates/loxia-core/src/keymap` |
| Queue ordering, shuffle, and appears-on logic | `crates/loxia-core/src/queue` |
| Emby authentication, requests, DTOs, streams, and retries | `crates/loxia-emby/src` |
| Playback and libmpv control | `crates/loxia-audio/src` |
| Cache, downloads, offline index, and session data | `crates/loxia-cache/src` |
| Terminal rendering, views, widgets, and modals | `crates/loxia-tui/src` |
| Process bootstrap, event dispatch, workers, and terminal lifecycle | `crates/loxia-player/src` |
| Bundled themes | `assets/themes` |
| Factory equalizer presets | `assets/eq_presets.toml` |
| CI checks | `.github/workflows/ci.yml` |
| Developer commands | `justfile` |

The reference documents use the same ownership boundaries: architecture is in
[`01-architecture.md`](01-architecture.md), external transport is in
[`03-emby-api.md`](03-emby-api.md), and dependency policy is in
[`13-dependencies.md`](13-dependencies.md).
