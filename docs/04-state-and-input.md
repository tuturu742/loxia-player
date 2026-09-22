# State, actions, and input

`loxia-core` models application behaviour as pure state transitions.

## Actions, events, and effects

`action` defines user and application intents. Reducers in `reducer` consume actions and events,
update `AppState`, and produce effects. `effect` describes I/O work for the runtime; `event`
describes completed work and external updates.

This separation keeps navigation, queue operations, modal behaviour, player state, settings, and
connectivity testable without a terminal, network, audio device, or filesystem.

## Keymaps

The `keymap` module parses key chords, resolves bindings, validates conflicts, and supplies the
default keymap. UI text obtains bindings through `KeyMap::hint_for(ActionId)` rather than embedding
literal keys, so remapped actions remain visible in hints and help.

Input handling in `loxia-player` converts terminal events into keymap lookups and actions.
`loxia-tui` may use hit testing for mouse interaction, but it also emits the same action layer.

## Queue and playback state

Queue transformations live in `loxia-core::queue`; reducer code decides when playback effects,
network requests, cache operations, and UI updates occur. Audio worker events update player state
without making libmpv types part of the core state model.

See [`05-audio-engine.md`](05-audio-engine.md) for the audio boundary and
[`07-ui-spec.md`](07-ui-spec.md) for presentation.
