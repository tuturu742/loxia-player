# Terminal user interface

`loxia-tui` renders `loxia-core` state with ratatui.

## Layout and views

The root renderer combines layout, styling, text helpers, header, sidebar, player bar, content
views, inspectors, and modal layers. Views include browsing, folders, genres, favourites,
playlists, search, now playing, settings, and zen mode.

Miller-style browsing uses reusable column widgets. Widgets also provide album art, lyrics,
progress, section headers, toasts, and contextual inspectors.

## Modals

The modal modules present confirmations, device selection, equalizer controls, help, keymap editing,
playlist saving, sleep timers, and sort-profile editing. Modal state remains in `loxia-core`; the
TUI renders it and sends actions in response to input.

## Themes

Themes are loaded from the TOML files in `assets/themes` and represented by
`loxia-core::theme`. The built-in theme names are listed in [`02-data-model.md`](02-data-model.md).
Themes define named roles instead of allowing views to hardcode terminal colours.

## Input and accessibility

Keyboard bindings resolve through `KeyMap` and action identifiers. Rendered binding hints use
`KeyMap::hint_for(ActionId)`, allowing user bindings to replace defaults consistently. Mouse hit
maps supplement the keyboard interface rather than defining a separate behaviour model.
