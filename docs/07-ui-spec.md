# Terminal user interface

`loxia-tui` renders `loxia-core` state with ratatui. It contains no network, cache, or playback
control implementation; it emits actions through the runtime boundary.

## Composition

The root renderer combines layout, sidebar, header, content views, inspector, player bar, modal
layer, and toast layer. Shared widgets include album art, columns, lyrics, progress, section
headers, sectioned lists, and inspector panels.

Views cover favourites, folders, genres, Miller-style browsing, now playing, playlists, search,
settings, and zen mode. Modal implementations cover confirmation, device selection, equalizer,
help, keymap editing, playlist saving, sleep timing, and sort-profile editing.

## Input presentation

The UI uses `ActionId` values and `KeyMap::hint_for(ActionId)` for command hints. This keeps
visible shortcuts aligned with custom keymaps. Hit testing maps mouse regions to the same actions
used by keyboard input.

## Styling

`style` applies roles from `loxia_core::theme`; `text` supplies width-aware text handling. Bundled
themes are loaded from `assets/themes`. The renderer respects an active theme's `ascii_only`
setting, which is enabled by `amber_crt` and `green_crt`.

Snapshot tests cover layouts, themes, widgets, views, and modals at representative terminal sizes.

See [`04-state-and-input.md`](04-state-and-input.md) for action dispatch and
[`02-data-model.md`](02-data-model.md) for theme definitions.
