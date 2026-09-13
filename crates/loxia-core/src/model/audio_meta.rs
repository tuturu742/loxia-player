//! Codec, AudioFormat, ReplayGainInfo.

use serde::{Deserialize, Serialize};

/// An audio codec. `Other` preserves the server's own string rather than failing — a client must
/// never refuse to describe a format it doesn't specifically recognise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Codec {
    Flac,
    Alac,
    Mp3,
    Aac,
    Opus,
    Vorbis,
    Wav,
    Other(String),
}

impl Codec {
    /// Case-insensitive; an unrecognised value becomes `Other` rather than an error.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "flac" => Codec::Flac,
            "alac" => Codec::Alac,
            "mp3" => Codec::Mp3,
            "aac" => Codec::Aac,
            "opus" => Codec::Opus,
            "vorbis" | "ogg" => Codec::Vorbis,
            "wav" | "wave" | "pcm" => Codec::Wav,
            _ => Codec::Other(s.to_string()),
        }
    }

    /// The upper-case label used in the player bar, e.g. "FLAC".
    pub fn label(&self) -> String {
        match self {
            Codec::Flac => "FLAC".to_string(),
            Codec::Alac => "ALAC".to_string(),
            Codec::Mp3 => "MP3".to_string(),
            Codec::Aac => "AAC".to_string(),
            Codec::Opus => "Opus".to_string(),
            Codec::Vorbis => "Vorbis".to_string(),
            Codec::Wav => "WAV".to_string(),
            Codec::Other(s) => s.to_uppercase(),
        }
    }
}

/// The decoded audio format, as reported by the playback engine (never the metadata guess) —
/// rendered verbatim in the player bar's `🎚 FLAC 16-bit / 44.1 kHz │ Bitrate: 1012 kbps` line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioFormat {
    pub codec: Codec,
    pub sample_rate_hz: u32,
    pub bit_depth: Option<u8>,
    pub channels: u8,
    pub bitrate_bps: Option<u32>,
}

impl AudioFormat {
    /// The player-bar summary, e.g. `FLAC 16-bit / 44.1 kHz` or `MP3 / 48.0 kHz` when the bit
    /// depth is unknown (as it always is for lossy codecs).
    pub fn summary(&self) -> String {
        let khz = self.sample_rate_hz as f64 / 1000.0;
        match self.bit_depth {
            Some(bits) => format!("{} {}-bit / {:.1} kHz", self.codec.label(), bits, khz),
            None => format!("{} / {:.1} kHz", self.codec.label(), khz),
        }
    }
}

/// ReplayGain tags read from the track/album metadata. `None` fields mean the tag was absent, not
/// zero — see `loxia-audio::replaygain` for the fallback chain when none of this is present.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReplayGainInfo {
    pub track_gain_db: Option<f32>,
    pub album_gain_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_peak: Option<f32>,
}

/// One entry of mpv's `audio-device-list` (`docs/05-audio-engine.md` §7). Lives in `loxia-core`
/// rather than `loxia-audio` because `state::modal::Modal::DevicePicker` needs it and `loxia-core`
/// cannot depend on `loxia-audio` (the dependency runs the other way).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub description: String,
    pub driver: String,
}

/// "Most capable first" (`docs/05-audio-engine.md` §4): ALSA, WASAPI, CoreAudio, then PipeWire,
/// Pulse. A driver not in this list (unlikely — mpv's own `ao` list is fixed) sorts after all of
/// these, in whatever order it was first encountered. Lives here rather than in
/// `loxia-audio::device` (which originally defined it, `09-01`) because it and [`device_label`]
/// are pure functions of [`AudioDevice`]'s own fields with no OS-specific behaviour at all.
/// `loxia-tui`'s device-picker modal (`10-05`) needs grouping/labelling too, and cannot depend on
/// `loxia-audio` (the dependency runs the other way) — defining this once here, re-exported by
/// `loxia-audio::device` for its own existing callers, avoids duplicating logic that has no reason
/// to diverge between the two crates.
const DRIVER_ORDER: [&str; 5] = ["alsa", "wasapi", "coreaudio", "pipewire", "pulse"];

fn driver_rank(driver: &str) -> usize {
    DRIVER_ORDER
        .iter()
        .position(|d| *d == driver)
        .unwrap_or(DRIVER_ORDER.len())
}

