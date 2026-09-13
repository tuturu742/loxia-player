//! Desktop notification bridge (`10-10`): a native OS toast on a genuine track change.
//!
//! The reducer (`reducer::queue::notify_track_change`) already decides *whether* a notification
//! should happen at all — `ui.desktop_notifications`, and whether the terminal has focus, are both
//! checked there, once, before `Effect::Sys(SysEffect::Notify(_))` is ever emitted. Everything left
//! for this worker is inherently time- and I/O-based, and so cannot live in the pure reducer: rate
//! limiting a stream of these effects to at most one real OS notification per three seconds,
//! coalescing a burst down to the track playing when that window closes, and calling into
//! `notify-rust` (which blocks on some platforms) without stalling this worker's own loop.
//!
//! `run` is generic over the actual "send" step purely so tests can substitute a recording stub for
//! the real `notify-rust` call — `spawn`'s own public signature (fixed by this task, `workers::mod`'s
//! own doc comment) takes only `effects`, no injected dependency, so the seam is internal.

use std::future::Future;

use loxia_core::effect::{Effect, SysEffect, TrackChange};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

/// "At most one notification per 3 seconds" (this task's own spec).
const RATE_LIMIT: Duration = Duration::from_secs(3);

pub fn spawn(effects: UnboundedReceiver<Effect>) -> JoinHandle<()> {
    tokio::spawn(run(effects, send_os_notification))
}

/// The rate-limiting/coalescing core, generic over `send` so it can be driven by a test spy
/// without any real notification daemon. Uses `tokio::time::Instant`/`sleep_until` throughout
/// (never `std::time::Instant`) so `#[tokio::test(start_paused = true)]` can drive the 3-second
/// window with `tokio::time::advance` instead of a real wall-clock wait.
///
/// Invariant: `pending` is only ever `Some` after at least one real send has happened, so
/// `last_sent` is always `Some` whenever `pending` is (checked once, on entry to the `else` arm
/// below, and never invalidated afterwards — nothing here ever resets `last_sent` to `None`).
async fn run<F, Fut>(mut effects: UnboundedReceiver<Effect>, send: F)
where
    F: Fn(TrackChange) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let mut last_sent: Option<Instant> = None;
    let mut pending: Option<TrackChange> = None;

    loop {
        tokio::select! {
            // Preferred over the timer whenever both are ready in the same poll — a fresh
            // arrival that itself clears the rate limit should win over firing a stale timer.
            biased;
            maybe = effects.recv() => {
                let Some(effect) = maybe else { break }; // channel closed: shutdown
                let Effect::Sys(SysEffect::Notify(track_change)) = effect else { continue };
                let now = Instant::now();
                let ready = last_sent.is_none_or(|t| now.duration_since(t) >= RATE_LIMIT);
                if ready {
                    last_sent = Some(now);
                    pending = None;
                    send(track_change).await;
                } else {
                    pending = Some(track_change);
                }
            }
            _ = tokio::time::sleep_until(last_sent.unwrap_or_else(Instant::now) + RATE_LIMIT),
                if pending.is_some() =>
            {
                if let Some(track_change) = pending.take() {
                    last_sent = Some(Instant::now());
                    send(track_change).await;
                }
            }
        }
    }
}

/// Builds the outgoing `notify-rust` notification from a `TrackChange` — split out from
/// `send_os_notification` so it can be tested with no notification daemon at all: `Notification`'s
/// own fields (`icon`, `summary`, `body`, ...) are public and readable without ever calling
/// `.show()`.
fn build_notification(track_change: &TrackChange) -> notify_rust::Notification {
    let mut notification = notify_rust::Notification::new();
    notification.summary(&track_change.title);
    notification.body(&format!("{}\n{}", track_change.artist, track_change.album));
    if let Some(icon) = &track_change.art_path {
        notification.icon(icon);
    }
    notification
}

