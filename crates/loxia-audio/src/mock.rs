//! MockEngine: deterministic fake backend driven by a virtual clock (`docs/05-audio-engine.md`
//! §§1, 9). Lets the whole test suite, and every UI task above this crate, run with no mpv and no
//! sound card — `AppState`/reducer tests never need this directly, but `loxia --no-audio` and
//! `loxia-audio`'s own suite do.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use loxia_core::model::{AudioDevice, AudioFormat, Codec};
use loxia_core::state::player::PlayStatus;
use tokio::sync::mpsc;

use crate::backend::{AudioBackend, AudioCommand, AudioEvent};
use crate::error::AudioError;

/// Position events fire at this cadence of *virtual* time, matching the real engine's own 4 Hz
/// throttle (`docs/05-audio-engine.md` §2) so tests exercise the same event rate.
const POSITION_INTERVAL: Duration = Duration::from_millis(250);

/// What a seeded `Load` simulates — the `Format` event it emits and the duration `advance()` runs
/// against. Not named in this task's own `MockControl` signature, but "`Format(..)` from a
/// per-track table the test can seed" names a capability with no given method or type; see
/// `docs/12-decisions.md`. `Default` gives any unseeded URL a plausible, generously long track so
/// a test that doesn't care about specific values can still call `advance()` freely.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackProfile {
    pub format: AudioFormat,
    pub duration: Duration,
}

impl Default for TrackProfile {
    fn default() -> Self {
        TrackProfile {
            format: AudioFormat {
                codec: Codec::Flac,
                sample_rate_hz: 44_100,
                bit_depth: Some(16),
                channels: 2,
                bitrate_bps: None,
            },
            duration: Duration::from_secs(180),
        }
    }
}

struct Inner {
    commands: Vec<AudioCommand>,
    subscribers: Vec<mpsc::UnboundedSender<AudioEvent>>,
    position: Duration,
    duration: Duration,
    loaded: bool,
    profiles: HashMap<String, TrackProfile>,
    devices: Vec<AudioDevice>,
    fail_next_load: Option<String>,
    stalled: bool,
}

impl Inner {
    fn emit(&mut self, event: AudioEvent) {
        self.subscribers.retain(|tx| tx.send(event.clone()).is_ok());
    }
}

/// The `AudioBackend` implementation under test. Cloning the shared handle (via `MockControl`)
/// rather than the engine itself, so both sides of the trait boundary see the same state.
pub struct MockEngine(Arc<Mutex<Inner>>);

/// The test-side handle: everything an acceptance test needs to drive and inspect `MockEngine`
/// that isn't part of the `AudioBackend` trait itself.
pub struct MockControl(Arc<Mutex<Inner>>);

impl MockEngine {
    pub fn new() -> (MockEngine, MockControl) {
        let inner = Arc::new(Mutex::new(Inner {
            commands: Vec::new(),
            subscribers: Vec::new(),
            position: Duration::ZERO,
            duration: Duration::ZERO,
            loaded: false,
            profiles: HashMap::new(),
            devices: Vec::new(),
            fail_next_load: None,
            stalled: false,
        }));
        (MockEngine(inner.clone()), MockControl(inner))
    }
}

