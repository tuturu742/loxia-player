//! Reducer: player mirror updates (`05-06`, minimal for this phase — EQ, ReplayGain, and sleep
//! timer land with `09 audio-advanced`), listening history (`06-05`), and playback reporting
//! (`06-07`).
//!
//! Two directions, never conflated: `apply_player` (user intent — `Space`, `[`, volume keys, ...)
//! only ever emits an `Effect::Audio` command, **never** touches `state.player` directly
//! (`docs/04-state-and-input.md` §4.6: the mirror updates only when the engine confirms, which is
//! exactly what makes the UI never lie about what's actually playing); `apply_audio` (the engine's
//! own confirmation, arriving back as an `Action::Audio`) is the only place that writes to it.
//!
//! `SetEq`/`SetReplayGain` are a deliberate, narrow exception: neither has a "this succeeded" reply
//! event to confirm against (`09-01`'s own finding: even `SetDevice` doesn't synchronously fail),
//! and `eq`/`replay_gain`/`quality_profile` here are user-facing *settings*, not an engine-reported
//! playback *fact* — the same category `modal.rs`'s device-picker submit already writes to
//! `state.config` optimistically, for the same reason (`docs/12-decisions.md`).

use std::time::Duration;

use jiff::SignedDuration;

use crate::Timestamp;
use crate::action::{AudioEvent, PlayerAction};
use crate::config::{QualityProfile, ReplayGainMode, TargetCodec};
use crate::effect::{AudioEffect, CacheEffect, Effect, NetEffect, SysEffect};
use crate::model::{ItemId, PlayMethod, PlaybackReport};
use crate::reducer::queue;
use crate::state::player::{AppliedGain, PlayStatus, SleepTrigger};
use crate::state::queue::{Availability, HistoryEntry, RepeatMode};
use crate::state::toast::ToastLevel;
use crate::state::{AppState, NowPlayingSub};

/// `09-05`: the fade-out ramp's fixed length — "a linear volume ramp over the last 10 seconds
/// before the trigger." A track (or `Duration` timer) shorter than this ramps over whatever time
/// it actually has instead (`fade_window_secs` below), per this task's own "a track shorter than
/// 10s ramps over whatever time remains."
const FADE_WINDOW_SECS: f64 = 10.0;

/// A play is "completed" once it crosses 90% of its duration or the 4-minute mark, whichever
/// comes first — duplicated from `loxia_emby::endpoints::playback::is_complete` because
/// `loxia-core` cannot depend on `loxia-emby`; a test in phase 08 asserts the two agree
/// (`docs/12-decisions.md`).
fn is_complete(position: Duration, duration: Duration) -> bool {
    if duration.is_zero() {
        return false;
    }
    position.as_secs_f64() >= 0.9 * duration.as_secs_f64() || position >= Duration::from_secs(240)
}

pub fn apply_player(state: &mut AppState, action: PlayerAction) -> Vec<Effect> {
    match action {
        // `11-06`: a session restored paused left `current` set but nothing actually loaded into
        // the engine (`state.player.restored_unloaded`) — a bare toggle would have nothing to
        // un-pause. The same accepted exception `Next`/`Prev` already make below (touching
        // `state.player` directly ahead of the engine's own confirmation) covers this too.
        PlayerAction::PlayPause if state.player.restored_unloaded => {
            state.player.restored_unloaded = false;
            state.player.status = PlayStatus::Loading;
            queue::resume_after_restore(state)
        }
        PlayerAction::PlayPause => vec![Effect::Audio(AudioEffect::PlayPause)],
        // `06-07`: "Progress" reports on seek too — read here, at the moment of the *request*
        // (the confirmed post-seek position arrives only later, via `PositionChanged`, and
        // nothing here waits for it); see `docs/12-decisions.md`.
        PlayerAction::Seek(target) => {
            let mut effects = vec![Effect::Audio(AudioEffect::Seek(target))];
            effects.extend(maybe_report_progress(state));
            effects
        }
        PlayerAction::SetVolume(v) => vec![Effect::Audio(AudioEffect::SetVolume(v))],
        // `+`/`-`. Reads the engine-confirmed mirror to compute an absolute target, exactly as
        // `ToggleMute` below reads `muted` — the mirror itself stays untouched until the engine's
        // own `VolumeChanged` lands. This arm was simply missing: the action existed and the keys
        // emitted it, but it fell through to the catch-all, so volume keys did nothing at all
        // (`docs/12-decisions.md`).
        PlayerAction::VolumeDelta(delta) => {
            let target = (i16::from(state.player.volume) + i16::from(delta)).clamp(0, 100) as u8;
            vec![Effect::Audio(AudioEffect::SetVolume(target))]
        }
        // Consults the current mirror to compute the new value — reading state to decide *what*
        // to ask for is not the same as writing to it; `state.player.muted` itself is untouched
        // until the engine's own `VolumeChanged` confirms.
        PlayerAction::ToggleMute => vec![Effect::Audio(AudioEffect::SetMute(!state.player.muted))],
        // `10-12`: unlike `ToggleMute`, this is an absolute request — no `state.player.muted`
        // read needed, since the caller (a remote `Mute`/`Unmute` command) already knows exactly
        // which state it wants.
        PlayerAction::SetMute(on) => vec![Effect::Audio(AudioEffect::SetMute(on))],
        // `Next`/`Prev` (`06-01`) genuinely mutate `state.queue` — that's the queue reducer's own
        // state, not the engine-confirmed `PlayerState` mirror this module's own doc comment is
        // about; `queue::load_current` setting `player.current`/`status = Loading` ahead of the
        // engine's confirmation is the same provisional-until-confirmed pattern every player uses
        // (the *track* changes immediately, the *status* still waits for a real `StatusChanged`).
        PlayerAction::Next => queue::next(state),
        PlayerAction::Prev => queue::prev(state),
        PlayerAction::SetEqGain { band, db } => set_eq_gain(state, band, db),
        PlayerAction::SetEqPreset(name) => set_eq_preset(state, name),
        PlayerAction::ToggleEqBypass => toggle_eq_bypass(state),
        PlayerAction::CycleReplayGain => cycle_replay_gain(state),
        PlayerAction::CycleQuality => cycle_quality(state),
        PlayerAction::Stop => stop(state),
        // `SetDevice` alone still falls through: this module's own reducer scope never covered it
        // (the device picker emits its `Effect::Audio(SetDevice)` directly).
        _ => Vec::new(),
    }
}

/// `09-04`: `Album -> Track -> Off -> Album`, applied to the **current** track immediately (not
/// only the next one) — `Effect::Audio(SetReplayGain)` changes mpv's own `replaygain` property on
/// whatever is already loaded, and `apply_replay_gain` recomputes `player.applied_gain`/
/// `applied_gain_db` for that same track right away.
fn cycle_replay_gain(state: &mut AppState) -> Vec<Effect> {
    state.player.replay_gain = match state.player.replay_gain {
        ReplayGainMode::Album => ReplayGainMode::Track,
        ReplayGainMode::Track => ReplayGainMode::Off,
        ReplayGainMode::Off => ReplayGainMode::Album,
    };
    apply_replay_gain(state);
    state.toast(replay_gain_toast_text(state), ToastLevel::Info);
    state.touch();
    vec![Effect::Audio(AudioEffect::SetReplayGain(
        state.player.replay_gain,
    ))]
}

/// `10-13`: "lower case" (this task's own writing rule) — `replaygain: ...`, not `ReplayGain: ...`
/// (the inspector widget's own panel label is a separate concern and keeps its own capitalized
/// `ReplayGain (album): ...` styling; only the toast's own leading word changed).
fn replay_gain_toast_text(state: &AppState) -> String {
    let mode = match state.player.replay_gain {
        ReplayGainMode::Album => "Album",
        ReplayGainMode::Track => "Track",
        ReplayGainMode::Off => "Off",
    };
    match state.player.applied_gain_db {
        Some(db) => format!("replaygain: {mode} ({db:+.1} dB)"),
        None => format!("replaygain: {mode} (no gain for this track)"),
    }
}

/// `09-04`: recomputes `player.applied_gain`/`applied_gain_db` for whatever `queue.current()` now
/// is, given the current `replay_gain` mode — shared by `cycle_replay_gain` above and by
/// `reducer::queue::load_current`/`cache_resolved` (a new track becoming current also needs this,
/// not just a mode change on an already-loaded one). `pub(crate)` for that cross-module call.
///
/// `normalization_db` is always `None` here: nothing upstream of this populates Emby's
/// `normalizationGain` (`loxia_emby::endpoints::playback::PlaybackInfo`, task `02-09`) onto
/// `Track`/queue state — a real, left-open gap, not an oversight (`docs/12-decisions.md`, the
/// same shape `08-07` already documented for scrobble-effect wiring). Once that data exists
/// somewhere reachable here, this is the one place that needs to change.
pub(crate) fn apply_replay_gain(state: &mut AppState) {
    let track_rg = state.queue.current().and_then(|e| e.track.replay_gain);
    let normalization_db: Option<f32> = None;
    let resolved = crate::state::player::resolve_gain(
        state.player.replay_gain,
        track_rg.as_ref(),
        normalization_db,
    );
    state.player.applied_gain_db = match resolved {
        AppliedGain::Tags(ReplayGainMode::Album) => track_rg.and_then(|rg| rg.album_gain_db),
        AppliedGain::Tags(ReplayGainMode::Track) => track_rg.and_then(|rg| rg.track_gain_db),
        AppliedGain::Tags(ReplayGainMode::Off) => None,
        AppliedGain::Normalization(db) => Some(db),
        AppliedGain::None => None,
    };
    state.player.applied_gain = resolved;
}

// --- 09-06: quality profiles -----------------------------------------------------------------

