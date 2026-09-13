//! souvlaki media-key bridge (`10-11`): hardware media-key control and OS now-playing
//! integration — MPRIS on Linux, SMTC on Windows, the CoreAudio now-playing centre on macOS.
//!
//! Outbound (`Effect::Sys(SysEffect::UpdateMpris(_))`) is straightforward: `souvlaki`'s own
//! `set_metadata`/`set_playback` just post to an internal channel its background service thread
//! already owns (see `MediaControls::attach`'s own doc comment — the actual D-Bus connection is
//! established once, inside `attach`, not on every update), so no additional throttling is needed
//! here beyond what `reducer::player::mpris_meta`'s three call sites already apply at the source.
//!
//! Inbound is the harder direction: `souvlaki`'s control callback fires on its own background
//! thread, and must never reach `AppState` directly (this task's own spec) — it is translated to a
//! `PlayerAction` and forwarded over the `events` channel as `Event::Player(_)` (a variant that did
//! not exist before this task; no earlier worker ever needed to inject a `PlayerAction`).

use loxia_core::action::PlayerAction;
use loxia_core::effect::{Effect, MprisMeta, SysEffect};
use loxia_core::event::Event;
use loxia_core::state::player::SeekTarget;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

pub fn spawn(effects: UnboundedReceiver<Effect>, events: UnboundedSender<Event>) -> JoinHandle<()> {
    tokio::spawn(run(effects, events))
}

async fn run(mut effects: UnboundedReceiver<Effect>, events: UnboundedSender<Event>) {
    // `init` performs a real (possibly blocking) connection attempt — `Connection::new_session`,
    // in `souvlaki`'s own dbus backend, runs synchronously inside `attach`, not lazily — so it
    // runs on the blocking pool rather than stalling this task, matching `workers::notify`'s own
    // discipline for OS calls that might block.
    let controls = tokio::task::spawn_blocking(move || init(events))
        .await
        .ok()
        .flatten();

    let Some(mut controls) = controls else {
        // `10-11`: "failure is non-fatal and silent... the app runs normally" — this worker simply
        // has nothing left to do, but must keep draining its channel for the rest of the process
        // lifetime rather than dropping it: an early return here would close `effects` from this
        // end, and `dispatch::send` logs an `error` for every subsequent effect sent to a worker
        // whose receiver has gone away, which is not "silent."
        while effects.recv().await.is_some() {}
        return;
    };

    while let Some(effect) = effects.recv().await {
        if let Effect::Sys(SysEffect::UpdateMpris(meta)) = effect {
            apply_metadata(&mut controls, &meta);
        }
    }
}

/// Real, possibly-blocking setup: `MediaControls::new` (never fails on the Linux backend — it
/// only stores names) followed by `attach`, which is where a headless system with no session bus
/// actually fails. Either failure is logged once at `info` and returns `None` — never `error`,
/// never a toast (there is no path back to one, `docs/12-decisions.md`, `10-10`'s own identical
/// reasoning for `workers::notify`) — "a terminal music player must work over SSH" (this task's
/// own spec).
fn init(events: UnboundedSender<Event>) -> Option<MediaControls> {
    let config = PlatformConfig {
        // Matches the binary name, so the MPRIS entry a desktop shows lines up with the command
        // that produced it. Hyphens are valid in a D-Bus *bus name* element (unlike an interface
        // name), which is what souvlaki builds from this.
        display_name: "loxia-player",
        dbus_name: "loxia-player",
        hwnd: None,
    };
    let mut controls = match MediaControls::new(config) {
        Ok(controls) => controls,
        Err(error) => {
            tracing::info!(?error, "media-key integration unavailable");
            return None;
        }
    };
    if let Err(error) = controls.attach(event_forwarder(events)) {
        tracing::info!(?error, "media-key integration unavailable");
        return None;
    }
    Some(controls)
}

/// Builds the closure `attach` invokes on `souvlaki`'s own callback thread — split out from
/// `init` so it can be tested with no live `MediaControls` at all: swap in a plain
/// `mpsc::unbounded_channel` and call the returned closure directly.
fn event_forwarder(events: UnboundedSender<Event>) -> impl Fn(MediaControlEvent) + Send + 'static {
    move |control_event| {
        if let Some(action) = map_control_event(control_event) {
            // A closed channel here means the runtime loop is gone (shutting down) — nothing
            // meaningful to do about it from `souvlaki`'s own background thread.
            let _ = events.send(Event::Player(action));
        }
    }
}

/// This task's own control table. `Seek`/`SeekBy` (undetermined/relative-amount seeking, distinct
/// from `SetPosition`'s absolute one), `OpenUri`, `Raise`, and `Quit` are not in that table and are
/// deliberately ignored, not guessed at.
fn map_control_event(event: MediaControlEvent) -> Option<PlayerAction> {
    match event {
        MediaControlEvent::Play | MediaControlEvent::Pause | MediaControlEvent::Toggle => {
            Some(PlayerAction::PlayPause)
        }
        MediaControlEvent::Next => Some(PlayerAction::Next),
        MediaControlEvent::Previous => Some(PlayerAction::Prev),
        MediaControlEvent::Stop => Some(PlayerAction::Stop),
        MediaControlEvent::SetPosition(MediaPosition(position)) => {
            Some(PlayerAction::Seek(SeekTarget::Absolute(position)))
        }
        MediaControlEvent::SetVolume(volume) => {
            Some(PlayerAction::SetVolume(volume_percent(volume)))
        }
        MediaControlEvent::Seek(_)
        | MediaControlEvent::SeekBy(_, _)
        | MediaControlEvent::OpenUri(_)
        | MediaControlEvent::Raise
        | MediaControlEvent::Quit => None,
    }
}

