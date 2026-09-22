# Terminal user interface

`loxia-tui` renders application state with Ratatui. It is a presentation crate: it reads state and
key-map hints and returns render metadata or actions, but it does not perform network, audio, or
filesystem work.

## Layout and rendering

`layout` computes the root regions. `render` draws the application frame, and `style` maps
`loxia-core` theme roles to Ratatui styles. `text` contains display helpers and `hit` records mouse
targets.

The interface includes navigation, media browsing, search, favourites, playlists, genres, folders,
now-playing, settings, and zen presentation. Those screens live under `views`.

## Reusable components

The `widgets` module contains shared UI components including album art, columns, headers,
inspectors, lyrics, player controls, progress, section headers, sidebars, and toasts. `modals`
contains confirmations, device selection, equalizer controls, help, key-map editing, playlist
saving, sleep timer controls, and sort-profile editing.

## Themes

The TUI renders the theme names defined in [`02-data-model.md`](02-data-model.md). CRT themes use
ASCII-safe presentation where their theme configuration requests it. `default_terminal` preserves
the terminal's background and palette choices.

## Interaction

Rendered key hints come from `KeyMap::hint_for(ActionId)`. Mouse targets map to actions through the
hit map. The input and reducer rules are described in
[`04-state-and-input.md`](04-state-and-input.md).

Snapshot tests cover layout, widgets, modals, views, and every bundled theme. They describe stable
rendered output rather than a separate source of behaviour.
