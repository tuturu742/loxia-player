# 07-07 · Lyrics pane

**Phase:** 07 — Views · **Agent:** D · **Size:** M
**Prerequisites:** `07-06`, `02-11`
**Reference:** `docs/07-ui-spec.md` §8, `docs/03-emby-api.md` §7

## Goal
Render synced lyrics that scroll in step with playback, fetched from Emby's subtitle-stream
mechanism.

## Files
- `crates/loxia-tui/src/widgets/lyrics.rs`
- `crates/loxia-core/src/reducer/player.rs` (extend)

## Specification

**Fetching.** When the current track changes and `ui.show_lyrics` is on and the track has a
`lyric_stream`, emit `Effect::Net(FetchLyrics { track, stream_ref })`. `Data::LyricsLoaded` stores
`(ItemId, Lyrics)` in `AppState.lyrics` — **one track at a time**; a new track replaces it.

A track with `lyric_stream: None` emits nothing and hides the pane entirely. An empty box labelled
"LYRICS" for the majority of tracks that have none is worse than no pane.

**Rendering — synced.** The active line index comes from
`lyrics.active_line(player.position)` (task `01-08`), computed **at render time**, never stored:
- The active line is `Accent` and prefixed `▶`.
- Lines before and after are `Fg`; distant lines fade to `Dim` beyond ±4 positions.
- Scrolling keeps the active line vertically centred. When the active line changes by one, the view
  advances by one — do not recentre with a jump that makes the whole block leap.
- Before the first timestamp, show the opening lines from the top with nothing highlighted.

**Rendering — unsynced.** A plain scrollable block with no highlighting, scrollable with `j`/`k`
when the pane has focus.

**Toggle.** `L` (`ToggleLyrics`) flips `ui.show_lyrics`, persisted to config. When off, the pane is
hidden and the right pane's other content expands.

**Long lines** wrap with `text::wrap`; a wrapped active line highlights all of its rows.

**Loading** shows a dim `loading lyrics…` for at most one frame in practice; a failure shows nothing
at all, because lyrics failures are never worth a user-visible error (`docs/03-emby-api.md` §7).

## Acceptance
- `no_lyric_stream_hides_pane_and_emits_nothing`
- `lyrics_fetched_on_track_change`
- `lyrics_replaced_not_accumulated`
- `active_line_derived_from_position_not_stored`
- `active_line_advances_with_playback` — table test at several positions against a fixture LRC.
- `before_first_timestamp_nothing_highlighted`
- `active_line_stays_centred`
- `scroll_advances_by_one_not_by_a_jump`
- `distant_lines_are_dimmed`
- `wrapped_active_line_highlights_all_rows`
- `unsynced_lyrics_render_without_highlight`
- `toggle_hides_pane_and_persists`
- `lyrics_failure_renders_nothing`
- `lyrics_snapshot_synced`, `_unsynced`, `_hidden`
- Manual, pasted into the PR: play a track with an `.lrc` sidecar on the live server and confirm
  the lines track the audio.

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 07's exit criteria in
`docs/08-roadmap.md` are met.
