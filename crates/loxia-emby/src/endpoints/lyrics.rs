//! Subtitle-stream discovery and lyric fetch. Emby has no lyrics service — it exposes `.lrc`
//! sidecars as Subtitle-type media streams on the audio item, so this is a subtitle fetch with a
//! different name (`03-emby-api.md` §7).

use reqwest::StatusCode;

use loxia_core::model::lyrics::{parse_lrc, parse_plain, parse_srt};
use loxia_core::model::{ItemId, LyricFormat, LyricStreamRef, Lyrics, MediaSourceId};

use crate::client::EmbyClient;
use crate::dto::media::MediaSourceDto;
use crate::error::EmbyError;

/// How much a lyric stream is worth picking, lowest first. A track can carry several — one file
/// with real timings alongside a plain-text transcript is common — and taking whichever came first
/// meant a track with perfectly good synced lyrics displayed the untimed copy instead (seen on
/// Amorphis, "The Gathering": a `text` stream at index 2 with every cue at `00:00:00`, and an `lrc`
/// stream at index 3 carrying the real times — `docs/12-decisions.md`).
fn format_rank(format: LyricFormat) -> u8 {
    match format {
        // Purpose-built for timed lyrics.
        LyricFormat::Lrc => 0,
        // Timed, and what a subtitle track normally is.
        LyricFormat::Srt => 1,
        // Never timed — a last resort.
        LyricFormat::Txt => 2,
    }
}

fn stream_format(codec: Option<&str>) -> Option<LyricFormat> {
    match codec?.to_ascii_lowercase().as_str() {
        "lrc" => Some(LyricFormat::Lrc),
        "srt" | "subrip" => Some(LyricFormat::Srt),
        "text" | "txt" => Some(LyricFormat::Txt),
        _ => None,
    }
}

/// Scans `source.media_streams` for the **most timeable** `Subtitle`-type stream whose codec is
/// `lrc`, `srt`/`subrip`, or `text` — see [`format_rank`]. Ties keep the file's own order. No match
/// — including no `Id` on the source itself, since a stream reference is meaningless without one —
/// yields `None`, meaning the track has no lyrics and the UI hides the pane entirely.
pub fn find_lyric_stream(source: &MediaSourceDto) -> Option<LyricStreamRef> {
    let media_source_id = source.id.clone()?;
    let (stream, format) = source
        .media_streams
        .iter()
        .filter(|s| s.stream_type.as_deref() == Some("Subtitle"))
        .filter_map(|s| stream_format(s.codec.as_deref()).map(|f| (s, f)))
        .min_by_key(|(_, format)| format_rank(*format))?;
    Some(LyricStreamRef {
        media_source_id: MediaSourceId::from(media_source_id),
        stream_index: stream.index?,
        format,
    })
}

fn parse_body(text: &str, format: LyricFormat) -> Lyrics {
    match format {
        LyricFormat::Lrc => parse_lrc(text),
        LyricFormat::Srt => parse_srt(text),
        LyricFormat::Txt => parse_plain(text),
    }
}

/// `GET /Items/{id}/{MediaSourceId}/Subtitles/{Index}/Stream.{format}`. Lyrics are cosmetic and
/// must never interrupt playback: a 404 or any other failure both degrade to
/// `Ok(Lyrics::Unsynced(vec![]))` rather than propagating an error — there is no user-visible
/// error path for lyrics. Conceptually a retryable read, but capped at a single attempt (unlike
/// `with_retry`'s fixed 3): a multi-second backoff loop for decoration would delay nothing useful,
/// and `with_retry` has no per-call attempt override, so this bypasses it entirely rather than
/// adding a parameter only this one call site would ever set to 1.
#[allow(
    clippy::unnecessary_wraps,
    reason = "Result<_, EmbyError> kept for API consistency with every sibling endpoint function; \
              this one never actually returns Err, by design"
)]
/// Emby's subtitle endpoint converts on the fly by file extension. Requesting `.lrc` is the natural
/// choice for an `lrc`-codec stream, but a live server answered it with an **empty body** — its
/// documented conversions are `srt`/`vtt`/`ass`, and `lrc` evidently isn't among them. So this asks
/// for the stream's own format first and falls back to `.srt` (universally supported, and timestamped,
/// so the result is still synced) whenever the first attempt yields nothing (`docs/12-decisions.md`).
pub async fn fetch(
    client: &EmbyClient,
    item: &ItemId,
    r: &LyricStreamRef,
) -> Result<Lyrics, EmbyError> {
    let primary = match r.format {
        LyricFormat::Lrc => "lrc",
        // A `text` stream is asked for as `.srt` too: Emby converts on the fly, and the SRT form is
        // the one that can carry timings at all.
        LyricFormat::Srt | LyricFormat::Txt => "srt",
    };
    let lyrics = fetch_as(client, item, r, primary).await?;
    if !lyrics.is_empty() || primary == "srt" {
        return Ok(lyrics);
    }
    tracing::debug!(item = %item, "empty lyrics for .{primary}; retrying as .srt");
    fetch_as(client, item, r, "srt").await
}