impl AudioBackend for MockEngine {
    fn send(&self, cmd: AudioCommand) -> Result<(), AudioError> {
        let mut inner = self.0.lock().expect("mock mutex is never poisoned");
        inner.commands.push(cmd.clone());
        if inner.stalled {
            return Ok(());
        }

        match cmd {
            AudioCommand::Load { url, .. } => {
                if let Some(reason) = inner.fail_next_load.take() {
                    inner.loaded = false;
                    inner.emit(AudioEvent::Error(AudioError::Load {
                        url_redacted: url.to_string(),
                        reason,
                    }));
                    return Ok(());
                }
                let profile = inner
                    .profiles
                    .get(url.as_str())
                    .cloned()
                    .unwrap_or_default();
                inner.position = Duration::ZERO;
                inner.duration = profile.duration;
                inner.loaded = true;
                inner.emit(AudioEvent::StatusChanged(PlayStatus::Loading));
                inner.emit(AudioEvent::Format(profile.format));
                inner.emit(AudioEvent::StatusChanged(PlayStatus::Playing));
            }
            AudioCommand::Play => {
                inner.emit(AudioEvent::StatusChanged(PlayStatus::Playing));
            }
            AudioCommand::Pause => {
                inner.emit(AudioEvent::StatusChanged(PlayStatus::Paused));
            }
            AudioCommand::Stop => {
                inner.loaded = false;
                inner.emit(AudioEvent::TrackEnded { natural: false });
                inner.emit(AudioEvent::StatusChanged(PlayStatus::Stopped));
            }
            AudioCommand::EnumerateDevices => {
                let devices = inner.devices.clone();
                inner.emit(AudioEvent::Devices(devices));
            }
            AudioCommand::SetDevice(id) if !inner.devices.iter().any(|d| d.id == id) => {
                inner.emit(AudioEvent::Error(AudioError::DeviceUnavailable { id }));
            }
            // `Preload`/`Seek`/`SetVolume`/`SetMute`/`SetEq`/`SetReplayGain`/
            // `Shutdown`: recorded in `commands()` but this task names no simulated event for
            // them.
            _ => {}
        }
        Ok(())
    }

    fn subscribe(&self) -> mpsc::UnboundedReceiver<AudioEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.0
            .lock()
            .expect("mock mutex is never poisoned")
            .subscribers
            .push(tx);
        rx
    }
}

impl MockControl {
    /// Drives the virtual clock forward by `by`, emitting one `Position` event per
    /// `POSITION_INTERVAL` of virtual time crossed — **never** a real timer or sleep. Reaching the
    /// loaded track's duration emits `TrackEnded { natural: true }` and stops advancing early,
    /// even if `by` overshoots past it.
    pub fn advance(&self, by: Duration) {
        let mut inner = self.0.lock().expect("mock mutex is never poisoned");
        if !inner.loaded {
            return;
        }
        let mut remaining = by;
        while remaining >= POSITION_INTERVAL {
            remaining -= POSITION_INTERVAL;
            inner.position += POSITION_INTERVAL;
            let reached_end = inner.position >= inner.duration;
            if reached_end {
                inner.position = inner.duration;
            }
            let secs = inner.position.as_secs_f64();
            let duration = inner.duration.as_secs_f64();
            inner.emit(AudioEvent::Position { secs, duration });
            if reached_end {
                inner.loaded = false;
                inner.emit(AudioEvent::TrackEnded { natural: true });
                return;
            }
        }
    }

    /// Forces `TrackEnded { natural: true }` immediately, without needing `advance()` to reach the
    /// loaded duration.
    pub fn finish_track(&self) {
        let mut inner = self.0.lock().expect("mock mutex is never poisoned");
        if !inner.loaded {
            return;
        }
        inner.loaded = false;
        inner.emit(AudioEvent::TrackEnded { natural: true });
    }

    /// Emits a bare `StatusChanged`, with no command behind it — for the states a real engine
    /// reaches on its own. `Buffering` in particular: mpv reports `core-idle` whenever a network
    /// stream rebuffers mid-track, and nothing a caller *sends* produces it.
    pub fn emit_status(&self, status: PlayStatus) {
        self.0
            .lock()
            .expect("mock mutex is never poisoned")
            .emit(AudioEvent::StatusChanged(status));
    }

    pub fn fail_next_load(&self, reason: &str) {
        self.0
            .lock()
            .expect("mock mutex is never poisoned")
            .fail_next_load = Some(reason.to_string());
    }

    pub fn stall(&self, yes: bool) {
        self.0.lock().expect("mock mutex is never poisoned").stalled = yes;
    }