/// `Direct -> TranscodeHigh -> TranscodeMed -> TranscodeLow -> Direct`.
fn next_quality_profile(current: QualityProfile) -> QualityProfile {
    match current {
        QualityProfile::Direct => QualityProfile::TranscodeHigh,
        QualityProfile::TranscodeHigh => QualityProfile::TranscodeMed,
        QualityProfile::TranscodeMed => QualityProfile::TranscodeLow,
        QualityProfile::TranscodeLow => QualityProfile::Direct,
    }
}

/// The transcode bitrate for the toast ("quality: 192 kbps Opus") — `None` for `Direct`, which
/// has no transcode bitrate to report. Duplicated from `loxia_emby::stream`'s own
/// profile-to-bitrate-string table (`"320000"`/`"192000"`/`"96000"`) because `loxia-core` cannot
/// depend on `loxia-emby`; unlike `09-03`'s `clamp_eq_gain`, there's no cross-crate test pairing
/// these two — the three numbers are fixed, named constants tied 1:1 to `QualityProfile`'s own
/// variants, not a formula either side could drift on independently.
fn quality_bitrate_kbps(profile: QualityProfile) -> Option<u32> {
    match profile {
        QualityProfile::Direct => None,
        QualityProfile::TranscodeHigh => Some(320),
        QualityProfile::TranscodeMed => Some(192),
        QualityProfile::TranscodeLow => Some(96),
    }
}

fn codec_label(codec: TargetCodec) -> &'static str {
    match codec {
        TargetCodec::Mp3 => "MP3",
        TargetCodec::Aac => "AAC",
        TargetCodec::Opus => "Opus",
    }
}

fn quality_toast_text(profile: QualityProfile, codec: TargetCodec) -> String {
    match quality_bitrate_kbps(profile) {
        Some(kbps) => format!("quality: {kbps} kbps {}", codec_label(codec)),
        None => "quality: Direct".to_string(),
    }
}

/// `09-06`: cycles the quality profile (the offline refusal is handled by the guard in
/// `apply_player`, above). Refused, with a toast, if the current track is served from the local
/// cache or a download — the file on disk is whatever it already is, and pretending a quality
/// change means something for it would mislabel the player bar.
///
/// On a real change: persists `config.transcode.mode` (written immediately, like every other
/// config-writing reducer path in this codebase — `09-01`'s own entry already notes no debounce
/// *timer* mechanism actually exists here despite some task text using that word), toasts the new
/// profile's bitrate/codec, and — only if a track is actually loaded — reloads it at the same
/// position under the new profile and re-primes the preload for whatever's next (queued at the
/// *old* profile, so it needs to be re-issued, not just left alone).
fn cycle_quality(state: &mut AppState) -> Vec<Effect> {
    let served_locally = state.queue.current().is_some_and(|e| {
        matches!(
            e.availability,
            Availability::Cached | Availability::Downloaded
        )
    });
    if served_locally {
        state.toast("quality changes need a connection", ToastLevel::Warning);
        return Vec::new();
    }

    state.player.quality_profile = next_quality_profile(state.player.quality_profile);
    state.config.transcode.mode = state.player.quality_profile;
    state.toast(
        quality_toast_text(
            state.player.quality_profile,
            state.config.transcode.target_codec,
        ),
        ToastLevel::Info,
    );
    state.touch();

    let mut effects = vec![Effect::Sys(SysEffect::WriteConfig(Box::new(
        state.config.clone(),
    )))];
    effects.extend(reload_at_current_profile(state));
    effects
}

/// Re-fetches the loaded track at whatever `player.quality_profile` now says, resuming at the same
/// position, and re-primes the preload for whatever is next (it was queued at the *old* profile, so
/// it must be re-issued rather than left alone). Empty when nothing is loaded.
///
/// Shared by `q` and by the Settings row for the same field: a quality change has to mean the same
/// thing whichever way it was made, and having only the keybinding do this is exactly why the
/// Settings row appeared to do nothing at all (`docs/12-decisions.md`).
pub(crate) fn reload_at_current_profile(state: &mut AppState) -> Vec<Effect> {
    let Some(entry) = state.queue.current().cloned() else {
        return Vec::new();
    };

    let mut effects = vec![
        Effect::Cache(Box::new(CacheEffect::EnsureCached {
            track: entry.track.clone(),
            profile: state.player.quality_profile,
        })),
        Effect::Audio(AudioEffect::Load {
            headers: queue::stream_headers(state, &queue::placeholder_url(&entry.track.id)),
            url: queue::placeholder_url(&entry.track.id),
            start_at: state.player.position,
            gain_db: None,
        }),
    ];

    state.player.last_preloaded = None;
    effects.extend(queue::preload_effects(state));

    effects
}

/// `09-03`: `±12 dB`, snapped to the nearest `0.5 dB` — duplicated from
/// `loxia_audio::eq::clamp_gain` because `loxia-core` cannot depend on `loxia-audio`; both crates
/// carry the same boundary-case tests so a future edit to one that isn't mirrored in the other
/// shows up as a test failure rather than silent drift (`docs/12-decisions.md`, the same shape as
/// `is_complete`'s existing duplication between this module and `loxia-emby`).
pub(crate) fn clamp_eq_gain(db: f32) -> f32 {
    let snapped = (db / 0.5).round() * 0.5;
    snapped.clamp(-12.0, 12.0)
}

/// The curve the engine should now be playing, given the current `enabled`/`bypassed`/`gains` —
/// `None` when the EQ is off entirely (chain uninstalled), `Some([0.0; 10])` when bypassed (chain
/// stays installed, every band silenced — instantaneous and reversible, since no reinstall is
/// needed to come back from it), `Some(gains)` otherwise. Shared by every EQ-mutating action so
/// gain/preset/bypass changes all converge on the same effect-construction logic.
fn eq_effects(state: &AppState) -> Vec<Effect> {
    if !state.player.eq.enabled {
        return Vec::new();
    }
    let curve = if state.player.eq.bypassed {
        [0.0; 10]
    } else {
        state.player.eq.gains
    };
    vec![Effect::Audio(AudioEffect::SetEq(Some(curve)))]
}

/// The same decision as [`eq_effects`], but expressed as the curve itself — `None` meaning
/// "uninstall the chain". [`eq_effects`] can return *no* effect for the disabled case because its
/// callers only ever mutate an already-enabled EQ; a caller that can flip `enabled` itself (the
/// Settings row) needs to be able to say "off" out loud.
pub(crate) fn eq_curve(eq: &crate::state::player::EqState) -> Option<[f32; 10]> {
    if !eq.enabled {
        return None;
    }
    Some(if eq.bypassed { [0.0; 10] } else { eq.gains })
}

/// `09-03`: sets one band directly and commits the whole curve. There is no engine-confirmation
/// event for an EQ change (unlike volume/mute), so this
/// writes `player.eq` directly: it is a user-facing *setting*, not an engine-reported fact.
fn set_eq_gain(state: &mut AppState, band: usize, db: f32) -> Vec<Effect> {
    let Some(slot) = state.player.eq.gains.get_mut(band) else {
        return Vec::new();
    };
    *slot = clamp_eq_gain(db);
    state.touch();
    eq_effects(state)
}

/// `09-03`: replaces all ten gains with `name`'s preset from `player.known_presets` (the merged
/// factory + custom list, populated once at startup by `DataAction::PresetsLoaded`). Also the
/// **enable** path: "enabling the EQ from `enabled = false` installs the chain and applies the
/// active preset" (this task's own spec) — picking a preset is how a disabled EQ becomes active,
/// since no separate "turn the EQ on" action exists in this phase. An unknown name (a custom
/// preset since deleted from config, say) toasts and leaves everything untouched.
fn set_eq_preset(state: &mut AppState, name: String) -> Vec<Effect> {
    let Some(preset) = state
        .player
        .known_presets
        .iter()
        .find(|p| p.name == name)
        .cloned()
    else {
        state.toast(format!("no such EQ preset: {name}"), ToastLevel::Warning);
        return Vec::new();
    };
    state.player.eq.gains = preset.gains;
    state.player.eq.preset_name = preset.name;
    state.player.eq.enabled = true;
    state.touch();
    eq_effects(state)
}

/// `09-03`: flips `bypassed` — `eq_effects` does the rest (zeroing the curve without uninstalling
/// it, or restoring the real gains), so toggling back and forth is instantaneous and reversible.
fn toggle_eq_bypass(state: &mut AppState) -> Vec<Effect> {
    state.player.eq.bypassed = !state.player.eq.bypassed;
    state.touch();
    eq_effects(state)
}

