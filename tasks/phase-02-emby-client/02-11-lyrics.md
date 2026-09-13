# 02-11 · Lyrics

**Phase:** 02 — Emby client · **Agent:** B · **Size:** S
**Prerequisites:** `02-09`, `01-08`
**Reference:** `docs/03-emby-api.md` §7, `docs/12-decisions.md` §7

## Goal
Fetch lyrics. Emby has no lyrics service — it exposes `.lrc` sidecars as **Subtitle-type media
streams on the audio item**, so this is a subtitle fetch with a different name.

## Files
- `crates/loxia-emby/src/endpoints/lyrics.rs`

## Specification

```
pub fn find_lyric_stream(source: &MediaSourceDto) -> Option<LyricStreamRef>;
pub async fn fetch(c: &EmbyClient, item: &ItemId, r: &LyricStreamRef) -> Result<Lyrics, EmbyError>;
```

**Discovery** (`find_lyric_stream`): scan `MediaStreams` for the first entry where
`Type == "Subtitle"` and `Codec` (lowercased) is one of `lrc`, `srt`, `subrip`, or `text`. Map the
codec to `LyricFormat`: `lrc` → `Lrc`, `srt`/`subrip` → `Srt`, `text` → `Txt`. Record `Index` and
the enclosing `MediaSources[].Id`. No match → `None`, meaning the track has no lyrics and the UI
hides the pane entirely.

**Fetch:**
```
GET /Items/{ItemId}/{MediaSourceId}/Subtitles/{Index}/Stream.{format}
```
`format` is `lrc` when `LyricFormat::Lrc`, otherwise `srt`. The response body is plain text; read it
as UTF-8 **lossily** — sidecar files are frequently mis-encoded and a decode error must not lose the
lyrics.

Parse with `loxia_core::model::lyrics::parse` (task `03-02`):
- `Lrc` → `Lyrics::Synced`
- `Srt` / `Txt` → `Lyrics::Unsynced`, keeping payload lines and discarding cue numbers and timings

**Lyrics are cosmetic and must never interrupt playback.** A 404 returns
`Ok(Lyrics::Unsynced(vec![]))`, not an error. Any other failure is logged at `debug` and returns the
same empty value. There is no user-visible error path for lyrics.

This is a read: it uses `with_retry`, but with `max_attempts = 1` — a slow retry loop for decoration
would delay nothing useful.

## Acceptance
- `finds_lrc_subtitle_stream` (fixture `track_with_lyrics.json`)
- `returns_none_when_no_subtitle_stream` (fixture `track_no_lyrics.json`)
- `ignores_non_text_subtitle_codecs` — a source whose only subtitle stream is `pgs` yields `None`.
- `codec_to_format_mapping` (table test over `lrc`, `srt`, `subrip`, `text`, mixed case)
- `fetch_builds_items_route_not_videos` — asserts the request path begins `/emby/Items/`.
- `fetch_parses_lrc_into_synced` (fixture `lyrics.lrc`)
- `fetch_404_returns_empty_not_error`
- `fetch_handles_invalid_utf8` — a body with an invalid byte still yields lines.

## Done when
The global DoD in `tasks/README.md` is satisfied.
