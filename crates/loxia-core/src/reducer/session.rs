//! Applies a restored `SessionSnapshot` onto `AppState` (`08-08`,
//! `docs/06-cache-and-offline.md` §8, steps 2-6) — `loxia_cache::session::load` owns the actual
//! file I/O and the `schema_version` check (self-contained there, since it needs no `Config`);
//! this is the half that needs `state.config.active_server`/`ui.restore_autoplay`, which only the
//! reducer has, wired from `Action::System(SystemEvent::SessionRestored)` (`reducer/mod.rs`).

use std::time::Duration;

use crate::effect::Effect;
use crate::model::ServerId;
use crate::state::player::PlayStatus;
use crate::state::toast::ToastLevel;
use crate::state::{AppState, SessionSnapshot};

/// `11-06`: `mm:ss`/`h:mm:ss`, matching `loxia_tui::widgets::player_bar::format_time` exactly —
/// duplicated rather than shared, since `loxia-core` cannot depend on `loxia-tui` (the dependency
/// only ever runs the other way), the same boundary `reducer::player::is_complete` already
/// duplicates a `loxia-emby` function across for the identical reason.
fn format_position(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The restore toast's own wording (`docs/06-cache-and-offline.md` §8: "a user who does not know
/// the feature exists otherwise finds a queue they did not create"). Adapts the task's own literal
/// example ("restored 14 tracks — paused at 01:24") to the `restore_autoplay` case too ("resuming
/// at", since it is no longer actually paused) and to an empty queue (no position clause at all —
/// there is nothing to be paused or resuming *at*).
fn restore_toast_message(
    count: usize,
    position: Duration,
    has_current: bool,
    autoplay: bool,
) -> String {
    let noun = if count == 1 { "track" } else { "tracks" };
    if !has_current {
        return format!("restored {count} {noun}");
    }
    let verb = if autoplay { "resuming" } else { "paused" };
    format!(
        "restored {count} {noun} — {verb} at {}",
        format_position(position)
    )
}

/// Discards (does nothing but toast) if `snapshot.server_id` doesn't match the currently
/// configured server — restoring one server's queue against another's library would produce a
/// queue of unplayable entries. Otherwise rebuilds `QueueState` **entirely from the snapshot
/// itself** and never touches the network: every entry keeps whatever `Availability` it was saved
/// with rather than being eagerly re-checked against the server — a track the server no longer
/// resolves is only marked `Unavailable` lazily, whenever something *else* later tries to use it
/// (`docs/12-decisions.md`).
pub fn restore(state: &mut AppState, snapshot: SessionSnapshot) -> Vec<Effect> {
    // The *server's* identity, not the profile's — otherwise switching between two addresses for
    // one server (a LAN one and an external one) throws the restored queue away as if it belonged
    // to somebody else's library (`Config::storage_server_id`).
    let active = ServerId::from(state.config.storage_server_id());
    if snapshot.server_id != active {
        state.toast(
            "saved session was for a different server; not restored",
            ToastLevel::Info,
        );
        state.touch();
        return Vec::new();
    }

    let track_count = snapshot.queue.entries.len();
    state.queue = snapshot.queue;
    state.nav.active_tab = snapshot.active_tab;
    state.zen_mode = snapshot.zen_mode;
    state.player.volume = snapshot.volume;
    state.player.quality_profile = snapshot.quality_profile;
    state.player.eq = snapshot.eq;
    state.player.position = Duration::from_secs_f64(snapshot.position_secs.max(0.0));

    let current = state.queue.current();
    state.player.current = current.map(|e| e.entry_id);
    state.player.duration = current.map(|e| e.track.duration).unwrap_or_default();

    let autoplay = state.config.ui.restore_autoplay;
    // The engine starts every run at its own default (100) and knows nothing of the snapshot, so
    // restoring the *mirror* alone left the bar reading the saved volume while playback was
    // actually full-scale — the number on screen was right and the sound was not
    // (`docs/12-decisions.md`). Same shape as `hydrate_from_config`'s ReplayGain/EQ effects, for
    // the one setting that lives in the session rather than the config.
    let mut effects = vec![Effect::Audio(crate::effect::AudioEffect::SetVolume(
        state.player.volume,
    ))];
    // "Restore paused" is the always-safe default — auto-play on launch seizes the audio device
    // and startles the user; `ui.restore_autoplay` is the explicit opt-in
    // (`docs/06-cache-and-offline.md` §8, step 5).
    state.player.status = match (state.player.current, autoplay) {
        (Some(_), true) => {
            // `11-06`: unlike the paused branch below, autoplay can't wait for a keypress to
            // actually load the track — `resume_after_restore` issues the real `Load` (at the
            // saved position) right now; `restored_unloaded` stays `false`, since this *is* the
            // load that would otherwise have been deferred to it.
            state.player.restored_unloaded = false;
            effects.extend(crate::reducer::queue::resume_after_restore(state));
            PlayStatus::Playing
        }
        (Some(_), false) => {
            state.player.restored_unloaded = true;
            PlayStatus::Paused
        }
        (None, _) => {
            state.player.restored_unloaded = false;
            PlayStatus::Stopped
        }
    };

    let message = restore_toast_message(
        track_count,
        state.player.position,
        state.player.current.is_some(),
        autoplay,
    );
    state.toast(message, ToastLevel::Info);

    state.touch();
    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::nav::Tab;
    use crate::state::player::EqState;
    use crate::state::queue::{Availability, QueueEntry, QueueSource, QueueState};
    use crate::test_support::fixtures;

    fn queue_with_one_entry() -> QueueState {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        let mut q = QueueState::default();
        q.entries.push(QueueEntry {
            entry_id: crate::model::QueueEntryId(0),
            track: t,
            source: QueueSource::Manual,
            availability: Availability::Downloaded,
        });
        q.play_order.push(0);
        q
    }

    fn snapshot(server: &str, queue: QueueState) -> SessionSnapshot {
        SessionSnapshot {
            schema_version: 1,
            server_id: ServerId::from(server),
            queue,
            position_secs: 84.0,
            active_tab: Tab::Playlists,
            zen_mode: true,
            volume: 55,
            quality_profile: crate::config::QualityProfile::TranscodeHigh,
            eq: EqState::default(),
            saved_at: fixtures::fixed_epoch(),
        }
    }

    fn state_for_server(server: &str) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.config.active_server = server.to_string();
        state
    }

    /// Two profiles, one server: a session saved under one address must restore under the other.
    /// The comparison used to be against the *profile* id, so switching between a LAN address and
    /// an external one silently threw the queue away (`docs/12-decisions.md`).
    #[test]
    fn a_session_restores_across_two_profiles_for_one_server() {
        let mut state = fixtures::fixture_empty();
        state.config.servers = vec![
            crate::config::ServerConfig {
                id: "lan".to_string(),
                server_id: "emby-guid".to_string(),
                ..Default::default()
            },
            crate::config::ServerConfig {
                id: "external".to_string(),
                server_id: "emby-guid".to_string(),
                ..Default::default()
            },
        ];
        state.config.active_server = "external".to_string();

        // Saved while the *other* profile was active — same server, so same namespace.
        let effects = restore(&mut state, snapshot("emby-guid", queue_with_one_entry()));

        assert!(!effects.is_empty(), "the queue must come back");
        assert_eq!(state.queue.entries.len(), 1);
    }

    /// A genuinely different server still gets its snapshot discarded — restoring one library's
    /// queue against another's would fill it with unplayable entries.
    #[test]
    fn a_different_server_is_still_refused() {
        let mut state = fixtures::fixture_empty();
        state.config.servers = vec![crate::config::ServerConfig {
            id: "home".to_string(),
            server_id: "emby-guid".to_string(),
            ..Default::default()
        }];
        state.config.active_server = "home".to_string();

        let effects = restore(
            &mut state,
            snapshot("someone-elses-guid", queue_with_one_entry()),
        );
        assert!(effects.is_empty());
        assert!(state.queue.entries.is_empty());
    }

    #[test]
    fn server_mismatch_discards() {
        let mut state = state_for_server("srv1");
        let before = state.queue.clone();

        let effects = restore(&mut state, snapshot("srv2", queue_with_one_entry()));

        assert!(effects.is_empty());
        assert_eq!(
            state.queue, before,
            "the queue is untouched on a server mismatch"
        );
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("different server"))
        );
    }

    #[test]
    fn restore_does_not_refetch_tracks() {
        // No `Effect::Net(_)` of any kind is ever returned — `QueueState` comes entirely from the
        // snapshot, never a fresh fetch.
        let mut state = state_for_server("srv1");
        let effects = restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert!(
            !effects.iter().any(|e| matches!(e, Effect::Net(_))),
            "restore must never touch the network"
        );
        assert_eq!(state.queue.entries.len(), 1);
    }

    #[test]
    fn session_restore_does_not_autoplay() {
        let mut state = state_for_server("srv1");
        state.config.ui.restore_autoplay = false;
        restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert_eq!(state.player.status, PlayStatus::Paused);
    }

    #[test]
    fn restore_autoplay_opt_in_works() {
        let mut state = state_for_server("srv1");
        state.config.ui.restore_autoplay = true;
        restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert_eq!(state.player.status, PlayStatus::Playing);
    }

    #[test]
    fn restore_applies_all_snapshot_fields() {
        let mut state = state_for_server("srv1");
        let effects = restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert_eq!(state.queue.entries.len(), 1);
        assert_eq!(state.nav.active_tab, Tab::Playlists);
        assert!(state.zen_mode);
        assert_eq!(state.player.volume, 55);
        // ...and the engine is actually told, or the bar reads 55 while playback runs at 100.
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(crate::effect::AudioEffect::SetVolume(55)))),
            "restoring the volume must reach the engine: {effects:?}"
        );
        assert_eq!(
            state.player.quality_profile,
            crate::config::QualityProfile::TranscodeHigh
        );
        assert_eq!(state.player.eq, EqState::default());
        assert_eq!(state.player.position, Duration::from_secs_f64(84.0));
    }

    #[test]
    fn restore_leaves_playback_paused() {
        let mut state = state_for_server("srv1");
        state.config.ui.restore_autoplay = false;
        restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert_eq!(state.player.status, PlayStatus::Paused);
        assert!(
            state.player.restored_unloaded,
            "the engine was never told to load anything; the next PlayPause must know to"
        );
    }

    #[test]
    fn restore_autoplay_starts_playback() {
        let mut state = state_for_server("srv1");
        state.config.ui.restore_autoplay = true;
        let effects = restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert_eq!(state.player.status, PlayStatus::Playing);
        assert!(!state.player.restored_unloaded);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(crate::effect::AudioEffect::Load { .. }))),
            "autoplay restore must actually tell the engine to load the track, not just flip status"
        );
    }

    #[test]
    fn restore_toast_names_count_and_position() {
        let mut state = state_for_server("srv1");
        restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message == "restored 1 track — paused at 1:24"),
            "unexpected toasts: {:?}",
            state.toasts
        );
    }

    /// `10-10`: `restore` sets `state.player`/`state.queue` fields directly and never calls
    /// `reducer::queue::load_current` (the one place `notify_track_change` is invoked) — a
    /// restored session is not a track the user just chose, and must produce no desktop
    /// notification (`docs/12-decisions.md`).
    #[test]
    fn does_not_notify_on_session_restore() {
        let mut state = state_for_server("srv1");
        let effects = restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(crate::effect::SysEffect::Notify(_))))
        );
    }

    #[test]
    fn unavailable_marked_lazily_not_eagerly() {
        // The one entry was saved as `Downloaded`; restore must not "helpfully" re-check it
        // against a server that might no longer have it — it stays exactly `Downloaded` until
        // something else (a later, real access) discovers otherwise.
        let mut state = state_for_server("srv1");
        restore(&mut state, snapshot("srv1", queue_with_one_entry()));
        assert_eq!(
            state.queue.entries[0].availability,
            Availability::Downloaded
        );
    }
}
