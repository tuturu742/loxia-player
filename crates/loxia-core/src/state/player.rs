//! PlayerState mirror of the audio engine.
//!
//! Written **only** in response to `Event::Audio(..)` — never speculatively. The reducer must not
//! claim playback the engine has not confirmed (`docs/04-state-and-input.md` §4 rule 6).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::config::{EqPreset, FACTORY_EQ_PRESET_NAMES, QualityProfile, ReplayGainMode};
use crate::model::{AudioDevice, AudioFormat, PlaySessionId, QueueEntryId, ReplayGainInfo};

/// `Relative` is signed milliseconds (negative = backward) since `Duration` cannot be negative;
/// `Fraction` is `0.0..=1.0` of total duration, from a seek-bar click or drag.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SeekTarget {
    Relative(i64),
    Absolute(Duration),
    Fraction(f32),
}

/// Where the loaded track's bytes are coming from — the rolling cache on disk, or the network.
///
/// Resolved by `reducer::queue::cache_resolved`, the one place that learns whether a local copy
/// existed, and `None` until that reply lands (a window of at most `RESOLVE_TIMEOUT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackSource {
    Streaming,
    /// Streaming *and* writing a local copy as it goes (`cache.prefetch_on_play`).
    ///
    /// Distinct from `Streaming` because a user who has turned caching on wants to see it
    /// happening: with only two states the readout said "stream" for the whole track and looked
    /// exactly like caching being switched off — reported as "force cache is enabled but the source
    /// still shows stream" (`docs/12-decisions.md`). The bytes really are coming off the network
    /// for this play; the copy is for the next one.
    Caching,
    Cache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlayStatus {
    #[default]
    Stopped,
    Loading,
    Playing,
    Paused,
    Buffering,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqState {
    pub enabled: bool,
    pub preset_name: String,
    pub gains: [f32; 10],
    pub selected_band: usize,
    pub bypassed: bool,
}

impl Default for EqState {
    fn default() -> Self {
        EqState {
            enabled: false,
            preset_name: FACTORY_EQ_PRESET_NAMES[0].to_string(),
            gains: [0.0; 10],
            selected_band: 0,
            bypassed: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SleepTrigger {
    Duration(Duration),
    EndOfTrack,
    EndOfQueue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepTimer {
    pub trigger: SleepTrigger,
    pub fade_out: bool,
    pub quit_after: bool,
    pub armed_at: Timestamp,
    /// `09-05`: the entry current when armed — `SleepTrigger::EndOfTrack` waits specifically for
    /// *this* entry's own `TrackEnded`, not just "whatever is current when a `TrackEnded` next
    /// arrives" (which a skip to a different track would otherwise satisfy early). Extends this
    /// task's own given `SleepTimer` shape; see `docs/12-decisions.md`.
    pub armed_entry: Option<QueueEntryId>,
    /// `09-05`: the volume in effect the moment the fade-out ramp actually begins (the first
    /// `Tick` where remaining time drops into the 10-second window) — captured once, lazily,
    /// rather than at arm time, since the user may still adjust volume during the (possibly long)
    /// wait before the ramp starts. `None` until the ramp begins; restored verbatim when the
    /// timer fires, so the user's volume is never left at zero the next morning.
    pub pre_fade_volume: Option<u8>,
}

/// Which ReplayGain path is in effect for the current track (`09-04`,
/// `docs/05-audio-engine.md` §6) — lives here, not `loxia-audio`, so `loxia-tui`'s inspector can
/// read it without that crate depending on `loxia-audio` (the dependency runs the other way).
/// `loxia_audio::replaygain::resolve_gain` is the pure function that decides this.
///
/// `Tags` deliberately carries the *mode*, not a dB value: once `replaygain` is set to
/// `album`/`track`, mpv applies the actual attenuation itself from the file's own tags, so this
/// crate never computes that number — the inspector shows it by reading the same
/// `ReplayGainInfo` field the mode implies (`album_gain_db`/`track_gain_db`) directly from the
/// track alongside this variant.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AppliedGain {
    Tags(ReplayGainMode),
    Normalization(f32),
    None,
}

/// `09-04`: decides which gain path applies to the current track and mode. Lives here rather
/// than `loxia-audio` (whose own `replaygain.rs` re-exports this, satisfying that task's literal
/// `crates/loxia-audio/src/replaygain.rs` file/signature requirement) because `reducer::player`
/// needs to call it too, and `loxia-core` cannot depend on `loxia-audio` — unlike `09-03`'s
/// `clamp_eq_gain` (genuinely needed, independently, on both sides of that boundary), this
/// function only ever needs `loxia-core`'s own types, so there is no reason to duplicate it: one
/// definition, used from both crates, beats two copies kept in sync by a cross-checked test.
///
/// `Off` always applies nothing, regardless of what tags or normalization data are available.
/// Otherwise, a tag matching `mode` (`album_gain_db` for `Album`, `track_gain_db` for `Track`)
/// wins over Emby's `normalizationGain` fallback (mpv applies the tag-based gain itself, from the
/// file's own metadata, once `replaygain` is set to `album`/`track` — this function only decides
/// *which* path is in effect, not the resulting number, which is why `Tags` carries the mode and
/// not a dB value). With neither available, nothing is applied.
pub fn resolve_gain(
    mode: ReplayGainMode,
    track_rg: Option<&ReplayGainInfo>,
    normalization_db: Option<f32>,
) -> AppliedGain {
    if mode == ReplayGainMode::Off {
        return AppliedGain::None;
    }
    let has_tag = track_rg.is_some_and(|rg| match mode {
        ReplayGainMode::Album => rg.album_gain_db.is_some(),
        ReplayGainMode::Track => rg.track_gain_db.is_some(),
        ReplayGainMode::Off => false,
    });
    if has_tag {
        return AppliedGain::Tags(mode);
    }
    match normalization_db {
        Some(db) => AppliedGain::Normalization(db),
        None => AppliedGain::None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub status: PlayStatus,
    pub current: Option<QueueEntryId>,
    pub position: Duration,
    pub duration: Duration,
    /// See [`PlaybackSource`]. `None` while the cache lookup for the current track is still in
    /// flight, and whenever nothing is loaded.
    pub playback_source: Option<PlaybackSource>,
    pub volume: u8,
    pub muted: bool,
    /// The **actual decoded** format reported by mpv, not the metadata guess.
    pub format: Option<AudioFormat>,
    pub eq: EqState,
    pub replay_gain: ReplayGainMode,
    /// Which path (`09-04`) produced `applied_gain_db` — `resolve_gain`'s own return value for
    /// the current track, kept alongside the raw number so the inspector can render the right
    /// label (`ReplayGain (album)`/`Emby normalization`/nothing) rather than a bare number with
    /// no indication of its source.
    pub applied_gain: AppliedGain,
    /// The dB value actually in effect for the current track, whichever path produced it, shown
    /// in the inspector.
    pub applied_gain_db: Option<f32>,
    pub quality_profile: QualityProfile,
    pub sleep_timer: Option<SleepTimer>,
    /// The last `DataAction::DevicesLoaded` reply — enumerated once at startup and again whenever
    /// the device picker opens. Kept here, not just inside the transient `Modal::DevicePicker`, so
    /// the Settings tab's own output-device dropdown has something to offer while the picker is
    /// closed. Empty until the first `EnumerateDevices` reply ever arrives.
    pub known_devices: Vec<AudioDevice>,
    /// The last `DataAction::PresetsLoaded` reply (`09-03`) — factory presets (parsed from
    /// `assets/eq_presets.toml` by `loxia_audio::eq`, since `loxia-core` cannot parse that asset
    /// itself) merged with `config.equalizer.custom_presets`, in that order. Empty until the
    /// `loxia` binary fires the one-time startup load; `SetEqPreset` looks a name up here.
    pub known_presets: Vec<EqPreset>,
    /// The entry last handed to `Effect::Audio(Preload)` (`06-06`) — lets
    /// `reducer::queue::preload_effects` tell "the target is unchanged" from "the target changed
    /// back to something already preloaded", so re-touching the queue doesn't re-append the same
    /// file to mpv's playlist on every mutation.
    pub last_preloaded: Option<QueueEntryId>,
    /// The active playback-report session (`06-07`), generated once per `Load`
    /// (`reducer::queue::load_current`) and kept for that track's whole lifetime — a fresh id per
    /// report would make Emby treat every update as a new session, corrupting the resume point.
    pub session: Option<PlaySessionId>,
    /// Backs [`PlayerState::next_session_id`] — **not** a real `Uuid::new_v4()` (which needs the
    /// OS RNG, unusable inside the deterministic reducer; see `docs/12-decisions.md`, the same
    /// tension `06-03`'s shuffle seed already hit). A monotonic counter formatted to look like one
    /// is all Emby actually needs: a stable, per-load-unique opaque string.
    next_session_seq: u64,
    /// Whether `Played` has already been reported for the current session (`06-07`) — reset on
    /// every `Load` so the very next threshold-crossing report fires exactly once per load.
    pub play_reported: bool,
    /// Set by `reducer::modal::submit`'s `DevicePicker` arm at the moment it optimistically writes
    /// `config.audio.{device_id,output_driver}` (`09-01`) — there is no "the swap succeeded" reply
    /// event to key a success-clear off, only `AudioEvent::DeviceUnavailable` on failure
    /// (`docs/12-decisions.md`), so a matching failure is what both reverts the config and clears
    /// this. An unrelated later swap attempt simply overwrites it, so at most one stale entry can
    /// ever accumulate.
    pub pending_device_swap: Option<PendingDeviceSwap>,
    /// `11-06`: set by `session::restore` when it leaves a current entry **paused** (`ui.
    /// restore_autoplay = false`) — mpv was never told to `Load` anything at restore time itself
    /// (auto-loading on launch would seize the audio device just as surely as auto-*playing*
    /// would), so `current.is_some()` no longer implies "already loaded" the way it always used to
    /// before session restore existed. The next `PlayerAction::PlayPause` checks this and issues a
    /// real `Load` (via `queue::resume_after_restore`) instead of a bare toggle, which would have
    /// nothing to un-pause; also doubles as the player bar's "was restored, not yet resumed" hint
    /// (`widgets::player_bar`), replacing its old `Stopped`-plus-nonzero-position guess.
    pub restored_unloaded: bool,
    /// Whether `POST /Sessions/Playing` has been sent for the play currently loaded — reset by
    /// `queue::load_current` on every fresh `Load`, set when the report goes out.
    ///
    /// The Start report used to be keyed off the status transition `Loading -> Playing` directly.
    /// Real mpv does not make that transition when streaming: it reports `core-idle` while it
    /// fetches, so the sequence is `Loading -> Buffering -> Playing` and the Start report was never
    /// emitted at all — only Progress and Stopped were. Emby answers those with `204` and marks the
    /// item watched, but records **no play**: `PlayCount` stays 0, `LastPlayedDate` is never set,
    /// and no `PlaybackStopped` session event fires — which is exactly the event the server-side
    /// Last.fm plugin scrobbles from, so nothing ever scrobbled (`docs/12-decisions.md`).
    ///
    /// A flag rather than a smarter transition test: what matters is "has this play been announced
    /// yet", which no pair of adjacent statuses can express — the engine may pass through
    /// `Buffering` any number of times, in any order, before or after the first `Playing`.
    #[serde(default)]
    pub start_reported: bool,
}

/// See [`PlayerState::pending_device_swap`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDeviceSwap {
    pub attempted_id: String,
    pub previous_id: String,
    pub previous_driver: String,
}

impl PlayerState {
    pub fn next_session_id(&mut self) -> PlaySessionId {
        let id = self.next_session_seq;
        self.next_session_seq += 1;
        PlaySessionId::from(format!("00000000-0000-4000-8000-{id:012x}"))
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            status: PlayStatus::default(),
            current: None,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            playback_source: None,
            volume: 100,
            muted: false,
            format: None,
            eq: EqState::default(),
            replay_gain: ReplayGainMode::default(),
            applied_gain: AppliedGain::None,
            applied_gain_db: None,
            quality_profile: QualityProfile::default(),
            sleep_timer: None,
            known_devices: Vec::new(),
            known_presets: Vec::new(),
            last_preloaded: None,
            session: None,
            next_session_seq: 0,
            play_reported: false,
            pending_device_swap: None,
            restored_unloaded: false,
            start_reported: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rg(track: Option<f32>, album: Option<f32>) -> ReplayGainInfo {
        ReplayGainInfo {
            track_gain_db: track,
            album_gain_db: album,
            track_peak: None,
            album_peak: None,
        }
    }

    #[test]
    fn resolve_gain_prefers_tags_over_normalization() {
        let info = rg(Some(-3.0), Some(-6.2));
        assert_eq!(
            resolve_gain(ReplayGainMode::Album, Some(&info), Some(-4.1)),
            AppliedGain::Tags(ReplayGainMode::Album)
        );
        assert_eq!(
            resolve_gain(ReplayGainMode::Track, Some(&info), Some(-4.1)),
            AppliedGain::Tags(ReplayGainMode::Track)
        );
    }

    #[test]
    fn resolve_gain_falls_back_to_normalization() {
        // Album mode, but only a track tag is present — the album-specific field is absent, so
        // the tag path doesn't apply even though `track_rg` is `Some`.
        let info = rg(Some(-3.0), None);
        assert_eq!(
            resolve_gain(ReplayGainMode::Album, Some(&info), Some(-4.1)),
            AppliedGain::Normalization(-4.1)
        );
    }

    #[test]
    fn resolve_gain_none_when_neither_available() {
        assert_eq!(
            resolve_gain(ReplayGainMode::Album, None, None),
            AppliedGain::None
        );
        let info = rg(None, None);
        assert_eq!(
            resolve_gain(ReplayGainMode::Track, Some(&info), None),
            AppliedGain::None
        );
    }

    #[test]
    fn off_mode_applies_no_gain_even_with_tags() {
        let info = rg(Some(-3.0), Some(-6.2));
        assert_eq!(
            resolve_gain(ReplayGainMode::Off, Some(&info), Some(-4.1)),
            AppliedGain::None
        );
    }
}
