# 07-01 · Search tab

**Phase:** 07 — Views · **Agent:** D · **Size:** M
**Prerequisites:** `04-11`, `06-01`, `02-07`
**Reference:** `docs/07-ui-spec.md` §9

## Goal
Live library search with a debounced query and three categorised result sections.

## Files
- `crates/loxia-tui/src/views/search.rs`
- `crates/loxia-core/src/state/search.rs`, `reducer/nav.rs` (extend)

## Specification

**State** (`SearchState`): `query: String`, `debounce_until: Option<Timestamp>`,
`results: SearchResults`, `load: LoadState`, `focused_section: Section`, `cursors: [usize; 3]`.

**Debounce.** Typing updates `query` and sets `debounce_until = now + 250ms`. The `Tick` handler
fires `Effect::Net(Search)` when the deadline passes and the query differs from the last one
searched. Searching per keystroke would issue three requests per character against the server.

A query shorter than 2 characters after trimming clears results and issues nothing.

**Layout** — three stacked sections in the canvas, each a bordered list with its count in the title:
```
┌─ ARTISTS (3) ──────┐
┌─ ALBUMS (12) ──────┐
┌─ TRACKS (47) ──────┐
```
Sections with no results are **collapsed to a single dim line**, not hidden — a user needs to see
that a category was searched and came back empty. Heights are distributed proportionally to result
counts, with a minimum of 3 rows and a maximum of half the canvas per section.

**Focus.** The query line is focused on tab entry, putting the app in `InputContext::TextInput`.
`Enter` leaves text-input mode and focuses the first non-empty section. `Tab` cycles sections.
`j`/`k` move within the focused section. `/` returns to the query line.

**Drilling.** `NavRight` on a result hands off to the Miller stack: selecting an artist switches to
the Artists tab with that artist's discography pushed. This is why search is not itself a Miller
view — its results are entry points, not a hierarchy.

Queueing actions (`a`, `A`, `i`, `m`) work directly on the focused result without leaving the tab.

**Loading and errors** render inside each section independently, since the three requests can fail
separately (task `02-07` returns partial results).

## Acceptance
- `search_debounces_250ms` — five keystrokes within the window produce one effect.
- `short_query_issues_no_request`
- `query_unchanged_after_debounce_issues_nothing`
- `empty_section_collapses_to_one_line`
- `section_heights_proportional_within_bounds`
- `tab_cycles_sections`
- `enter_focuses_first_nonempty_section`
- `drill_into_artist_switches_to_artists_tab`
- `queue_from_search_result_works`
- `partial_failure_shows_per_section_error`
- `search_snapshot_results`, `search_snapshot_empty`, `search_snapshot_loading`

## Done when
The global DoD in `tasks/README.md` is satisfied.
