# 06-04 · Sort profiles

**Phase:** 06 — Queue engine · **Agent:** A · **Size:** M
**Prerequisites:** `06-01`
**Reference:** `docs/02-data-model.md` §8, `design_overview` §4.2

## Goal
Multi-criteria sorting with up to four stacked rules, applied to the queue and to browse columns.

## Files
- `crates/loxia-core/src/queue/sort.rs`

## Specification

```
pub fn sort_tracks(tracks: &mut [Track], profile: &SortProfile);
pub fn sort_items(items: &mut [MediaItem], profile: &SortProfile);   // headers stay in place
pub fn compare(a: &Track, b: &Track, profile: &SortProfile) -> Ordering;
```

Rules apply **in order**, each breaking the previous one's ties. A maximum of 4 is enforced at config
validation (task `01-02`); a profile arriving here with more sorts by the first 4.

**Field extraction** — `SortField` → key:
| Field | Key | `None` handling |
| :-- | :-- | :-- |
| `Name` | `sort_name`, falling back to `name` | empty string |
| `Artist` | first of `artist_names` | empty string |
| `AlbumArtist` | first of `album_artist_names` | falls back to `Artist` |
| `Album` | `album_name` | empty string |
| `Year` | `year` | sorts **last** in both directions |
| `TrackNumber` | `(disc_number, track_number)` as a tuple | 0 |
| `Genre` | first of `genres` | empty string |
| `DateAdded` | `date_created` | sorts last |

String comparison is **case-insensitive** and ignores a leading `The `, `A `, `An ` — otherwise every
band beginning with "The" clusters under T, which is not what anyone means by sorting by artist.
Numeric-aware comparison for names containing digits (`Track 2` before `Track 10`).

**`None` sorts last regardless of direction.** Reversing a descending sort should not promote
unknown values to the top; they are always the least informative rows.

**Stability.** Use `sort_by` with a total comparator, and make the comparator total by appending
`ItemId` as an implicit final tiebreak. Applying a profile twice must be a no-op — the property
test below is the check.

**`sort_items`** sorts only within sections: items between two `SectionHeader`s are sorted as a
group, and headers never move. Sorting a discography column must not merge ALBUMS into APPEARS ON.

**`ApplySortProfile`** rebuilds `entries` in sorted order, then rebuilds `play_order` to the identity
and repositions to keep the current track playing. It clears `shuffled`, because the two are
mutually exclusive views of ordering.

## Acceptance
- `sort_profile_applies_all_four_rules_in_order` — the `chronological_discog` profile from
  `design_overview` §7 on a hand-built fixture.
- `release_chronology_profile_matches_expected`
- `sort_is_idempotent` (proptest) — sorting twice equals sorting once.
- `sort_is_stable_for_equal_keys`
- `none_sorts_last_ascending`
- `none_sorts_last_descending`
- `leading_article_is_ignored` — "The Soft Moon" sorts under S.
- `case_insensitive_comparison`
- `numeric_aware_comparison` — `Track 2` before `Track 10`.
- `track_number_sorts_by_disc_then_track`
- `sort_items_does_not_move_headers`
- `sort_items_does_not_merge_sections`
- `apply_sort_profile_keeps_current_track_playing`
- `apply_sort_profile_clears_shuffle`

## Done when
The global DoD in `tasks/README.md` is satisfied.
