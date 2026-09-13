# 02-07 · Search, favourites, instant mix

**Phase:** 02 — Emby client · **Agent:** B · **Size:** S
**Prerequisites:** `02-05`
**Reference:** `docs/03-emby-api.md` §§3, 8

## Goal
Three small endpoint groups that share the query builder. Straightforward, but the concurrency in
search and the optimistic-update contract in favourites both matter.

## Files
- `crates/loxia-emby/src/endpoints/search.rs`
- `crates/loxia-emby/src/endpoints/favorites.rs`
- `crates/loxia-emby/src/endpoints/instant_mix.rs`

## Specification

**Search:**
```
pub struct SearchResults { pub artists: Vec<Artist>, pub albums: Vec<Album>, pub tracks: Vec<Track> }
pub async fn search(c: &EmbyClient, q: &str, limit: usize) -> Result<SearchResults, EmbyError>;
```
Runs the three typed queries **concurrently** with `futures::join!` (not `try_join!` — that macro
short-circuits on the first error and would discard the other two successes, which is exactly what
the next sentence forbids; see `12-decisions.md`), each `SearchTerm={q}&Recursive=true&Limit={limit}`
(default 50). A blank or whitespace-only query returns empty results without issuing any request. If
one of the three fails while the others succeed, return the partial result and log the failure at
`warn` — a working artist list beats an error toast.

**Favourites:**
```
pub async fn favorites(c: &EmbyClient, page: Page) -> Result<SearchResults, EmbyError>;
pub async fn set_favorite(c: &EmbyClient, id: &ItemId, on: bool) -> Result<(), EmbyError>;
```
`favorites` uses `Filters=IsFavorite` with all three item types in one query and splits the response
by `Type`. `set_favorite` is `POST` or `DELETE /Users/{uid}/FavoriteItems/{id}`. It is a **mutation**
and therefore does not go through `with_retry` — the reducer already applied the change optimistically
and needs a prompt failure to roll back.

**Instant mix:**
```
pub async fn instant_mix(c: &EmbyClient, seed: &ItemId, limit: usize) -> Result<Vec<Track>, EmbyError>;
```
`GET /Items/{seed}/InstantMix?UserId={uid}&Limit={limit}` (default 100). Works for any item type.
An empty result is `Ok(vec![])`, not an error — some libraries genuinely cannot generate a mix, and
the UI shows an explanatory toast.

## Acceptance
- `search_runs_three_queries_concurrently` — wiremock asserts three requests with the right
  `IncludeItemTypes`.
- `blank_query_issues_no_request`
- `partial_search_failure_returns_others` — the album query 500s; artists and tracks still return.
- `favorites_splits_by_type` (fixture `favorites.json`)
- `set_favorite_sends_post_and_delete`
- `set_favorite_does_not_retry` — a 500 results in exactly one request.
- `instant_mix_empty_is_ok`

## Done when
The global DoD in `tasks/README.md` is satisfied.
