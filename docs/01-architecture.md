# Architecture

loxia is a Rust Cargo workspace for a terminal Emby music client. The workspace separates pure
application state from network, playback, persistence, rendering, and process-management concerns.

## Workspace crates

| Crate | Responsibility |
| :-- | :-- |
| `loxia-core` | Domain models, configuration schema, actions, effects, events, reducers, state, key maps, paths, queues, and themes |
| `loxia-emby` | Emby HTTP, WebSocket, DTO, query, stream, and retry support |
| `loxia-audio` | The `AudioBackend` abstraction, libmpv backend, mock backend, equalizer, devices, and ReplayGain mapping |
| `loxia-cache` | Cache layout, manifests, downloads, offline index, LRU policy, scrobbles, and session persistence |
| `loxia-tui` | Ratatui layout, views, modals, widgets, styling, rendering, and hit testing |
| `loxia-player` | Binary bootstrap, terminal lifecycle, dispatch, runtime, input, diagnostics, and workers |

`loxia-core` has no I/O dependencies. The other crates depend inward on it rather than making the
core depend on their transport or presentation details.

## Runtime flow

`loxia-player` receives terminal input and worker events, converts them into
`loxia_core::action::Action` values, and sends them through the reducer. Reducers update
`AppState` and emit `Effect` values. Workers execute effects through `loxia-emby`, `loxia-audio`,
and `loxia-cache`, then return `Event` values to the runtime. `loxia-tui` renders the resulting
state and reports mouse hit targets as actions.

This loop keeps state transitions deterministic and makes reducer tests independent of terminals,
network services, audio devices, and filesystems.

## Module boundaries

The public module roots are documented in each crate's `lib.rs`. `loxia-core` groups code by
application concern: `config`, `keymap`, `model`, `queue`, `reducer`, `state`, and supporting
action/effect/event modules. `loxia-audio` groups libmpv integration under `mpv`; `loxia-emby`
groups endpoint code under `endpoints`; and `loxia-tui` groups screens under `views`, overlays under
`modals`, and reusable presentation components under `widgets`.

See [`02-data-model.md`](02-data-model.md) for the values that cross these boundaries and
[`04-state-and-input.md`](04-state-and-input.md) for the state-transition loop.
