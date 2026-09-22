# State, actions, and input

The application state machine lives in `loxia-core`. It separates user intent, state transitions,
side effects, and external results.

## Actions, effects, and events

`Action` represents an input or requested operation. Reducers consume an action and current
`AppState`, update state, and emit `Effect` values for work outside the core. Workers execute
effects and return `Event` values. The runtime feeds those events back through the same reducer
path.

Reducer modules are grouped by concern: `connectivity`, `modal`, `nav`, `player`, `queue`,
`session`, and `settings`.

## Key maps

`ActionId` identifies bindable actions. `KeyMap` parses configured chords, resolves contexts,
detects conflicts, and exposes bindings through `hint_for(ActionId)`. UI text obtains key hints
from this method instead of embedding literal keys, so customised bindings appear consistently.

Default bindings live in `loxia_core::keymap::defaults`. Parsing, resolution, and validation live in
`keymap::parse`, `keymap::resolve`, and `keymap::validate`. The default-binding snapshot verifies
that the documented binding table and the implementation remain aligned.

## Input routing

`loxia-player::input` translates terminal events into actions. `loxia-tui` contributes hit targets
for mouse interaction but does not mutate application state directly. Modal and focused-view
contexts determine which configured chord resolves to an action.

## Queue and playback state

Queue operations are represented in `loxia_core::queue` and reduced by `reducer::queue`.
Playback state is represented by `state::player` and reduced by `reducer::player`. The player and
queue views render their action hints through the current key map; a configured default binding
therefore remains visible in empty-queue and now-playing UI states.

See [`07-ui-spec.md`](07-ui-spec.md) for the presentation layer and
[`12-decisions.md`](12-decisions.md) §9 for documented behaviour changes.
