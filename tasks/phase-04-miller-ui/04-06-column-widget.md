# 04-06 · Column widget

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** M
**Prerequisites:** `04-05`
**Reference:** `docs/07-ui-spec.md` §5

## Goal
Render one Miller column: rows, cursor, scrolling, section headers, selection checkboxes,
appears-on highlighting, and the load states. This widget is used by every browsing view.

## Files
- `crates/loxia-tui/src/widgets/column.rs`, `section_header.rs`

## Specification

```
pub fn render_column(f: &mut Frame, area: Rect, col: &Column, idx: usize,
                     focused: bool, state: &AppState, theme: &Theme, hits: &mut HitMap);
```

**Row anatomy**, left to right:
`[selection box] [status glyph] [name] [right-aligned meta]`
- The selection box `[X]`/`[ ]` appears **only** when `col.selection.visual_mode`.
- Status glyphs: `♥` favourite, `↓` downloaded, `⚠` unavailable, `▶` currently playing.
- Right meta is duration for tracks, year for albums, counts for artists — ellipsized away first
  when the column is narrow.

**Section headers** (`section_header.rs`) render as a full-width dim line:
`── ALBUMS (2) ─────────────`, using `theme.glyphs().rule`. They are **never** given the cursor
style and are **not** registered as hit targets — a click on one must do nothing.

**Appears-on highlighting.** When the column's parent album is `AlbumRelation::AppearsOn { context }`,
a track whose `artist_ids` contains `context` uses `Accent`; every other track uses `Dim`. This is
what tells the user, at a glance, which tracks `a` will queue.

**Scrolling.** Render `col.items[scroll_offset..]` clipped to the viewport. The reducer maintains
`scroll_offset` (task `03-06`); the widget must not adjust it — a widget that scrolls during render
fights the reducer and produces flicker.

**Load states:**
| State | Render |
| :-- | :-- |
| `Loading` | a centred spinner frame chosen from `state` tick count, plus `loading…` |
| `Loaded` with no items | a centred dim empty-state line appropriate to the `ColumnKind` |
| `Error(msg)` | the message, wrapped, plus `{hint} retry` from `keymap.hint_for(Refresh)` |

**Border.** Title is `col.title`, ellipsized from the start when it is a path
(`…/Darkwave/`). Border style is `BorderFocus` when `focused`, else `Border`.

Every rendered selectable row registers `HitTarget::ColumnItem { column: idx, index }`.

## Acceptance
- `column_snapshot_basic`, `column_snapshot_with_sections`, `column_snapshot_visual_select`,
  `column_snapshot_appears_on_highlighting`, `column_snapshot_loading`, `column_snapshot_empty`,
  `column_snapshot_error`
- `section_headers_are_not_hit_targets`
- `section_header_never_gets_cursor_style`
- `appears_on_dims_third_party_tracks` — assert the style of a non-artist row differs from an
  artist row.
- `checkbox_only_in_visual_mode`
- `widget_does_not_modify_scroll_offset` — render, then assert the column is unchanged.
- `long_unicode_title_does_not_break_border` — a CJK title longer than the column width.
- `narrow_column_drops_meta_first`

## Done when
The global DoD in `tasks/README.md` is satisfied.