    pub fn remove_device(&self, id: &str) {
        self.0
            .lock()
            .expect("mock mutex is never poisoned")
            .devices
            .retain(|d| d.id != id);
    }

    pub fn set_devices(&self, d: Vec<AudioDevice>) {
        self.0.lock().expect("mock mutex is never poisoned").devices = d;
    }

    pub fn commands(&self) -> Vec<AudioCommand> {
        self.0
            .lock()
            .expect("mock mutex is never poisoned")
            .commands
            .clone()
    }

    /// Seeds the `Format`/duration a subsequent `Load` for this exact `url` string simulates. See
    /// `TrackProfile`'s own doc comment for why this exists despite not being in the task's
    /// literal `MockControl` signature.
    pub fn seed_track(&self, url: &str, profile: TrackProfile) {
        self.0
            .lock()
            .expect("mock mutex is never poisoned")
            .profiles
            .insert(url.to_string(), profile);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::effect::RedactedUrl;

    fn load_cmd(url: &str) -> AudioCommand {
        AudioCommand::Load {
            url: RedactedUrl::new(url),
            headers: Vec::new(),
            start_at: Duration::ZERO,
            gain_db: None,
        }
    }

    /// Everything currently buffered on `rx`, without blocking — every emission in this module
    /// happens synchronously inside `send()`/`advance()`/etc., so nothing is ever still in flight
    /// by the time a test calls this.
    fn drain(rx: &mut mpsc::UnboundedReceiver<AudioEvent>) -> Vec<AudioEvent> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    #[test]
    fn load_emits_loading_format_playing_in_order() {
        let (engine, _control) = MockEngine::new();
        let mut rx = engine.subscribe();
        engine.send(load_cmd("http://host/track")).unwrap();
        let events = drain(&mut rx);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], AudioEvent::StatusChanged(PlayStatus::Loading));
        assert!(matches!(events[1], AudioEvent::Format(_)));
        assert_eq!(events[2], AudioEvent::StatusChanged(PlayStatus::Playing));
    }

    #[test]
    fn advance_emits_throttled_position_events() {
        let (engine, control) = MockEngine::new();
        let mut rx = engine.subscribe();
        engine.send(load_cmd("http://host/track")).unwrap();
        drain(&mut rx);

        control.advance(Duration::from_secs(1));
        let events = drain(&mut rx);
        let positions = events
            .iter()
            .filter(|e| matches!(e, AudioEvent::Position { .. }))
            .count();
        assert_eq!(
            positions, 4,
            "1s of virtual time at 4 Hz must yield 4 events"
        );
    }

    #[test]
    fn reaching_duration_emits_track_ended_natural() {
        let (engine, control) = MockEngine::new();
        control.seed_track(
            "http://host/track",
            TrackProfile {
                duration: Duration::from_millis(500),
                ..TrackProfile::default()
            },
        );
        let mut rx = engine.subscribe();
        engine.send(load_cmd("http://host/track")).unwrap();
        drain(&mut rx);

        // Overshoots past the 500ms duration; must stop exactly at the end, not run past it.
        control.advance(Duration::from_secs(2));
        let events = drain(&mut rx);
        assert_eq!(
            events.last(),
            Some(&AudioEvent::TrackEnded { natural: true })
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, AudioEvent::TrackEnded { .. }))
                .count(),
            1,
            "must not fire TrackEnded more than once for one overshoot"
        );
    }

    #[test]
    fn finish_track_forces_track_ended_early() {
        let (engine, control) = MockEngine::new();
        let mut rx = engine.subscribe();
        engine.send(load_cmd("http://host/track")).unwrap();
        drain(&mut rx);

        control.finish_track();
        let events = drain(&mut rx);
        assert_eq!(events, vec![AudioEvent::TrackEnded { natural: true }]);
    }

    #[test]
    fn stop_emits_track_ended_not_natural() {
        let (engine, _control) = MockEngine::new();
        let mut rx = engine.subscribe();
        engine.send(load_cmd("http://host/track")).unwrap();
        drain(&mut rx);

        engine.send(AudioCommand::Stop).unwrap();
        let events = drain(&mut rx);
        assert_eq!(
            events,
            vec![
                AudioEvent::TrackEnded { natural: false },
                AudioEvent::StatusChanged(PlayStatus::Stopped),
            ]
        );
    }

    #[test]
    fn fail_next_load_emits_error_and_stays_stopped() {
        let (engine, control) = MockEngine::new();
        control.fail_next_load("offline");
        let mut rx = engine.subscribe();
        engine.send(load_cmd("http://host/track")).unwrap();
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            AudioEvent::Error(AudioError::Load { reason, .. }) if reason == "offline"
        ));

        // One-shot: the *next* load must succeed normally.
        engine.send(load_cmd("http://host/track")).unwrap();
        let events = drain(&mut rx);
        assert_eq!(events[0], AudioEvent::StatusChanged(PlayStatus::Loading));
    }

    #[test]
    fn stall_records_commands_but_emits_nothing() {
        let (engine, control) = MockEngine::new();
        control.stall(true);
        let mut rx = engine.subscribe();
        engine.send(load_cmd("http://host/track")).unwrap();
        engine.send(AudioCommand::Play).unwrap();

        assert!(drain(&mut rx).is_empty());
        assert_eq!(control.commands().len(), 2);
    }

    #[test]
    fn set_device_unknown_id_errors() {
        let (engine, control) = MockEngine::new();
        control.set_devices(vec![AudioDevice {
            id: "alsa/hw:0,0".to_string(),
            description: "Built-in".to_string(),
            driver: "alsa".to_string(),
        }]);
        let mut rx = engine.subscribe();

        engine
            .send(AudioCommand::SetDevice("pulse/nope".to_string()))
            .unwrap();
        let events = drain(&mut rx);
        assert_eq!(
            events,
            vec![AudioEvent::Error(AudioError::DeviceUnavailable {
                id: "pulse/nope".to_string()
            })]
        );
    }

    #[test]
    fn remove_device_then_set_device_errors() {
        let (engine, control) = MockEngine::new();
        let device = AudioDevice {
            id: "alsa/hw:0,0".to_string(),
            description: "Built-in".to_string(),
            driver: "alsa".to_string(),
        };
        control.set_devices(vec![device.clone()]);
        control.remove_device(&device.id);

        let mut rx = engine.subscribe();
        engine
            .send(AudioCommand::SetDevice(device.id.clone()))
            .unwrap();
        assert!(matches!(
            drain(&mut rx).as_slice(),
            [AudioEvent::Error(AudioError::DeviceUnavailable { .. })]
        ));
    }

    #[test]
    fn mock_is_deterministic() {
        fn run() -> Vec<AudioEvent> {
            let (engine, control) = MockEngine::new();
            control.seed_track(
                "http://host/track",
                TrackProfile {
                    duration: Duration::from_secs(2),
                    ..TrackProfile::default()
                },
            );
            let mut rx = engine.subscribe();
            engine.send(load_cmd("http://host/track")).unwrap();
            control.advance(Duration::from_millis(750));
            engine.send(AudioCommand::Pause).unwrap();
            control.advance(Duration::from_secs(5)); // overshoots into TrackEnded
            drain(&mut rx)
        }

        let first = run();
        for _ in 0..100 {
            assert_eq!(run(), first);
        }
    }

    #[test]
    fn mock_uses_no_wall_clock() {
        let (engine, _control) = MockEngine::new();
        let mut rx = engine.subscribe();
        engine.send(load_cmd("http://host/track")).unwrap();
        drain(&mut rx);

        std::thread::sleep(Duration::from_millis(100));
        assert!(
            drain(&mut rx).is_empty(),
            "no Position event may appear without an explicit advance()"
        );
    }
}
