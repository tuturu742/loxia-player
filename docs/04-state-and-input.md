# State, actions, and input

loxia uses a reducer architecture. Terminal input and worker results become events; events produce
actions; reducers apply actions to `AppState` and return effects for the runtime.

## Core types

- `ActionId` identifies a user-visible command and is the keymap's stable action identifier.
- `Action` carries an invoked command and its data.
- `Effect` requests I/O or integration work outside `loxia-core`.
- `Event` represents runtime, terminal, and worker input.
- Reducers in `loxia_core::reducer` update state and emit effects.

Reducers are divided by concern: connectivity, modal, navigation, player, queue, session, and
settings. This keeps policy deterministic and independently testable.

## Keymaps

`loxia_core::keymap` parses, resolves, validates, and supplies default keybindings. A key chord
resolves to an `ActionId` in the current context. UI text obtains shortcuts through
`KeyMap::hint_for(ActionId)` instead of embedding key literals, so custom bindings appear
consistently.

The keymap editor and help modal display the same action identifiers and resolved bindings used by
input dispatch.

## Runtime ownership

`loxia-player` owns terminal event collection and dispatch. It sends actions into the core state
machine and executes the resulting effects through audio, network, cache, notification, MPRIS, and
download workers. A worker reports its result as an event; it does not alter `AppState` itself.

See [`01-architecture.md`](01-architecture.md) for crate boundaries and
[`07-ui-spec.md`](07-ui-spec.md) for how state is presented.
