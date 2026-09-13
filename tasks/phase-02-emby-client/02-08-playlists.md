# 02-08 · Playlists

**Phase:** 02 — Emby client · **Agent:** B · **Size:** M
**Prerequisites:** `02-05`
**Reference:** `docs/03-emby-api.md` §8

## Goal
Full playlist CRUD. The one detail that must not be missed: removal needs Emby's per-row playlist
entry id, not the track's item id.

## Files
- `crates/loxia-emby/src/endpoints/playlists.rs`

## Specification

```
pub struct PlaylistEntry { pub entry_id: PlaylistEntryId, pub track: Track }

pub async fn list(c: &EmbyClient, page: Page) -> Result<Paged<Playlist>, EmbyError>;
pub async fn items(c: &EmbyClient, id: &PlaylistId) -> Result<Vec<PlaylistEntry>, EmbyError>;
pub async fn create(c: &EmbyClient, name: &str, tracks: &[ItemId]) -> Result<PlaylistId, EmbyError>;
pub async fn set_overview(c: &EmbyClient, id: &PlaylistId, overview: &str) -> Result<(), EmbyError>;
pub async fn add(c: &EmbyClient, id: &PlaylistId, tracks: &[ItemId]) -> Result<(), EmbyError>;
pub async fn remove(c: &EmbyClient, id: &PlaylistId, entries: &[PlaylistEntryId]) -> Result<(), EmbyError>;
pub async fn move_item(c: &EmbyClient, id: &PlaylistId, item: &ItemId, new_index: usize) -> Result<(), EmbyError>;
pub async fn delete(c: &EmbyClient, id: &PlaylistId) -> Result<(), EmbyError>;
```

| Function | Request |
| :-- | :-- |
| `list` | `GET /Users/{uid}/Items?IncludeItemTypes=Playlist&Recursive=true` |
| `items` | `GET /Playlists/{id}/Items?UserId={uid}` |
| `create` | `POST /Playlists?Name={name}&Ids={csv}&UserId={uid}&MediaType=Audio` — **query parameters, not a JSON body.** Verified live in task `02-01`: a JSON body fails `500 Unrecognized Guid format`; the identical call as query parameters succeeds |
| `add` | `POST /Playlists/{id}/Items?Ids={csv}&UserId={uid}` |
| `remove` | `DELETE /Playlists/{id}/Items?EntryIds={csv}` |
| `move_item` | `POST /Playlists/{id}/Items/{itemId}/Move/{newIndex}` |
| `delete` | `DELETE /Items/{id}` |

**`Overview` cannot be set at creation.** Verified live: passing `Overview` as a creation query
parameter produces no error but silently fails to persist. `create` therefore takes no `overview`
parameter at all. Setting a description is a **separate, second step** —
`set_overview(c, id, text)`: `GET /Users/{uid}/Items/{id}` for the full item DTO, set `Overview` on
it, then `POST /Items/{id}` with that DTO as the body (confirmed live: `204`, value persists).
Callers that want a description on a new playlist call `create` then `set_overview`.

**`items` must capture the per-row entry id** into `PlaylistEntry.entry_id`, reading the field name
confirmed by task `02-01` row 9 (`PlaylistItemId` — confirmed live, exact match). Without it,
`remove` cannot be implemented without an extra refetch, and the UI cannot remove the correct row
when a playlist contains the same track twice.

All of these except `list` and `items` are mutations: **no retry**. `create` with an empty `tracks`
slice is valid — an empty playlist is a legitimate thing to make (confirmed live:
`ItemAddedCount: 0`, no error).

Batch limits: `add` and `remove` chunk at 100 ids per request; a URL with 500 ids will exceed
server limits. Chunks run sequentially so a partial failure leaves a knowable state.

## Acceptance
- `items_captures_entry_ids` (fixture `playlist_items.json`) — every entry has a non-empty
  `entry_id`, and a playlist containing the same track twice yields two distinct entry ids.
- `remove_sends_entry_ids_not_item_ids` — the request query contains the entry ids.
- `create_sends_query_params_not_json_body` — asserts the request has no JSON body and the query
  string contains `Name`, `Ids`, `UserId`, `MediaType`.
- `create_with_no_tracks_is_ok`
- `set_overview_reads_then_posts_full_dto`
- `add_chunks_at_100`
- `mutations_do_not_retry` — a 500 on `add` results in exactly one request.
- `delete_uses_items_route`
- `move_uses_path_index`

## Done when
The global DoD in `tasks/README.md` is satisfied.
