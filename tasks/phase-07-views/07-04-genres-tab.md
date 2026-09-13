# 07-04 · Genres tab

**Phase:** 07 — Views · **Agent:** D · **Size:** S
**Prerequisites:** `04-11`
**Reference:** `docs/07-ui-spec.md` §9

## Goal
A four-level Miller stack: Genres → Artists → Albums → Tracks.

## Files
- `crates/loxia-core/src/reducer/nav.rs` (extend)
- `crates/loxia-player/src/workers/network.rs` (extend)

## Specification

This tab is almost entirely reuse — the Miller view, column widget, and inspector already exist. The
work is the seed column, two new `ColumnKind`s, and their fetches.

| Level | `ColumnKind` | Fetch |
| :-- | :-- | :-- |
| 1 | `Genres` | `items::genres(lib)` |
| 2 | `GenreArtists { of_genre }` | `items::genre_artists(genre)` |
| 3 | `Albums { of_artist }` | `discography::discography(artist)` |
| 4 | `Tracks { of_album }` | `items::album_tracks(album)` |

Levels 3 and 4 are the **same** kinds the Artists tab uses, so the ALBUMS / APPEARS ON split and the
`a`-versus-`A` queue rules apply here unchanged. Reusing the kinds rather than inventing
genre-specific ones is what makes that automatic.

Drilling four deep exercises the sliding window: at level 4 the window shows levels 2–4 with the
parent path indicator reading `…/Darkwave/`.

Emby filters genres **by name**, not id (`docs/03-emby-api.md` §3), so `GenreArtists` carries the
genre's name in its `ColumnKind`. Names containing commas must be percent-encoded, not split as a
multi-value parameter — a genre literally named "Rock, Alternative" otherwise silently matches
nothing.

**Queueing a genre directly** (`a` on a level-1 row) is refused with
`select an artist or album to queue` — a genre can span tens of thousands of tracks and queueing it
wholesale is never the intent.

**Empty state:** `no genres in this library`.

## Acceptance
- `genres_seed_column_loads_on_first_entry`
- `drill_genre_to_artists`
- `drill_artist_reuses_discography_kind` — assert the emitted effect is `FetchDiscography`.
- `four_level_drill_slides_window`
- `parent_path_shows_genre_at_level_four`
- `genre_name_with_comma_is_encoded_not_split`
- `queueing_a_genre_is_refused_with_toast`
- `appears_on_split_present_at_level_three`
- `genres_snapshot_four_columns`
- `empty_genre_library_state`

## Done when
The global DoD in `tasks/README.md` is satisfied.
