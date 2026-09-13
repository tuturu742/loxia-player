# 04-03 · Text measurement helpers

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** S
**Prerequisites:** `04-01`
**Reference:** `docs/07-ui-spec.md` §13

## Goal
Grapheme-safe truncation and width measurement. Without this, a CJK or emoji track title breaks
column borders and can panic on a slice at a non-boundary — this is the single most common class of
TUI bug.

## Files
- `crates/loxia-tui/src/text.rs`

## Specification

```
pub fn width(s: &str) -> usize;                              // display cells, not bytes or chars
pub fn truncate(s: &str, max: usize) -> Cow<'_, str>;        // hard cut at a grapheme boundary
pub fn ellipsize(s: &str, max: usize) -> Cow<'_, str>;       // cut and append '…' (or "..." in ascii_only)
pub fn ellipsize_start(s: &str, max: usize) -> Cow<'_, str>; // for paths: "…/Darkwave/"
pub fn pad_to(s: &str, w: usize) -> String;
pub fn fit_columns(items: &[&str], total: usize, gap: usize) -> Vec<usize>;
pub fn wrap(s: &str, w: usize) -> Vec<String>;               // word wrap, grapheme-safe
```

Rules:
- `width` uses `unicode-width`; **never** `str::len()` or `chars().count()`.
- All cutting uses `unicode-segmentation` grapheme clusters. A wide character that would straddle
  the limit is dropped entirely rather than half-rendered.
- `ellipsize` reserves the ellipsis width from `max`; when `max < 2` it degrades to `truncate`.
- A zero `max` returns an empty string, never panics.
- Combining marks stay attached to their base character.
- Control characters and ANSI escape sequences are stripped from any server-supplied string — a
  track title containing an escape sequence must not be able to move the cursor or repaint the
  screen. This is a small but real injection surface, since titles come from the server.

`fit_columns` distributes `total` across `items.len()` columns with `gap` between them, giving the
remainder to the leftmost columns so the layout is stable as widths change.

## Acceptance
- `width_counts_cells_not_bytes` — `"日本語"` is 6, `"café"` is 4, `"👍"` is 2.
- `truncate_never_splits_a_grapheme` (proptest over arbitrary strings and limits — the property is
  that the result is always valid UTF-8 and its width is ≤ the limit).
- `truncate_drops_straddling_wide_char` — `truncate("日本", 1)` is empty, not a half character.
- `ellipsize_reserves_ellipsis_width`
- `ellipsize_with_tiny_max_degrades`
- `zero_max_returns_empty`
- `combining_marks_stay_with_base`
- `control_chars_are_stripped`
- `ansi_escape_is_stripped` — `"\x1b[2Jtitle"` yields `"title"`.
- `wrap_is_grapheme_safe`
- `fit_columns_sums_to_total`

## Done when
The global DoD in `tasks/README.md` is satisfied.
