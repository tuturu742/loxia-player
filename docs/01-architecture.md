# Architecture

loxia is a Rust Cargo workspace for an Emby music client with a terminal user interface. The
workspace keeps pure application state separate from I/O, rendering, and process integration.

## Workspace

| Crate | Directory | Responsibility |
| --- | --- | --- |
| `loxia-core` | `crates/loxia-core` | Domain types, configuration, actions, effects, state, reducers, queue logic, key maps, paths, and themes |
| `loxia-emby` | `crates/loxia-emby` | Emby HTTP, WebSocket, DTO, query, authentication, streaming, and endpoint support |
| `loxia-audio` | `crates/loxia-audio` | Audio-backend abstraction, libmpv implementation, mock backend, equalizer, ReplayGain, and device helpers |
| `loxia-cache` | `crates/loxia-cache` | Cache layout, manifests, LRU accounting, downloads, offline index, sessions, and scrobble storage |
| `loxia-tui` | `crates/loxia-tui` | Ratatui rendering, views, widgets, hit testing, layouts, styles, and modals |
| `loxia-player` | `crates/loxia-player` | Binary entry point, bootstrap, terminal lifecycle, dispatch loop, runtime, diagnostics, and workers |

The binary package and executable crate are both `loxia-player`, located at
`crates/loxia-player`. The workspace does not contain a `crates/loxia` package.

## Dependency direction

`loxia-core` is the lowest application layer and has no I/O dependencies. The integration crates
depend on `loxia-core`; `loxia-player` assembles the integrations and drives the application.
`loxia-tui` renders state and produces actions without owning network or audio backends.

```text
loxia-player
 ├── loxia-tui
 ├── loxia-audio
 ├── loxia-cache
 ├── loxia-emby
 └── loxia-core
```

This direction keeps reducers deterministic and lets tests exercise application behaviour without a
terminal, server, cache directory, or libmpv installation.

## Source layout

The public module declarations in each library crate's `src/lib.rs` are the authoritative module
map. The binary crate has `src/main.rs` and uses `bootstrap`, `dispatch`, `doctor`, `input`,
`runtime`, `terminal`, and `workers` modules.

The major `loxia-core` areas are:

- `action`, `event`, and `effect` define the messages crossing the application boundary.
- `state` holds the rendered application state.
- `reducer` turns actions and events into state changes and effects.
- `config`, `keymap`, `model`, `queue`, `theme`, and `paths` provide shared pure services.

See `04-state-and-input.md` for the runtime flow and `09-traceability.md` when locating a feature.
