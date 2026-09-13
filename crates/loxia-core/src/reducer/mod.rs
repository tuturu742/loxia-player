//! apply(&mut AppState, Action) -> Vec<Effect> — the reducer entry point.

pub mod connectivity;
pub mod modal;
pub mod nav;
pub mod player;
pub mod queue;
pub mod session;
pub mod settings;

use jiff::SignedDuration;

use crate::Timestamp;
use crate::action::{Action, SystemEvent, ViewAction};
use crate::effect::Effect;
use crate::state::AppState;
use crate::state::nav::Tab;
use crate::state::player::PlayStatus;

/// A half-typed `g`-prefixed chord is abandoned if the second key doesn't arrive within this
/// window — matches `AppState::pending_chord`'s own doc comment (`03-01`).
const PENDING_CHORD_TTL: SignedDuration = SignedDuration::from_secs(1);

/// Applies persisted `Config` values onto the **runtime** `AppState` mirror at startup, and returns
/// the engine effects needed to make them real.
///
/// A real class of bug found in the field: several settings were written to `config` when changed
/// (and correctly saved to disk) but only ever *read back* by the Settings UI — the runtime fields
/// the rest of the app actually reads (`state.theme`, `player.quality_profile`, `player.replay_gain`,
/// `player.eq`) were left at `Default::default()` on every launch. So a chosen theme reverted to the
/// default, transcoding silently stayed `Direct`, and the equalizer did nothing at all, however the
/// config read (`docs/12-decisions.md`).
///
/// Call **after** `DataAction::PresetsLoaded` — resolving `equalizer.active_preset` into real gains
/// needs `player.known_presets` populated, exactly as `set_eq_preset` does at runtime.
pub fn hydrate_from_config(state: &mut AppState) -> Vec<Effect> {
    if let Some(theme) = crate::theme::Theme::builtin(&state.config.ui.theme) {
        state.theme = theme;
    }
    apply_ascii_only(state);
    state.player.quality_profile = state.config.transcode.mode;
    state.player.replay_gain = state.config.audio.default_replaygain;

    state.player.eq.enabled = state.config.equalizer.enabled;
    let active = state.config.equalizer.active_preset.clone();
    if let Some(preset) = state
        .player
        .known_presets
        .iter()
        .find(|p| p.name == active)
        .cloned()
    {
        state.player.eq.gains = preset.gains;
        state.player.eq.preset_name = preset.name;
    }

    let mut effects = vec![Effect::Audio(crate::effect::AudioEffect::SetReplayGain(
        state.player.replay_gain,
    ))];
    if state.player.eq.enabled {
        let curve = if state.player.eq.bypassed {
            [0.0; 10]
        } else {
            state.player.eq.gains
        };
        effects.push(Effect::Audio(crate::effect::AudioEffect::SetEq(Some(
            curve,
        ))));
    }
    state.touch();
    effects
}

/// Folds `ui.ascii_only` into the live theme.
///
/// `Theme::ascii_only` is loaded from the *theme file* (only `green_crt` and `amber_crt` declare
/// it), and nothing ever combined it with the user's own setting — so the Interface toggle changed
/// a config field that no widget ever read, and appeared to do nothing at all
/// (`docs/12-decisions.md`). ORed rather than assigned: a theme built for a vintage terminal stays
/// ASCII whatever the setting says, since its glyph choices assume it.
fn apply_ascii_only(state: &mut AppState) {
    state.theme.ascii_only |= state.config.ui.ascii_only;
}

