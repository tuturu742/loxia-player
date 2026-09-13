//! Next-track preloading via the mpv playlist (`06-06`).
//!
//! There is no production logic in this module: gapless playback is entirely a consequence of
//! mpv's own `gapless-audio=yes`/`prefetch-playlist=yes` options (set at init, `05-03`) plus
//! `AudioCommand::Preload` appending the next track to mpv's internal playlist instead of
//! replacing the current file (`mpv::handle::apply_command`, also `05-03`) — `reducer::queue`
//! (`loxia-core`, this same task) is what decides *when* to send that `Preload`. This module
//! exists to hold the one thing that genuinely needs real mpv to verify: that two tracks appended
//! this way actually play back to back with no audible gap.

#[cfg(all(test, feature = "mpv-tests"))]
mod real_mpv {
    use std::time::{Duration, Instant};

    use loxia_core::config::AudioConfig;
    use loxia_core::effect::RedactedUrl;

    use crate::backend::{AudioBackend, AudioCommand, AudioEvent};
    use crate::mpv::handle::MpvEngine;

    /// `ao=null`: deterministic, real-time-paced, hardware-independent — the same reasoning
    /// `mpv::handle`'s own `real_mpv` tests use.
    fn engine() -> MpvEngine {
        MpvEngine::with_audio_output(&AudioConfig::default(), Some("null"))
            .expect("libmpv must be present for mpv-tests")
    }

    /// A minimal, valid mono 8-bit PCM WAV file of `secs` seconds of silence — substituted for
    /// "a generated FLAC" (`docs/12-decisions.md`, same substitution `mpv::handle`'s own tests
    /// make): mpv's built-in WAV demuxer needs no external codec, and the gapless mechanism under
    /// test (`gapless-audio`/`prefetch-playlist`/playlist `append`) doesn't care about the codec.
    fn silent_wav_bytes(secs: u32) -> Vec<u8> {
        let sample_rate: u32 = 8000;
        let data_len = sample_rate * secs;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_len).to_le_bytes());
        buf.extend_from_slice(b"WAVEfmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&8u16.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_len.to_le_bytes());
        buf.extend(std::iter::repeat_n(128u8, data_len as usize));
        buf
    }

    fn write_silent_wav(path: &std::path::Path, secs: u32) {
        std::fs::write(path, silent_wav_bytes(secs)).unwrap();
    }

    /// Polls `rx` until a natural `TrackEnded` has been seen (the first track finishing) *and* a
    /// subsequent `Position` near zero has arrived (the second track already reporting), or the
    /// deadline passes — pairing each event with the wall-clock instant it was received, since
    /// that's what the gap is measured in.
    fn collect_transition(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<AudioEvent>,
        timeout: Duration,
    ) -> Vec<(Instant, AudioEvent)> {
        let mut events = Vec::new();
        let deadline = Instant::now() + timeout;
        let mut seen_ended = false;
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(event) => {
                    let now = Instant::now();
                    if matches!(event, AudioEvent::TrackEnded { natural: true }) {
                        seen_ended = true;
                    }
                    let resumed = seen_ended
                        && matches!(&event, AudioEvent::Position { secs, .. } if *secs < 0.5);
                    events.push((now, event));
                    if resumed {
                        break;
                    }
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        events
    }

    #[test]
    fn gapless_gap_under_20ms() {
        let dir = std::env::temp_dir();
        let path_a = dir.join(format!("loxia-gapless-a-{}.wav", std::process::id()));
        let path_b = dir.join(format!("loxia-gapless-b-{}.wav", std::process::id()));
        write_silent_wav(&path_a, 1);
        write_silent_wav(&path_b, 1);

        let e = engine();
        let mut rx = e.subscribe();
        e.send(AudioCommand::Load {
            url: RedactedUrl::new(path_a.to_str().unwrap()),
            headers: Vec::new(),
            start_at: Duration::ZERO,
            gain_db: None,
        })
        .unwrap();
        // Sent immediately, not after waiting for `A` to nearly finish — real usage preloads as
        // soon as the reducer knows what's next (`docs/12-decisions.md`), and mpv's own
        // `prefetch-playlist` is what actually times the buffering.
        e.send(AudioCommand::Preload {
            url: RedactedUrl::new(path_b.to_str().unwrap()),
            headers: Vec::new(),
            gain_db: None,
        })
        .unwrap();

        let events = collect_transition(&mut rx, Duration::from_secs(5));
        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);

        let ended_at = events
            .iter()
            .find(|(_, e)| matches!(e, AudioEvent::TrackEnded { natural: true }))
            .map(|(t, _)| *t)
            .unwrap_or_else(|| panic!("expected a natural TrackEnded for track A, got {events:?}"));
        let (resumed_at, resumed_secs) = events
            .iter()
            .find_map(|(t, e)| match e {
                AudioEvent::Position { secs, .. } if *t >= ended_at && *secs < 0.5 => {
                    Some((*t, *secs))
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "expected track B to resume reporting position after A ended, got {events:?}"
                )
            });

        // `Position` is throttled to 4 Hz (`docs/05-audio-engine.md` §2) — the wall-clock delay
        // until this event *arrives* is dominated by that throttle, not by any real playback gap.
        // Subtracting the position `B` had already reached by the time it was reported backs out
        // the throttle delay, leaving the actual gap between `A` ending and `B` starting.
        let wall_gap = resumed_at.duration_since(ended_at);
        let implied_gap = wall_gap.saturating_sub(Duration::from_secs_f64(resumed_secs));
        assert!(
            implied_gap < Duration::from_millis(20),
            "implied gap was {implied_gap:?} (wall delay {wall_gap:?} minus reported position {resumed_secs}s)"
        );
    }
}
