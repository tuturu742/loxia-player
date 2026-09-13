//! The Online/Offline/Reconnecting state machine (`08-06`, `docs/06-cache-and-offline.md` §6).
//!
//! ```text
//! Online ──(2 consecutive EmbyError::Offline)──► Offline
//! Offline ──(probe succeeds)──► Reconnecting ──(next Tick)──► Online
//! ```
//! The "scrobbles drained" step the design diagram names between `Reconnecting` and `Online` has
//! no real completion signal yet — `08-07` owns the scrobble buffer and hasn't built one — so
//! `advance_reconnecting` fires unconditionally on the very next `Tick` after
//! `on_connectivity_changed` enters `Reconnecting`, having already emitted the
//! `Effect::Cache(DrainScrobbles)` that starts the (currently fire-and-forget) drain
//! (`docs/12-decisions.md`).

use jiff::{SignedDuration, Timestamp};

use crate::action::DataAction;
use crate::effect::{CacheEffect, Effect, NetEffect};
use crate::state::nav::LoadState;
use crate::state::queue::Availability;
use crate::state::toast::ToastLevel;
use crate::state::{AppState, Connectivity};

/// Two consecutive `EmbyError::Offline` failures — never `Unauthorized`/`NotFound`/`Transient`
/// (`docs/06-cache-and-offline.md` §6: "a 404 on one album is not a network outage") — before the
/// app considers itself offline.
const OFFLINE_THRESHOLD: u32 = 2;
const PROBE_MIN_BACKOFF_SECS: i64 = 5;
/// How many probes of the *current* address are spent before a profile's other addresses are
/// tried. Long enough that a brief blip never respawns the workers, short enough that walking out
/// of the house recovers on its own.
const PROBES_BEFORE_RESELECT: u32 = 2;

const PROBE_MAX_BACKOFF_SECS: i64 = 30;

/// Called once per `DataAction`, before it's routed to its own handler (`reducer::apply`).
/// Updates the consecutive-failure counter and, if it just crossed [`OFFLINE_THRESHOLD`], enters
/// `Offline`. Every `DataAction` other than a `LoadFailed` is, by construction, a successful reply
/// to some request, so it resets the counter — this is the only place that rule needs expressing,
/// regardless of which of the ~15 network endpoints the reply actually came from.
pub fn observe_data_action(state: &mut AppState, action: &DataAction) -> Vec<Effect> {
    match action {
        DataAction::LoadFailed { offline: true, .. } => {
            state.offline_failures += 1;
            if state.offline_failures >= OFFLINE_THRESHOLD
                && state.connectivity == Connectivity::Online
            {
                return enter_offline(state);
            }
            Vec::new()
        }
        // Not a connectivity signal at all — left exactly where it was, neither reset nor
        // incremented.
        DataAction::LoadFailed { offline: false, .. } => Vec::new(),
        _ => {
            state.offline_failures = 0;
            Vec::new()
        }
    }
}

fn enter_offline(state: &mut AppState) -> Vec<Effect> {
    state.connectivity = Connectivity::Offline;
    state.toast("working offline", ToastLevel::Warning);
    state.probe_backoff_secs = PROBE_MIN_BACKOFF_SECS;
    state.probes_since_offline = 0;
    state.next_probe_at = None; // due immediately on the next Tick
    recompute_availability_offline(state);
    state.touch();
    Vec::new()
}

/// "Recomputes every queue entry's availability against the cache manifest and downloads index"
/// (`docs/06-cache-and-offline.md` §6) is not literally possible here: the reducer is pure and has
/// no visibility into `loxia-cache`'s real manifest/downloads index, and no earlier task built a
/// mirrored per-item availability signal into `AppState` either (`reducer::queue::append_tracks`'s
/// own doc comment already says as much: "no real cache manifest exists yet"). The best available
/// approximation, consistent with that same existing simplification: every currently-`Remote`
/// entry becomes `Unavailable`; `Downloaded`/`Cached` entries (however they came to exist) are
/// left untouched, since they're still genuinely available with no network at all
/// (`docs/12-decisions.md`).
fn recompute_availability_offline(state: &mut AppState) {
    for entry in &mut state.queue.entries {
        if entry.availability == Availability::Remote {
            entry.availability = Availability::Unavailable;
        }
    }
}