/// Total and panic-free: every index is clamped, every `Option` handled
/// (`docs/04-state-and-input.md` §4 rule 1) — a reducer panic crashes the app with the terminal in
/// raw mode. Dispatches by action group; groups no landed task implements yet return no effects
/// rather than panicking, so this stays total *today*, not just once every module is filled in.
/// `03-06` implements `Nav`/`Select`/`Data`; `03-07` implements `Modal` and the modal-first
/// interception gate; the rest land in later phases (06 queue, 09 audio-advanced, 11 settings) by
/// editing this match, not by changing its shape.
pub fn apply(state: &mut AppState, action: Action) -> Vec<Effect> {
    if !modal::allows(state, &action) {
        return Vec::new();
    }

    // `08-06`: every `DataAction` first updates the consecutive-offline-failure counter (and, if
    // it just crossed the threshold, enters `Offline`) before being routed to its own handler
    // below — this is the one place that needs to see *every* reply regardless of which of the
    // ~15 network endpoints produced it.
    let mut effects = if let Action::Data(data_action) = &action {
        connectivity::observe_data_action(state, data_action)
    } else {
        Vec::new()
    };

    let mut rest = match action {
        Action::Nav(a) => nav::apply_nav(state, a),
        Action::Select(a) => nav::apply_select(state, a),
        Action::Data(a) => nav::apply_data(state, a),
        Action::Queue(a) => queue::apply_queue(state, a),
        Action::Item(a) => queue::apply_item(state, a),
        Action::Player(a) => player::apply_player(state, a),
        Action::Audio(a) => player::apply_audio(state, a),
        Action::Modal(a) => modal::apply_modal(state, a),
        Action::View(ViewAction::ToggleHelp) => modal::toggle_help(state),
        Action::View(ViewAction::ToggleHistory) => player::toggle_history_subview(state),
        Action::View(ViewAction::ToggleLyrics) => player::toggle_lyrics(state),
        Action::View(ViewAction::ScrollLyrics(delta)) => scroll_lyrics(state, delta),
        // `10-02`: `z` — a momentary view, not a setting, so this only ever flips the in-memory
        // flag; nothing here writes `state.config` (`Effect::Sys(WriteConfig)` is never emitted),
        // matching this task's own "persists to state but not to config" (`reducer::modal`'s own
        // stale comment on this variant, previously "not yet wired to a handler", is now out of
        // date — see `docs/12-decisions.md`).
        Action::View(ViewAction::ToggleZen) => {
            state.zen_mode = !state.zen_mode;
            state.touch();
            Vec::new()
        }
        Action::View(_) => Vec::new(),
        // No dedicated sub-reducer owns these three System variants (the module table assigns
        // only `ConfigChanged` to `reducer::settings`); wired inline since they're trivial,
        // module-agnostic bookkeeping, not because this task claims the rest of `System`.
        // `10-13`: routed through `AppState::toast` (not pushed directly) so this reaches the
        // exact same deduplication-by-message logic every internal `state.toast(...)` call site
        // already gets.
        Action::System(SystemEvent::Toast { message, level }) => {
            state.toast(message, level);
            Vec::new()
        }
        // `08-06`: `Offline -> Reconnecting` is the one transition with real state-machine
        // behaviour (draining scrobbles) — every other requested value is a plain assignment.
        Action::System(SystemEvent::ConnectivityChanged(connectivity)) => {
            connectivity::on_connectivity_changed(state, connectivity)
        }
        Action::System(SystemEvent::Quit) => {
            // A final `Stopped` report so a clean quit doesn't lose the resume point (`06-07`) —
            // the runtime dispatches whatever this returns before it ever calls `shutdown`'s own
            // `workers.drain()`, so this reaches the network worker's channel in time to be sent
            // within the drain's own bounded timeout (`03-08`).
            let effects = player::report_stopped(state);
            state.should_quit = true;
            state.touch();
            effects
        }
        Action::System(SystemEvent::Tick(now)) => tick(state, now),
        // `07-02`: `Ctrl+R` — only the Favourites tab actually refreshes anything today; no
        // earlier task wired `Refresh` for any other tab, so this is not a regression, just this
        // task's own narrower scope (`docs/12-decisions.md`).
        Action::System(SystemEvent::Refresh) => {
            if state.nav.active_tab == Tab::Favourites {
                nav::refresh_favourites(state)
            } else {
                Vec::new()
            }
        }
        Action::System(SystemEvent::SetPendingChord(chord)) => {
            // `SetPendingChord` carries no timestamp to thread through; stamped under the same
            // documented exception `AppState::toast()` and the `SleepTimer` modal's `armed_at`
            // already use (`03-01`, `03-07`) — display/expiry bookkeeping only, never branched on
            // by this action's own processing, only by a later `Tick`'s own timestamp.
            state.pending_chord = Some((chord, Timestamp::now()));
            state.touch();
            Vec::new()
        }
        // `08-08`: the `loxia-cache::session::load` reply — validated and applied by
        // `reducer::session::restore`, which needs `state.config` (server id, `ui.restore_autoplay`)
        // that `loxia-cache` itself has no access to.
        Action::System(SystemEvent::SessionRestored(snapshot)) => {
            session::restore(state, *snapshot)
        }
        // `10-10`: bookkeeping only — read back by `reducer::queue::notify_track_change` to
        // suppress a desktop notification while the terminal has focus.
        Action::System(SystemEvent::TerminalFocusChanged(focused)) => {
            state.terminal_focused = Some(focused);
            Vec::new()
        }
        // `11-01`: the module table already named `reducer::settings` as this variant's owner
        // (see this match's own pre-existing comment above); it had no handler at all until now.
        Action::System(SystemEvent::ConfigChanged(new_cfg)) => {
            settings::on_config_changed(state, *new_cfg)
        }
        Action::System(_) => Vec::new(),
        Action::Settings(a) => settings::apply(state, a),
    };
    effects.append(&mut rest);

    // `08-06`: whatever this dispatch itself just asked the network for, while offline, also
    // gets an immediate probe — "a user who reconnects their VPN and presses a key should not
    // wait 30 seconds" (`docs/06-cache-and-offline.md` §6).
    if let Some(probe) = connectivity::maybe_immediate_probe(state, &effects) {
        effects.push(probe);
    }

    effects
}