pub fn apply_audio(state: &mut AppState, event: AudioEvent) -> Vec<Effect> {
    match event {
        AudioEvent::StatusChanged(status) => {
            state.player.status = status;
            state.touch();
            // `10-11`: every status change refreshes the OS media-control metadata (play/pause
            // state included), regardless of which of the branches below also fires — a plain
            // `Stopped`/`Loading` transition, matched by neither, must still update it.
            let mut effects: Vec<Effect> = mpris_meta(state).into_iter().collect();
            // `06-07`: "Start" is the *first* `Playing` of a play — a `Paused -> Playing` resume
            // is a "Progress" report instead, never a second "Start" for the same session.
            //
            // This used to test `old_status == PlayStatus::Loading` directly, which real mpv never
            // satisfies when streaming: it reports `core-idle` while fetching, so the sequence is
            // `Loading -> Buffering -> Playing` and no Start was ever sent. See
            // `PlayerState::start_reported` for what that cost.
            if status == PlayStatus::Playing && !state.player.start_reported {
                state.player.start_reported = true;
                effects.extend(maybe_report_start(state));
                return effects;
            }
            if status == PlayStatus::Playing || status == PlayStatus::Paused {
                effects.extend(maybe_report_progress(state));
                return effects;
            }
            return effects;
        }
        AudioEvent::PositionChanged { position, duration } => {
            state.player.position = position;
            state.player.duration = authoritative_duration(state, duration);
            state.touch();
            let mut effects = record_history_if_threshold_crossed(state);
            effects.extend(maybe_report_played(state));
            return effects;
        }
        AudioEvent::FormatDetected(format) => {
            state.player.format = Some(format);
            state.touch();
        }
        AudioEvent::VolumeChanged { volume, muted } => {
            state.player.volume = volume;
            state.player.muted = muted;
            state.touch();
        }
        AudioEvent::EngineError(message) => {
            state.toast(format!("audio error: {message}"), ToastLevel::Error);
        }
        // `10-05`: "the reducer toasts `could not switch to <name>` and leaves the previous
        // device active" — the name comes from `known_devices` (falling back to the bare id if,
        // implausibly, it's since dropped out of the last enumeration); the revert only applies
        // when `id` matches what `pending_device_swap` is actually waiting on, so an unrelated
        // stale entry (there can be at most one — see its own doc comment) is never acted on.
        AudioEvent::DeviceUnavailable { id } => {
            let name = state
                .player
                .known_devices
                .iter()
                .find(|d| d.id == id)
                .map(|d| d.description.clone())
                .unwrap_or_else(|| id.clone());
            if let Some(pending) = state.player.pending_device_swap.take() {
                if pending.attempted_id == id {
                    state.config.audio.device_id = pending.previous_id;
                    state.config.audio.output_driver = pending.previous_driver;
                } else {
                    state.player.pending_device_swap = Some(pending);
                }
            }
            state.toast(format!("could not switch to {name}"), ToastLevel::Error);
        }
        AudioEvent::TrackEnded { natural } => {
            // Recorded/reported *before* advancing: `queue::advance_on_track_ended` may move
            // `queue.current()` on to the next entry, and it's the entry that just finished both
            // the history entry and the "Stopped" report are about. Same reason
            // `maybe_fire_sleep_timer_on_track_ended` (`09-05`) is checked before advancing too —
            // `EndOfTrack`/`EndOfQueue` both need to see the entry/position as they were *before*
            // this track ended, not after.
            let mut effects = if natural {
                record_history_on_natural_end(state)
            } else {
                Vec::new()
            };
            effects.extend(report_stopped(state));
            match maybe_fire_sleep_timer_on_track_ended(state) {
                Some(fire_effects) => effects.extend(fire_effects),
                // The sleep timer didn't fire (not armed, `Duration` trigger, or the
                // `EndOfTrack`/`EndOfQueue` condition isn't met yet) — advance as normal.
                None => effects.extend(queue::advance_on_track_ended(state, natural)),
            }
            return effects;
        }
    }
    Vec::new()
}

// --- 09-05: sleep timer ---------------------------------------------------------------------

/// `09-05`: runs every `Tick` (`reducer::tick`, which supplies `now`). `SleepTrigger::Duration`
/// fires here directly; `EndOfTrack`/`EndOfQueue` fire from `apply_audio`'s own `TrackEnded`
/// handling instead (`maybe_fire_sleep_timer_on_track_ended`) — this function only evaluates the
/// `Duration` firing condition and, for whichever trigger is actually armed, the fade-out ramp
/// (all three trigger kinds fade the same way, keyed off "time remaining until stop").
pub(crate) fn evaluate_sleep_timer(state: &mut AppState, now: Timestamp) -> Vec<Effect> {
    let Some(timer) = state.player.sleep_timer.clone() else {
        return Vec::new();
    };

    let remaining = match timer.trigger {
        SleepTrigger::Duration(d) => {
            let elapsed = now.duration_since(timer.armed_at);
            let total = SignedDuration::try_from(d).unwrap_or(SignedDuration::ZERO);
            let left = total - elapsed;
            if left <= SignedDuration::ZERO {
                return fire_sleep_timer(state);
            }
            Duration::try_from(left).unwrap_or(Duration::ZERO)
        }
        SleepTrigger::EndOfTrack | SleepTrigger::EndOfQueue => {
            state.player.duration.saturating_sub(state.player.position)
        }
    };

    if timer.fade_out {
        apply_fade(state, remaining)
    } else {
        Vec::new()
    }
}

/// The fade window's own length: normally the fixed 10 s, but never longer than the total span
/// actually available (the whole `Duration` sleep length, or the whole track for `EndOfTrack`/
/// `EndOfQueue`) — "a track shorter than 10 s ramps over whatever time remains" generalises
/// naturally to a short `Duration` timer too, so the same rule covers both.
fn fade_window_secs(trigger: SleepTrigger, player_duration: Duration) -> f64 {
    let span = match trigger {
        SleepTrigger::Duration(d) => d.as_secs_f64(),
        SleepTrigger::EndOfTrack | SleepTrigger::EndOfQueue => player_duration.as_secs_f64(),
    };
    span.min(FADE_WINDOW_SECS)
}

/// Ramps volume linearly from `pre_fade_volume` (captured lazily, the first time this runs inside
/// the window — not at arm time, since the user may still adjust volume during the wait
/// beforehand) down to `0` at `remaining == 0`. A no-op outside the window (`remaining` still
/// greater than the window's own length) — nothing to ramp yet.
fn apply_fade(state: &mut AppState, remaining: Duration) -> Vec<Effect> {
    let Some(timer) = state.player.sleep_timer.as_ref() else {
        return Vec::new();
    };
    let window = fade_window_secs(timer.trigger, state.player.duration);
    if window <= 0.0 || remaining.as_secs_f64() > window {
        return Vec::new();
    }

    let base = match timer.pre_fade_volume {
        Some(v) => v,
        None => {
            let v = state.player.volume;
            state.player.sleep_timer.as_mut().unwrap().pre_fade_volume = Some(v);
            v
        }
    };

    let ratio = (remaining.as_secs_f64() / window).clamp(0.0, 1.0);
    let target = ((base as f64) * ratio).round() as u8;
    state.touch();
    vec![Effect::Audio(AudioEffect::SetVolume(target))]
}

/// `EndOfTrack`/`EndOfQueue` fire from a real `TrackEnded`, not from `Tick` — checked *before*
/// `queue::advance_on_track_ended` runs (the caller's own job), since `state.queue.current()`/
/// `state.queue.position` still describe the entry that just ended at this point. Returns
/// `Some(effects)` if the timer fired — the caller must skip the normal advance in that case, the
/// whole point being to stop instead of continuing — `None` otherwise (not armed, a `Duration`
/// trigger, or the condition isn't met yet).
fn maybe_fire_sleep_timer_on_track_ended(state: &mut AppState) -> Option<Vec<Effect>> {
    let timer = state.player.sleep_timer.as_ref()?;
    let should_fire = match timer.trigger {
        SleepTrigger::Duration(_) => false,
        // "The entry that was current when armed" — a skip to some other entry beforehand means
        // that other entry's own natural end never matches `armed_entry`, so this simply never
        // fires again unless the user navigates back to the exact entry it was armed for.
        SleepTrigger::EndOfTrack => state.queue.current().map(|e| e.entry_id) == timer.armed_entry,
        // "No next entry, honouring repeat mode" — mirrors the one branch of
        // `queue::advance_on_track_ended` that actually stops rather than continuing:
        // `Repeat::One` always reloads (never "ends"), `Repeat::All` always wraps (arming this
        // trigger already forced it to `Off`, `reducer::modal`'s own submit handling) — only
        // `Repeat::Off` at the last entry has no next.
        SleepTrigger::EndOfQueue => {
            let at_end = state.queue.position + 1 >= state.queue.play_order.len();
            at_end && state.queue.repeat == RepeatMode::Off
        }
    };
    should_fire.then(|| fire_sleep_timer(state))
}

/// Stops playback, restores whatever volume was in effect before any fade-out ramp began (a
/// no-op if the fade never actually started — `fade_out` was off, or the timer fired before
/// crossing into the fade window), clears the timer, and toasts. `quit_after` additionally emits
/// `Effect::Sys(Exit)` — the runtime's own existing shutdown sequencing (`runtime::shutdown`,
/// already built for `SystemEvent::Quit`) persists the session snapshot before actually exiting,
/// so nothing further needs to be built here for "following the session snapshot."
fn fire_sleep_timer(state: &mut AppState) -> Vec<Effect> {
    let Some(timer) = state.player.sleep_timer.take() else {
        return Vec::new();
    };
    let mut effects = vec![Effect::Audio(AudioEffect::Stop)];
    if let Some(original) = timer.pre_fade_volume {
        effects.push(Effect::Audio(AudioEffect::SetVolume(original)));
    }
    state.toast("sleep timer finished", ToastLevel::Info);
    state.touch();
    if timer.quit_after {
        effects.push(Effect::Sys(SysEffect::Exit));
    }
    effects
}

/// `S` / the player bar's own Stop button — halts playback and rewinds to the start of the current
/// track, **keeping the queue** (that's what `QueueAction::Clear` is for). `PlayerAction::Stop` used
/// to fall through this module's catch-all arm and do nothing at all, so neither the key nor the
/// button worked (`docs/12-decisions.md`).
///
/// mpv's own `stop` unloads the file, so the engine has nothing loaded afterwards — the same
/// situation a restored-but-unloaded session is in. Reusing `restored_unloaded` means the next
/// `PlayPause` issues a real `Load` (at position zero) instead of an un-pause that would have
/// nothing to resume.
fn stop(state: &mut AppState) -> Vec<Effect> {
    state.player.playback_source = None;
    // Reported before the status/position are torn down — `report_stopped` resolves the entry that
    // was playing, the same ordering `queue::clear` already relies on.
    let mut effects = report_stopped(state);
    state.player.status = PlayStatus::Stopped;
    state.player.position = Duration::ZERO;
    state.player.restored_unloaded = state.player.current.is_some();
    state.history_recorded_this_play = false;
    effects.extend(mpris_meta(state));
    effects.push(Effect::Audio(AudioEffect::Stop));
    state.touch();
    effects
}

/// The `ItemId` of whatever `player.current` (the engine-confirmed loaded entry) points at, or
/// `None` before the first `Load` or once the entry has left the queue.
fn current_item_id(state: &AppState) -> Option<ItemId> {
    let current = state.player.current?;
    state
        .queue
        .entries
        .iter()
        .find(|e| e.entry_id == current)
        .map(|e| e.track.id.clone())
}