/// Which parser a body must be read with — derived from the extension **actually requested**, never
/// from the stream's declared format.
///
/// Those two are not the same thing: a `text` stream is fetched as `.srt`, and parsing that SRT
/// with `parse_plain` turned every cue number and every `00:00:00,000 --> 00:00:00,000` line into a
/// lyric line, which is exactly what a user saw on screen (`docs/12-decisions.md`). Deriving the
/// parser here, inside the one function that performs the request, is what stops the two drifting
/// apart again.
fn format_for_extension(ext: &str) -> LyricFormat {
    match ext {
        "lrc" => LyricFormat::Lrc,
        "txt" => LyricFormat::Txt,
        _ => LyricFormat::Srt,
    }
}

/// One attempt at a specific extension, parsed as that extension demands
/// ([`format_for_extension`]). Every failure (transport, 404, any other status) degrades to empty
/// lyrics rather than an error — lyrics are cosmetic (`docs/03-emby-api.md` §7) — which also lets
/// `fetch` above treat "empty" as "try the fallback".
async fn fetch_as(
    client: &EmbyClient,
    item: &ItemId,
    r: &LyricStreamRef,
    ext: &str,
) -> Result<Lyrics, EmbyError> {
    let parse_as = format_for_extension(ext);
    let path = format!(
        "Items/{item}/{}/Subtitles/{}/Stream.{ext}",
        r.media_source_id, r.stream_index
    );

    match client.get(&path).send().await {
        Ok(response) if response.status() == StatusCode::NOT_FOUND => {
            Ok(Lyrics::Unsynced(Vec::new()))
        }
        Ok(response) if response.status().is_success() => {
            let bytes = response.bytes().await.unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes);
            Ok(parse_body(&text, parse_as))
        }
        Ok(response) => {
            tracing::debug!(item = %item, status = %response.status(), "lyrics fetch failed; showing no lyrics");
            Ok(Lyrics::Unsynced(Vec::new()))
        }
        Err(source) => {
            tracing::debug!(item = %item, error = %source, "lyrics fetch failed; showing no lyrics");
            Ok(Lyrics::Unsynced(Vec::new()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use loxia_core::config::ServerConfig;

    use super::*;
    use crate::dto::media::MediaStreamDto;

    fn lrc_ref() -> LyricStreamRef {
        LyricStreamRef {
            media_source_id: MediaSourceId::from("src-1"),
            stream_index: 2,
            format: LyricFormat::Lrc,
        }
    }

    /// The real failure seen in the field: the server answers `.lrc` with an empty body (its
    /// conversions are srt/vtt/ass), which parsed into one blank line that counted as content — the
    /// pane reserved space and drew nothing. `.srt` is retried and is what actually has the lyrics
    /// (`docs/12-decisions.md`).
    #[tokio::test]
    async fn an_empty_lrc_response_falls_back_to_srt() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/src-1/Subtitles/2/Stream.lrc"))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/src-1/Subtitles/2/Stream.srt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("1\n00:00:01,000 --> 00:00:04,000\nthe first line\n\n"),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let lyrics = fetch(&client, &ItemId::from("t1"), &lrc_ref())
            .await
            .unwrap();

        assert!(
            !lyrics.is_empty(),
            "the .srt fallback must supply the lyrics"
        );
        // `parse_srt` keeps each cue's start time, so the fallback scrolls with playback exactly
        // like a real `.lrc` rather than sitting static (`docs/12-decisions.md`).
        match lyrics {
            Lyrics::Synced(lines) => assert_eq!(
                lines,
                vec![loxia_core::model::LyricLine {
                    at: std::time::Duration::from_secs(1),
                    text: "the first line".to_string(),
                }]
            ),
            other => panic!("expected timed lines from srt, got {other:?}"),
        }
    }

    /// A server that *does* serve `.lrc` is left alone — no second request.
    #[tokio::test]
    async fn a_usable_lrc_response_is_not_refetched() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/src-1/Subtitles/2/Stream.lrc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[00:01.00]hello\n"))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let lyrics = fetch(&client, &ItemId::from("t1"), &lrc_ref())
            .await
            .unwrap();
        assert!(!lyrics.is_empty());
    }

    fn cfg(url: &str) -> ServerConfig {
        ServerConfig {
            id: "srv".to_string(),
            name: "Test".to_string(),
            url: url.to_string(),
            user_id: "user-1".to_string(),
            access_token: "tok".to_string(),
            device_id: "dev".to_string(),
            custom_headers: BTreeMap::new(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }
    }

    fn stream(stream_type: &str, codec: &str, index: u32) -> MediaStreamDto {
        MediaStreamDto {
            index: Some(index),
            stream_type: Some(stream_type.to_string()),
            codec: Some(codec.to_string()),
            language: None,
            is_default: false,
            bit_rate: None,
            sample_rate: None,
            bit_depth: None,
            channels: None,
        }
    }

    fn source_with_streams(streams: Vec<MediaStreamDto>) -> MediaSourceDto {
        MediaSourceDto {
            id: Some("ms1".to_string()),
            container: None,
            bitrate: None,
            size: None,
            supports_direct_stream: false,
            supports_transcoding: false,
            media_streams: streams,
            normalization_gain: None,
        }
    }

    /// The exact shape of Amorphis' "The Gathering": an untimed `text` transcript first, and the
    /// `lrc` stream carrying the real times after it. Taking the first stream showed the untimed
    /// copy for a track that has perfectly good synced lyrics (`docs/12-decisions.md`).
    #[test]
    fn a_timed_stream_wins_over_an_untimed_one_whatever_the_order() {
        let picked = find_lyric_stream(&source_with_streams(vec![
            stream("Audio", "flac", 1),
            stream("Subtitle", "text", 2),
            stream("Subtitle", "lrc", 3),
        ]))
        .expect("a lyric stream");
        assert_eq!(picked.stream_index, 3);
        assert_eq!(picked.format, LyricFormat::Lrc);

        // ...and the same when the good one happens to come first.
        let picked = find_lyric_stream(&source_with_streams(vec![
            stream("Subtitle", "lrc", 2),
            stream("Subtitle", "text", 3),
        ]))
        .expect("a lyric stream");
        assert_eq!(picked.stream_index, 2);
    }

    /// A `text` stream is still used when it is all there is.
    #[test]
    fn an_untimed_stream_is_used_when_it_is_the_only_one() {
        let picked = find_lyric_stream(&source_with_streams(vec![stream("Subtitle", "text", 2)]))
            .expect("a lyric stream");
        assert_eq!(picked.format, LyricFormat::Txt);
    }

    /// Equal-ranked streams keep the file's own order, so the pick is stable rather than arbitrary.
    #[test]
    fn equally_ranked_streams_keep_file_order() {
        let picked = find_lyric_stream(&source_with_streams(vec![
            stream("Subtitle", "srt", 4),
            stream("Subtitle", "subrip", 5),
        ]))
        .expect("a lyric stream");
        assert_eq!(picked.stream_index, 4);
    }

    /// The body must be read with the parser the **requested extension** demands, not the stream's
    /// declared format. A `text` stream is fetched as `.srt`, and reading that with `parse_plain`
    /// put every cue number and every `00:00:00,000 --> 00:00:00,000` line on screen as a lyric —
    /// exactly what a user reported (`docs/12-decisions.md`).
    #[tokio::test]
    async fn a_text_stream_fetched_as_srt_is_parsed_as_srt() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/src-1/Subtitles/2/Stream.srt"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "\u{feff}1\n00:00:00,000 --> 00:00:00,000\nAs I sense their steel,\n\n\
                 2\n00:00:00,000 --> 00:00:00,000\nAs I see the mighty one,\n",
            ))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let lyrics = fetch(
            &client,
            &ItemId::from("t1"),
            &LyricStreamRef {
                media_source_id: MediaSourceId::from("src-1"),
                stream_index: 2,
                format: LyricFormat::Txt,
            },
        )
        .await
        .unwrap();

        // All-zero cues carry no timing, so this degrades to plain lines — but *only* the payload.
        assert_eq!(
            lyrics,
            Lyrics::Unsynced(vec![
                "As I sense their steel,".to_string(),
                "As I see the mighty one,".to_string(),
            ]),
            "cue numbers and timecodes must never reach the pane"
        );
    }

    fn load_fixture_items(name: &str) -> serde_json::Value {
        let text = std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn media_source_from_items_fixture(name: &str) -> MediaSourceDto {
        let fixture = load_fixture_items(name);
        let raw = fixture["Items"][0]["MediaSources"][0].clone();
        serde_json::from_value(raw).unwrap()
    }

    #[test]
    fn finds_lrc_subtitle_stream() {
        let source = media_source_from_items_fixture("track_with_lyrics.json");
        let found = find_lyric_stream(&source).unwrap();
        assert_eq!(found.format, LyricFormat::Lrc);
        assert_eq!(found.stream_index, 2);
        assert_eq!(found.media_source_id.as_str(), "mediasource_133105");
    }

    #[test]
    fn returns_none_when_no_subtitle_stream() {
        let source = media_source_from_items_fixture("track_no_lyrics.json");
        assert!(find_lyric_stream(&source).is_none());
    }

    #[test]
    fn ignores_non_text_subtitle_codecs() {
        let source = source_with_streams(vec![
            stream("Audio", "mp3", 0),
            stream("Subtitle", "pgs", 1),
        ]);
        assert!(find_lyric_stream(&source).is_none());
    }

    #[test]
    fn codec_to_format_mapping() {
        let cases = [
            ("lrc", LyricFormat::Lrc),
            ("LRC", LyricFormat::Lrc),
            ("srt", LyricFormat::Srt),
            ("subrip", LyricFormat::Srt),
            ("SubRip", LyricFormat::Srt),
            ("text", LyricFormat::Txt),
            ("TEXT", LyricFormat::Txt),
        ];
        for (codec, expected) in cases {
            let source = source_with_streams(vec![stream("Subtitle", codec, 0)]);
            let found = find_lyric_stream(&source)
                .unwrap_or_else(|| panic!("expected a match for {codec}"));
            assert_eq!(found.format, expected, "codec {codec}");
        }
    }

    #[tokio::test]
    async fn fetch_builds_items_route_not_videos() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/ms1/Subtitles/2/Stream.lrc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[00:01.00]hello"))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let r = LyricStreamRef {
            media_source_id: MediaSourceId::from("ms1"),
            stream_index: 2,
            format: LyricFormat::Lrc,
        };
        let lyrics = fetch(&client, &ItemId::from("t1"), &r).await.unwrap();
        assert!(!lyrics.is_empty());
    }

    #[tokio::test]
    async fn fetch_parses_lrc_into_synced() {
        let server = MockServer::start().await;
        let body = std::fs::read_to_string(format!(
            "{}/tests/fixtures/lyrics.lrc",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/ms1/Subtitles/2/Stream.lrc"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let r = LyricStreamRef {
            media_source_id: MediaSourceId::from("ms1"),
            stream_index: 2,
            format: LyricFormat::Lrc,
        };
        let lyrics = fetch(&client, &ItemId::from("t1"), &r).await.unwrap();
        assert!(matches!(lyrics, Lyrics::Synced(_)));
        assert!(!lyrics.is_empty());
    }

    #[tokio::test]
    async fn fetch_404_returns_empty_not_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/ms1/Subtitles/2/Stream.lrc"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let r = LyricStreamRef {
            media_source_id: MediaSourceId::from("ms1"),
            stream_index: 2,
            format: LyricFormat::Lrc,
        };
        let lyrics = fetch(&client, &ItemId::from("t1"), &r).await.unwrap();
        assert_eq!(lyrics, Lyrics::Unsynced(Vec::new()));
    }

    #[tokio::test]
    async fn fetch_handles_invalid_utf8() {
        let server = MockServer::start().await;
        let mut body = b"[00:01.00]before ".to_vec();
        body.push(0xFF); // invalid UTF-8 byte
        body.extend_from_slice(b" after");
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/ms1/Subtitles/2/Stream.lrc"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let r = LyricStreamRef {
            media_source_id: MediaSourceId::from("ms1"),
            stream_index: 2,
            format: LyricFormat::Lrc,
        };
        let lyrics = fetch(&client, &ItemId::from("t1"), &r).await.unwrap();
        assert!(!lyrics.is_empty());
    }
}
