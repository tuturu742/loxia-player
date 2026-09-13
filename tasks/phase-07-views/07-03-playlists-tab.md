# 07-03 · Playlists tab

**Phase:** 07 — Views · **Agent:** D · **Size:** L
**Prerequisites:** `07-02`, `02-08`
**Reference:** `docs/07-ui-spec.md` §9, `docs/03-emby-api.md` §8

## Goal
Browse and edit playlists: add, remove, reorder, and delete, each with optimistic UI and rollback.

## Files
- `crates/loxia-tui/src/views/playlists.rs`
- `crates/loxia-core/src/reducer/queue.rs` (extend)

## Specification

A two-column Miller stack: `Playlists → PlaylistTracks`. Reuses `render_column` (task `04-06`).

**State.** `PlaylistTracks` columns store `PlaylistEntryId` alongside each track — Emby's removal
API needs the per-row entry id, not the item id (`docs/12-decisions.md` §7). A playlist containing
the same track twice has two distinct entry ids, and removing the right row depends on it.

**Operations:**
| Key | Action | Effect |
| :-- | :-- | :-- |
| `x` | remove the focused track | `PlaylistRemove { playlist, entries: [entry_id] }` |
| `X` | delete the playlist | opens `Confirm`, then `PlaylistDelete` |
| `Ctrl+P` | add the selection to a playlist | opens the `SavePlaylist` modal (task `10-08`) |
| `Ctrl+↑`/`Ctrl+↓` | move the focused track | `PlaylistMove { playlist, item, new_index }` |
| `Enter`/`a` | queue the playlist or track | normal queue rules |

All four mutations are **optimistic with rollback**: apply locally, emit, and on failure restore the
previous list and toast. For a reorder, restore means moving the row back to its original index.

**`X` always confirms.** `x` and `X` are adjacent keys, and deleting a playlist is not recoverable
through the app. The `Confirm` prompt names the playlist and its track count.

**Multi-select** works in the tracks column: `v` then `.` marks rows, and `x` removes all selected
entries in one request.

**Empty states:** no playlists → `no playlists — press {P} to save the queue as one`; an empty
playlist → `this playlist is empty`.

**Offline:** playlists render from cache if present; all four mutations are refused with
`playlist changes need a connection`.

## Acceptance
- `playlist_tracks_carry_entry_ids`
- `duplicate_track_has_distinct_entry_ids`
- `remove_uses_entry_id_not_item_id`
- `remove_is_optimistic_and_rolls_back`
- `reorder_rolls_back_to_original_index`
- `delete_requires_confirmation`
- `confirm_prompt_names_playlist_and_count`
- `multiselect_remove_sends_one_request`
- `mutations_refused_when_offline`
- `empty_playlist_and_no_playlists_states`
- `playlists_snapshot`, `playlist_tracks_snapshot`
- Manual, pasted into the PR: create a playlist, add tracks, reorder one, remove one, and delete it
  — verifying each step in the Emby web UI.

## Done when
The global DoD in `tasks/README.md` is satisfied.