/// Every 100th tick (10 s) — the cadence `docs/04-state-and-input.md` §9 rule 5 gives the
/// periodic playback-progress report (`06-07`).
const PROGRESS_REPORT_EVERY_N_TICKS: u64 = 100;

/// `docs/04-state-and-input.md` §9's `Tick` responsibilities are the reducer's, not the runtime
/// loop's (which only supplies the timestamp) — this implements: expiring toasts and
/// `pending_chord`, recording the tick's timestamp as `state.clock` for the header clock to read
/// (`04-05`), the periodic playback-progress report (`06-07`), sleep-timer evaluation (`09-05`,
/// `evaluate_sleep_timer` — `Duration` firing plus the fade-out ramp for whichever trigger is
/// armed), the settings view's own debounced config write (`11-01`,
/// `settings::maybe_write_config`), and the keymap editor's own 2-second capture window (`11-02`,
/// `modal::expire_capture_window`). The session-snapshot effect still lands with whichever phase
/// owns its machinery (08 cache) rather than being guessed at here.
fn tick(state: &mut AppState, now: Timestamp) -> Vec<Effect> {
    state.clock = now;

    // `10-13`: each toast's own level decides its lifetime ("errors last longest") — no single
    // shared TTL constant any more.
    state
        .toasts
        .retain(|t| now.duration_since(t.created_at) < t.level.lifetime());

    if let Some((_, chord_at)) = &state.pending_chord
        && now.duration_since(*chord_at) >= PENDING_CHORD_TTL
    {
        state.pending_chord = None;
    }

    state.tick_count += 1;
    // `08-06`: `Reconnecting -> Online` (see `reducer::connectivity`'s own doc comment for why
    // this fires unconditionally rather than waiting on a real "scrobbles drained" signal that
    // doesn't exist yet), then the jittered offline probe schedule.
    connectivity::advance_reconnecting(state);
    let mut effects = connectivity::maybe_probe(state, now);
    if state
        .tick_count
        .is_multiple_of(PROGRESS_REPORT_EVERY_N_TICKS)
    {
        effects.extend(player::maybe_report_progress(state));
        // `10-11`: "every 10 s while playing (position updates)" — MPRIS clients poll `Position`
        // themselves, so this is skipped while paused/stopped/loading, unlike the scrobble
        // progress report just above (which also reports a paused position).
        if state.player.status == PlayStatus::Playing {
            effects.extend(player::mpris_meta(state));
        }
    }
    effects.extend(nav::maybe_fire_search(state));
    effects.extend(player::evaluate_sleep_timer(state, now));
    // `11-01`: "persistence is debounced 1 second" — the settings view's own edits only; every
    // other pre-existing config-writing path in this codebase still writes immediately
    // (`docs/12-decisions.md`).
    effects.extend(settings::maybe_write_config(state, now));
    // `11-02`: the keymap editor's own 2-second "was that a 2-chord sequence?" window —
    // finalizes a lone captured chord once nothing else has arrived by now.
    modal::expire_capture_window(state, now);

    // The clock advancing is itself a visible change (the header repaints every second) —
    // `touch()` unconditionally, not only when a toast/chord also expired.
    state.touch();
    effects
}