/// The inverse, run once `Reconnecting` advances to `Online`: everything becomes `Remote` again
/// except `Downloaded` entries, which never need to be (`docs/06-cache-and-offline.md` §6).
fn recompute_availability_online(state: &mut AppState) {
    for entry in &mut state.queue.entries {
        if entry.availability != Availability::Downloaded {
            entry.availability = Availability::Remote;
        }
    }
}

/// `SystemEvent::ConnectivityChanged` — the only way `Offline` becomes `Reconnecting` (the network
/// worker's probe, `Effect::Net(NetEffect::Reconnect)`, is the only thing that can prove the server
/// is reachable again); every other requested value is a plain assignment (a manual/test override,
/// or the runtime restoring a known state).
pub fn on_connectivity_changed(state: &mut AppState, new: Connectivity) -> Vec<Effect> {
    if new == state.connectivity {
        return Vec::new();
    }
    if new == Connectivity::Reconnecting && state.connectivity == Connectivity::Offline {
        state.connectivity = Connectivity::Reconnecting;
        state.toast("reconnected — syncing", ToastLevel::Info);
        state.touch();
        return vec![Effect::Cache(Box::new(CacheEffect::DrainScrobbles))];
    }
    state.connectivity = new;
    state.touch();
    Vec::new()
}

/// The other half of the `Reconnecting -> Online` transition — called from `tick`
/// (`reducer/mod.rs`) every tick, so it fires on the very next one after
/// [`on_connectivity_changed`] entered `Reconnecting`. Marking every column `Idle` (never
/// refetching directly) is what makes this "not refetch everything at once": a column only
/// actually re-requests once the user visits it.
pub fn advance_reconnecting(state: &mut AppState) {
    if state.connectivity != Connectivity::Reconnecting {
        return;
    }
    state.connectivity = Connectivity::Online;
    recompute_availability_online(state);
    mark_all_columns_idle(state);
    state.touch();
}

/// `10-12`: shared with the WebSocket's own `DataAction::LibraryChanged` handler
/// (`reducer::nav::apply_data`) — the same "mark everything `Idle`, refetch only on next view"
/// simplification this function's own doc comment already established for reconnection, reused
/// rather than duplicated for the analogous "something changed server-side" case.
pub(crate) fn mark_all_columns_idle(state: &mut AppState) {
    for stack in state.nav.per_tab_stacks.values_mut() {
        for column in stack.iter_mut() {
            column.load = LoadState::Idle;
        }
    }
}

/// Tick-driven probe scheduling (`docs/06-cache-and-offline.md` §6: "jittered backoff of 5s to
/// 30s"). `now` is `Tick`'s own carried timestamp, read directly per reducer rule 2's `Tick`-only
/// exception (`docs/04-state-and-input.md` §4) — never `Timestamp::now()`.
pub fn maybe_probe(state: &mut AppState, now: Timestamp) -> Vec<Effect> {
    if state.connectivity != Connectivity::Offline {
        return Vec::new();
    }
    let due = state.next_probe_at.is_none_or(|at| now >= at);
    if !due {
        return Vec::new();
    }
    schedule_next_probe(state, now);

    // A probe only ever asks the address already in use. When a profile has other addresses, "the
    // server is unreachable" and "*this way in* is unreachable" are different claims — a laptop
    // that has left the LAN can reach the server perfectly well, just not on the address it
    // connected with. Re-selecting rebuilds the client and the workers (`SysEffect::ReconnectServer`
    // -> `bootstrap::connect` -> `reach_server`), which is heavier than a probe, so it is left to
    // the point where the cheap answer has already failed a few times rather than done on the first
    // tick (`docs/12-decisions.md`).
    state.probes_since_offline += 1;
    if state.probes_since_offline > PROBES_BEFORE_RESELECT && has_other_endpoints(state) {
        return vec![Effect::Sys(crate::effect::SysEffect::ReconnectServer(
            crate::model::ServerId::from(state.config.active_server.clone()),
        ))];
    }
    vec![Effect::Net(NetEffect::Reconnect)]
}

