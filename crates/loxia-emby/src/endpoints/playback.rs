//! PlaybackInfo, stream URL selection, and playback reporting.

use std::time::Duration;

use serde::Deserialize;
use serde::de::Error as _;

use loxia_core::model::{
    AudioFormat, Codec, ItemId, LyricStreamRef, MediaSourceId, PlaybackReport,
};

use crate::client::{EmbyClient, parse_retry_after};
use crate::dto::media::MediaSourceDto;
use crate::dto::ticks::duration_to_ticks;
use crate::error::{EmbyError, classify_status, classify_transport};

use super::send_mutation;

pub struct PlaybackSource {
    pub media_source_id: MediaSourceId,
    pub container: String,
    pub supports_direct_stream: bool,
    pub supports_transcoding: bool,
    pub normalization_gain_db: Option<f32>,
    pub audio_stream: Option<AudioFormat>,
    pub lyric_stream: Option<LyricStreamRef>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlaybackInfoResponse {
    #[serde(default)]
    media_sources: Vec<MediaSourceDto>,
}

fn audio_stream_of(source: &MediaSourceDto) -> Option<AudioFormat> {
    let stream = source
        .media_streams
        .iter()
        .find(|s| s.stream_type.as_deref() == Some("Audio"))?;
    Some(AudioFormat {
        codec: stream
            .codec
            .as_deref()
            .map(Codec::from_str_lossy)
            .unwrap_or(Codec::Other(String::new())),
        sample_rate_hz: stream.sample_rate.unwrap_or(0),
        bit_depth: stream.bit_depth,
        channels: stream.channels.unwrap_or(0),
        bitrate_bps: stream.bit_rate,
    })
}

/// `POST /Items/{id}/PlaybackInfo?UserId={uid}`. Takes the first media source; logs at `debug`
/// when there is more than one. Populating `lyric_stream` here means the lyrics task (`02-11`)
/// gets its input for free.
pub async fn playback_info(
    client: &EmbyClient,
    item: &ItemId,
) -> Result<PlaybackSource, EmbyError> {
    let path = format!("Items/{item}/PlaybackInfo");
    let pairs = [("UserId".to_string(), client.user_id().to_string())];

    let response = client
        .post(&path)
        .query(&pairs)
        .send()
        .await
        .map_err(classify_transport)?;
    let status = response.status();
    if !status.is_success() {
        let retry_after = parse_retry_after(response.headers());
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(
            status.as_u16(),
            retry_after,
            Some(item.clone()),
            message,
        ));
    }

    let text = response.text().await.map_err(classify_transport)?;
    let parsed: PlaybackInfoResponse =
        serde_json::from_str(&text).map_err(|source| EmbyError::Decode {
            endpoint: path.clone(),
            source,
        })?;

    if parsed.media_sources.len() > 1 {
        tracing::debug!(item = %item, count = parsed.media_sources.len(), "multiple media sources in PlaybackInfo; using the first");
    }

    let source = parsed
        .media_sources
        .into_iter()
        .next()
        .ok_or_else(|| EmbyError::Decode {
            endpoint: path,
            source: serde_json::Error::custom("PlaybackInfo returned no media sources"),
        })?;

    let media_source_id = MediaSourceId::from(source.id.clone().unwrap_or_default());
    let audio_stream = audio_stream_of(&source);
    let lyric_stream = super::lyrics::find_lyric_stream(&source);

    Ok(PlaybackSource {
        container: source.container.clone().unwrap_or_default(),
        supports_direct_stream: source.supports_direct_stream,
        supports_transcoding: source.supports_transcoding,
        normalization_gain_db: source.normalization_gain,
        audio_stream,
        lyric_stream,
        media_source_id,
    })
}