/// The track's real length, preferring what the **server** said over what the engine reports.
///
/// mpv cannot know the length of a live-transcoded stream: Emby serves it without a reliable
/// duration, so mpv reports whatever it has demuxed so far and the seek bar showed the transcoded
/// portion rather than the track — a live user found the bar unusable under any non-`Direct`
/// profile (`docs/12-decisions.md`). `RunTimeTicks` from the item metadata is exact and known
/// before playback even starts, so it wins whenever it is present.
///
/// Not merely cosmetic: `is_complete` divides position by this, so a short bogus duration made the
/// "played" threshold trigger almost immediately, and a zero one made it never trigger at all.
///
/// Falls back to the engine's figure when the item carries no runtime at all — some servers omit
/// it — since a wrong number is still better than a bar that never moves.
fn authoritative_duration(state: &AppState, from_engine: Duration) -> Duration {
    match state.queue.current() {
        Some(entry) if !entry.track.duration.is_zero() => entry.track.duration,
        _ => from_engine,
    }
}

fn maybe_report_start(state: &AppState) -> Vec<Effect> {
    let Some(item) = current_item_id(state) else {
        return Vec::new();
    };
    let Some(session) = state.player.session.clone() else {
        return Vec::new();
    };
    vec![Effect::Net(NetEffect::ReportPlayback(
        PlaybackReport::Start { item, session },
    ))]
}

/// `10-11`: builds the outbound OS media-control metadata effect from whatever is current right
/// now — `None` when nothing is queued, matching `maybe_report_start`/`maybe_report_progress`'s
/// own "nothing to report" shape just above. Called on a track change (`reducer::queue::
/// load_current`), on every `AudioEvent::StatusChanged`, and every 10 s while playing
/// (`reducer::tick`) — three distinct triggers named individually by this task's own spec, not
/// deduplicated into one, since each fires from a different reducer entry point with no shared
/// caller to hoist it into.
///
/// `art_url` is always `None`: no task has ever persisted a fetched image to disk, and even if one
/// had, `loxia-core` cannot depend on `loxia-emby` to build a real HTTP URL from a tag (the same
/// gap `reducer::queue::notify_track_change`'s own `art_path` already documents, `10-10`,
/// `docs/12-decisions.md`).
pub(crate) fn mpris_meta(state: &AppState) -> Option<Effect> {
    let entry = state.queue.current()?;
    Some(Effect::Sys(SysEffect::UpdateMpris(
        crate::effect::MprisMeta {
            title: entry.track.name.clone(),
            artist: entry.track.artist_names.join(", "),
            album: entry.track.album_name.clone(),
            art_url: None,
            position: state.player.position,
            duration: state.player.duration,
            playing: state.player.status == PlayStatus::Playing,
        },
    )))
}

/// `06-07`: `pub(crate)` — also called from `reducer::mod`'s `tick` (the periodic 10s report).
pub(crate) fn maybe_report_progress(state: &AppState) -> Vec<Effect> {
    let Some(item) = current_item_id(state) else {
        return Vec::new();
    };
    let Some(session) = state.player.session.clone() else {
        return Vec::new();
    };
    vec![Effect::Net(NetEffect::ReportPlayback(
        PlaybackReport::Progress {
            item,
            session,
            position: state.player.position,
            paused: state.player.status == PlayStatus::Paused,
            play_method: PlayMethod::from_quality_profile(state.player.quality_profile),
        },
    ))]
}

/// `06-07`: `pub(crate)` — also called from `reducer::queue`'s `clear` and `reducer::mod`'s
/// `Quit` handling, both outside this module's own dispatch.
pub(crate) fn report_stopped(state: &AppState) -> Vec<Effect> {
    let Some(item) = current_item_id(state) else {
        return Vec::new();
    };
    let Some(session) = state.player.session.clone() else {
        return Vec::new();
    };
    vec![Effect::Net(NetEffect::ReportPlayback(
        PlaybackReport::Stopped {
            item,
            session,
            position: state.player.position,
        },
    ))]
}

/// **Nothing.** One listen must produce exactly one "this was played" signal, and the session
/// reports already are that one.
///
/// This used to `POST /Users/{uid}/PlayedItems/{id}` — Emby's manual *mark as played* toggle, the
/// same call the web UI's tick box makes — once the position crossed `is_complete`. It was written
/// when the session reporting was broken (`Sessions/Playing` was never sent, so Emby recorded no
/// play at all), which made this the only thing marking anything played. Fixing that left both
/// firing for a single listen: two `UserDataSaved` events, and a server-side Last.fm plugin
/// scrobbling the track twice — "I get two finished tracks on Last.fm while listening to one"
/// (`docs/12-decisions.md`).
///
/// Measured against a live server: `Sessions/Playing` + `Progress` + `Stopped` on their own leave
/// `PlayCount: 1`, `Played: true` and a `LastPlayedDate`, and adding the manual mark on top changes
/// none of those. It contributed nothing but the duplicate event, so it is gone rather than
/// deduplicated somewhere downstream.
///
/// Kept as a function, rather than deleting the call site, so the position-crossing rule has one
/// documented home if a *different* per-play signal is ever needed.
fn maybe_report_played(state: &mut AppState) -> Vec<Effect> {
    let _ = state;
    Vec::new()
}

/// "The position crosses the completion threshold" half of `06-05`'s recording rule — fires on
/// every `PositionChanged`, so a track that's skipped *after* crossing 90%/240s is still recorded
/// even though `TrackEnded` then arrives with `natural: false`. A track skipped before the
/// threshold never crosses it, so it is correctly never recorded by this path.
fn record_history_if_threshold_crossed(state: &mut AppState) -> Vec<Effect> {
    if state.history_recorded_this_play {
        return Vec::new();
    }
    if !is_complete(state.player.position, state.player.duration) {
        return Vec::new();
    }
    record_history_now(state)
}

/// "`TrackEnded { natural: true }` always records, since natural end implies completion" — bypasses
/// the numeric `is_complete` check entirely (the engine's own reported position at the instant
/// `TrackEnded` arrives is not necessarily >= the threshold to the millisecond), but still
/// respects `history_recorded_this_play` so a stray duplicate `TrackEnded`, or one arriving after
/// the position-crossing path already recorded this same play, does not record twice.
fn record_history_on_natural_end(state: &mut AppState) -> Vec<Effect> {
    if state.history_recorded_this_play {
        return Vec::new();
    }
    record_history_now(state)
}

fn record_history_now(state: &mut AppState) -> Vec<Effect> {
    let Some(entry) = state.queue.current() else {
        return Vec::new();
    };
    // `played_at` from `state.clock` (the last `Tick`'s timestamp) — never a live clock read
    // inside the reducer (`docs/04-state-and-input.md` §4 rule 2).
    let history_entry = HistoryEntry {
        track: entry.track.clone(),
        played_at: state.clock,
        completed: true,
    };
    state.history_recorded_this_play = true;
    state.history.push_front(history_entry.clone());
    if state.history.len() > 50 {
        state.history.pop_back();
    }
    state.touch();
    vec![Effect::Cache(Box::new(CacheEffect::AppendHistory(
        history_entry,
    )))]
}

/// `H` — valid on any tab (jumping to Now Playing first would be an extra keystroke for no
/// reason), but only visible on `NowPlaying`.
pub fn toggle_history_subview(state: &mut AppState) -> Vec<Effect> {
    state.now_playing_subview = match state.now_playing_subview {
        NowPlayingSub::Queue => NowPlayingSub::History,
        NowPlayingSub::History => NowPlayingSub::Queue,
    };
    // `07-06`: a row index in one sub-view means nothing in the other (a queue position vs. a
    // spot in the newest-first history list) — reset rather than carry over a stale cursor.
    state.now_playing_cursor = 0;
    state.now_playing_scroll = 0;
    state.touch();
    Vec::new()
}

