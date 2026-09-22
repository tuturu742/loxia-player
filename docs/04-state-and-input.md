# State, actions, and input

`loxia-core` implements a reducer-driven application model. The runtime maps
external input into actions, reducers transform state and emit effects, and
workers translate effects into I/O before returning events.

## Core modules

| Module | Responsibility |
|---|---|
| `action` | User and system intents identified by `ActionId`. |
| `event` | Results arriving from input, workers, and runtime services. |
| `effect` | I/O requests emitted by reducers. |
| `state` | The complete application state and its focused substates. |
| `reducer` | Pure state transitions for navigation, playback, queue, settings, modals, connectivity, and sessions. |
| `keymap` | Key chord parsing, resolution, validation, and defaults. |
| `queue` | Queue construction, sorting, shuffling, and appears-on behaviour. |

## Input

`loxia-player::input` converts terminal input to key chords and uses
`loxia_core::keymap::KeyMap` to resolve an `ActionId`. The UI does not hardcode
key text: it obtains displayed shortcuts through `KeyMap::hint_for`.

Default bindings live in `loxia_core::keymap::defaults`. The snapshot named
`defaults_match_documented_table` protects the correspondence between the
defaults and the keymap documentation embedded in the source.

## Effects and workers

Effects are declarations, not I/O. `loxia-player` dispatches them to workers
for audio, network, cache, downloads, MPRIS, and notifications. Workers return
events, which re-enter the same dispatch path. This keeps reducers testable
without terminals, sockets, filesystems, or audio devices.

`AppState` remains suitable for deterministic reducer tests. Test fixtures and
scenarios live in `loxia_core::test_support`.