/// Pure request shape (path + JSON body) for a report, kept separate from the actual send so it
/// can be snapshot-tested without any I/O. `Played` has no body — it is `POST`ed with an empty one.
fn request_shape(user_id: &str, report: &PlaybackReport) -> (String, Option<serde_json::Value>) {
    match report {
        PlaybackReport::Start { item, session } => (
            "Sessions/Playing".to_string(),
            Some(serde_json::json!({
                "ItemId": item.as_str(),
                "PlaySessionId": session.as_str(),
                "CanSeek": true,
                "PositionTicks": 0,
            })),
        ),
        PlaybackReport::Progress {
            item,
            session,
            position,
            paused,
            play_method,
        } => (
            "Sessions/Playing/Progress".to_string(),
            Some(serde_json::json!({
                "ItemId": item.as_str(),
                "PlaySessionId": session.as_str(),
                "PositionTicks": duration_to_ticks(*position),
                "IsPaused": paused,
                "PlayMethod": play_method.as_str(),
            })),
        ),
        PlaybackReport::Stopped {
            item,
            session,
            position,
        } => (
            "Sessions/Playing/Stopped".to_string(),
            Some(serde_json::json!({
                "ItemId": item.as_str(),
                "PlaySessionId": session.as_str(),
                "PositionTicks": duration_to_ticks(*position),
            })),
        ),
        PlaybackReport::Played { item } => (format!("Users/{user_id}/PlayedItems/{item}"), None),
    }
}

fn report_item(report: &PlaybackReport) -> ItemId {
    match report {
        PlaybackReport::Start { item, .. }
        | PlaybackReport::Progress { item, .. }
        | PlaybackReport::Stopped { item, .. }
        | PlaybackReport::Played { item } => item.clone(),
    }
}

/// These are mutations: **no retry**. A failure propagates so the caller can buffer it offline
/// (`08-07`) rather than silently losing the scrobble.
pub async fn report(client: &EmbyClient, report: &PlaybackReport) -> Result<(), EmbyError> {
    let (path, body) = request_shape(client.user_id().as_str(), report);
    let request = match &body {
        Some(b) => client.post(&path).json(b),
        None => client.post(&path),
    };
    send_mutation(request, Some(report_item(report))).await
}

