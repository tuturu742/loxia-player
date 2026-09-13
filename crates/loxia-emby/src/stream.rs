//! Audio stream URL builder for direct and transcode profiles.
//!
//! `api_key` must be in the query string because mpv issues its own HTTP request and does not
//! share `EmbyClient`'s header map. Custom headers reach mpv separately via `http-header-fields`
//! (`05-05`).

use std::fmt;

use reqwest::Url;

use loxia_core::config::{QualityProfile, TargetCodec};
use loxia_core::model::ItemId;

use crate::client::EmbyClient;

// No angle brackets: a URL query value is percent-encoded, so "<redacted>" would round-trip as
// "%3Credacted%3E" and defeat every substring assertion that looks for the literal text.
const REDACTED: &str = "REDACTED";

/// A stream URL, `api_key` and all. `Debug`/`Display` redact the token; [`StreamUrl::as_str`] is
/// the only accessor that returns the real thing, and is **for the audio engine only — never log
/// the result.**
pub struct StreamUrl(Url);

impl StreamUrl {
    pub fn build(
        client: &EmbyClient,
        item: &ItemId,
        profile: QualityProfile,
        codec: TargetCodec,
    ) -> StreamUrl {
        let token = client.access_token();
        let mut url = match profile {
            QualityProfile::Direct => client.url(&format!("Audio/{item}/stream")),
            _ => client.url(&format!("Audio/{item}/stream.{}", codec_extension(codec))),
        };

        {
            let mut query = url.query_pairs_mut();
            match profile {
                QualityProfile::Direct => {
                    query.append_pair("static", "true");
                }
                QualityProfile::TranscodeHigh
                | QualityProfile::TranscodeMed
                | QualityProfile::TranscodeLow => {
                    query
                        .append_pair("audioCodec", codec_query_name(codec))
                        .append_pair("audioBitRate", bitrate_for(profile))
                        .append_pair("maxAudioChannels", "2");
                }
            }
            query.append_pair("api_key", token);
        }

        StreamUrl(url)
    }

    /// The real URL, `api_key` and all — for the audio engine only, never log the result.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// `08-03`: `endpoints::download::fetch_to_file`'s own way in — crate-internal, since a
    /// caller outside `loxia-emby` should only ever see the redacted `Display`/`Debug` or
    /// `as_str`'s explicit "for the audio engine only" escape hatch, never a structured `Url`.
    pub(crate) fn as_url(&self) -> reqwest::Url {
        self.0.clone()
    }
}

fn bitrate_for(profile: QualityProfile) -> &'static str {
    match profile {
        QualityProfile::TranscodeHigh => "320000",
        QualityProfile::TranscodeMed => "192000",
        QualityProfile::TranscodeLow => "96000",
        QualityProfile::Direct => "",
    }
}

fn codec_extension(codec: TargetCodec) -> &'static str {
    match codec {
        TargetCodec::Mp3 => "mp3",
        TargetCodec::Aac => "m4a",
        TargetCodec::Opus => "opus",
    }
}

fn codec_query_name(codec: TargetCodec) -> &'static str {
    match codec {
        TargetCodec::Mp3 => "mp3",
        TargetCodec::Aac => "aac",
        TargetCodec::Opus => "opus",
    }
}

fn redacted(url: &Url) -> String {
    let mut out = url.clone();
    let replaced: Vec<(String, String)> = out
        .query_pairs()
        .map(|(k, v)| {
            if k == "api_key" {
                (k.into_owned(), REDACTED.to_string())
            } else {
                (k.into_owned(), v.into_owned())
            }
        })
        .collect();
    out.query_pairs_mut().clear().extend_pairs(&replaced);
    out.to_string()
}

impl fmt::Debug for StreamUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StreamUrl({})", redacted(&self.0))
    }
}

impl fmt::Display for StreamUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", redacted(&self.0))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use loxia_core::config::ServerConfig;

    use super::*;

    fn client() -> EmbyClient {
        let cfg = ServerConfig {
            id: "srv".to_string(),
            name: "Test".to_string(),
            url: "http://192.168.1.1:8096".to_string(),
            user_id: "user-1".to_string(),
            access_token: "SECRET-TOKEN".to_string(),
            device_id: "dev".to_string(),
            custom_headers: BTreeMap::new(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        };
        EmbyClient::new(&cfg).unwrap()
    }

    fn placeholder_token(url: &str) -> String {
        url.replace("SECRET-TOKEN", "TOKEN_PLACEHOLDER")
    }

    #[test]
    fn stream_url_snapshots() {
        let client = client();
        let item = ItemId::from("t1");

        let direct = StreamUrl::build(&client, &item, QualityProfile::Direct, TargetCodec::Mp3);
        insta::assert_snapshot!("direct", placeholder_token(direct.as_str()));

        let high = StreamUrl::build(
            &client,
            &item,
            QualityProfile::TranscodeHigh,
            TargetCodec::Mp3,
        );
        insta::assert_snapshot!("transcode_high_mp3", placeholder_token(high.as_str()));

        let med = StreamUrl::build(
            &client,
            &item,
            QualityProfile::TranscodeMed,
            TargetCodec::Aac,
        );
        insta::assert_snapshot!("transcode_med_aac", placeholder_token(med.as_str()));

        let low = StreamUrl::build(
            &client,
            &item,
            QualityProfile::TranscodeLow,
            TargetCodec::Opus,
        );
        insta::assert_snapshot!("transcode_low_opus", placeholder_token(low.as_str()));
    }

    #[test]
    fn codec_extension_mapping() {
        assert_eq!(codec_extension(TargetCodec::Mp3), "mp3");
        assert_eq!(codec_extension(TargetCodec::Aac), "m4a");
        assert_eq!(codec_extension(TargetCodec::Opus), "opus");
    }

    #[test]
    fn stream_url_debug_redacts_token() {
        let client = client();
        let url = StreamUrl::build(
            &client,
            &ItemId::from("t1"),
            QualityProfile::Direct,
            TargetCodec::Mp3,
        );
        let debug = format!("{url:?}");
        assert!(!debug.contains("SECRET-TOKEN"));
        assert!(debug.contains(REDACTED));
    }

    #[test]
    fn stream_url_display_redacts_token() {
        let client = client();
        let url = StreamUrl::build(
            &client,
            &ItemId::from("t1"),
            QualityProfile::Direct,
            TargetCodec::Mp3,
        );
        let display = format!("{url}");
        assert!(!display.contains("SECRET-TOKEN"));
        assert!(display.contains(REDACTED));
    }

    /// `EmbyError` has no variant carrying a `StreamUrl` — mpv fetches the stream URL directly,
    /// bypassing `loxia-emby`'s own HTTP client and its error classification entirely, so there is
    /// no real code path that constructs one. This proves the same property an error message would
    /// need: redaction survives when `StreamUrl` is nested inside another `Debug`-deriving type,
    /// which is what a future `loxia-audio` playback-failure error will do.
    #[test]
    fn stream_url_in_error_message_is_redacted() {
        #[derive(Debug)]
        struct FakePlaybackError {
            #[allow(
                dead_code,
                reason = "only ever read through the derived Debug impl under test"
            )]
            url: StreamUrl,
        }
        let client = client();
        let url = StreamUrl::build(
            &client,
            &ItemId::from("t1"),
            QualityProfile::Direct,
            TargetCodec::Mp3,
        );
        let err = FakePlaybackError { url };
        let message = format!("{err:?}");
        assert!(!message.contains("SECRET-TOKEN"));
    }
}
