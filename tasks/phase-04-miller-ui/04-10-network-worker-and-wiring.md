# 04-10 · Network worker and wiring

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** M
**Prerequisites:** `04-07`, `04-08`, `04-09`, `02-13`
**Reference:** `docs/01-architecture.md` §§4–5

## Goal
Connect the UI to the real server: implement the network worker, wire bootstrap to authenticate, and
make the Artists and Albums tabs work end to end against the live library.

## Files
- `crates/loxia-player/src/workers/network.rs`
- `crates/loxia-player/src/bootstrap.rs`

## Specification

**Worker.** Owns the `EmbyClient`. Consumes `Effect::Net(..)` and emits `Event`s. Runs each request
as a `tokio::spawn`ed task so a slow fetch never blocks the queue, with **at most 4 concurrent
requests** via a semaphore — an unbounded fan-out on a large library will exhaust the server's
connection pool.

Effect → endpoint mapping for this task:
| Effect | Call | Reply |
| :-- | :-- | :-- |
| `FetchColumn { Artists, page }` | `items::artists` | `Event::ItemsLoaded` |
| `FetchDiscography { artist }` | `discography::discography` | `Event::DiscographyLoaded` |
| `FetchAlbumTracks { album, filter }` | `items::album_tracks` | `Event::TracksLoaded` |

Remaining `Effect::Net` variants log `warn!("unhandled effect")` and are implemented in later phases.

**Request coalescing.** Track in-flight `(ColumnKind, page)` pairs and drop a duplicate request. Fast
scrolling otherwise fires the same page fetch repeatedly.

**Cancellation.** When a column is popped, its outstanding request's result is discarded — the
reply carries the `ColumnKind` and the reducer ignores replies for columns that no longer exist.

**Reducer additions** (`reducer/nav.rs`): handle `Data::DiscographyLoaded` by building the two
sections with `MediaItem::SectionHeader` rows between them —
`── ALBUMS (n) ──` then the primary albums, `── APPEARS ON (n) ──` then the rest. Omit a section
entirely when its count is zero rather than rendering an empty header.

**Pagination trigger.** When the cursor enters the last 50 loaded items of a column whose
`page_loaded * 200 < total`, emit `FetchColumn` for the next page. `ItemsLoaded` for page > 0
**appends** rather than replacing.

**Bootstrap.** After loading config: construct the client, `validate_token`, fetch music libraries,
seed the Artists column, and enter the loop. A missing or invalid token opens Settings with an
explanatory toast instead of exiting — a bad token must not make the app unusable.

## Acceptance
- `worker_limits_concurrency_to_four` — wiremock with a delay; assert at most 4 in flight.
- `duplicate_page_request_is_coalesced`
- `reply_for_popped_column_is_ignored`
- `discography_loaded_builds_section_headers`
- `empty_appears_on_section_is_omitted`
- `pagination_triggers_within_last_50`
- `page_two_appends_not_replaces`
- `invalid_token_opens_settings_with_toast`
- Manual, pasted into the PR: browse the live library Artists → Albums → Tracks; confirm the
  ALBUMS / APPEARS ON split matches what `probe discography` printed in task `02-13`; scroll past
  200 artists and confirm page 2 loads.

## Done when
The global DoD in `tasks/README.md` is satisfied.
