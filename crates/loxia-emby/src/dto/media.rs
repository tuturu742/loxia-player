//! MediaSourceDto, MediaStreamDto.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaSourceDto {
    pub id: Option<String>,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub bitrate: Option<u32>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub supports_direct_stream: bool,
    #[serde(default)]
    pub supports_transcoding: bool,
    #[serde(default)]
    pub media_streams: Vec<MediaStreamDto>,
    /// Loudness-normalisation value. Not observed in any `02-01` fixture — the captured tracks
    /// carry no embedded ReplayGain tags — but Emby exposes it on some libraries; kept here
    /// unused until `09-04`'s fallback chain reads it from its own `PlaybackInfo` fetch.
    #[serde(default)]
    pub normalization_gain: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaStreamDto {
    #[serde(default)]
    pub index: Option<u32>,
    #[serde(rename = "Type", default)]
    pub stream_type: Option<String>,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub bit_rate: Option<u32>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub bit_depth: Option<u8>,
    #[serde(default)]
    pub channels: Option<u8>,
}