/// True when `position >= 0.9 * duration` **or** `position >= 240s`, whichever comes first. A
/// zero `duration` is never complete. This one function gates both the `Played` report and the
/// history entry, so the two can never disagree.
pub fn is_complete(position: Duration, duration: Duration) -> bool {
    if duration.is_zero() {
        return false;
    }
    position.as_secs_f64() >= 0.9 * duration.as_secs_f64() || position >= Duration::from_secs(240)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use loxia_core::config::{QualityProfile, ServerConfig};
    use loxia_core::model::{LyricFormat, PlayMethod, PlaySessionId};

    use super::*;

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

    fn load_fixture(name: &str) -> serde_json::Value {
        let text = std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// `playback_info.json`/`track_with_lyrics.json`/`track_no_lyrics.json` were captured against
    /// different endpoints (`02-01`) — the latter two are `Items` envelopes carrying a full item,
    /// captured specifically to verify the lyric subtitle-stream shape. Their `MediaSources` array
    /// is exactly what a real `PlaybackInfo` response carries, so tests repackage it into that
    /// response's actual shape (`{MediaSources, PlaySessionId}`) rather than feeding the raw file.
    fn playback_info_body_from_items_fixture(name: &str) -> serde_json::Value {
        let fixture = load_fixture(name);
        let media_sources = fixture["Items"][0]["MediaSources"].clone();
        serde_json::json!({ "MediaSources": media_sources, "PlaySessionId": "test-session" })
    }

    #[tokio::test]
    async fn playback_info_extracts_media_source_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Items/1517/PlaybackInfo"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(load_fixture("playback_info.json").to_string()),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let source = playback_info(&client, &ItemId::from("1517")).await.unwrap();
        assert_eq!(source.media_source_id.as_str(), "mediasource_1517");
    }

    #[tokio::test]
    async fn playback_info_extracts_audio_format() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Items/1517/PlaybackInfo"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(load_fixture("playback_info.json").to_string()),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let source = playback_info(&client, &ItemId::from("1517")).await.unwrap();
        let audio = source.audio_stream.unwrap();
        assert_eq!(audio.codec, Codec::Mp3);
        assert_eq!(audio.sample_rate_hz, 44_100);
        assert_eq!(audio.channels, 2);
    }

    #[tokio::test]
    async fn playback_info_extracts_lyric_stream_when_present() {
        let server = MockServer::start().await;
        let body = playback_info_body_from_items_fixture("track_with_lyrics.json");
        Mock::given(method("POST"))
            .and(path("/emby/Items/133105/PlaybackInfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let source = playback_info(&client, &ItemId::from("133105"))
            .await
            .unwrap();
        let lyric = source.lyric_stream.unwrap();
        assert_eq!(lyric.format, LyricFormat::Lrc);
        assert_eq!(lyric.stream_index, 2);
    }

    #[tokio::test]
    async fn playback_info_lyric_stream_none_when_absent() {
        let server = MockServer::start().await;
        let body = playback_info_body_from_items_fixture("track_no_lyrics.json");
        Mock::given(method("POST"))
            .and(path("/emby/Items/no-lyrics/PlaybackInfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let source = playback_info(&client, &ItemId::from("no-lyrics"))
            .await
            .unwrap();
        assert!(source.lyric_stream.is_none());
    }

    fn session() -> PlaySessionId {
        PlaySessionId::from("session-1")
    }

    #[test]
    fn report_snapshots() {
        let start = PlaybackReport::Start {
            item: ItemId::from("t1"),
            session: session(),
        };
        let progress = PlaybackReport::Progress {
            item: ItemId::from("t1"),
            session: session(),
            position: Duration::from_secs(90),
            paused: false,
            play_method: PlayMethod::DirectStream,
        };
        let stopped = PlaybackReport::Stopped {
            item: ItemId::from("t1"),
            session: session(),
            position: Duration::from_secs(200),
        };
        let played = PlaybackReport::Played {
            item: ItemId::from("t1"),
        };

        for (name, r) in [
            ("start", &start),
            ("progress", &progress),
            ("stopped", &stopped),
            ("played", &played),
        ] {
            let (path, body) = request_shape("user-1", r);
            let snapshot = match body {
                Some(b) => format!("{path}\n{}", serde_json::to_string_pretty(&b).unwrap()),
                None => path,
            };
            insta::assert_snapshot!(name, snapshot);
        }
    }

    #[test]
    fn positions_are_converted_to_ticks() {
        let (_, body) = request_shape(
            "user-1",
            &PlaybackReport::Stopped {
                item: ItemId::from("t1"),
                session: session(),
                position: Duration::from_secs(5),
            },
        );
        assert_eq!(body.unwrap()["PositionTicks"], 50_000_000);
    }

    #[test]
    fn play_method_reflects_quality_profile() {
        assert_eq!(
            PlayMethod::from_quality_profile(QualityProfile::Direct),
            PlayMethod::DirectStream
        );
        assert_eq!(
            PlayMethod::from_quality_profile(QualityProfile::TranscodeHigh),
            PlayMethod::Transcode
        );
        assert_eq!(
            PlayMethod::from_quality_profile(QualityProfile::TranscodeMed),
            PlayMethod::Transcode
        );
        assert_eq!(
            PlayMethod::from_quality_profile(QualityProfile::TranscodeLow),
            PlayMethod::Transcode
        );
    }

    #[test]
    fn is_complete_at_90_percent() {
        let duration = Duration::from_secs(200);
        assert!(!is_complete(Duration::from_secs(179), duration));
        assert!(is_complete(Duration::from_secs(180), duration));
    }

    #[test]
    fn is_complete_at_four_minutes_for_long_track() {
        let hour = Duration::from_secs(3600);
        assert!(!is_complete(Duration::from_secs(239), hour));
        assert!(is_complete(Duration::from_secs(240), hour));
    }

    #[test]
    fn is_complete_false_for_zero_duration() {
        assert!(!is_complete(Duration::from_secs(0), Duration::ZERO));
        assert!(!is_complete(Duration::from_secs(100), Duration::ZERO));
    }

    #[test]
    fn report_is_serde_roundtrippable() {
        let r = PlaybackReport::Progress {
            item: ItemId::from("t1"),
            session: session(),
            position: Duration::from_secs(42),
            paused: true,
            play_method: PlayMethod::Transcode,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: PlaybackReport = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }

    #[tokio::test]
    async fn report_does_not_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Sessions/Playing"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let err = report(
            &client,
            &PlaybackReport::Start {
                item: ItemId::from("t1"),
                session: session(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, EmbyError::Transient { .. }));
    }
}