/// `J`/`K`: scrolls the **unsynced** lyrics pane by whole source lines. Timed lyrics follow
/// playback themselves, so there is nothing here for them to do — a manual offset would only fight
/// the auto-scroll.
///
/// Counted in source lines rather than rendered rows so the clamp is exact: the reducer has no idea
/// how the pane wraps, and guessing a row count is what left `ASSUMED_VIEWPORT_ROWS` a known wart
/// elsewhere (`docs/12-decisions.md`). The last line can always be scrolled to the top, never past.
fn scroll_lyrics(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let Some((_, crate::model::Lyrics::Unsynced(lines))) = state.lyrics.as_ref() else {
        return Vec::new();
    };
    let max = lines.len().saturating_sub(1);
    let next = (state.lyrics_scroll as i64 + i64::from(delta)).clamp(0, max as i64) as usize;
    if next != state.lyrics_scroll {
        state.lyrics_scroll = next;
        state.touch();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::SysEffect;
    use crate::keymap::{KeyChord, KeyCode, KeyModifiers};
    use crate::state::toast::{Toast, ToastLevel};
    use crate::test_support::fixtures;

    fn state_with_unsynced_lyrics(count: usize) -> AppState {
        let mut state = fixtures::fixture_empty();
        let lines: Vec<String> = (0..count).map(|i| format!("line {i}")).collect();
        state.lyrics = Some((
            crate::model::ItemId::from("t1"),
            crate::model::Lyrics::Unsynced(lines),
        ));
        state
    }

    /// Untimed lyrics have no active line to follow, so a long track's lyrics simply ran off the
    /// bottom of the pane with no way to read the rest (`docs/12-decisions.md`).
    #[test]
    fn unsynced_lyrics_scroll_and_clamp_at_both_ends() {
        let mut state = state_with_unsynced_lyrics(40);

        apply(&mut state, Action::View(ViewAction::ScrollLyrics(1)));
        assert_eq!(state.lyrics_scroll, 1);

        apply(&mut state, Action::View(ViewAction::ScrollLyrics(-5)));
        assert_eq!(state.lyrics_scroll, 0, "cannot scroll above the first line");

        for _ in 0..100 {
            apply(&mut state, Action::View(ViewAction::ScrollLyrics(1)));
        }
        assert_eq!(
            state.lyrics_scroll, 39,
            "the last line can reach the top, but not scroll past it"
        );
    }

    /// Timed lyrics scroll themselves against the playback position; a manual offset would only
    /// fight that, so the keys do nothing there.
    #[test]
    fn synced_lyrics_ignore_manual_scrolling() {
        let mut state = fixtures::fixture_empty();
        state.lyrics = Some((
            crate::model::ItemId::from("t1"),
            crate::model::Lyrics::Synced(vec![crate::model::LyricLine {
                at: std::time::Duration::from_secs(1),
                text: "a line".to_string(),
            }]),
        ));

        apply(&mut state, Action::View(ViewAction::ScrollLyrics(3)));

        assert_eq!(state.lyrics_scroll, 0);
    }

    /// A new track's lyrics start at the top — otherwise they open mid-song, or past the end of a
    /// shorter set entirely.
    #[test]
    fn newly_loaded_lyrics_reset_the_scroll() {
        let mut state = state_with_unsynced_lyrics(40);
        apply(&mut state, Action::View(ViewAction::ScrollLyrics(10)));
        assert_eq!(state.lyrics_scroll, 10);

        apply(
            &mut state,
            Action::Data(crate::action::DataAction::LyricsLoaded {
                track: crate::model::ItemId::from("t2"),
                lyrics: crate::model::Lyrics::Unsynced(vec!["fresh".to_string()]),
            }),
        );

        assert_eq!(state.lyrics_scroll, 0);
    }

    /// `Theme::ascii_only` comes from the *theme file*, and the user's own `ui.ascii_only` setting
    /// was never folded into it — so the Interface toggle wrote a config field no widget ever read
    /// and appeared to do nothing at all (`docs/12-decisions.md`).
    #[test]
    fn ascii_only_setting_reaches_the_live_theme() {
        let mut state = fixtures::fixture_empty();
        state.config.ui.theme = "default_terminal".to_string();
        state.config.ui.ascii_only = false;
        hydrate_from_config(&mut state);
        assert!(
            !state.theme.ascii_only,
            "precondition: this theme is unicode"
        );

        state.config.ui.ascii_only = true;
        hydrate_from_config(&mut state);
        assert!(state.theme.ascii_only, "the setting must reach the theme");
    }

    /// A theme built for a vintage terminal stays ASCII whatever the setting says — its glyph
    /// choices assume it.
    #[test]
    fn an_ascii_theme_stays_ascii_with_the_setting_off() {
        let mut state = fixtures::fixture_empty();
        state.config.ui.theme = "green_crt".to_string();
        state.config.ui.ascii_only = false;
        hydrate_from_config(&mut state);
        assert!(state.theme.ascii_only);
    }

    /// Turning the setting back off restores the theme's own baseline rather than sticking on.
    #[test]
    fn turning_ascii_only_off_again_restores_the_theme_baseline() {
        let mut state = fixtures::fixture_empty();
        state.config.ui.theme = "default_terminal".to_string();
        state.config.ui.ascii_only = true;
        hydrate_from_config(&mut state);
        assert!(state.theme.ascii_only);

        let mut next = state.config.clone();
        next.ui.ascii_only = false;
        apply(
            &mut state,
            Action::System(SystemEvent::ConfigChanged(Box::new(next))),
        );
        assert!(
            !state.theme.ascii_only,
            "should follow the setting back down"
        );
    }

    /// A real class of bug: settings were persisted to `config` but never applied to the runtime
    /// mirror at startup, so a chosen theme reverted, transcoding stayed `Direct`, and the EQ did
    /// nothing however the config read (`docs/12-decisions.md`).
    #[test]
    fn hydrate_from_config_applies_persisted_settings_to_runtime_state() {
        let mut state = fixtures::fixture_empty();
        state.config.ui.theme = "gruvbox_dark".to_string();
        state.config.transcode.mode = crate::config::QualityProfile::TranscodeHigh;
        state.config.audio.default_replaygain = crate::config::ReplayGainMode::Track;
        state.config.equalizer.enabled = true;
        state.config.equalizer.active_preset = "bass_boost".to_string();
        state.player.known_presets = vec![crate::config::EqPreset {
            name: "bass_boost".to_string(),
            gains: [6.0, 5.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }];

        let effects = hydrate_from_config(&mut state);

        assert_eq!(
            state.player.quality_profile,
            crate::config::QualityProfile::TranscodeHigh
        );
        assert_eq!(
            state.player.replay_gain,
            crate::config::ReplayGainMode::Track
        );
        assert!(state.player.eq.enabled);
        assert_eq!(state.player.eq.preset_name, "bass_boost");
        assert_eq!(state.player.eq.gains[0], 6.0);
        // The engine is told, not just the mirror updated.
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(crate::effect::AudioEffect::SetEq(Some(_)))))
        );
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Audio(crate::effect::AudioEffect::SetReplayGain(_))
        )));
    }

    #[test]
    fn hydrate_from_config_leaves_eq_off_and_silent_when_disabled() {
        let mut state = fixtures::fixture_empty();
        state.config.equalizer.enabled = false;

        let effects = hydrate_from_config(&mut state);

        assert!(!state.player.eq.enabled);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(crate::effect::AudioEffect::SetEq(_)))),
            "a disabled EQ must not install a filter chain"
        );
    }

    #[test]
    fn hydrate_from_config_ignores_an_unknown_theme_name() {
        let mut state = fixtures::fixture_empty();
        let before = state.theme.clone();
        state.config.ui.theme = "no_such_theme".to_string();
        hydrate_from_config(&mut state);
        assert_eq!(state.theme, before);
    }

    fn at(secs: i64) -> Timestamp {
        fixtures::fixed_epoch()
            .checked_add(SignedDuration::from_secs(secs))
            .unwrap()
    }

    #[test]
    fn toasts_expire_on_tick() {
        let mut state = fixtures::fixture_empty();
        state.toasts.push(Toast {
            id: 0,
            message: "old".to_string(),
            level: ToastLevel::Info,
            created_at: at(0),
        });
        state.toasts.push(Toast {
            id: 1,
            message: "fresh".to_string(),
            level: ToastLevel::Info,
            created_at: at(10),
        });

        // 5s after the fixed epoch: the toast created at t=0 (age 5s) has outlived `Info`'s own
        // 3s lifetime; the one created at t=10 has not been reached yet, so its age is negative
        // and it must survive.
        apply(&mut state, Action::System(SystemEvent::Tick(at(5))));

        assert_eq!(state.toasts.len(), 1);
        assert_eq!(state.toasts[0].message, "fresh");
    }

    #[test]
    fn pending_chord_expires_after_one_second() {
        let mut state = fixtures::fixture_empty();
        let g = KeyChord {
            code: KeyCode::Char('g'),
            mods: KeyModifiers::default(),
        };
        state.pending_chord = Some((g, at(0)));

        let just_under = at(0).checked_add(SignedDuration::from_millis(900)).unwrap();
        apply(&mut state, Action::System(SystemEvent::Tick(just_under)));
        assert!(
            state.pending_chord.is_some(),
            "must not expire before the full window has elapsed"
        );

        apply(
            &mut state,
            Action::System(SystemEvent::Tick(
                at(0)
                    .checked_add(SignedDuration::from_millis(1001))
                    .unwrap(),
            )),
        );
        assert!(state.pending_chord.is_none());
    }

    // --- 06-07: playback reporting wiring (tick-driven progress + quit) ----------------------

    use crate::effect::NetEffect;
    use crate::model::{PlaySessionId, PlaybackReport};

    fn has_progress_report(effects: &[Effect]) -> bool {
        effects.iter().any(|e| {
            matches!(
                e,
                Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Progress { .. }))
            )
        })
    }

    #[test]
    fn progress_reported_every_ten_seconds() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.session = Some(PlaySessionId::from("session-1"));

        for i in 1..100 {
            let effects = apply(&mut state, Action::System(SystemEvent::Tick(at(i))));
            assert!(
                !has_progress_report(&effects),
                "tick {i} must not report progress yet"
            );
        }
        let effects = apply(&mut state, Action::System(SystemEvent::Tick(at(100))));
        assert!(has_progress_report(&effects), "the 100th tick must report");
    }

    fn has_mpris_update(effects: &[Effect]) -> bool {
        effects
            .iter()
            .any(|e| matches!(e, Effect::Sys(crate::effect::SysEffect::UpdateMpris(_))))
    }

    /// `10-11`: same 10-second cadence as the scrobble progress report, but only "while playing" —
    /// unlike the scrobble report, which fires paused too (`reducer::player::maybe_report_progress`
    /// reports `paused: true` rather than skipping).
    #[test]
    fn position_updates_throttled_to_ten_seconds() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.session = Some(PlaySessionId::from("session-1"));

        for i in 1..100 {
            let effects = apply(&mut state, Action::System(SystemEvent::Tick(at(i))));
            assert!(
                !has_mpris_update(&effects),
                "tick {i} must not update MPRIS position yet"
            );
        }
        let effects = apply(&mut state, Action::System(SystemEvent::Tick(at(100))));
        assert!(
            has_mpris_update(&effects),
            "the 100th tick must update MPRIS position while playing"
        );
    }

    #[test]
    fn no_mpris_position_update_while_paused() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.session = Some(PlaySessionId::from("session-1"));
        state.player.status = crate::state::player::PlayStatus::Paused;

        let effects = apply(&mut state, Action::System(SystemEvent::Tick(at(100))));
        assert!(!has_mpris_update(&effects));
    }

    #[test]
    fn quit_emits_final_stopped() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.session = Some(PlaySessionId::from("session-1"));
        let effects = apply(&mut state, Action::System(SystemEvent::Quit));
        assert!(state.should_quit);
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Stopped { .. }))
        )));
    }

    // --- 10-02: Zen mode toggle -----------------------------------------------------------------

    #[test]
    fn toggle_zen_flips_the_flag() {
        let mut state = fixtures::fixture_empty();
        assert!(!state.zen_mode);

        apply(&mut state, Action::View(ViewAction::ToggleZen));
        assert!(state.zen_mode);

        apply(&mut state, Action::View(ViewAction::ToggleZen));
        assert!(!state.zen_mode);
    }

    /// `z` persists to state (proven above) but never to `config.toml` — Zen is a momentary mode,
    /// not a setting a restart should remember.
    #[test]
    fn zen_state_not_persisted_to_config() {
        let mut state = fixtures::fixture_empty();
        let config_before = state.config.clone();

        let effects = apply(&mut state, Action::View(ViewAction::ToggleZen));

        assert_eq!(state.config, config_before);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
    }
}
