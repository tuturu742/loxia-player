# State, actions, and input

The application uses a reducer-driven state model. Input and worker results enter as actions or
events, reducers update `AppState`, and reducers emit effects for work outside the pure core.

## Message flow

```text
terminal input / worker result
            ↓
     Action or Event
            ↓
       core reducers
            ↓
 updated AppState + Effects
            ↓
 player workers and integrations
```

`loxia-player` owns dispatch and runtime coordination. Its input layer maps terminal input through
the configured keymap; workers perform network, audio, cache, download, MPRIS, and notification
work. `loxia-tui` reads state and sends actions rather than mutating state directly.

## Key bindings

`loxia_core::keymap` parses, resolves, validates, and supplies default bindings. Actions use
`ActionId` values, not display strings. UI hints resolve through the keymap so custom bindings and
the help modal remain consistent.

Bindings are contextual where the active view or modal requires it. Validation reports conflicts
rather than silently picking an unrelated binding.

## Reducers

The reducer modules split responsibilities by area: connectivity, modal, navigation, player,
queue, session, and settings. Reducers remain free of terminal, filesystem, HTTP, and audio-engine
I/O. Effects express those boundary operations explicitly.

This split makes scenario tests deterministic and keeps UI rendering independent of asynchronous
worker timing.
