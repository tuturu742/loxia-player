# Architecture

loxia is a Rust Cargo workspace for an Emby music client with a terminal user interface. The
workspace separates pure domain logic from networking, playback, persistence, rendering, and
process orchestration.

## Crates

| Crate | Responsibility |
|---|---|
| `loxia-core` | Zero-I/O domain types, configuration, state, reducers, actions, effects, keymaps, themes, and queue logic. |
| `loxia-emby` | Async Emby REST and WebSocket client, DTO conversion, retries, queries, images, playback reporting, and stream URLs. |
| `loxia-audio` | `AudioBackend`, libmpv playback, equalizer, ReplayGain, gapless playback, and audio devices. |
| `loxia-cache` | Cache layout, manifests, LRU management, downloads, offline indexes, scrobbles, and sessions. |
| `loxia-tui` | ratatui layouts, widgets, views, modals, styling, hit testing, and rendering. |
| `loxia-player` | The executable, terminal setup, event dispatch, runtime, workers, and diagnostics. |

## Runtime flow

`loxia-player` receives terminal and worker events, maps input to `ActionId` values, and passes
actions to `loxia-core` reducers. Reducers update `AppState` and emit `Effect` values. Runtime
workers perform the I/O represented by effects and return events to the dispatcher.

The TUI reads state and the resolved keymap; it does not perform network, cache, or audio I/O.
`loxia-core` similarly contains no terminal, filesystem, networking, or async-runtime dependency.

## Boundaries

- `loxia-core` is the dependency foundation.
- `loxia-emby`, `loxia-audio`, and `loxia-cache` depend on `loxia-core`.
- `loxia-tui` depends on `loxia-core` and renders state without owning application policy.
- `loxia-player` composes the other crates and owns process-level I/O.
- Stream URLs use redacted wrappers where they can reach diagnostics or debug output.
- Workers communicate across the runtime boundary with typed actions, effects, and events rather
  than mutating UI state directly.

See [`02-data-model.md`](02-data-model.md) for the types that cross these boundaries and
[`04-state-and-input.md`](04-state-and-input.md) for reducer flow.
