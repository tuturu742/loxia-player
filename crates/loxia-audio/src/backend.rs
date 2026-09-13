//! AudioBackend trait, AudioCommand, AudioEvent (`docs/05-audio-engine.md` §§1-2).
//!
//! Everything above this layer talks only to [`AudioBackend`] — `MpvEngine` (`05-03`/`05-04`) and
//! `MockEngine` (`05-02`) are its only two implementations.

use std::time::Duration;

/// Moved to `loxia-core` (`10-06`) since it's pure data `loxia-tui`'s equalizer modal also needs
/// and cannot reach otherwise — re-exported here for this crate's own existing callers.
pub use loxia_core::config::EQ_BANDS_HZ;
use loxia_core::config::ReplayGainMode;
use loxia_core::effect::RedactedUrl;
use loxia_core::model::{AudioDevice, AudioFormat};
use loxia_core::state::player::{PlayStatus, SeekTarget};
use tokio::sync::mpsc;

use crate::error::AudioError;

/// One gain in dB per `EQ_BANDS_HZ` entry. A distinct type from `loxia_core::effect::EqCurve`
/// (a bare `[f32; 10]` alias `Effect::Audio::SetEq` already carries) — this task's own spec names
/// a wrapping struct rather than reusing that alias; see `docs/12-decisions.md` for why they
/// aren't unified. The audio worker (`05-06`) converts one to the other at the effect/command
/// boundary (`EqCurve { gains: curve }`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqCurve {
    pub gains: [f32; 10],
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioCommand {
    Load {
        // A plain `String` per this task's own spec would defeat the point of redacting it —
        // `loxia_core::effect::RedactedUrl` already exists for exactly this (a stream URL's
        // `api_key=...` query parameter must never reach a derived `Debug`), lives in
        // `loxia-core` (not `loxia-emby`, so the "must not depend on loxia-emby" rule still
        // holds), and lets `AudioCommand` keep a plain `#[derive(Debug, ...)]` while still
        // redacting correctly.
        url: RedactedUrl,
        headers: Vec<(String, String)>,
        start_at: Duration,
        gain_db: Option<f32>,
    },
    Preload {
        url: RedactedUrl,
        headers: Vec<(String, String)>,
        gain_db: Option<f32>,
    },
    Play,
    Pause,
    Stop,
    Seek(SeekTarget),
    SetVolume(u8),
    SetMute(bool),
    SetEq(Option<EqCurve>),
    SetReplayGain(ReplayGainMode),
    SetDevice(String),
    EnumerateDevices,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    StatusChanged(PlayStatus),
    /// Throttled to 4 Hz by the backend — a raw per-mpv-callback rate would flood the channel and
    /// the render loop (`docs/05-audio-engine.md` §2).
    Position {
        secs: f64,
        duration: f64,
    },
    /// Emitted once per load, not per position tick.
    Format(AudioFormat),
    TrackEnded {
        natural: bool,
    },
    Devices(Vec<AudioDevice>),
    Buffering(u8),
    /// The engine's own volume/mute state, e.g. after `SetVolume`/`SetMute` completes. Added in
    /// `05-04`: `docs/05-audio-engine.md` §2's own table has no volume/mute-carrying variant, but
    /// §3's property→event mapping requires one (`PlayStatus`, `StatusChanged`'s payload, has no
    /// room for either value). See `docs/12-decisions.md`.
    VolumeChanged {
        volume: u8,
        muted: bool,
    },
    Error(AudioError),
}

/// The whole workspace's only door into the audio engine. `Send` (not `Sync`) because the real
/// implementation owns a dedicated mpv thread and communicates over channels internally — nothing
/// above this layer needs to call it from more than one thread at once.
pub trait AudioBackend: Send {
    fn send(&self, cmd: AudioCommand) -> Result<(), AudioError>;
    fn subscribe(&self) -> mpsc::UnboundedReceiver<AudioEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_command_debug_redacts_api_key() {
        let cmd = AudioCommand::Load {
            url: RedactedUrl::new("http://host/stream?api_key=secret&Static=true"),
            headers: Vec::new(),
            start_at: Duration::ZERO,
            gain_db: None,
        };
        let debug = format!("{cmd:?}");
        assert!(!debug.contains("secret"));
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn eq_bands_are_iso_standard() {
        assert_eq!(
            EQ_BANDS_HZ,
            [31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
        );
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn command_and_event_are_send_sync() {
        assert_send_sync::<AudioCommand>();
        assert_send_sync::<AudioEvent>();
    }

    #[test]
    fn backend_trait_is_object_safe() {
        fn assert_object_safe(_: &dyn AudioBackend) {}
        let _ = assert_object_safe;
    }
}
