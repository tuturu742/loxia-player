//! Player state: what the audio engine is doing, and the playback-reporting bookkeeping tied to
//! the current session (`02-10`, `06-01`).

use std::time::Duration;

/// Mirrors the audio backend's own status (`loxia_audio::backend::AudioEvent::StatusChanged`),
/// kept in `loxia-core` so the reducer and UI can read it without depending on `loxia-audio`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayStatus {
    #[default]
    Stopped,
    Playing,
    Paused,
    Buffering,
}

/// Where a seek lands: an absolute position, or a relative nudge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeekTarget {
    Absolute(Duration),
    Relative(f64),
}

/// Playback-reporting bookkeeping is scoped to one `session` — a fresh `Load` starts a new one
/// (`crate::reducer::queue::load_current`) and resets `play_reported`/`start_reported` with it.
/// Queue edits (`i`/`a`) never touch any of these three fields — the currently loaded session
/// keeps reporting against the same session it already had.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerState {
    pub status: PlayStatus,
    pub position_secs: f64,
    pub duration_secs: f64,
    /// Identifies the current playback session, e.g. for de-duplicating "now playing" reports
    /// against Emby. `None` until the first `Load`.
    pub session: Option<String>,
    /// Whether the "playback progress" report has already fired for `session`.
    pub play_reported: bool,
    /// Whether the "playback start" report has already fired for `session`.
    pub start_reported: bool,
}
