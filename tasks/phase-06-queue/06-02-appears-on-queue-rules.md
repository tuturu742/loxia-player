# 06-02 · Appears-on queue rules

**Phase:** 06 — Queue engine · **Agent:** A · **Size:** M
**Prerequisites:** `06-01`
**Reference:** `docs/03-emby-api.md` §4, `design_overview` §2.3

## Goal
Implement the `a` versus `A` distinction — the feature the design document is built around. Queueing
an artist or a compilation must either include only that artist's tracks, or the full record.

## Files
- `crates/loxia-core/src/queue/appears_on.rs`
- `crates/loxia-core/src/reducer/queue.rs` (extend)

## Specification

```
pub enum TrackFilter { All, ArtistOnly(ItemId) }
pub fn filter_tracks(tracks: &[Track], f: &TrackFilter) -> Vec<Track>;
```
`ArtistOnly` keeps tracks whose `artist_ids` contains the id. It matches **by id, never by name** —
name matching breaks on "The Beatles" versus "Beatles, The" and on any artist whose name is a
substring of another's.

**Behaviour by selection and key:**

| Selected | `Enter` / `a` → `QueueSelection { full_context: false }` | `Shift+Enter` / `A` → `{ full_context: true }` |
| :-- | :-- | :-- |
| `Artist` | every track by that artist, from the discography cache | same |
| `Album` (`Primary`) | the album's full tracklist | the album's full tracklist |
| `Album` (`AppearsOn { context }`) | **only tracks featuring `context`** | the **complete** compilation tracklist in album order |
| `Track` | that track | that track |
| Multi-select | the selected tracks | the selected tracks |

For a `Primary` album the two keys are deliberately identical — the artist leads the record, so
there is nothing to filter.

**Effects.** `02-06`'s `discography` returns albums only, never track lists (see
`docs/12-decisions.md` §10 item 1 — the earlier pre-fetched `tracks_by_album` cache this task
originally assumed no longer exists). So: pressing `a` or `A` **on an Album row** (Primary or
AppearsOn, selected in the Albums column, not yet drilled into its Tracks column) always emits
`Effect::Net(FetchAlbumTracks { album, filter: All })` — the fetch is identical for both keys. The
`ArtistOnly` filter, when it applies, is applied client-side to the **reply**, not before the
fetch. Pressing `a`/`A` **on a Track row**, or on a selection already sitting in a loaded Tracks
column, needs no fetch at all — those tracks are already in `column.items`.

**Ordering.** Filtered tracks keep their album order — disc, then track number — never the order
they arrived in. Then the active sort profile applies (task `06-04`).

**Empty result.** An `ArtistOnly` filter that matches nothing queues nothing and toasts
`no tracks by <artist> on this release`. This can legitimately happen when a server's metadata is
inconsistent, and a silent no-op would look like a bug.

## Acceptance
- `appears_on_enter_queues_only_artist_tracks` — from `fixture_appears_on()`: a 10-track compilation
  with 2 artist tracks queues exactly 2.
- `appears_on_shift_enter_queues_full_compilation` — queues all 10.
- `queue_full_context_preserves_compilation_order`
- `primary_album_ignores_full_context_flag` — both keys queue the same tracks.
- `artist_selection_queues_all_artist_tracks`
- `track_selection_queues_one`
- `multiselect_queues_selected_only`
- `filter_matches_by_id_not_name` — two artists whose names are substrings of each other.
- `album_row_a_and_shift_a_both_emit_identical_fetch_effect` — the request is the same either way;
  only the reply handling differs.
- `artist_only_filter_applied_to_reply_not_before_fetch`
- `track_row_selection_needs_no_fetch` — pressing `a` with the cursor on a track already loaded in
  a Tracks column queues immediately, no effect emitted.
- `empty_filter_result_toasts_and_queues_nothing`
- `filtered_tracks_keep_disc_and_track_order`

## Done when
The global DoD in `tasks/README.md` is satisfied.
