# 10-13 · Toasts and empty states

**Phase:** 10 — Polish · **Agent:** D · **Size:** S
**Prerequisites:** `10-04`
**Reference:** `docs/07-ui-spec.md` §13

## Goal
A consistent toast system and an audit that every view has a real empty and error state. This is the
task that turns a working app into one that explains itself.

## Files
- `crates/loxia-tui/src/widgets/toast.rs`
- `crates/loxia-core/src/state/toast.rs` (extend)

## Specification

**Toast** = `{ message: String, level: ToastLevel, created_at: Timestamp, id: u64 }` with
`ToastLevel` = `Info | Success | Warning | Error`.

**Rendering.** Bottom-right, stacked upward, at most 3 visible with `+n more` when there are more.
Each is a bordered single line styled by level. They overlay the player bar rather than resizing the
layout — content that jumps as toasts appear and vanish is far more disruptive than an overlay.

**Lifetimes:** `Info` 3 s, `Success` 3 s, `Warning` 6 s, `Error` 8 s. Expiry happens on `Tick`
(task `03-08`). Errors last longest because they are the ones a user needs time to read.

**Deduplication.** An identical message arriving while the same one is displayed refreshes its
timestamp instead of stacking. Ten failed favourite toggles produce one toast, not ten.

**Writing rules** — enforced by review and by the test below:
- Lower case, no trailing period, no jargon.
- Say what happened and, when there is one, what to do: `not available offline`,
  `playlists need a connection`, `could not save: server returned 500`.
- Never expose an error type name, a URL, or a token.

**Empty-state audit.** Every view must have one, written as a sentence with the relevant key hint
from the keymap. Add any that are missing:
| View | Empty state |
| :-- | :-- |
| Artists / Albums / Genres / Folders | `nothing here` variants naming the level |
| Search | `type to search` before a query; `no results for "<q>"` after |
| Favourites | `no favourites yet — press {f} on anything to add it` |
| Playlists | `no playlists — press {P} to save the queue as one` |
| Queue | `queue is empty — press {a} on an album to start` |
| History | `nothing played yet` |
| Offline, unsupported tab | `<tab> needs a connection` |

**Error states** in columns show the message plus `{Ctrl+R} retry`, and preserve any previously
loaded items beneath (task `03-06`).

## Acceptance
- `toast_lifetimes_by_level`
- `at_most_three_visible_with_overflow_count`
- `duplicate_message_refreshes_instead_of_stacking`
- `toasts_overlay_without_resizing_layout`
- `toast_style_per_level`
- `no_toast_message_contains_a_url_or_token` — a test iterating every literal toast string in the
  workspace, asserting none matches a URL or a long hex pattern.
- `toast_messages_are_lowercase_without_trailing_period` — same iteration.
- `every_view_has_an_empty_state` — a table test rendering each view with empty data and asserting
  the buffer is not blank.
- `empty_states_use_keymap_hints`
- `column_error_state_preserves_items_and_shows_retry`
- `toast_snapshot_stacked`, `_overflow`

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 10's exit criteria in
`docs/08-roadmap.md` are met.
