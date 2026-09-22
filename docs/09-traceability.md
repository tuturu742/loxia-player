# Implementation map

This map routes system responsibilities to their current implementation areas.

| Responsibility | Primary location | Related reference |
|---|---|---|
| Domain state, actions, reducers, effects | `crates/loxia-core/src` | [`04-state-and-input.md`](04-state-and-input.md) |
| Configuration, paths, themes | `crates/loxia-core/src/config`, `paths.rs`, `theme.rs` | [`02-data-model.md`](02-data-model.md) |
| Emby REST, WebSocket, stream URLs | `crates/loxia-emby/src` | [`03-emby-api.md`](03-emby-api.md) |
| Playback and libmpv integration | `crates/loxia-audio/src` | [`05-audio-engine.md`](05-audio-engine.md) |
| Cache, downloads, offline index, session data | `crates/loxia-cache/src` | [`06-cache-and-offline.md`](06-cache-and-offline.md) |
| Rendering, widgets, views, modals | `crates/loxia-tui/src` | [`07-ui-spec.md`](07-ui-spec.md) |
| Runtime, dispatch, terminal, workers | `crates/loxia-player/src` | [`01-architecture.md`](01-architecture.md) |
| Automated checks | `justfile`, `.github/workflows/ci.yml`, crate tests | [`10-testing-and-ci.md`](10-testing-and-ci.md) |
| Dependency policy | workspace manifests and `deny.toml` | [`13-dependencies.md`](13-dependencies.md) |

The source tree and tests remain authoritative when this map and implementation differ. Behavioural
documentation corrections are recorded in [`12-decisions.md`](12-decisions.md).
