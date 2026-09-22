# Terminal user interface

`loxia-tui` renders `loxia-core::state::AppState` with ratatui. It owns layout,
styles, text helpers, views, widgets, modal presentation, and terminal hit
testing; it does not own reducer or network behaviour.

## Presentation structure

- `render` draws the root frame.
- `layout` assigns terminal regions.
- `style` maps `loxia_core::theme` roles to ratatui styles.
- `views` renders library, search, favourites, playlists, folders, genres,
  now-playing, settings, and zen-mode content.
- `widgets` renders reusable headers, columns, sidebars, player bars,
  inspectors, lyrics, album art, progress, section lists, and toasts.
- `modals` renders confirmation, device picker, equalizer, help, keymap editor,
  playlist save, sleep timer, and sort-profile dialogs.
- `hit` maps mouse regions to actions.

The TUI renders state and dispatches actions. It uses keymap hints supplied by
the core keymap rather than embedding shortcut strings.

## Themes

The UI loads the theme role set defined by `loxia-core`. Shipped theme files are
listed in [`02-data-model.md`](02-data-model.md). Themes marked `ascii_only`
use ASCII-safe visual alternatives.

## Rendering constraints

The renderer adapts to narrow terminals, unavailable images, empty lists,
loading states, errors, offline status, and active modal focus. Snapshot tests
cover representative sizes, themes, widgets, views, and modal states. The
manual workflows in [`14-manual-test-plan.md`](14-manual-test-plan.md) cover
terminal behaviour that snapshots cannot observe.
