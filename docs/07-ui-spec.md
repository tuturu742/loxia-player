# Terminal UI

`loxia-tui` renders `loxia-core` state with Ratatui. It contains layouts, styles, text helpers,
hit testing, views, widgets, and modal rendering.

## Views and widgets

The view modules cover favourites, folders, genres, Miller navigation, now playing, playlists,
search, settings, and zen mode. Shared widgets render album art, columns, headers, inspectors,
lyrics, player status, progress, sectioned lists, sidebars, and toasts.

The renderer composes these pieces from current state. Views do not own network requests, audio
engines, or mutable domain state.

## Modals

Modal modules provide confirmation, device selection, equalizer editing, help, keymap editing,
playlist saving, sleep-timer control, and sort-profile editing. Modal state lives in
`loxia-core::state::modal`; modal interactions produce actions for the normal reducer flow.

## Themes and text

Styles derive from the active `loxia-core::theme` value. The bundled themes are listed in
`02-data-model.md`. CRT themes use ASCII-safe rendering through their `ascii_only` setting, while
other themes permit the normal terminal glyph set.

User-visible key hints resolve from the active keymap rather than containing hard-coded key names.
The help modal therefore reflects customized bindings and reports conflicts consistently.

## Interaction

Terminal input is translated in `loxia-player`, then routed through the same action system used by
mouse hit testing and modal controls. The UI renders loading, empty, error, and offline states from
state values supplied by the core and workers.