/// `souvlaki`'s own `SetVolume` doc comment: "intended to be from 0.0 to 1.0. But other values are
/// also accepted. It is up to the user to set constraints on this value" — clamped here, since
/// `PlayerAction::SetVolume` takes a `u8` percentage with no headroom for out-of-range input.
fn volume_percent(volume: f64) -> u8 {
    (volume.clamp(0.0, 1.0) * 100.0).round() as u8
}

fn apply_metadata(controls: &mut MediaControls, meta: &MprisMeta) {
    let progress = Some(MediaPosition(meta.position));
    let playback = if meta.playing {
        MediaPlayback::Playing { progress }
    } else {
        MediaPlayback::Paused { progress }
    };
    // Both calls are non-blocking posts to `souvlaki`'s own background thread (this module's own
    // doc comment) — failure here means that thread has already died (e.g. `detach`d, or the
    // D-Bus connection dropped), which is exactly as non-fatal and silent as an `init` failure.
    let _ = controls.set_metadata(MediaMetadata {
        title: Some(&meta.title),
        album: Some(&meta.album),
        artist: Some(&meta.artist),
        cover_url: meta.art_url.as_deref(),
        duration: Some(meta.duration),
    });
    let _ = controls.set_playback(playback);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc;

    /// `10-11`: table test over all six controls this task's own spec names.
    #[test]
    fn control_event_mapping() {
        let cases = [
            (MediaControlEvent::Play, Some(PlayerAction::PlayPause)),
            (MediaControlEvent::Pause, Some(PlayerAction::PlayPause)),
            (MediaControlEvent::Toggle, Some(PlayerAction::PlayPause)),
            (MediaControlEvent::Next, Some(PlayerAction::Next)),
            (MediaControlEvent::Previous, Some(PlayerAction::Prev)),
            (MediaControlEvent::Stop, Some(PlayerAction::Stop)),
            (
                MediaControlEvent::SetPosition(MediaPosition(Duration::from_secs(42))),
                Some(PlayerAction::Seek(SeekTarget::Absolute(
                    Duration::from_secs(42),
                ))),
            ),
            (
                MediaControlEvent::SetVolume(0.5),
                Some(PlayerAction::SetVolume(50)),
            ),
        ];
        for (event, expected) in cases {
            assert_eq!(map_control_event(event.clone()), expected, "{event:?}");
        }
    }

    #[test]
    fn unmapped_controls_are_ignored() {
        for event in [
            MediaControlEvent::OpenUri("https://example.com".to_string()),
            MediaControlEvent::Raise,
            MediaControlEvent::Quit,
        ] {
            assert_eq!(map_control_event(event), None);
        }
    }

    #[test]
    fn volume_is_clamped_and_converted_to_a_percentage() {
        assert_eq!(volume_percent(0.0), 0);
        assert_eq!(volume_percent(1.0), 100);
        assert_eq!(volume_percent(1.5), 100);
        assert_eq!(volume_percent(-0.5), 0);
    }

    /// `10-11`: proves the callback goes through the `events` channel, never touching any shared
    /// state directly — no `MediaControls`/D-Bus involved at all, so this runs identically
    /// whether or not this environment has a session bus.
    #[test]
    fn controls_forwarded_via_event_channel_not_direct_mutation() {
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let forward = event_forwarder(events_tx);

        forward(MediaControlEvent::Next);

        assert_eq!(events_rx.try_recv(), Ok(Event::Player(PlayerAction::Next)));
    }

    #[test]
    fn ignored_controls_forward_nothing() {
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let forward = event_forwarder(events_tx);

        forward(MediaControlEvent::Raise);

        assert!(events_rx.try_recv().is_err());
    }

    /// `10-11`: this sandbox genuinely has no D-Bus session bus (`docs/12-decisions.md`), so this
    /// is not a mock — `spawn`'s real `init` path is exercised end-to-end and is expected to fail.
    /// The worker must still run normally: process (drain) its channel, and exit cleanly once it
    /// closes, with no panic.
    #[tokio::test]
    async fn init_failure_is_non_fatal() {
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let handle = spawn(effects_rx, events_tx);

        effects_tx
            .send(Effect::Sys(SysEffect::UpdateMpris(MprisMeta {
                title: "Motion".to_string(),
                artist: "Boy Harsher".to_string(),
                album: "Care".to_string(),
                art_url: None,
                position: Duration::ZERO,
                duration: Duration::from_secs(180),
                playing: true,
            })))
            .unwrap();
        drop(effects_tx);

        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(result.is_ok(), "worker must exit promptly, not hang");
        assert!(result.unwrap().is_ok(), "worker task must not panic");
    }
}
