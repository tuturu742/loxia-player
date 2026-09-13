# 10-08 · Save playlist modal

**Phase:** 10 — Polish · **Agent:** D · **Size:** M
**Prerequisites:** `07-03`, `03-07`
**Reference:** `design_overview` §3.4

## Goal
The `P` and `Ctrl+P` modal for saving the queue or a selection to a playlist, new or existing.

## Files
- `crates/loxia-tui/src/modals/save_playlist.rs`

## Specification

```
┌─ SAVE TO PLAYLIST ────────────────────────────────────────────────────┐
│  Tracks to save: 14 tracks (Active Queue)                             │
│                                                                       │
│  Target Playlist : [ Create New Playlist...                    ▼ ]    │
│  Playlist Name   : [ Late Night Darkwave                        ]     │
│  Description     : [                                            ]     │
│                                                                       │
│  [X] Sort tracks with the active sort profile before saving           │
│                                                                       │
│  [ Enter ] Save    [ Esc ] Cancel                                     │
└───────────────────────────────────────────────────────────────────────┘
```

**Source.** `P` saves the **active queue**; `Ctrl+P` saves the **current selection**, or the focused
item when nothing is multi-selected. The summary line states which, and the count, so the two keys
are never confused.

**Target dropdown** lists `Create New Playlist…` first, then existing playlists loaded via
`Effect::Net(FetchColumn { Playlists })` on open. Selecting an existing playlist **hides** the Name
and Description fields — they are meaningless for an append — and changes the button to `Add`.

**Fields.** `Tab`/`Shift+Tab` move between controls; typing edits the focused text field.
Name is required for a new playlist; `Enter` with an empty name shows an inline
`name is required` in `Error` style rather than closing.

**Sort checkbox** applies the active sort profile to the track list before saving. It defaults to
**off** — a queue's order is usually deliberate, and silently reordering it on save would be
surprising.

**On submit:** emit `PlaylistCreate` or `PlaylistAdd`, close immediately, and toast
`saving <n> tracks…`. The completion toast replaces it: `saved to <playlist>` or
`could not save: <reason>`. Blocking the UI on a network round-trip for a bulk operation would be
worse than optimistic feedback.

**The Description field, for a new playlist, requires two requests, not one.** Emby's playlist
creation endpoint silently ignores an `Overview` parameter (verified live, task `02-01`) — a
non-empty Description means `Effect::Net(PlaylistCreate)` is followed by a second
`Effect::Net(PlaylistSetOverview)` once the create reply supplies the new playlist's id
(`loxia-emby::endpoints::playlists::set_overview`, task `02-08`). An empty Description skips the
second call entirely.

**Empty source** (nothing selected and an empty queue): the modal refuses to open and toasts
`nothing to save`.

**Offline:** refuses to open with `playlists need a connection`.

## Acceptance
- `p_uses_queue_ctrl_p_uses_selection`
- `summary_states_source_and_count`
- `existing_target_hides_name_and_description`
- `existing_target_button_says_add`
- `empty_name_for_new_playlist_shows_inline_error`
- `sort_checkbox_defaults_off`
- `sort_checkbox_applies_profile_to_saved_order`
- `submit_closes_immediately_and_toasts`
- `completion_toast_replaces_pending`
- `failure_toast_names_reason`
- `empty_source_refuses_to_open`
- `refused_when_offline`
- `save_playlist_snapshot_new`, `_existing`, `_error`

## Done when
The global DoD in `tasks/README.md` is satisfied.
