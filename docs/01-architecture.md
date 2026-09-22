# Architecture

loxia is a Rust Cargo workspace for an Emby music client with a terminal user
interface. The workspace separates pure domain logic from network, audio,
storage, presentation, and process-boundary code.

## Crates

| Crate | Responsibility |
|---|---|
| `loxia-core` | Zero-I/O domain types, configuration, state, reducers, actions, effects, keymaps, themes, and queue logic. |
| `loxia-emby` | Async Emby REST and WebSocket client, DTO conversion, retrying, queries, and stream URLs. |
| `loxia-audio` | `AudioBackend` implementations, including the libmpv-backed engine and deterministic mock. |
| `loxia-cache` | Cache layout, downloads, manifests, LRU data, offline indexes, scrobbles, and sessions. |
| `loxia-tui` | ratatui rendering, views, widgets, hit testing, and modal presentation. |
| `loxia-player` | The executable, terminal lifecycle, event loop, dispatch, and runtime workers. |

## Direction of dependencies

`loxia-core` is the domain boundary and has no I/O. The adapter crates depend
on it; it does not depend on them. `loxia-player` composes the adapters and
drives the application. `loxia-tui` renders `loxia-core` state and does not
own network or audio connections.

The normal flow is:

```text
terminal / worker event
        ↓
input and dispatch
        ↓
Action → loxia-core reducer → AppState + Effect
        ↓
runtime worker
        ↓
Emby, cache, audio, or platform adapter
        ↓
Event → dispatch
```

## Source layout

Public module lists in each crate's `src/lib.rs` are the authoritative map of
that crate. The following documents describe the principal boundaries:

- [`02-data-model.md`](02-data-model.md) describes persistent and domain data.
- [`03-emby-api.md`](03-emby-api.md) describes the network adapter.
- [`04-state-and-input.md`](04-state-and-input.md) describes the reducer loop.
- [`05-audio-engine.md`](05-audio-engine.md) describes playback.
- [`06-cache-and-offline.md`](06-cache-and-offline.md) describes persistence.
- [`07-ui-spec.md`](07-ui-spec.md) describes presentation.