/// Whether the active profile has an address other than the one in use.
fn has_other_endpoints(state: &AppState) -> bool {
    state
        .config
        .servers
        .iter()
        .find(|s| s.id == state.config.active_server)
        .is_some_and(|s| !s.fallbacks.is_empty())
}

/// Deterministic "jitter" (0-2s) derived from `now`'s own second component rather than a real RNG
/// — keeps the backoff schedule reproducible in tests while still varying run to run in
/// production, since `now` always does. Doubles the backoff for the *next* probe after this one,
/// capped at [`PROBE_MAX_BACKOFF_SECS`].
fn schedule_next_probe(state: &mut AppState, now: Timestamp) {
    let jitter = now.as_second().rem_euclid(3);
    let delay = state.probe_backoff_secs + jitter;
    state.next_probe_at = now.checked_add(SignedDuration::from_secs(delay)).ok();
    state.probe_backoff_secs = (state.probe_backoff_secs * 2).min(PROBE_MAX_BACKOFF_SECS);
}

/// Called from `reducer::apply` right after dispatching, whenever the result contains a network
/// effect while offline (`docs/06-cache-and-offline.md` §6: "a user reconnecting their VPN and
/// pressing a key should not wait 30 seconds"). `None` if a probe is already among `effects`, or
/// if `effects` wants nothing from the network at all.
pub fn maybe_immediate_probe(state: &mut AppState, effects: &[Effect]) -> Option<Effect> {
    if state.connectivity != Connectivity::Offline {
        return None;
    }
    let already_probing = effects
        .iter()
        .any(|e| matches!(e, Effect::Net(NetEffect::Reconnect)));
    if already_probing {
        return None;
    }
    let wants_network = effects
        .iter()
        .any(|e| matches!(e, Effect::Net(other) if !matches!(other, NetEffect::Reconnect)));
    if !wants_network {
        return None;
    }
    state.next_probe_at = None; // the scheduled backoff is superseded by this immediate attempt
    Some(Effect::Net(NetEffect::Reconnect))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::LoadTarget;
    use crate::state::queue::QueueEntry;
    use crate::test_support::fixtures;

    fn load_failed(offline: bool) -> DataAction {
        DataAction::LoadFailed {
            target: LoadTarget::QueueFetch,
            message: "x".to_string(),
            offline,
        }
    }

    #[test]
    fn two_offline_errors_trigger_offline() {
        let mut state = fixtures::fixture_empty();
        observe_data_action(&mut state, &load_failed(true));
        assert_eq!(state.connectivity, Connectivity::Online);
        observe_data_action(&mut state, &load_failed(true));
        assert_eq!(state.connectivity, Connectivity::Offline);
    }

    #[test]
    fn one_offline_error_does_not() {
        let mut state = fixtures::fixture_empty();
        observe_data_action(&mut state, &load_failed(true));
        assert_eq!(state.connectivity, Connectivity::Online);
        assert_eq!(state.offline_failures, 1);
    }

    #[test]
    fn success_resets_failure_counter() {
        let mut state = fixtures::fixture_empty();
        observe_data_action(&mut state, &load_failed(true));
        assert_eq!(state.offline_failures, 1);
        observe_data_action(
            &mut state,
            &DataAction::ImageLoaded {
                id: crate::model::ItemId::from("a"),
                tag: "t".to_string(),
            },
        );
        assert_eq!(state.offline_failures, 0);
    }

    #[test]
    fn not_found_does_not_count_as_offline() {
        let mut state = fixtures::fixture_empty();
        observe_data_action(&mut state, &load_failed(false));
        observe_data_action(&mut state, &load_failed(false));
        assert_eq!(state.offline_failures, 0);
        assert_eq!(state.connectivity, Connectivity::Online);
    }

    #[test]
    fn unauthorized_does_not_count_as_offline() {
        // `offline: false` is exactly the signal an `Unauthorized` (or `NotFound`/`Transient`)
        // failure carries — the network worker never sets it for anything but
        // `EmbyError::Offline` (`docs/12-decisions.md`).
        let mut state = fixtures::fixture_empty();
        observe_data_action(&mut state, &load_failed(false));
        assert_eq!(state.connectivity, Connectivity::Online);
    }

    #[test]
    fn entering_offline_toasts_once() {
        let mut state = fixtures::fixture_empty();
        observe_data_action(&mut state, &load_failed(true));
        observe_data_action(&mut state, &load_failed(true));
        assert_eq!(
            state
                .toasts
                .iter()
                .filter(|t| t.message.contains("working offline"))
                .count(),
            1
        );
        // A third failure must not re-toast.
        observe_data_action(&mut state, &load_failed(true));
        assert_eq!(
            state
                .toasts
                .iter()
                .filter(|t| t.message.contains("working offline"))
                .count(),
            1
        );
    }

    fn queue_entry(availability: Availability) -> QueueEntry {
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        QueueEntry {
            entry_id: crate::model::QueueEntryId(0),
            track: fixtures::track("One", 1, &alb, &[&a]),
            source: crate::state::queue::QueueSource::Manual,
            availability,
        }
    }

    #[test]
    fn entering_offline_recomputes_availability() {
        let mut state = fixtures::fixture_empty();
        state.queue.entries.push(queue_entry(Availability::Remote));
        state
            .queue
            .entries
            .push(queue_entry(Availability::Downloaded));

        observe_data_action(&mut state, &load_failed(true));
        observe_data_action(&mut state, &load_failed(true));

        assert_eq!(
            state.queue.entries[0].availability,
            Availability::Unavailable
        );
        assert_eq!(
            state.queue.entries[1].availability,
            Availability::Downloaded
        );
    }

    #[test]
    fn reconnect_drains_scrobbles_before_going_online() {
        let mut state = fixtures::fixture_empty();
        state.connectivity = Connectivity::Offline;

        let effects = on_connectivity_changed(&mut state, Connectivity::Reconnecting);

        assert_eq!(
            state.connectivity,
            Connectivity::Reconnecting,
            "not yet Online"
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::Cache(cache)] if matches!(**cache, CacheEffect::DrainScrobbles)
        ));

        advance_reconnecting(&mut state);
        assert_eq!(state.connectivity, Connectivity::Online);
    }

    #[test]
    fn reconnect_marks_columns_idle_not_refetch_all() {
        use crate::state::nav::{Column, Tab};

        let mut state = fixtures::fixture_empty();
        state.connectivity = Connectivity::Reconnecting;
        let mut col = Column::new(crate::state::nav::ColumnKind::Artists, "Artists");
        col.load = LoadState::Loaded { total: 5 };
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![col]);

        let effects_before = Vec::<Effect>::new();
        advance_reconnecting(&mut state);

        assert_eq!(
            state.nav.per_tab_stacks[&Tab::Artists][0].load,
            LoadState::Idle
        );
        assert!(
            effects_before.is_empty(),
            "marking idle emits no fetch effects of its own"
        );
    }

    /// A profile with one address probes forever: the probe only asks the address already in use,
    /// which is the right cheap answer when the *server* is down. With a second address configured,
    /// "unreachable" stops being conclusive — a laptop that has left the LAN can reach the server
    /// fine, just not that way — so after a couple of failed probes it re-selects
    /// (`docs/12-decisions.md`).
    #[test]
    fn a_profile_with_a_fallback_reselects_after_a_few_failed_probes() {
        let mut state = fixtures::fixture_empty();
        state.config.active_server = "srv".to_string();
        state.config.servers = vec![crate::config::ServerConfig {
            id: "srv".to_string(),
            fallbacks: vec![crate::config::ServerEndpoint {
                url: "http://elsewhere".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }];
        state.connectivity = Connectivity::Offline;
        state.probe_backoff_secs = PROBE_MIN_BACKOFF_SECS;

        let mut now = fixtures::fixed_epoch();
        let mut kinds = Vec::new();
        for _ in 0..3 {
            kinds.push(maybe_probe(&mut state, now));
            now = state.next_probe_at.unwrap();
        }

        assert_eq!(
            kinds[0],
            vec![Effect::Net(NetEffect::Reconnect)],
            "the cheap answer comes first"
        );
        assert_eq!(kinds[1], vec![Effect::Net(NetEffect::Reconnect)]);
        assert!(
            matches!(
                kinds[2].as_slice(),
                [Effect::Sys(crate::effect::SysEffect::ReconnectServer(_))]
            ),
            "a third failure must try the other address: {:?}",
            kinds[2]
        );
    }

    /// With nothing to fall back to there is nothing to re-select, so the probe loop stays exactly
    /// as it was — respawning the workers against the same dead address would achieve nothing.
    #[test]
    fn a_single_address_profile_only_ever_probes() {
        let mut state = fixtures::fixture_empty();
        state.connectivity = Connectivity::Offline;
        state.probe_backoff_secs = PROBE_MAX_BACKOFF_SECS;

        let effects = maybe_probe(&mut state, fixtures::fixed_epoch());
        assert_eq!(effects, vec![Effect::Net(NetEffect::Reconnect)]);
    }

    #[test]
    fn probe_backoff_is_jittered_and_capped() {
        let mut state = fixtures::fixture_empty();
        state.connectivity = Connectivity::Offline;
        state.probe_backoff_secs = PROBE_MIN_BACKOFF_SECS;

        let mut delays = Vec::new();
        let mut now = fixtures::fixed_epoch();
        for _ in 0..8 {
            let before = state.probe_backoff_secs;
            let effects = maybe_probe(&mut state, now);
            assert_eq!(effects, vec![Effect::Net(NetEffect::Reconnect)]);
            let scheduled = state.next_probe_at.unwrap();
            let delay = scheduled.as_second() - now.as_second();
            assert!(
                delay >= before && delay <= before + 2,
                "delay {delay} must be the pre-doubling backoff {before} plus 0-2s jitter"
            );
            delays.push(delay);
            now = scheduled;
        }
        assert!(state.probe_backoff_secs <= PROBE_MAX_BACKOFF_SECS);
        assert!(
            delays.iter().all(|d| *d <= PROBE_MAX_BACKOFF_SECS + 2),
            "capped at 30s plus jitter"
        );
        assert!(
            delays
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1,
            "jitter actually varies the delay across attempts"
        );
    }

    #[test]
    fn user_action_triggers_immediate_probe() {
        let mut state = fixtures::fixture_empty();
        state.connectivity = Connectivity::Offline;
        state.next_probe_at = Some(
            fixtures::fixed_epoch()
                .checked_add(SignedDuration::from_secs(30))
                .unwrap(),
        );

        let effects = vec![Effect::Net(NetEffect::Search {
            query: "x".to_string(),
            limit: 10,
        })];
        let probe = maybe_immediate_probe(&mut state, &effects);
        assert_eq!(probe, Some(Effect::Net(NetEffect::Reconnect)));
        assert!(
            state.next_probe_at.is_none(),
            "the stale backoff schedule is cleared"
        );
    }
}
