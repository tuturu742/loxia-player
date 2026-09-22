# Architecture

loxia is a Rust Cargo workspace for an Emby music client with a terminal user interface.

## Workspace layers

| Crate | Responsibility |
|---|---|
| `loxia-core` | Pure domain models, configuration, state, reducers, actions, effects, keymaps, queues, and themes. |
| `loxia-emby` | Async Emby HTTP and WebSocket client, endpoint wrappers, DTO conversion, retries, and stream URL construction. |
| `loxia-audio` | `AudioBackend` abstraction, libmpv-backed playback, equalizer, ReplayGain, device handling, and deterministic mocks. |
| `loxia-cache` | Cache layout, manifests, downloads, LRU management, offline indexes, scrobble storage, and session storage. |
| `loxia-tui` | ratatui rendering, views, widgets, hit testing, and modal presentation. |
| `loxia-player` | Executable bootstrap, terminal lifecycle, dispatch, runtime loop, and background workers. |

## Boundaries

`loxia-core` performs no I/O. It expresses work through actions, events, and effects. `loxia-player`
owns I/O and dispatches effects to the Emby, audio, cache, notification, MPRIS, and download workers.

`loxia-tui` renders `loxia-core` state and emits actions. It does not own networking, playback, or
persistent storage. `loxia-audio`, `loxia-emby`, and `loxia-cache` depend on `loxia-core` where they
share domain types, but `loxia-core` does not depend on them.

## Runtime flow

1. Input is translated into an `ActionId`.
2. Reducers update `AppState` and emit effects.
3. Workers execute effects outside `loxia-core`.
4. Worker results return as events.
5. The runtime applies events, renders the updated state, and continues polling input.

The detailed state and input contract is in [`04-state-and-input.md`](04-state-and-input.md).