/// Groups `devices` by their own `driver` field, the groups themselves ordered by
/// [`DRIVER_ORDER`] — a stable sort, so two drivers tied for "not in the list" keep whatever
/// relative order `devices` itself gave them.
pub fn group_by_driver(devices: &[AudioDevice]) -> Vec<(String, Vec<AudioDevice>)> {
    let mut drivers: Vec<String> = Vec::new();
    for d in devices {
        if !drivers.contains(&d.driver) {
            drivers.push(d.driver.clone());
        }
    }
    drivers.sort_by_key(|driver| driver_rank(driver));

    drivers
        .into_iter()
        .map(|driver| {
            let group = devices
                .iter()
                .filter(|d| d.driver == driver)
                .cloned()
                .collect();
            (driver, group)
        })
        .collect()
}

/// The device-picker's own row text.
pub fn device_label(d: &AudioDevice) -> String {
    d.description.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, driver: &str) -> AudioDevice {
        AudioDevice {
            id: id.to_string(),
            description: format!("{id} description"),
            driver: driver.to_string(),
        }
    }

    #[test]
    fn group_by_driver_orders_most_capable_first() {
        let devices = vec![
            device("pulse/a", "pulse"),
            device("coreaudio/a", "coreaudio"),
            device("alsa/hw:0,0", "alsa"),
            device("wasapi/a", "wasapi"),
            device("pipewire/a", "pipewire"),
            device("weird/a", "weird"),
        ];

        let groups = group_by_driver(&devices);
        let order: Vec<&str> = groups.iter().map(|(driver, _)| driver.as_str()).collect();

        assert_eq!(
            order,
            vec!["alsa", "wasapi", "coreaudio", "pipewire", "pulse", "weird"]
        );
    }

    #[test]
    fn group_by_driver_keeps_every_device() {
        let devices = vec![
            device("alsa/hw:0,0", "alsa"),
            device("alsa/hw:1,0", "alsa"),
            device("pulse/a", "pulse"),
        ];
        let groups = group_by_driver(&devices);
        let total: usize = groups.iter().map(|(_, g)| g.len()).sum();
        assert_eq!(total, 3);
        let alsa_group = groups.iter().find(|(d, _)| d == "alsa").unwrap();
        assert_eq!(alsa_group.1.len(), 2);
    }

    #[test]
    fn device_label_is_the_description() {
        assert_eq!(
            device_label(&device("pulse/a", "pulse")),
            "pulse/a description"
        );
    }

    #[test]
    fn codec_from_str_is_case_insensitive() {
        assert_eq!(Codec::from_str_lossy("FLAC"), Codec::Flac);
        assert_eq!(Codec::from_str_lossy("flac"), Codec::Flac);
        assert_eq!(Codec::from_str_lossy("FlAc"), Codec::Flac);
    }

    #[test]
    fn unknown_codec_becomes_other() {
        assert_eq!(
            Codec::from_str_lossy("dts"),
            Codec::Other("dts".to_string())
        );
    }

    #[test]
    fn audio_format_summary_snapshot() {
        let cases = [
            (
                AudioFormat {
                    codec: Codec::Flac,
                    sample_rate_hz: 44_100,
                    bit_depth: Some(16),
                    channels: 2,
                    bitrate_bps: Some(1_012_000),
                },
                "FLAC 16-bit / 44.1 kHz",
            ),
            (
                AudioFormat {
                    codec: Codec::Flac,
                    sample_rate_hz: 96_000,
                    bit_depth: Some(24),
                    channels: 2,
                    bitrate_bps: Some(2_840_000),
                },
                "FLAC 24-bit / 96.0 kHz",
            ),
            (
                AudioFormat {
                    codec: Codec::Mp3,
                    sample_rate_hz: 44_100,
                    bit_depth: None,
                    channels: 2,
                    bitrate_bps: Some(320_000),
                },
                "MP3 / 44.1 kHz",
            ),
            (
                AudioFormat {
                    codec: Codec::Opus,
                    sample_rate_hz: 48_000,
                    bit_depth: None,
                    channels: 2,
                    bitrate_bps: Some(128_000),
                },
                "Opus / 48.0 kHz",
            ),
        ];
        for (fmt, expected) in cases {
            assert_eq!(fmt.summary(), expected);
        }
    }
}