/// The real "send" step `spawn` wires up. Returns once the send has been *kicked off*, not once
/// it completes — `notify-rust` blocks on some platforms (this task's own spec), so the actual
/// `.show()` call runs on the blocking pool inside a detached task, never awaited by `run`'s own
/// loop. A missing notification daemon, a D-Bus error, or an unsupported platform is exactly the
/// `Err` arm below: logged at `debug` and otherwise ignored — "a toast complaining that a toast
/// could not be shown is absurd" (this task's own spec) — and since this worker's `spawn` signature
/// carries no `events: UnboundedSender<Event>` (unlike every other worker in `workers::mod`), a
/// failure here has structurally no way to reach the reducer as a toast even if it wanted to.
async fn send_os_notification(track_change: TrackChange) {
    tokio::spawn(async move {
        let notification = build_notification(&track_change);
        match tokio::task::spawn_blocking(move || notification.show()).await {
            Ok(Ok(_handle)) => {}
            Ok(Err(error)) => {
                tracing::debug!(%error, "desktop notification failed");
            }
            Err(error) => {
                tracing::debug!(%error, "desktop notification task panicked");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    fn track_change(title: &str) -> TrackChange {
        TrackChange {
            title: title.to_string(),
            artist: "Boy Harsher".to_string(),
            album: "Care".to_string(),
            art_path: None,
        }
    }

    fn spawn_recording_run(
        effects_rx: mpsc::UnboundedReceiver<Effect>,
    ) -> (JoinHandle<()>, Arc<Mutex<Vec<TrackChange>>>) {
        let sent: Arc<Mutex<Vec<TrackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let sent_for_closure = sent.clone();
        let handle = tokio::spawn(run(effects_rx, move |tc| {
            let sent = sent_for_closure.clone();
            async move {
                sent.lock().unwrap().push(tc);
            }
        }));
        (handle, sent)
    }

    #[test]
    fn missing_artwork_sends_without_icon() {
        let tc = track_change("Motion");
        let notification = build_notification(&tc);
        assert_eq!(notification.icon, "");
        assert_eq!(notification.summary, "Motion");
        assert_eq!(notification.body, "Boy Harsher\nCare");
    }

    #[test]
    fn artwork_present_is_used_as_the_icon() {
        let mut tc = track_change("Motion");
        tc.art_path = Some("/tmp/cover.jpg".to_string());
        let notification = build_notification(&tc);
        assert_eq!(notification.icon, "/tmp/cover.jpg");
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limited_to_one_per_three_seconds() {
        let (tx, rx) = mpsc::unbounded_channel();
        let (_handle, sent) = spawn_recording_run(rx);

        tx.send(Effect::Sys(SysEffect::Notify(track_change("First"))))
            .unwrap();
        tokio::task::yield_now().await;
        assert_eq!(sent.lock().unwrap().len(), 1, "the first send is immediate");

        // Within the same 3-second window: coalesced, not sent yet.
        tx.send(Effect::Sys(SysEffect::Notify(track_change("Second"))))
            .unwrap();
        tokio::task::yield_now().await;
        assert_eq!(sent.lock().unwrap().len(), 1, "still rate-limited");

        tokio::time::advance(RATE_LIMIT + Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            sent.lock().unwrap().len(),
            2,
            "the pending track sends once the window closes"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn rapid_skips_coalesce_to_final_track() {
        let (tx, rx) = mpsc::unbounded_channel();
        let (_handle, sent) = spawn_recording_run(rx);

        tx.send(Effect::Sys(SysEffect::Notify(track_change("One"))))
            .unwrap();
        tokio::task::yield_now().await;

        // Three rapid skips inside the rate-limit window — only the last must ever be sent.
        for title in ["Two", "Three", "Four"] {
            tx.send(Effect::Sys(SysEffect::Notify(track_change(title))))
                .unwrap();
            tokio::task::yield_now().await;
        }

        tokio::time::advance(RATE_LIMIT + Duration::from_millis(1)).await;
        tokio::task::yield_now().await;

        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 2, "the leading send plus one coalesced send");
        assert_eq!(sent[0].title, "One");
        assert_eq!(
            sent[1].title, "Four",
            "intermediate skips are dropped; only the final track notifies"
        );
    }

    #[tokio::test]
    async fn daemon_failure_is_silent() {
        // No notification daemon exists in this environment, so the real `notify-rust` send
        // genuinely fails here — this is not a mock. `spawn`'s own loop must still process the
        // effect and shut down cleanly (no panic, no propagated error) once the channel closes.
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = spawn(rx);

        tx.send(Effect::Sys(SysEffect::Notify(track_change("Motion"))))
            .unwrap();
        drop(tx);

        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(result.is_ok(), "worker must exit promptly, not hang");
        assert!(result.unwrap().is_ok(), "worker task must not panic");
    }

    #[tokio::test]
    async fn non_notify_effects_are_ignored() {
        let (tx, rx) = mpsc::unbounded_channel();
        let (_handle, sent) = spawn_recording_run(rx);

        tx.send(Effect::Audio(loxia_core::effect::AudioEffect::PlayPause))
            .unwrap();
        tokio::task::yield_now().await;
        assert!(sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn channel_close_ends_the_worker() {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = tokio::spawn(run(rx, |_: TrackChange| async {}));
        drop(tx);
        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }
}