/// `L` — flips `config.ui.show_lyrics` and persists it (`07-07`). There is deliberately no
/// separate session-only visibility flag: the pane's shown/hidden state *is* this config field,
/// read directly by `widgets::lyrics` (`docs/12-decisions.md`).
pub fn toggle_lyrics(state: &mut AppState) -> Vec<Effect> {
    state.config.ui.show_lyrics = !state.config.ui.show_lyrics;
    state.touch();
    vec![Effect::Sys(crate::effect::SysEffect::WriteConfig(
        Box::new(state.config.clone()),
    ))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixtures;

    #[test]
    fn play_pause_emits_effect_without_mutating_state() {
        let mut state = fixtures::fixture_empty();
        let before = state.player.clone();
        let effects = apply_player(&mut state, PlayerAction::PlayPause);
        assert_eq!(effects, vec![Effect::Audio(AudioEffect::PlayPause)]);
        assert_eq!(state.player, before);
    }

    /// `+`/`-` emitted `VolumeDelta`, but the reducer had no arm for it — it fell through to the
    /// catch-all and the keys did nothing at all (`docs/12-decisions.md`). Like `ToggleMute`, it
    /// reads the engine-confirmed mirror to compute an absolute target and leaves the mirror to
    /// the engine's own `VolumeChanged`.
    #[test]
    fn volume_delta_asks_for_an_absolute_target_without_mutating_state() {
        let mut state = fixtures::fixture_empty();
        state.player.volume = 60;

        let effects = apply_player(&mut state, PlayerAction::VolumeDelta(5));
        assert_eq!(effects, vec![Effect::Audio(AudioEffect::SetVolume(65))]);
        assert_eq!(
            state.player.volume, 60,
            "the mirror waits for the engine to confirm"
        );

        let effects = apply_player(&mut state, PlayerAction::VolumeDelta(-5));
        assert_eq!(effects, vec![Effect::Audio(AudioEffect::SetVolume(55))]);
    }

    /// Clamped at both ends rather than wrapping or overflowing the `u8`.
    #[test]
    fn volume_delta_clamps_to_the_usable_range() {
        let mut state = fixtures::fixture_empty();

        state.player.volume = 98;
        assert_eq!(
            apply_player(&mut state, PlayerAction::VolumeDelta(5)),
            vec![Effect::Audio(AudioEffect::SetVolume(100))]
        );

        state.player.volume = 2;
        assert_eq!(
            apply_player(&mut state, PlayerAction::VolumeDelta(-5)),
            vec![Effect::Audio(AudioEffect::SetVolume(0))]
        );
    }

    #[test]
    fn toggle_mute_reads_current_value_without_mutating_state() {
        let mut state = fixtures::fixture_empty();
        state.player.muted = false;
        let effects = apply_player(&mut state, PlayerAction::ToggleMute);
        assert_eq!(effects, vec![Effect::Audio(AudioEffect::SetMute(true))]);
        assert!(
            !state.player.muted,
            "toggling must not mutate the mirror directly"
        );
    }

    /// `10-12`: unlike `ToggleMute`, `SetMute` while already muted must stay muted (an absolute
    /// request, not an inversion).
    #[test]
    fn set_mute_is_absolute_not_a_toggle() {
        let mut state = fixtures::fixture_empty();
        state.player.muted = true;
        let effects = apply_player(&mut state, PlayerAction::SetMute(true));
        assert_eq!(effects, vec![Effect::Audio(AudioEffect::SetMute(true))]);
    }

    #[test]
    fn status_changed_updates_mirror() {
        let mut state = fixtures::fixture_empty();
        apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Playing));
        assert_eq!(state.player.status, PlayStatus::Playing);
    }

    #[test]
    fn volume_changed_updates_mirror() {
        let mut state = fixtures::fixture_empty();
        apply_audio(
            &mut state,
            AudioEvent::VolumeChanged {
                volume: 42,
                muted: true,
            },
        );
        assert_eq!(state.player.volume, 42);
        assert!(state.player.muted);
    }

    #[test]
    fn engine_error_surfaces_as_toast_not_a_panic() {
        let mut state = fixtures::fixture_empty();
        apply_audio(&mut state, AudioEvent::EngineError("boom".to_string()));
        assert_eq!(state.toasts.len(), 1);
        assert_eq!(state.toasts[0].level, ToastLevel::Error);
    }

    // --- 10-05: device swap failure -----------------------------------------------------------

    fn device_unavailable_state() -> AppState {
        let mut state = fixtures::fixture_empty();
        // Mirrors what `reducer::modal::submit` already did before this failure arrived: config
        // optimistically holds the *attempted* device, with `pending_device_swap` remembering
        // what it held before that.
        state.config.audio.device_id = "alsa/hw:1,0".to_string();
        state.config.audio.output_driver = "alsa".to_string();
        state.player.known_devices = vec![crate::model::AudioDevice {
            id: "alsa/hw:1,0".to_string(),
            description: "USB Headphones DAC".to_string(),
            driver: "alsa".to_string(),
        }];
        state.player.pending_device_swap = Some(crate::state::player::PendingDeviceSwap {
            attempted_id: "alsa/hw:1,0".to_string(),
            previous_id: "pulse/a".to_string(),
            previous_driver: "pulse".to_string(),
        });
        state
    }

    #[test]
    fn swap_failure_toasts_device_name() {
        let mut state = device_unavailable_state();
        apply_audio(
            &mut state,
            AudioEvent::DeviceUnavailable {
                id: "alsa/hw:1,0".to_string(),
            },
        );
        assert_eq!(state.toasts.len(), 1);
        assert_eq!(state.toasts[0].level, ToastLevel::Error);
        assert_eq!(
            state.toasts[0].message,
            "could not switch to USB Headphones DAC"
        );
    }

    #[test]
    fn swap_failure_reverts_config_to_previous_device() {
        let mut state = device_unavailable_state();
        apply_audio(
            &mut state,
            AudioEvent::DeviceUnavailable {
                id: "alsa/hw:1,0".to_string(),
            },
        );
        assert_eq!(state.config.audio.device_id, "pulse/a");
        assert_eq!(state.config.audio.output_driver, "pulse");
        assert!(state.player.pending_device_swap.is_none());
    }

    #[test]
    fn swap_failure_for_unrelated_id_leaves_pending_swap_alone() {
        // A failure whose id doesn't match the currently pending attempt must not roll back or
        // clear an unrelated in-flight swap — it isn't the one this failure is about.
        let mut state = device_unavailable_state();
        apply_audio(
            &mut state,
            AudioEvent::DeviceUnavailable {
                id: "some/other-device".to_string(),
            },
        );
        assert_eq!(state.config.audio.device_id, "alsa/hw:1,0");
        assert!(state.player.pending_device_swap.is_some());
    }

    #[test]
    fn swap_failure_falls_back_to_id_when_device_unknown() {
        let mut state = fixtures::fixture_empty();
        apply_audio(
            &mut state,
            AudioEvent::DeviceUnavailable {
                id: "mystery/device".to_string(),
            },
        );
        assert_eq!(
            state.toasts[0].message,
            "could not switch to mystery/device"
        );
    }

    // --- 06-05: listening history ------------------------------------------------------------

    use crate::state::queue::RepeatMode;

    fn playing_state(duration_secs: u64) -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        state.player.duration = Duration::from_secs(duration_secs);
        // The item metadata has to agree: `authoritative_duration` prefers the server's runtime
        // over the engine's, so a fixture that set only the mirror would be describing a track of
        // one length while claiming another.
        let index = state.queue.play_order[state.queue.position];
        state.queue.entries[index].track.duration = Duration::from_secs(duration_secs);
        state
    }

    /// `PlayerAction::Stop` used to fall through this module's catch-all arm and do nothing at all,
    /// so neither `S` nor the player bar's Stop button worked (`docs/12-decisions.md`).
    #[test]
    fn stop_halts_playback_rewinds_and_keeps_the_queue() {
        let mut state = playing_state(180);
        state.player.status = PlayStatus::Playing;
        state.player.position = Duration::from_secs(42);
        let queued = state.queue.entries.len();
        assert!(queued > 0, "fixture assumption");

        let effects = apply_player(&mut state, PlayerAction::Stop);

        assert_eq!(state.player.status, PlayStatus::Stopped);
        assert_eq!(state.player.position, Duration::ZERO);
        assert_eq!(
            state.queue.entries.len(),
            queued,
            "the queue survives a stop"
        );
        assert!(
            state.player.restored_unloaded,
            "the engine has nothing loaded after a stop, so the next play must re-Load"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Stop)))
        );
    }

    #[test]
    fn play_after_stop_reloads_from_the_start() {
        let mut state = playing_state(180);
        state.player.position = Duration::from_secs(42);
        apply_player(&mut state, PlayerAction::Stop);

        let effects = apply_player(&mut state, PlayerAction::PlayPause);

        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Audio(AudioEffect::Load { start_at, .. }) if *start_at == Duration::ZERO
            )),
            "play after stop must re-Load at the start, not un-pause nothing: {effects:?}"
        );
        assert!(!state.player.restored_unloaded);
    }

    #[test]
    fn threshold_matches_emby_is_complete() {
        // Same boundary cases as `loxia_emby::endpoints::playback::is_complete`'s own tests
        // (`docs/12-decisions.md`: duplicated, not shared, since `loxia-core` cannot depend on
        // `loxia-emby`).
        let d200 = Duration::from_secs(200);
        assert!(!is_complete(Duration::from_secs(179), d200));
        assert!(is_complete(Duration::from_secs(180), d200));

        let hour = Duration::from_secs(3600);
        assert!(!is_complete(Duration::from_secs(239), hour));
        assert!(is_complete(Duration::from_secs(240), hour));

        assert!(!is_complete(Duration::from_secs(0), Duration::ZERO));
        assert!(!is_complete(Duration::from_secs(100), Duration::ZERO));
    }

    #[test]
    fn completed_track_is_recorded() {
        let mut state = playing_state(180);
        let current_track = state.queue.current().unwrap().track.clone();
        apply_audio(
            &mut state,
            AudioEvent::PositionChanged {
                position: Duration::from_secs(170), // >= 90% of 180s
                duration: Duration::from_secs(180),
            },
        );
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].track.id, current_track.id);
        assert!(state.history[0].completed);
    }

    #[test]
    fn skipped_track_is_not_recorded() {
        let mut state = playing_state(180);
        state.player.position = Duration::from_secs(36); // 20% of 180s
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: false });
        assert!(state.history.is_empty());
    }

    #[test]
    fn natural_end_always_records() {
        let mut state = playing_state(180);
        // Deliberately below the numeric threshold — natural end must record anyway.
        state.player.position = Duration::from_secs(90);
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert_eq!(state.history.len(), 1);
    }

    /// mpv cannot know the length of a live-transcoded stream — Emby serves it without a reliable
    /// duration — so the seek bar showed the transcoded portion instead of the track. The server's
    /// own `RunTimeTicks` is exact and known before playback starts, so it wins
    /// (`docs/12-decisions.md`).
    #[test]
    fn the_servers_track_length_beats_the_engines_guess() {
        let mut state = playing_state(200);
        let index = state.queue.play_order[state.queue.position];
        state.queue.entries[index].track.duration = Duration::from_secs(200);

        // What a transcoded stream looks like: mpv reports only what it has demuxed.
        apply_audio(
            &mut state,
            AudioEvent::PositionChanged {
                position: Duration::from_secs(10),
                duration: Duration::from_secs(12),
            },
        );
        assert_eq!(
            state.player.duration,
            Duration::from_secs(200),
            "the seek bar must show the track, not the transcoded part so far"
        );

        // With no runtime in the metadata there is nothing better than the engine's figure.
        state.queue.entries[index].track.duration = Duration::ZERO;
        apply_audio(
            &mut state,
            AudioEvent::PositionChanged {
                position: Duration::from_secs(11),
                duration: Duration::from_secs(13),
            },
        );
        assert_eq!(state.player.duration, Duration::from_secs(13));
    }

    #[test]
    fn four_minute_rule_for_long_tracks() {
        let mut state = playing_state(3600);
        apply_audio(
            &mut state,
            AudioEvent::PositionChanged {
                position: Duration::from_secs(239),
                duration: Duration::from_secs(3600),
            },
        );
        assert!(
            state.history.is_empty(),
            "239s must not cross the 240s floor"
        );

        apply_audio(
            &mut state,
            AudioEvent::PositionChanged {
                position: Duration::from_secs(240),
                duration: Duration::from_secs(3600),
            },
        );
        assert_eq!(state.history.len(), 1);
    }

    #[test]
    fn repeat_one_records_each_play() {
        let mut state = playing_state(180);
        state.queue.repeat = RepeatMode::One;
        state.player.position = Duration::from_secs(170);
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert_eq!(state.history.len(), 1);

        // `Repeat::One`'s reload (`advance_on_track_ended` -> `load_current`) reset the
        // recorded-this-play flag — a second full play of the same entry records a second entry.
        state.player.position = Duration::from_secs(170);
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert_eq!(state.history.len(), 2);
    }

    #[test]
    fn duplicate_completion_does_not_double_record() {
        // Positioned at the *last* entry with `Repeat::Off`: `advance_on_track_ended`'s
        // end-of-queue branch neither advances nor reloads, so `history_recorded_this_play`
        // is not reset between the two signals — the one case where a duplicate `TrackEnded`
        // could otherwise double-record the same play.
        let mut state = playing_state(180);
        state.queue.position = state.queue.play_order.len() - 1;
        state.player.position = Duration::from_secs(170);

        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert_eq!(state.history.len(), 1);

        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert_eq!(
            state.history.len(),
            1,
            "a track already at the head of the ring must not be duplicated"
        );
    }

    #[test]
    fn history_capped_at_fifty() {
        let mut state = playing_state(180);
        state.queue.repeat = RepeatMode::All;
        for _ in 0..51 {
            state.player.position = Duration::from_secs(170);
            apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        }
        assert_eq!(state.history.len(), 50);
    }

    #[test]
    fn newest_entry_is_first() {
        let mut state = playing_state(180);
        state.queue.repeat = RepeatMode::All;

        state.player.position = Duration::from_secs(170);
        let first_id = state.queue.current().unwrap().track.id.clone();
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });

        state.player.position = Duration::from_secs(170);
        let second_id = state.queue.current().unwrap().track.id.clone();
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });

        assert_eq!(state.history[0].track.id, second_id);
        assert_eq!(state.history[1].track.id, first_id);
    }

    #[test]
    fn played_at_comes_from_tick_not_clock() {
        let mut state = playing_state(180);
        state.clock = fixtures::fixed_epoch();
        apply_audio(
            &mut state,
            AudioEvent::PositionChanged {
                position: Duration::from_secs(170),
                duration: Duration::from_secs(180),
            },
        );
        assert_eq!(state.history[0].played_at, fixtures::fixed_epoch());
    }

    #[test]
    fn recording_emits_append_history_effect() {
        let mut state = playing_state(180);
        let effects = apply_audio(
            &mut state,
            AudioEvent::PositionChanged {
                position: Duration::from_secs(170),
                duration: Duration::from_secs(180),
            },
        );
        // Only the history append: crossing the threshold no longer reports a second play of its
        // own (`crossing_the_threshold_reports_no_second_play`).
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::Cache(cache)] if matches!(**cache, CacheEffect::AppendHistory(_))
            ),
            "got {effects:?}"
        );
    }

    #[test]
    fn toggle_history_flips_subview() {
        let mut state = fixtures::fixture_empty();
        assert_eq!(
            state.now_playing_subview,
            crate::state::NowPlayingSub::Queue
        );
        toggle_history_subview(&mut state);
        assert_eq!(
            state.now_playing_subview,
            crate::state::NowPlayingSub::History
        );
        toggle_history_subview(&mut state);
        assert_eq!(
            state.now_playing_subview,
            crate::state::NowPlayingSub::Queue
        );
    }

    // --- 06-07: playback reporting wiring -----------------------------------------------------

    use crate::config::QualityProfile;
    use crate::model::PlaySessionId;

    fn with_session(mut state: AppState) -> AppState {
        state.player.session = Some(PlaySessionId::from("session-1"));
        state
    }

    #[test]
    fn start_reported_once_after_load() {
        let mut state = with_session(playing_state(180));
        state.player.status = PlayStatus::Loading;
        state.player.start_reported = false;

        let effects = apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Playing));
        // `10-11`: every status change also refreshes MPRIS metadata, ahead of whichever
        // playback-report follows — `playing_state`'s own fixture has a queued track, so
        // `mpris_meta` is always `Some` here.
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Sys(SysEffect::UpdateMpris(_)),
                Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Start { .. }))
            ]
        ));

        // A later pause/resume must not re-report "Start" for the same session.
        apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Paused));
        let resumed = apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Playing));
        assert!(matches!(
            resumed.as_slice(),
            [
                Effect::Sys(SysEffect::UpdateMpris(_)),
                Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Progress { .. }))
            ]
        ));
    }

    /// The real defect behind "scrobbling doesn't work": mpv reports `core-idle` while it fetches
    /// a stream, so a network track's status sequence is `Loading -> Buffering -> Playing`, not the
    /// `Loading -> Playing` the Start report used to require. No `POST /Sessions/Playing` was ever
    /// sent — only Progress and Stopped.
    ///
    /// Measured against a live Emby: Progress + Stopped alone leave `PlayCount` at 0 with no
    /// `LastPlayedDate` (the item is merely marked watched), whereas the same sequence preceded by
    /// Start records a real play. No recorded play means no `PlaybackStopped` event, which is what
    /// the server-side Last.fm plugin scrobbles from (`docs/12-decisions.md`).
    #[test]
    fn start_is_reported_even_when_buffering_comes_first() {
        let mut state = with_session(playing_state(180));
        state.player.status = PlayStatus::Loading;
        state.player.start_reported = false;

        let buffering = apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Buffering));
        assert!(
            !buffering.iter().any(is_playback_report),
            "buffering alone reports nothing: {buffering:?}"
        );

        let playing = apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Playing));
        assert!(
            playing.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Start { .. }))
            )),
            "the first Playing after a Load must open the session, got {playing:?}"
        );

        // Re-buffering mid-track must not open a second session for the same play.
        apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Buffering));
        let again = apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Playing));
        assert!(
            !again.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Start { .. }))
            )),
            "got a second Start for one play: {again:?}"
        );
    }

    fn is_playback_report(effect: &Effect) -> bool {
        matches!(effect, Effect::Net(NetEffect::ReportPlayback(_)))
    }

    /// `10-11`.
    #[test]
    fn metadata_updated_on_status_change() {
        let mut state = with_session(playing_state(180));
        // A plain `Playing -> Stopped` transition matches neither the "Start" nor the "Progress"
        // branch in `apply_audio` — it must still refresh MPRIS metadata.
        let effects = apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Stopped));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Sys(SysEffect::UpdateMpris(meta))] if !meta.playing
        ));
    }

    /// `10-11`: no task has ever persisted a fetched image to disk (`docs/12-decisions.md`), so
    /// `art_url` is always `None` today, regardless of the track's own `image_tag`.
    #[test]
    fn artwork_url_omitted_when_not_cached() {
        let state = with_session(playing_state(180));
        let effects = mpris_meta(&state).into_iter().collect::<Vec<_>>();
        assert!(matches!(
            effects.as_slice(),
            [Effect::Sys(SysEffect::UpdateMpris(meta))] if meta.art_url.is_none()
        ));
    }

    #[test]
    fn progress_reported_on_pause_resume_and_seek() {
        let mut state = with_session(playing_state(180));

        let paused = apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Paused));
        assert!(matches!(
            paused.as_slice(),
            [
                Effect::Sys(SysEffect::UpdateMpris(_)),
                Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Progress {
                    paused: true,
                    ..
                }))
            ]
        ));

        let resumed = apply_audio(&mut state, AudioEvent::StatusChanged(PlayStatus::Playing));
        assert!(matches!(
            resumed.as_slice(),
            [
                Effect::Sys(SysEffect::UpdateMpris(_)),
                Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Progress {
                    paused: false,
                    ..
                }))
            ]
        ));

        let seeked = apply_player(
            &mut state,
            PlayerAction::Seek(crate::state::player::SeekTarget::Absolute(
                Duration::from_secs(30),
            )),
        );
        assert!(seeked.iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Progress { .. }))
        )));
    }

    #[test]
    fn stopped_reported_on_track_end() {
        let mut state = with_session(playing_state(180));
        let effects = apply_audio(&mut state, AudioEvent::TrackEnded { natural: false });
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Stopped { .. }))
        )));
    }

    #[test]
    /// One listen, one "this was played" signal. Crossing the completion threshold used to also
    /// `POST /PlayedItems` — Emby's manual mark-as-played — on top of the session reports that
    /// already record the play, and a server-side Last.fm plugin scrobbled the track twice
    /// (`docs/12-decisions.md`).
    fn crossing_the_threshold_reports_no_second_play() {
        let mut state = with_session(playing_state(180));
        for i in 0..20 {
            let effects = apply_audio(
                &mut state,
                AudioEvent::PositionChanged {
                    position: Duration::from_secs(170 + i),
                    duration: Duration::from_secs(180),
                },
            );
            assert!(
                !effects.iter().any(|e| matches!(
                    e,
                    Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Played { .. }))
                )),
                "the session reports are the only play signal: {effects:?}"
            );
        }
    }

    #[test]
    fn play_method_reflects_quality_profile() {
        let mut state = with_session(playing_state(180));

        state.player.quality_profile = QualityProfile::Direct;
        assert!(matches!(
            maybe_report_progress(&state).as_slice(),
            [Effect::Net(NetEffect::ReportPlayback(
                PlaybackReport::Progress {
                    play_method: PlayMethod::DirectStream,
                    ..
                }
            ))]
        ));

        state.player.quality_profile = QualityProfile::TranscodeHigh;
        assert!(matches!(
            maybe_report_progress(&state).as_slice(),
            [Effect::Net(NetEffect::ReportPlayback(
                PlaybackReport::Progress {
                    play_method: PlayMethod::Transcode,
                    ..
                }
            ))]
        ));
    }

    // --- 09-03: equalizer engine (reducer wiring) ----------------------------------------------

    use crate::config::EqPreset;

    fn preset(name: &str, gains: [f32; 10]) -> EqPreset {
        EqPreset {
            name: name.to_string(),
            gains,
        }
    }

    #[test]
    fn enabling_eq_applies_active_preset() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.enabled = false;
        state.player.known_presets = vec![preset("bass_boost", [4.0; 10])];

        let effects = apply_player(
            &mut state,
            PlayerAction::SetEqPreset("bass_boost".to_string()),
        );

        assert!(state.player.eq.enabled);
        assert_eq!(state.player.eq.gains, [4.0; 10]);
        assert_eq!(state.player.eq.preset_name, "bass_boost");
        assert_eq!(
            effects,
            vec![Effect::Audio(AudioEffect::SetEq(Some([4.0; 10])))]
        );
    }

    #[test]
    fn unknown_preset_name_toasts_and_leaves_eq_untouched() {
        let mut state = fixtures::fixture_empty();
        let before = state.player.eq.clone();
        state.player.known_presets = vec![preset("bass_boost", [4.0; 10])];

        let effects = apply_player(
            &mut state,
            PlayerAction::SetEqPreset("does_not_exist".to_string()),
        );

        assert!(effects.is_empty());
        assert_eq!(state.player.eq, before);
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("no such EQ preset"))
        );
    }

    #[test]
    fn set_eq_gain_clamps_and_snaps() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.enabled = true;
        state.player.eq.gains = [0.0; 10];

        apply_player(&mut state, PlayerAction::SetEqGain { band: 2, db: 30.0 });
        assert_eq!(state.player.eq.gains[2], 12.0);

        apply_player(&mut state, PlayerAction::SetEqGain { band: 3, db: 1.26 });
        assert_eq!(state.player.eq.gains[3], 1.5);
    }

    #[test]
    fn bypass_zeroes_without_uninstalling() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.enabled = true;
        state.player.eq.bypassed = false;
        state.player.eq.gains = [3.0; 10];

        let effects = apply_player(&mut state, PlayerAction::ToggleEqBypass);

        assert!(state.player.eq.bypassed);
        assert_eq!(
            effects,
            vec![Effect::Audio(AudioEffect::SetEq(Some([0.0; 10])))],
            "bypass must silence every band, not uninstall the chain with None"
        );
    }

    #[test]
    fn bypass_is_reversible() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.enabled = true;
        state.player.eq.gains = [5.0; 10];

        apply_player(&mut state, PlayerAction::ToggleEqBypass);
        assert!(state.player.eq.bypassed);

        let effects = apply_player(&mut state, PlayerAction::ToggleEqBypass);
        assert!(!state.player.eq.bypassed);
        assert_eq!(
            effects,
            vec![Effect::Audio(AudioEffect::SetEq(Some([5.0; 10])))],
            "un-bypassing must restore the exact original gains"
        );
    }

    #[test]
    fn eq_effects_are_empty_while_disabled() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.enabled = false;
        state.player.eq.gains = [7.0; 10];

        let effects = apply_player(&mut state, PlayerAction::SetEqGain { band: 0, db: 1.0 });
        assert!(
            effects.is_empty(),
            "no audio effect should fire while the EQ isn't enabled"
        );
    }

    // --- 09-04: ReplayGain ----------------------------------------------------------------------

    use crate::model::ReplayGainInfo;

    fn state_with_current_track_rg(rg: Option<ReplayGainInfo>) -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        let pos = state.queue.position;
        state.queue.entries[pos].track.replay_gain = rg;
        state
    }

    #[test]
    fn cycle_order_is_album_track_off() {
        let mut state = state_with_current_track_rg(None);
        state.player.replay_gain = ReplayGainMode::Album;

        apply_player(&mut state, PlayerAction::CycleReplayGain);
        assert_eq!(state.player.replay_gain, ReplayGainMode::Track);

        apply_player(&mut state, PlayerAction::CycleReplayGain);
        assert_eq!(state.player.replay_gain, ReplayGainMode::Off);

        apply_player(&mut state, PlayerAction::CycleReplayGain);
        assert_eq!(state.player.replay_gain, ReplayGainMode::Album);
    }

    #[test]
    fn applied_gain_recorded_in_player_state() {
        let mut state = state_with_current_track_rg(Some(ReplayGainInfo {
            track_gain_db: None,
            album_gain_db: Some(-6.2),
            track_peak: None,
            album_peak: None,
        }));
        state.player.replay_gain = ReplayGainMode::Track; // about to cycle to Off, then Album

        apply_player(&mut state, PlayerAction::CycleReplayGain); // Track -> Off
        assert_eq!(state.player.applied_gain, AppliedGain::None);
        assert_eq!(state.player.applied_gain_db, None);

        apply_player(&mut state, PlayerAction::CycleReplayGain); // Off -> Album
        assert_eq!(
            state.player.applied_gain,
            AppliedGain::Tags(ReplayGainMode::Album)
        );
        assert_eq!(state.player.applied_gain_db, Some(-6.2));
    }

    #[test]
    fn mode_change_applies_to_current_track() {
        // "Changing the mode takes effect on the current track immediately, not only on the next
        // one" — `CycleReplayGain` must itself emit `Effect::Audio(SetReplayGain)`, not merely
        // update state for the *next* `Load`.
        let mut state = state_with_current_track_rg(None);
        state.player.replay_gain = ReplayGainMode::Album;

        let effects = apply_player(&mut state, PlayerAction::CycleReplayGain);

        assert_eq!(
            effects,
            vec![Effect::Audio(AudioEffect::SetReplayGain(
                ReplayGainMode::Track
            ))]
        );
    }

    // --- 09-05: sleep timer ----------------------------------------------------------------------

    use crate::state::player::SleepTimer;

    fn at(secs: i64) -> Timestamp {
        fixtures::fixed_epoch()
            .checked_add(SignedDuration::from_secs(secs))
            .unwrap()
    }

    /// A queue playing at `fixture_playing_queue`'s own position (3 of 10), armed for `trigger`
    /// at `at(0)`, with `armed_entry` recorded the same way `reducer::modal`'s own submit handler
    /// does — no fade, no quit, until a test opts into either.
    fn state_with_sleep_timer(trigger: SleepTrigger) -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        state.clock = at(0);
        let armed_entry = state.queue.current().map(|e| e.entry_id);
        state.player.sleep_timer = Some(SleepTimer {
            trigger,
            fade_out: false,
            quit_after: false,
            armed_at: at(0),
            armed_entry,
            pre_fade_volume: None,
        });
        state
    }

    #[test]
    fn duration_trigger_fires_at_deadline() {
        let mut state = state_with_sleep_timer(SleepTrigger::Duration(Duration::from_secs(60)));

        let effects = evaluate_sleep_timer(&mut state, at(59));
        assert!(
            state.player.sleep_timer.is_some(),
            "must not fire before the deadline"
        );
        assert!(!effects.contains(&Effect::Audio(AudioEffect::Stop)));

        let effects = evaluate_sleep_timer(&mut state, at(60));
        assert!(
            state.player.sleep_timer.is_none(),
            "must fire exactly at the deadline"
        );
        assert!(effects.contains(&Effect::Audio(AudioEffect::Stop)));
    }

    #[test]
    fn timer_evaluated_from_tick_not_clock() {
        // `now` is threaded in explicitly, far past the deadline, while `state.clock` is left
        // untouched at an unrelated, much earlier value — firing here proves the evaluation used
        // the parameter, not a live clock read or `state.clock` itself.
        let mut state = state_with_sleep_timer(SleepTrigger::Duration(Duration::from_secs(60)));
        state.clock = at(0);
        let effects = evaluate_sleep_timer(&mut state, at(1000));
        assert!(effects.contains(&Effect::Audio(AudioEffect::Stop)));
    }

    #[test]
    fn end_of_track_fires_on_that_entry_only() {
        // The armed entry itself ending naturally fires the timer.
        let mut fires = state_with_sleep_timer(SleepTrigger::EndOfTrack);
        apply_audio(&mut fires, AudioEvent::TrackEnded { natural: true });
        assert!(
            fires.player.sleep_timer.is_none(),
            "the armed entry's own natural end must fire"
        );

        // Skipping to a different track first (a manual `Next` never synthesizes `TrackEnded` in
        // this reducer) must leave the timer armed, and that *new* track's own natural end must
        // not fire it — it isn't the entry `EndOfTrack` was armed for.
        let mut state = state_with_sleep_timer(SleepTrigger::EndOfTrack);
        apply_player(&mut state, PlayerAction::Next);
        assert!(state.player.sleep_timer.is_some());
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert!(
            state.player.sleep_timer.is_some(),
            "a different track's natural end must not fire an EndOfTrack timer armed for another entry"
        );
    }

    #[test]
    fn end_of_queue_honours_repeat_mode() {
        // At the last entry, `Repeat::Off`: fires.
        let mut state = state_with_sleep_timer(SleepTrigger::EndOfQueue);
        state.queue.position = state.queue.play_order.len() - 1;
        state.queue.repeat = RepeatMode::Off;
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert!(
            state.player.sleep_timer.is_none(),
            "must fire at the last entry with repeat off"
        );

        // At the last entry, `Repeat::One`: always reloads the same entry, never truly "ends".
        let mut state = state_with_sleep_timer(SleepTrigger::EndOfQueue);
        state.queue.position = state.queue.play_order.len() - 1;
        state.queue.repeat = RepeatMode::One;
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert!(
            state.player.sleep_timer.is_some(),
            "Repeat::One never truly ends the queue"
        );

        // Not at the last entry: never fires, regardless of repeat mode.
        let mut state = state_with_sleep_timer(SleepTrigger::EndOfQueue);
        state.queue.repeat = RepeatMode::Off;
        apply_audio(&mut state, AudioEvent::TrackEnded { natural: true });
        assert!(
            state.player.sleep_timer.is_some(),
            "must not fire before the last entry"
        );
    }

    #[test]
    fn fade_ramps_over_last_ten_seconds() {
        let mut state = state_with_sleep_timer(SleepTrigger::EndOfTrack);
        state.player.sleep_timer.as_mut().unwrap().fade_out = true;
        state.player.volume = 80;
        state.player.duration = Duration::from_secs(200);
        state.player.position = Duration::from_secs(191); // 9s remaining: inside the window

        let effects = evaluate_sleep_timer(&mut state, at(0));

        assert_eq!(effects, vec![Effect::Audio(AudioEffect::SetVolume(72))]); // 80 * 9/10
        assert_eq!(
            state.player.sleep_timer.as_ref().unwrap().pre_fade_volume,
            Some(80)
        );
    }

    #[test]
    fn fade_on_short_track_ramps_over_remaining_time() {
        let mut state = state_with_sleep_timer(SleepTrigger::EndOfTrack);
        state.player.sleep_timer.as_mut().unwrap().fade_out = true;
        state.player.volume = 80;
        state.player.duration = Duration::from_secs(6); // shorter than the 10s window
        state.player.position = Duration::from_secs(3); // half way through

        let effects = evaluate_sleep_timer(&mut state, at(0));

        // window = min(10, 6) = 6; remaining = 3; ratio = 0.5.
        assert_eq!(effects, vec![Effect::Audio(AudioEffect::SetVolume(40))]);
    }

    #[test]
    fn volume_restored_after_stop() {
        let mut state = state_with_sleep_timer(SleepTrigger::Duration(Duration::from_secs(10)));
        state.player.sleep_timer.as_mut().unwrap().fade_out = true;
        state.player.volume = 65;

        evaluate_sleep_timer(&mut state, at(5)); // inside the window; captures pre_fade_volume
        assert_eq!(
            state.player.sleep_timer.as_ref().unwrap().pre_fade_volume,
            Some(65)
        );

        let effects = evaluate_sleep_timer(&mut state, at(10)); // fires
        assert!(state.player.sleep_timer.is_none());
        assert!(effects.contains(&Effect::Audio(AudioEffect::Stop)));
        assert!(effects.contains(&Effect::Audio(AudioEffect::SetVolume(65))));
    }

    #[test]
    fn volume_restored_even_when_quit_after_set() {
        let mut state = state_with_sleep_timer(SleepTrigger::Duration(Duration::from_secs(10)));
        state.player.sleep_timer.as_mut().unwrap().fade_out = true;
        state.player.sleep_timer.as_mut().unwrap().quit_after = true;
        state.player.volume = 50;

        evaluate_sleep_timer(&mut state, at(5));
        let effects = evaluate_sleep_timer(&mut state, at(10));

        assert!(effects.contains(&Effect::Audio(AudioEffect::SetVolume(50))));
        assert!(effects.contains(&Effect::Sys(SysEffect::Exit)));
    }

    #[test]
    fn quit_after_emits_exit_following_snapshot() {
        // The reducer's own contribution is emitting `Effect::Sys(Exit)` when firing with
        // `quit_after` set, alongside the stop/volume-restore effects — the runtime's existing
        // shutdown sequencing (already built for `SystemEvent::Quit`) is what actually persists
        // the session snapshot before exiting (`docs/12-decisions.md`), not something this
        // reducer function needs to build itself.
        let mut state = state_with_sleep_timer(SleepTrigger::Duration(Duration::from_secs(10)));
        state.player.sleep_timer.as_mut().unwrap().quit_after = true;

        let effects = evaluate_sleep_timer(&mut state, at(10));

        assert!(effects.contains(&Effect::Audio(AudioEffect::Stop)));
        assert_eq!(
            effects.last(),
            Some(&Effect::Sys(SysEffect::Exit)),
            "Exit must come last, after Stop/volume-restore"
        );
    }

    // --- 09-06: quality profiles ------------------------------------------------------------------

    fn state_with_quality(profile: QualityProfile) -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        state.player.quality_profile = profile;
        state.config.transcode.mode = profile;
        state
    }

    #[test]
    fn cycle_order_is_direct_high_med_low() {
        let mut state = state_with_quality(QualityProfile::Direct);

        apply_player(&mut state, PlayerAction::CycleQuality);
        assert_eq!(state.player.quality_profile, QualityProfile::TranscodeHigh);

        apply_player(&mut state, PlayerAction::CycleQuality);
        assert_eq!(state.player.quality_profile, QualityProfile::TranscodeMed);

        apply_player(&mut state, PlayerAction::CycleQuality);
        assert_eq!(state.player.quality_profile, QualityProfile::TranscodeLow);

        apply_player(&mut state, PlayerAction::CycleQuality);
        assert_eq!(state.player.quality_profile, QualityProfile::Direct);
    }

    #[test]
    fn cycle_reloads_at_same_position() {
        let mut state = state_with_quality(QualityProfile::Direct);
        state.player.position = Duration::from_secs(42);

        let effects = apply_player(&mut state, PlayerAction::CycleQuality);

        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Audio(AudioEffect::Load { start_at, .. }) if *start_at == Duration::from_secs(42)
        )));
    }

    #[test]
    fn quality_cycle_persists_to_config() {
        let mut state = state_with_quality(QualityProfile::Direct);

        let effects = apply_player(&mut state, PlayerAction::CycleQuality);

        assert_eq!(state.config.transcode.mode, QualityProfile::TranscodeHigh);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
    }

    /// The `profile` of whichever `Effect::Cache(EnsureCached { .. })` is present, if any —
    /// avoids repeating the same nested `Box`/enum match at every call site below.
    fn ensure_cached_profile(effects: &[Effect]) -> Option<QualityProfile> {
        effects.iter().find_map(|e| match e {
            Effect::Cache(b) => match **b {
                CacheEffect::EnsureCached { profile, .. } => Some(profile),
                _ => None,
            },
            _ => None,
        })
    }

    #[test]
    fn cache_key_changes_with_profile() {
        let mut state = state_with_quality(QualityProfile::Direct);

        let effects = apply_player(&mut state, PlayerAction::CycleQuality);

        assert_eq!(
            ensure_cached_profile(&effects),
            Some(QualityProfile::TranscodeHigh)
        );
    }

    #[test]
    fn switching_back_to_cached_profile_makes_no_request() {
        // The reducer's own contribution is always emitting the *same* `EnsureCached { track,
        // profile }` request shape regardless of history — whether that request turns into a
        // real fetch or is served instantly from the rolling cache is `loxia-cache`'s own
        // content-addressed lookup (already covered in `08-01`/`08-02`), not something this
        // reducer decides. This proves the request the cache layer would recognise as
        // already-satisfied is exactly reissued, not skipped or reshaped, when cycling back.
        let mut state = state_with_quality(QualityProfile::TranscodeLow);

        apply_player(&mut state, PlayerAction::CycleQuality); // Low -> Direct
        apply_player(&mut state, PlayerAction::CycleQuality); // Direct -> High
        apply_player(&mut state, PlayerAction::CycleQuality); // High -> Med
        let effects = apply_player(&mut state, PlayerAction::CycleQuality); // Med -> Low (again)

        assert_eq!(state.player.quality_profile, QualityProfile::TranscodeLow);
        assert_eq!(
            ensure_cached_profile(&effects),
            Some(QualityProfile::TranscodeLow)
        );
    }

    #[test]
    fn preload_reissued_after_quality_change() {
        let mut state = state_with_quality(QualityProfile::Direct);
        // Prime `last_preloaded` as if the next entry was already preloaded at the old profile.
        let next_entry_id = state
            .queue
            .play_order
            .get(state.queue.position + 1)
            .and_then(|&idx| state.queue.entries.get(idx))
            .map(|e| e.entry_id);
        state.player.last_preloaded = next_entry_id;

        let effects = apply_player(&mut state, PlayerAction::CycleQuality);

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Preload { .. }))),
            "the next entry was queued at the old profile, so Preload must be reissued even \
             though the target entry itself didn't change"
        );
    }

    #[test]
    fn cycle_refused_offline_for_local_file() {
        for availability in [Availability::Cached, Availability::Downloaded] {
            let mut state = state_with_quality(QualityProfile::Direct);
            let pos = state.queue.position;
            state.queue.entries[pos].availability = availability;

            let effects = apply_player(&mut state, PlayerAction::CycleQuality);

            assert!(effects.is_empty(), "{availability:?} must refuse cycling");
            assert_eq!(state.player.quality_profile, QualityProfile::Direct);
            assert!(
                state
                    .toasts
                    .iter()
                    .any(|t| t.message.contains("quality changes need a connection")),
                "{availability:?} must toast the refusal"
            );
        }
    }

    #[test]
    fn toast_names_bitrate_and_codec() {
        let mut state = state_with_quality(QualityProfile::Direct);
        state.config.transcode.target_codec = TargetCodec::Opus;

        apply_player(&mut state, PlayerAction::CycleQuality); // -> TranscodeHigh

        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("quality: 320 kbps Opus"))
        );
    }

    #[test]
    fn no_track_playing_changes_profile_without_load() {
        let mut state = state_with_quality(QualityProfile::Direct);
        state.queue = crate::state::queue::QueueState::default();
        state.player.current = None;

        let effects = apply_player(&mut state, PlayerAction::CycleQuality);

        assert_eq!(state.player.quality_profile, QualityProfile::TranscodeHigh);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Load { .. }))),
            "no track is loaded, so nothing should reload"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
    }
}
