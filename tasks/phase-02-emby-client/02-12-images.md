# 02-12 · Images

**Phase:** 02 — Emby client · **Agent:** B · **Size:** S
**Prerequisites:** `02-02`
**Reference:** `docs/03-emby-api.md` §9

## Goal
Fetch artwork with tag-keyed disk caching and a fallback chain, so the UI never blocks on artwork and
a server-side art change invalidates automatically.

## Files
- `crates/loxia-emby/src/endpoints/images.rs`

## Specification

```
pub enum ImageSize { Thumb, Large }        // 64 px, 600 px max height

pub fn image_url(c: &EmbyClient, id: &ItemId, tag: &str, size: ImageSize) -> Url;
pub async fn fetch(c: &EmbyClient, id: &ItemId, tag: &str, size: ImageSize)
    -> Result<Bytes, EmbyError>;
```
`GET /Items/{id}/Images/Primary?maxHeight={px}&tag={tag}&quality=90`

**Caching is the caller's job** (`loxia-cache`, task `08-02`), but the **cache key is defined here**
so both sides agree: `{item_id}_{size}_{tag}.jpg`. Including the tag is what makes a changed cover
invalidate without a manual purge.

**Fallback chain**, resolved by the caller with the ids it already holds: track art → album art →
artist art → none (the UI draws a themed placeholder). This module provides:
```
pub fn art_candidates(track: &Track, album: Option<&Album>, artist: Option<&Artist>)
    -> Vec<(ItemId, String)>;   // (id, tag) in priority order, skipping entries with no tag
```

A 404 on any candidate is **not** an error — it means "try the next one". Only a transport failure
propagates.

Requests carry a 10-second timeout, shorter than the default: artwork is decoration and must never
hold up a UI update.

## Acceptance
- `image_url_snapshot` — one `insta` snapshot per `ImageSize`.
- `cache_key_includes_tag` — two different tags for the same item produce different keys.
- `art_candidates_priority_order`
- `art_candidates_skips_missing_tags`
- `art_candidates_empty_when_no_tags_anywhere`
- `fetch_404_is_not_an_error`
- `fetch_uses_short_timeout`

## Done when
The global DoD in `tasks/README.md` is satisfied.
