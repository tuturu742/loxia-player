# 02-09 · PlaybackInfo and stream URLs

**Phase:** 02 — Emby client · **Agent:** B · **Size:** M
**Prerequisites:** `02-04`
**Reference:** `docs/03-emby-api.md` §5

## Goal
Ask the server what it can serve, then build the correct stream URL for each quality profile. Also
the place where token redaction is enforced, because these URLs carry credentials.

## Files
- `crates/loxia-emby/src/endpoints/playback.rs`
- `crates/loxia-emby/src/stream.rs`

## Specification

```
pub struct PlaybackSource {
    pub media_source_id: MediaSourceId,
    pub container: String,
    pub supports_direct_stream: bool,
    pub supports_transcoding: bool,
    pub normalization_gain_db: Option<f32>,
    pub audio_stream: Option<AudioFormat>,
    pub lyric_stream: Option<LyricStreamRef>,
}

pub async fn playback_info(c: &EmbyClient, item: &ItemId) -> Result<PlaybackSource, EmbyError>;
```
`POST /Items/{id}/PlaybackInfo?UserId={uid}`. Take the first media source; log at `debug` when there
is more than one. Populating `lyric_stream` here means the lyrics task gets its input for free —
see `02-11`.

```
pub struct StreamUrl(Url);
impl StreamUrl {
    pub fn build(c: &EmbyClient, item: &ItemId, profile: QualityProfile, codec: TargetCodec) -> StreamUrl;
    pub fn as_str(&self) -> &str;
}
```

| Profile | URL |
| :-- | :-- |
| `Direct` | `/Audio/{id}/stream?static=true&api_key={token}` |
| `TranscodeHigh` | `/Audio/{id}/stream.{ext}?audioCodec={codec}&audioBitRate=320000&maxAudioChannels=2&api_key={token}` |
| `TranscodeMed` | same, `192000` |
| `TranscodeLow` | same, `96000` |

`ext` follows the codec: `mp3`, `aac` → `m4a`, `opus` → `opus`.

`api_key` must be in the query string because mpv issues its own HTTP request and does not share the
client's header map. Custom headers reach mpv separately via `http-header-fields` (task `05-05`).

**Redaction is mandatory and tested.** `StreamUrl`'s `Debug` and `Display` replace the `api_key`
value with `<redacted>`. `as_str()` returns the real URL and is the only accessor that does; it is
documented as "for the audio engine only — never log the result".

## Acceptance
- `playback_info_extracts_media_source_id` (fixture `playback_info.json`)
- `playback_info_extracts_audio_format`
- `playback_info_extracts_lyric_stream_when_present` (fixture `track_with_lyrics.json`)
- `playback_info_lyric_stream_none_when_absent` (fixture `track_no_lyrics.json`)
- `stream_url_snapshots` — an `insta` snapshot per profile, with the token replaced by a fixed
  placeholder.
- `codec_extension_mapping`
- `stream_url_debug_redacts_token`
- `stream_url_display_redacts_token`
- `stream_url_in_error_message_is_redacted` — format an `EmbyError` carrying a `StreamUrl` and
  assert the token is absent.

## Done when
The global DoD in `tasks/README.md` is satisfied.
