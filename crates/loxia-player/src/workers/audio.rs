//! Owns the AudioBackend; serves playback effects (`05-06`, `docs/01-architecture.md` §4).

use loxia_audio::backend::{
    AudioBackend, AudioCommand as EngineCommand, AudioEvent as EngineEvent,
    EqCurve as EngineEqCurve,
};
use loxia_core::action::{AudioEvent, DataAction};
use loxia_core::effect::{AudioEffect, Effect};
use loxia_core::event::Event;
use loxia_core::state::player::PlayStatus;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Two loops in one task via `select!`: drain `Effect::Audio` into `backend.send`, and forward
/// the backend's own events into the shared event channel after translation. Ends when either
/// channel closes — `effects` closing means the app is shutting down; `backend`'s own event
/// stream closing means the engine itself is gone, which is just as much a reason to stop.
pub fn spawn(
    backend: Box<dyn AudioBackend>,
    mut effects: mpsc::UnboundedReceiver<Effect>,
    events: mpsc::UnboundedSender<Event>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut backend_events = backend.subscribe();
        // Tracked purely to resolve `AudioEffect::PlayPause`'s toggle — mpv (and `AudioCommand`)
        // has no combined "toggle" command, only distinct `Play`/`Pause`, so *something* has to
        // remember which one is currently in effect. The backend's own `StatusChanged` events
        // (already being forwarded regardless) are what keep this current, with no separate
        // query needed.
        //
        // The question is "is it **paused**", not "is it playing": mpv reports `core-idle`
        // (`PlayStatus::Buffering`) on every rebuffer of a network stream, and this used to hold
        // `playing = status == Playing`, so a rebuffer made it `false` and the next PlayPause
        // resolved to `Play` — a no-op against something already playing. Pressing pause during a
        // rebuffer therefore did nothing at all, silently: playback carried on and, because
        // nothing paused, no `IsPaused: true` was ever reported to Emby either
        // (`docs/12-decisions.md`).
        //
        // Starts `true` — nothing is loaded yet, so the first press means "play".
        let mut paused = true;

        loop {
            tokio::select! {
                biased;
                maybe = effects.recv() => {
                    match maybe {
                        // `load_current`/`preload_effects` (loxia-core, which can't build a real
                        // Emby URL) emit `Load`/`Preload` with an inert `emby-track:{id}` placeholder,
                        // relying on the cache worker's `CacheResolved` reply to follow up with the
                        // real, playable URL. Handing that placeholder to mpv makes it fail to open
                        // the file (`Cannot open file 'emby-track:...'`) and, for a `Preload`
                        // appended to mpv's playlist, auto-advance straight into a broken entry when
                        // the current track ends — the reported "not every song is played." Dropped
                        // here so only real URLs ever reach the engine (`docs/12-decisions.md`).
                        Some(Effect::Audio(effect)) if is_placeholder_load(&effect) => {
                            tracing::debug!("dropped inert placeholder load before mpv");
                        }
                        Some(Effect::Audio(effect)) => {
                            // Diagnostic: every real Load/Preload actually handed to mpv, with the
                            // token-redacted URL and start offset — so a track that fails to play can
                            // be traced to exactly what the engine was told to open (`RedactedUrl`'s
                            // `Display` strips `api_key`, so this is safe to log).
                            match &effect {
                                AudioEffect::Load { url, start_at, .. } => {
                                    tracing::info!(url = %url, start_at = ?start_at, "loading track into mpv");
                                }
                                AudioEffect::Preload { url, .. } => {
                                    tracing::info!(url = %url, "preloading track into mpv");
                                }
                                _ => {}
                            }
                            let cmd = if let AudioEffect::PlayPause = effect {
                                if paused { EngineCommand::Play } else { EngineCommand::Pause }
                            } else {
                                to_engine_command(effect)
                            };
                            if let Err(e) = backend.send(cmd) {
                                let _ = events.send(Event::Audio(AudioEvent::EngineError(e.to_string())));
                            }
                        }
                        Some(_) => {} // not ours; every worker's channel carries the whole `Effect`
                        None => break,
                    }
                }
                maybe = backend_events.recv() => {
                    match maybe {
                        Some(event) => {
                            if let EngineEvent::StatusChanged(status) = &event {
                                match status {
                                    PlayStatus::Paused => paused = true,
                                    // Buffering and Loading are both "on its way to playing" —
                                    // pressing pause there must pause, not re-issue play.
                                    PlayStatus::Playing
                                    | PlayStatus::Buffering
                                    | PlayStatus::Loading => paused = false,
                                    // mpv unloads on stop, so the next press means "play" again —
                                    // the same thing this starts out as.
                                    PlayStatus::Stopped => paused = true,
                                }
                            }
                            if let Some(action) = to_action(event) {
                                let _ = events.send(action);
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    })
}

/// The inert placeholder scheme `loxia_core::reducer::queue::placeholder_url` builds — kept in sync
/// with it by the `placeholder_scheme_matches_core` test below.
const PLACEHOLDER_SCHEME: &str = "emby-track:";

/// Whether this is a `Load`/`Preload` still carrying the placeholder URL (rather than a real stream
/// or `file://` URL). Only `Load`/`Preload` carry a URL; every other effect is never a placeholder.
fn is_placeholder_load(effect: &AudioEffect) -> bool {
    match effect {
        AudioEffect::Load { url, .. } | AudioEffect::Preload { url, .. } => {
            url.as_str().starts_with(PLACEHOLDER_SCHEME)
        }
        _ => false,
    }
}

fn to_engine_command(effect: AudioEffect) -> EngineCommand {
    match effect {
        AudioEffect::Load {
            url,
            headers,
            start_at,
            gain_db,
        } => EngineCommand::Load {
            url,
            headers: headers.into_iter().collect(),
            start_at,
            gain_db,
        },
        AudioEffect::Preload {
            url,
            headers,
            gain_db,
        } => EngineCommand::Preload {
            url,
            headers: headers.into_iter().collect(),
            gain_db,
        },
        // Resolved by the caller, which needs the worker's own tracked status to pick one.
        AudioEffect::PlayPause => unreachable!("PlayPause is resolved before calling this"),
        AudioEffect::Stop => EngineCommand::Stop,
        AudioEffect::Seek(target) => EngineCommand::Seek(target),
        AudioEffect::SetVolume(v) => EngineCommand::SetVolume(v),
        AudioEffect::SetMute(m) => EngineCommand::SetMute(m),
        AudioEffect::SetEq(curve) => {
            EngineCommand::SetEq(curve.map(|gains| EngineEqCurve { gains }))
        }
        AudioEffect::SetReplayGain(mode) => EngineCommand::SetReplayGain(mode),
        AudioEffect::SetDevice(id) => EngineCommand::SetDevice(id),
        AudioEffect::EnumerateDevices => EngineCommand::EnumerateDevices,
    }
}

/// `docs/01-architecture.md` §4 / this task's own event table. `Buffering`'s percentage is
/// discarded — `loxia_core::action::AudioEvent` has no field to carry it, matching the task's own
/// literal mapping ("`Buffering` → `StatusChanged(Buffering)`"); see `docs/12-decisions.md`.
fn to_action(event: EngineEvent) -> Option<Event> {
    match event {
        EngineEvent::StatusChanged(status) => Some(Event::Audio(AudioEvent::StatusChanged(status))),
        EngineEvent::Position { secs, duration } => {
            Some(Event::Audio(AudioEvent::PositionChanged {
                position: std::time::Duration::from_secs_f64(secs.max(0.0)),
                duration: std::time::Duration::from_secs_f64(duration.max(0.0)),
            }))
        }
        EngineEvent::Format(format) => Some(Event::Audio(AudioEvent::FormatDetected(format))),
        EngineEvent::TrackEnded { natural } => {
            Some(Event::Audio(AudioEvent::TrackEnded { natural }))
        }
        EngineEvent::Devices(devices) => Some(Event::Data(DataAction::DevicesLoaded { devices })),
        EngineEvent::Buffering(_percent) => Some(Event::Audio(AudioEvent::StatusChanged(
            PlayStatus::Buffering,
        ))),
        EngineEvent::VolumeChanged { volume, muted } => {
            Some(Event::Audio(AudioEvent::VolumeChanged { volume, muted }))
        }
        // `10-05`: `DeviceUnavailable` is special-cased ahead of the generic fallback below —
        // `AudioError`'s own `Display` is a fixed, generic sentence that never interpolates a
        // field's content (`loxia_audio::error`'s own doc comment), so the device id the picker's
        // "could not switch to `<name>`" toast needs would otherwise already be lost by the time
        // this reaches the reducer.
        EngineEvent::Error(loxia_audio::error::AudioError::DeviceUnavailable { id }) => {
            Some(Event::Audio(AudioEvent::DeviceUnavailable { id }))
        }
        EngineEvent::Error(e) => Some(Event::Audio(AudioEvent::EngineError(e.to_string()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_audio::error::AudioError;
    use loxia_audio::mock::MockEngine;
    use loxia_core::effect::RedactedUrl;
    use loxia_core::model::{AudioFormat, Codec, ItemId};
    use loxia_core::state::player::SeekTarget;
    use std::time::Duration;

    async fn drain(rx: &mut mpsc::UnboundedReceiver<Event>) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn effect_to_command_mapping() {
        let (engine, control) = MockEngine::new();
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let handle = spawn(Box::new(engine), effects_rx, events_tx);

        let load = Effect::Audio(AudioEffect::Load {
            url: RedactedUrl::new("http://host/track"),
            headers: std::collections::BTreeMap::new(),
            start_at: Duration::ZERO,
            gain_db: None,
        });
        effects_tx.send(load).unwrap();
        effects_tx
            .send(Effect::Audio(AudioEffect::Seek(SeekTarget::Relative(5))))
            .unwrap();
        effects_tx
            .send(Effect::Audio(AudioEffect::SetVolume(50)))
            .unwrap();
        effects_tx
            .send(Effect::Audio(AudioEffect::SetMute(true)))
            .unwrap();
        effects_tx.send(Effect::Audio(AudioEffect::Stop)).unwrap();
        drop(effects_tx);
        handle.await.unwrap();

        let commands = control.commands();
        assert!(matches!(commands[0], EngineCommand::Load { .. }));
        assert!(matches!(
            commands[1],
            EngineCommand::Seek(SeekTarget::Relative(5))
        ));
        assert!(matches!(commands[2], EngineCommand::SetVolume(50)));
        assert!(matches!(commands[3], EngineCommand::SetMute(true)));
        assert!(matches!(commands[4], EngineCommand::Stop));
        let _ = drain(&mut events_rx).await;
    }

    #[tokio::test]
    async fn placeholder_load_and_preload_are_dropped_before_the_engine() {
        let (engine, control) = MockEngine::new();
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let handle = spawn(Box::new(engine), effects_rx, events_tx);

        // A placeholder Load + Preload (dropped), then a real Load (forwarded).
        let placeholder = loxia_core::reducer::queue::placeholder_url(&ItemId::from("t1"));
        effects_tx
            .send(Effect::Audio(AudioEffect::Load {
                url: placeholder.clone(),
                headers: std::collections::BTreeMap::new(),
                start_at: Duration::ZERO,
                gain_db: None,
            }))
            .unwrap();
        effects_tx
            .send(Effect::Audio(AudioEffect::Preload {
                url: placeholder,
                headers: std::collections::BTreeMap::new(),
                gain_db: None,
            }))
            .unwrap();
        effects_tx
            .send(Effect::Audio(AudioEffect::Load {
                url: RedactedUrl::new("http://host/Audio/t1/stream?api_key=x"),
                headers: std::collections::BTreeMap::new(),
                start_at: Duration::ZERO,
                gain_db: None,
            }))
            .unwrap();
        drop(effects_tx);
        handle.await.unwrap();

        let commands = control.commands();
        assert_eq!(
            commands.len(),
            1,
            "only the real Load should reach the engine, got {commands:?}"
        );
        assert!(matches!(commands[0], EngineCommand::Load { .. }));
        let _ = drain(&mut events_rx).await;
    }

    #[test]
    fn placeholder_scheme_matches_core() {
        // If `placeholder_url`'s scheme ever changes, this drop guard must change with it.
        let url = loxia_core::reducer::queue::placeholder_url(&ItemId::from("abc"));
        assert!(url.as_str().starts_with(PLACEHOLDER_SCHEME));
    }

    #[tokio::test]
    async fn play_pause_toggles_using_tracked_status() {
        let (engine, control) = MockEngine::new();
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let handle = spawn(Box::new(engine), effects_rx, events_tx);

        // Not yet playing: the first PlayPause must resolve to `Play`.
        effects_tx
            .send(Effect::Audio(AudioEffect::PlayPause))
            .unwrap();
        // Give the worker a moment to process and observe its own `StatusChanged(Playing)`
        // (`MockEngine::send` for `Play` emits it synchronously, but the select loop still needs
        // a scheduling turn to read it back off `backend_events` before the next effect).
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        effects_tx
            .send(Effect::Audio(AudioEffect::PlayPause))
            .unwrap();
        drop(effects_tx);
        handle.await.unwrap();

        let commands = control.commands();
        assert!(matches!(commands[0], EngineCommand::Play));
        assert!(matches!(commands[1], EngineCommand::Pause));
        let _ = drain(&mut events_rx).await;
    }

    /// mpv reports `core-idle` — `PlayStatus::Buffering` — every time a network stream rebuffers
    /// mid-track. The toggle used to track "is the status `Playing`", so a rebuffer flipped it to
    /// "not playing" and the next press resolved to `Play`: pressing pause during a rebuffer did
    /// nothing at all, and since nothing paused, no `IsPaused: true` ever reached Emby either
    /// (`docs/12-decisions.md`).
    #[tokio::test]
    async fn pause_still_pauses_while_the_stream_is_rebuffering() {
        let (engine, control) = MockEngine::new();
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let handle = spawn(Box::new(engine), effects_rx, events_tx);

        effects_tx
            .send(Effect::Audio(AudioEffect::PlayPause))
            .unwrap();
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        // Mid-track rebuffer, then the press.
        control.emit_status(PlayStatus::Buffering);
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        effects_tx
            .send(Effect::Audio(AudioEffect::PlayPause))
            .unwrap();
        drop(effects_tx);
        handle.await.unwrap();

        let commands = control.commands();
        assert!(matches!(commands[0], EngineCommand::Play));
        assert!(
            matches!(commands[1], EngineCommand::Pause),
            "a press during a rebuffer must pause, got {:?}",
            commands[1]
        );
        let _ = drain(&mut events_rx).await;
    }

    #[tokio::test]
    async fn event_to_action_mapping() {
        for (engine_event, expected) in [
            (
                EngineEvent::StatusChanged(PlayStatus::Paused),
                Event::Audio(AudioEvent::StatusChanged(PlayStatus::Paused)),
            ),
            (
                EngineEvent::Position {
                    secs: 1.5,
                    duration: 200.0,
                },
                Event::Audio(AudioEvent::PositionChanged {
                    position: Duration::from_secs_f64(1.5),
                    duration: Duration::from_secs_f64(200.0),
                }),
            ),
            (
                EngineEvent::Format(AudioFormat {
                    codec: Codec::Flac,
                    sample_rate_hz: 44_100,
                    bit_depth: Some(16),
                    channels: 2,
                    bitrate_bps: None,
                }),
                Event::Audio(AudioEvent::FormatDetected(AudioFormat {
                    codec: Codec::Flac,
                    sample_rate_hz: 44_100,
                    bit_depth: Some(16),
                    channels: 2,
                    bitrate_bps: None,
                })),
            ),
            (
                EngineEvent::TrackEnded { natural: true },
                Event::Audio(AudioEvent::TrackEnded { natural: true }),
            ),
            (
                EngineEvent::Buffering(42),
                Event::Audio(AudioEvent::StatusChanged(PlayStatus::Buffering)),
            ),
            (
                EngineEvent::VolumeChanged {
                    volume: 80,
                    muted: false,
                },
                Event::Audio(AudioEvent::VolumeChanged {
                    volume: 80,
                    muted: false,
                }),
            ),
            (
                EngineEvent::Devices(vec![]),
                Event::Data(DataAction::DevicesLoaded { devices: vec![] }),
            ),
        ] {
            assert_eq!(to_action(engine_event), Some(expected));
        }
    }

    #[test]
    fn device_unavailable_keeps_the_id_instead_of_folding_into_engine_error() {
        let event = EngineEvent::Error(AudioError::DeviceUnavailable {
            id: "alsa/hw:9,0".to_string(),
        });
        assert_eq!(
            to_action(event),
            Some(Event::Audio(AudioEvent::DeviceUnavailable {
                id: "alsa/hw:9,0".to_string()
            }))
        );
    }

    #[test]
    fn other_engine_errors_still_fold_into_the_generic_variant() {
        let event = EngineEvent::Error(AudioError::Shutdown);
        assert!(matches!(
            to_action(event),
            Some(Event::Audio(AudioEvent::EngineError(_)))
        ));
    }

    /// A backend whose every `send` fails — the one behaviour `MockEngine` can't exercise, since
    /// it never fails.
    struct FailingBackend;
    impl AudioBackend for FailingBackend {
        fn send(&self, _cmd: EngineCommand) -> Result<(), AudioError> {
            Err(AudioError::Shutdown)
        }
        fn subscribe(&self) -> mpsc::UnboundedReceiver<EngineEvent> {
            mpsc::unbounded_channel().1
        }
    }

    #[tokio::test]
    async fn send_failure_becomes_engine_error_action() {
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let handle = spawn(Box::new(FailingBackend), effects_rx, events_tx);

        effects_tx
            .send(Effect::Audio(AudioEffect::PlayPause))
            .unwrap();
        drop(effects_tx);
        handle.await.unwrap();

        let events = drain(&mut events_rx).await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Audio(AudioEvent::EngineError(_)))),
            "a failed send must surface as EngineError, not be silently dropped: {events:?}"
        );
    }
}
