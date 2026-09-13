# 04-11 · Inline filter

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** S
**Prerequisites:** `04-10`
**Reference:** `docs/07-ui-spec.md` §5, `docs/04-state-and-input.md` §5

## Goal
The `/` fuzzy filter for the active column, including the text-input mode it puts the app into.

## Files
- `crates/loxia-core/src/reducer/nav.rs` (extend)
- `crates/loxia-tui/src/widgets/column.rs` (extend)

## Specification

**Behaviour.** `/` (`OpenFilter`) sets `column.filter = Some(String::new())` and puts the app in
`InputContext::TextInput`. Typing appends; `Backspace` removes a grapheme; `Enter` keeps the filter
and leaves text-input mode; `Esc` clears the filter entirely (the first rung of the `Cancel` ladder
from task `03-06`).

**Matching.** `nucleo-matcher`, case-insensitive, against `MediaItem::display_name()`. Items are
**filtered, not reordered** — a Miller column's order is meaningful (year, track number), and
re-ranking by fuzzy score would scramble it. Score is used only to decide inclusion.

**Section headers are always retained**, but a header whose section has no surviving items is
dropped along with it, and the retained headers' counts are recomputed to the filtered totals.

**Cursor and selection.** After a filter change, the cursor moves to the first selectable visible
item. Selection is keyed by `ItemId` (task `03-01`), so it survives untouched — this is exactly the
case that index-keyed selection would corrupt.

**Rendering.** The filter appears in the column's border title as `<title> /<query>`, with the query
in `Accent` and a cursor block at the end while in text-input mode. When the filter matches nothing,
the body shows `no matches for "<query>"`.

Filtering is a pure function of `items` and `filter`; `Column::visible_items()` (task `03-01`)
already expresses it, so the widget needs no filtering logic of its own.

## Acceptance
- `filter_narrows_items`
- `filter_is_case_insensitive`
- `filter_preserves_original_order` — a filter matching items 5, 1, 9 yields them in the order 1, 5, 9.
- `filter_retains_headers_with_surviving_items`
- `filter_drops_empty_sections`
- `filter_recomputes_header_counts`
- `filter_preserves_selection_by_id`
- `cursor_moves_to_first_visible_after_filter`
- `esc_clears_filter`
- `enter_keeps_filter_and_exits_text_mode`
- `backspace_removes_grapheme` — a filter containing an emoji.
- `column_snapshot_filtering`, `column_snapshot_no_matches`

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 04's exit criteria in
`docs/08-roadmap.md` are met.
