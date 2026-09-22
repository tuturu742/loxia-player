# Source traceability

This map routes maintainers from a system concern to its primary implementation area.

| Concern | Primary location |
| --- | --- |
| Configuration, paths, themes, domain model | `crates/loxia-core/src` |
| Actions, events, effects, state, reducers | `crates/loxia-core/src/{action,event,effect,state,reducer}` |
| Key parsing, defaults, resolution, validation | `crates/loxia-core/src/keymap` |
| Queue ordering, shuffle, and appears-on rules | `crates/loxia-core/src/queue` |
| Emby client and endpoints | `crates/loxia-emby/src` |
| Audio abstraction, mock, mpv integration, EQ | `crates/loxia-audio/src` |
| Cache, downloads, offline index, session, scrobbles | `crates/loxia-cache/src` |
| Rendering, views, widgets, and modals | `crates/loxia-tui/src` |
| Bootstrap, dispatch, runtime, terminal, workers | `crates/loxia-player/src` |
| Bundled themes and EQ presets | `assets/themes` and `assets/eq_presets.toml` |
| CI checks | `.github/workflows/ci.yml` |

The module declarations in crate roots remain the authoritative public maps. This document routes
readers to code; it does not replace source-level API documentation.
