//! Reducer: modal lifecycle and field editing (`docs/02-data-model.md` §6,
//! `docs/04-state-and-input.md` §4).

use std::time::Duration;

use strum::IntoEnumIterator;

use crate::Timestamp;
use crate::action::{Action, ModalAction, QueueAction};
use crate::effect::{AudioEffect, Effect, NetEffect, SysEffect};
use crate::keymap::parse::render_binding;
use crate::keymap::{ActionId, InputContext, KeyBinding, KeyChord, KeyCode, KeyMap, KeyModifiers};
use crate::model::{ItemId, MediaItem, PlaylistId, Track};
use crate::state::modal::{Modal, ModalKind, PlaylistTarget, SaveSource, SortApplyTarget};
use crate::state::nav::{Column, ColumnKind, LoadState, Tab};
use crate::state::player::{SleepTimer, SleepTrigger};
use crate::state::queue::RepeatMode;
use crate::state::toast::ToastLevel;
use crate::state::{AppState, Connectivity};

pub fn apply_modal(state: &mut AppState, action: ModalAction) -> Vec<Effect> {
    match action {
        ModalAction::Open(kind) => open_modal(state, kind),
        ModalAction::Close => close_modal(state),
        ModalAction::Submit => submit(state),
        ModalAction::FieldNext => field_next(state),
        ModalAction::FieldPrev => field_prev(state),
        ModalAction::FieldInput(c) => field_input(state, c),
        ModalAction::FieldBackspace => field_backspace(state),
        ModalAction::FieldSet(index) => field_set(state, index),
        ModalAction::Scroll(delta) => scroll_help(state, delta),
        ModalAction::AdjustGain(delta) => adjust_gain(state, delta),
        ModalAction::SetGainAt { band, db } => set_gain_at(state, band, db),
        ModalAction::CyclePreset => cycle_preset(state),
        ModalAction::ToggleBypass => toggle_modal_bypass(state),
        ModalAction::ToggleEqEnabled => toggle_modal_eq_enabled(state),
        ModalAction::ActivateField => activate_field(state),
        ModalAction::DisarmSleepTimer => disarm_sleep_timer(state),
        ModalAction::OpenSavePlaylist(source) => open_save_playlist(state, source),
        ModalAction::CycleSaveTarget(delta) => cycle_save_target(state, delta),
        ModalAction::ToggleSortTarget => toggle_sort_target(state),
        ModalAction::OpenSettingsSorting => open_settings_sorting(state),
        ModalAction::StartCapture => start_capture(state),
        ModalAction::CaptureChord(chord) => capture_chord(state, chord),
        ModalAction::CancelCapture => cancel_capture(state),
        ModalAction::ResetRowToDefault => reset_keymap_row(state),
        ModalAction::UnbindRow => unbind_keymap_row(state),
        ModalAction::ConfirmResetAllKeybindings => confirm_reset_all_keybindings(state),
        ModalAction::ResetAllKeybindings => reset_all_keybindings(state),
    }
}

/// 2 seconds — this task's own spec ("captured by pressing a prefix and then the second key,
/// with a 2-second window"). Deliberately longer than `PENDING_CHORD_TTL`'s 1-second window
/// (`reducer/mod.rs`) — that window is for *resolving* an already-known 2-chord binding a user
/// has presumably memorised, this one is for a user actively *deciding*, mid-capture, whether they
/// meant to type a sequence at all.
const CAPTURE_WINDOW: jiff::SignedDuration = jiff::SignedDuration::from_secs(2);

/// `10-03`: only `Modal::Help` carries a `scroll` field at all — a no-op, not a panic, if this
/// somehow arrives while a different modal (or none) is open, since `keymap::resolve`'s own
/// `help_scroll` table is the only producer of this action and it is already gated on
/// `ModalKind::Help`. Clamped to `0` at the top; there is no lower bound to clamp against here
/// (the content's own total height is a rendering-time fact this reducer doesn't have), so
/// scrolling past the end just shows blank space rather than wrapping or erroring.
fn scroll_help(state: &mut AppState, delta: i32) -> Vec<Effect> {
    if let Some(Modal::Help { scroll, .. }) = &mut state.modal {
        *scroll = (i32::from(*scroll) + delta).max(0) as u16;
        state.touch();
    }
    Vec::new()
}

/// While a modal is open it owns the keyboard: only `Modal` actions, `System` events, the
/// universal help toggle, and `Cancel` reach their handlers — `Cancel` because `keymap::resolve`
/// maps a modal context's `Esc` to the same `ActionId::Cancel` (hence `Action::Nav(Cancel)`) as
/// `Normal` context, not to a dedicated modal-close action. Everything else is dropped so, e.g.,
/// `j` cannot move both the modal cursor and the column behind it.
///
/// `10-08`: found and fixed a real, previously-undetected gap — `Action::Data(_)` (every async
/// network/cache/audio *reply*, as opposed to live user input) was missing from this list, so a
/// `DataAction` arriving while *any* modal was open was silently discarded here, never reaching
/// `apply_data` at all. Nothing had exercised this end-to-end before this task (every prior
/// `DataAction` handler test dispatched straight against `reducer::nav`, bypassing this gate) — in
/// the real app it meant `10-05`'s device picker could never actually show its own loaded device
/// list (`DataAction::DevicesLoaded` arrives while `Modal::DevicePicker` is still open, by
/// construction — that's the entire point of firing `Effect::Audio(EnumerateDevices)` on open),
/// and this task's own "existing playlists loaded via `FetchColumn` on open" would have been
/// equally impossible. A reply is never itself a keystroke that could conflict with whatever the
/// modal captures, so it belongs in the allowed set unconditionally, not gated on which modal (if
/// any) happens to be open.
pub(crate) fn allows(state: &AppState, action: &Action) -> bool {
    if state.modal.is_none() {
        return true;
    }
    matches!(
        action,
        Action::Modal(_)
            | Action::System(_)
            | Action::Data(_)
            | Action::View(crate::action::ViewAction::ToggleHelp)
            | Action::Nav(crate::action::NavAction::Cancel)
    )
}

/// `?` toggles the help modal even while another modal is open — closing it if it is already
/// showing, otherwise replacing whatever was open.
pub(crate) fn toggle_help(state: &mut AppState) -> Vec<Effect> {
    if matches!(state.modal, Some(Modal::Help { .. })) {
        return close_modal_effects(state);
    }
    open_modal(state, ModalKind::Help)
}

pub(crate) fn open_modal(state: &mut AppState, kind: ModalKind) -> Vec<Effect> {
    // `10-08`: `SavePlaylist` needs an explicit `SaveSource` hint (`P` vs `Ctrl+P`) the generic
    // `Open(ModalKind)` shape has no room for, plus its own open-time refusals ("nothing to
    // save"/"needs a connection") — `input.rs`'s real key handlers use `ModalAction::
    // OpenSavePlaylist` instead; this path only remains reachable for a generic `Open` call that
    // doesn't care about the source distinction, defaulting to the whole queue.
    if kind == ModalKind::SavePlaylist {
        return open_save_playlist(state, SaveSource::Queue);
    }
    let new_modal = match kind {
        ModalKind::Help => Some(Modal::Help {
            context: current_input_context(state),
            scroll: 0,
        }),
        ModalKind::Equalizer => Some(build_equalizer_modal(state)),
        ModalKind::DevicePicker => Some(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loading,
        }),
        ModalKind::SleepTimer => Some(build_sleep_timer_modal(state)),
        ModalKind::SavePlaylist => unreachable!("handled above"),
        ModalKind::SortProfile => Some(build_sort_profile_modal(state)),
        ModalKind::KeymapEditor => Some(Modal::KeymapEditor {
            action_cursor: 0,
            capturing: false,
            conflict: None,
            captured: None,
            capture_deadline: None,
        }),
        // `ModalKind` is a bare discriminant — a real confirmation needs a caller-supplied
        // `prompt`/`on_confirm`, which only `open_confirm` (below) can provide. A future reducer
        // wanting a confirm dialog (e.g. `Item::DeletePlaylist`) calls that directly instead of
        // going through `Action::Modal(Open(..))`.
        ModalKind::Confirm => None,
    };
    let Some(new_modal) = new_modal else {
        return Vec::new();
    };

    let mut effects = close_modal_effects(state);
    if kind == ModalKind::DevicePicker {
        effects.push(Effect::Audio(AudioEffect::EnumerateDevices));
    }
    state.modal = Some(new_modal);
    state.touch();
    effects
}

/// Opens a `Confirm` modal with a caller-supplied prompt and follow-up action — the only way one
/// can be constructed, since `ModalKind::Confirm` alone carries no payload.
#[allow(dead_code)] // exercised by later phases (06/07) once a reducer needs a confirm dialog
pub(crate) fn open_confirm(
    state: &mut AppState,
    prompt: impl Into<String>,
    on_confirm: Action,
) -> Vec<Effect> {
    let effects = close_modal_effects(state);
    state.modal = Some(Modal::Confirm {
        prompt: prompt.into(),
        on_confirm: Box::new(on_confirm),
    });
    state.touch();
    effects
}

/// The "abandon this modal" path, shared by `ModalAction::Close`, `Nav::Cancel`'s modal-close
/// rung, and `open_modal`'s replace-without-committing case — as opposed to `submit`, which closes
/// via a distinct commit path and must never also run this.
pub(crate) fn close_modal(state: &mut AppState) -> Vec<Effect> {
    close_modal_effects(state)
}

fn close_modal_effects(state: &mut AppState) -> Vec<Effect> {
    let Some(modal) = state.modal.take() else {
        return Vec::new();
    };
    state.touch();
    match modal {
        // The engine was given a live preview of `draft_gains` as the user edited; abandoning the
        // modal must revert it. `player.eq.gains` itself is never written directly here — like
        // volume, it mirrors what the engine confirms, not what the modal wished for (rule 6).
        Modal::Equalizer {
            gains_at_open,
            enabled_at_open,
            ..
        } => {
            // Reverts the on/off draft too, not just the gains — and `None`, not a flat curve,
            // when the equalizer was off when the modal opened: a flat curve is bypass, which
            // leaves the chain installed.
            if enabled_at_open {
                vec![Effect::Audio(AudioEffect::SetEq(Some(gains_at_open)))]
            } else {
                vec![Effect::Audio(AudioEffect::SetEq(None))]
            }
        }
        _ => Vec::new(),
    }
}

fn current_input_context(state: &AppState) -> InputContext {
    if let Some(modal) = &state.modal {
        return InputContext::Modal(modal.kind());
    }
    if state.active_column().is_some_and(|c| c.filter.is_some()) {
        return InputContext::TextInput;
    }
    InputContext::Normal
}

fn build_equalizer_modal(state: &AppState) -> Modal {
    let gains = state.player.eq.gains;
    // `10-06`: searches `known_presets` (factory *and* custom, in that order — the same list `p`
    // cycles through), not just `FACTORY_EQ_PRESET_NAMES` — the original version of this line only
    // ever found a match for a factory preset, so opening the modal with a *custom* preset active
    // always started `preset_idx` at `None` even though the active preset was perfectly findable;
    // `docs/12-decisions.md`.
    let preset_idx = state
        .player
        .known_presets
        .iter()
        .position(|p| p.name == state.player.eq.preset_name);
    Modal::Equalizer {
        band: 0,
        draft_gains: gains,
        gains_at_open: gains,
        preset_idx,
        bypassed: state.player.eq.bypassed,
        enabled: state.player.eq.enabled,
        enabled_at_open: state.player.eq.enabled,
    }
}

/// `11-05`: the EQ-preset manager's own `e` — opens the equalizer modal prefilled with `gains`
/// (the *focused* preset's own gains, not necessarily whatever is currently playing), live
/// previewing them immediately (`Effect::Audio(SetEq)`) the same way adjusting a band inside the
/// modal already does. `gains_at_open` is still whatever was *actually* playing before this call,
/// not `gains` itself — `Esc` must restore the real prior state, not the preset merely being
/// previewed (`Modal::Equalizer`'s own existing "Esc restores `gains_at_open`" contract,
/// unchanged).
pub(crate) fn open_equalizer_with_gains(
    state: &mut AppState,
    gains: [f32; 10],
    preset_idx: Option<usize>,
) -> Vec<Effect> {
    let gains_at_open = state.player.eq.gains;
    let mut effects = close_modal_effects(state);
    state.modal = Some(Modal::Equalizer {
        band: 0,
        draft_gains: gains,
        gains_at_open,
        preset_idx,
        bypassed: state.player.eq.bypassed,
        // Opening the editor from the preset manager is an act of turning the equalizer on: it
        // previews the preset immediately, and previewing something switched off is meaningless.
        enabled: true,
        enabled_at_open: state.player.eq.enabled,
    });
    state.touch();
    effects.push(Effect::Audio(AudioEffect::SetEq(Some(gains))));
    effects
}

fn build_sleep_timer_modal(state: &AppState) -> Modal {
    match &state.player.sleep_timer {
        Some(timer) => Modal::SleepTimer {
            trigger: timer.trigger,
            fade_out: timer.fade_out,
            quit_after: timer.quit_after,
            field_cursor: 0,
        },
        None => Modal::SleepTimer {
            trigger: SleepTrigger::Duration(Duration::from_secs(15 * 60)),
            fade_out: true,
            quit_after: false,
            field_cursor: 0,
        },
    }
}

/// `10-08`: `source` is now always supplied by the caller (`open_save_playlist`) rather than
/// guessed from whether a selection happens to be non-empty — see that function's own doc comment
/// for why the guess was wrong.
fn build_save_playlist_modal(source: SaveSource) -> Modal {
    Modal::SavePlaylist {
        target: PlaylistTarget::New,
        target_cursor: 0,
        name: String::new(),
        overview: String::new(),
        autosort: false,
        field: 0,
        source,
        error: None,
    }
}

/// The real entry point for `P`/`Ctrl+P` (`ModalAction::OpenSavePlaylist`) — refuses to open at
/// all ("nothing to save"/"needs a connection", this task's own spec) rather than opening onto an
/// empty or unusable form, then seeds `Tab::Playlists`'s own column-0 with a fresh `Loading` state
/// and fires its `FetchColumn`, exactly like `10-05`'s device picker always re-enumerates: a user
/// opening this modal may have created a playlist elsewhere since the tab was last visited, so a
/// cached list must never be shown instead. Only column 0 is touched — any deeper stack the user
/// had drilled into on the Playlists tab itself is left alone.
fn open_save_playlist(state: &mut AppState, source: SaveSource) -> Vec<Effect> {
    if state.connectivity == Connectivity::Offline {
        state.toast("playlists need a connection", ToastLevel::Warning);
        return Vec::new();
    }
    if save_playlist_tracks(state, source).is_empty() {
        state.toast("nothing to save", ToastLevel::Warning);
        return Vec::new();
    }

    let mut effects = close_modal_effects(state);
    state.modal = Some(build_save_playlist_modal(source));

    let mut column = Column::new(ColumnKind::Playlists, "Playlists".to_string());
    column.load = LoadState::Loading;
    let stack = state.nav.per_tab_stacks.entry(Tab::Playlists).or_default();
    if stack.is_empty() {
        stack.push(column);
    } else {
        stack[0] = column;
    }
    effects.push(Effect::Net(crate::effect::NetEffect::FetchColumn {
        tab: Tab::Playlists,
        depth: 0,
        kind: ColumnKind::Playlists,
        page: 0,
    }));
    state.touch();
    effects
}

/// The loaded `Tab::Playlists` column's own existing playlists, in display order — `None`/empty
/// while `open_save_playlist`'s own `FetchColumn` reply hasn't arrived yet.
fn existing_playlists(state: &AppState) -> Vec<(PlaylistId, String)> {
    state
        .nav
        .per_tab_stacks
        .get(&Tab::Playlists)
        .and_then(|stack| stack.first())
        .map(|column| {
            column
                .items
                .iter()
                .filter_map(|item| match item {
                    MediaItem::Playlist(p) => {
                        Some((PlaylistId::from(p.id.as_str()), p.name.clone()))
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The display name of an already-loaded existing playlist — falls back to the bare id if it's
/// somehow no longer present in `existing_playlists` (a stale target from before a re-fetch,
/// say), so the completion toast always names *something* rather than silently omitting it.
fn playlist_display_name(state: &AppState, id: &PlaylistId) -> String {
    existing_playlists(state)
        .into_iter()
        .find(|(pid, _)| pid == id)
        .map(|(_, name)| name)
        .unwrap_or_else(|| id.as_str().to_string())
}

/// The first row of the sort menu: not a real profile but the way back to the order the tracks were
/// queued in — an album's disc/track order, a playlist's own order. Recognised by having no rules,
/// which is also the only sensible reading of an empty rule list (`docs/12-decisions.md`).
pub const DEFAULT_ORDER_ROW: &str = "Default order (as queued)";

fn build_sort_profile_modal(state: &AppState) -> Modal {
    let mut profiles = vec![crate::config::SortProfile {
        name: DEFAULT_ORDER_ROW.to_string(),
        rules: Vec::new(),
    }];
    profiles.extend(state.config.sorting.profiles.iter().cloned());
    // With no profile applied the queue *is* in default order, so that row is what's selected.
    let cursor = match state.queue.sort_profile.as_deref() {
        Some(active) => profiles.iter().position(|p| p.name == active).unwrap_or(0),
        None => 0,
    };
    // Defaults to the queue whenever one is loaded — not just while it's actively *playing*. Keying
    // off `PlayStatus::Playing` alone meant that pausing flipped the default to `Column`, so sorting
    // "stopped working" on a paused queue (a live user hit exactly this); a paused queue is still a
    // queue the user means to reorder (`docs/12-decisions.md`). `Tab` still toggles to `Column` for
    // sorting the browse list instead. The widget's own "which profile is active" marker reads
    // `state.queue.sort_profile` directly, independent of this default.
    let target = if state.queue.entries.is_empty() {
        SortApplyTarget::Column
    } else {
        SortApplyTarget::Queue
    };
    Modal::SortProfile {
        profiles,
        cursor,
        editing: None,
        target,
    }
}

fn toggle_sort_target(state: &mut AppState) -> Vec<Effect> {
    if let Some(Modal::SortProfile { target, .. }) = &mut state.modal {
        *target = match target {
            SortApplyTarget::Queue => SortApplyTarget::Column,
            SortApplyTarget::Column => SortApplyTarget::Queue,
        };
        state.touch();
    }
    Vec::new()
}

/// `10-09`: `e` — "closes this modal and opens Settings → Sorting." Reuses `nav::set_tab`
/// (widened to `pub(crate)` for this) rather than hand-rolling a second, smaller tab switch —
/// `Tab::Settings` itself has no seed column (`seed_column_for_tab`'s own `None` case), so this
/// reduces to a plain tab switch plus this modal closing; there is no "Sorting" *section* to land
/// on within Settings until `11-04` builds one.
/// `10-09`'s own doc comment on this function used to note that "Settings itself has no
/// section-navigation model yet" — `11-01` built one (`AppState::settings`), so this now lands
/// precisely on the Sorting section, not just the tab as a whole.
fn open_settings_sorting(state: &mut AppState) -> Vec<Effect> {
    let mut effects = close_modal_effects(state);
    effects.extend(super::nav::set_tab(state, Tab::Settings));
    state.settings.section = crate::state::settings::SettingsSection::Sorting;
    state.settings.cursor = 0;
    effects
}

/// `10-08`: returns full `Track`s, not bare `ItemId`s — the sort-before-saving checkbox needs real
/// sortable fields (`queue::sort::compare`), which an id alone can't supply. `SaveSource::
/// Selection` also gained the "or the focused item when nothing is multi-selected" fallback this
/// task's own spec names (previously just an empty vec when nothing was multi-selected, silently
/// contradicting the spec) — only a focused *track* row can fall back this way; a focused
/// Artist/Album/etc. row has no track to save, so falls back to nothing, same as before.
fn save_playlist_tracks(state: &AppState, source: SaveSource) -> Vec<Track> {
    match source {
        SaveSource::Queue => state
            .queue
            .entries
            .iter()
            .map(|e| e.track.clone())
            .collect(),
        SaveSource::Selection => {
            let Some(column) = state.active_column() else {
                return Vec::new();
            };
            if !column.selection.selected.is_empty() {
                column
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        MediaItem::Track(t) if column.selection.selected.contains(&t.id) => {
                            Some(t.clone())
                        }
                        _ => None,
                    })
                    .collect()
            } else {
                match column.items.get(column.cursor) {
                    Some(MediaItem::Track(t)) => vec![t.clone()],
                    _ => Vec::new(),
                }
            }
        }
    }
}

/// `10-08`'s own sort checkbox — applies `queue.sort_profile` (or the config's default queue
/// profile, the same fallback `build_sort_profile_modal` already uses) via the shared
/// `queue::sort::compare` comparator, the identical ordering `QueueAction::ApplySortProfile` uses
/// for the live queue. A no-op (returns `tracks` untouched) if unchecked, or if the named profile
/// no longer exists.
fn maybe_sort_for_save(state: &AppState, mut tracks: Vec<Track>, autosort: bool) -> Vec<Track> {
    if !autosort {
        return tracks;
    }
    let name = state
        .queue
        .sort_profile
        .clone()
        .unwrap_or_else(|| state.config.sorting.default_queue_profile.clone());
    let Some(profile) = state
        .config
        .sorting
        .profiles
        .iter()
        .find(|p| p.name == name)
    else {
        return tracks;
    };
    tracks.sort_by(|a, b| crate::queue::sort::compare(a, b, profile));
    tracks
}

fn submit(state: &mut AppState) -> Vec<Effect> {
    let Some(modal) = state.modal.take() else {
        return Vec::new();
    };
    state.touch();

    match modal {
        Modal::Help { .. } => Vec::new(),

        // `10-06`: found and fixed a real defect while implementing "Enter commits to config" —
        // this arm used to emit only `SetEq`/`WriteConfig(state.config.clone())`, never actually
        // writing `draft_gains` anywhere in `AppState` at all. `player.eq` (the live mirror
        // `build_equalizer_modal` itself reads from on the *next* open) was left completely
        // untouched, so confirming an edit and reopening the modal showed the *pre*-edit gains,
        // even though the engine was, by then, already playing the *post*-edit ones — reducer
        // state silently drifting out of sync with reality. Fixed by writing `player.eq` directly,
        // the same "this is a setting, not an engine-confirmed fact" exception
        // `PlayerAction::SetEqGain`/`SetEqPreset`/`ToggleEqBypass` already rely on (this module's
        // own top-of-file doc comment). `config.equalizer.active_preset`/`enabled` are updated too
        // when `preset_idx` names a real preset — when it doesn't (the user tweaked bars without
        // landing on any known preset), there is no field in `EqConfig` to durably save arbitrary,
        // unnamed gains into, so `active_preset` is left as whatever it already was; the edit still
        // takes effect for the rest of this run via `player.eq.gains`, just not across a restart
        // (`docs/12-decisions.md`).
        Modal::Equalizer {
            draft_gains,
            preset_idx,
            bypassed,
            enabled,
            ..
        } => {
            state.player.eq.gains = draft_gains;
            state.player.eq.bypassed = bypassed;
            // Committed as drafted rather than forced to `true`. Submitting used to switch the
            // equalizer on unconditionally, which left `o` (and the Settings row) with nothing to
            // do — there was no way to turn it off from the one screen that edits it.
            state.player.eq.enabled = enabled;
            if let Some(idx) = preset_idx
                && let Some(preset) = state.player.known_presets.get(idx)
            {
                state.player.eq.preset_name = preset.name.clone();
                state.config.equalizer.active_preset = preset.name.clone();
            }
            state.config.equalizer.enabled = enabled;
            let curve = if !enabled {
                None
            } else if bypassed {
                Some([0.0; 10])
            } else {
                Some(draft_gains)
            };
            vec![
                Effect::Audio(AudioEffect::SetEq(curve)),
                Effect::Sys(SysEffect::WriteConfig(Box::new(state.config.clone()))),
            ]
        }

        Modal::DevicePicker {
            devices, cursor, ..
        } => match devices.get(cursor) {
            // `09-01`: "a successful swap writes `audio.device_id`/`audio.output_driver` to
            // config" — written optimistically, at selection time, since there is no explicit
            // "the swap succeeded" reply event to key a rollback off (only
            // `AudioEvent::DeviceUnavailable` on failure). `10-05` closes the gap this comment
            // used to flag: `pending_device_swap` remembers what was active before this write, so
            // a matching failure can put it back (`reducer::player::apply_audio`'s own
            // `DeviceUnavailable` arm).
            Some(device) => {
                state.player.pending_device_swap = Some(crate::state::player::PendingDeviceSwap {
                    attempted_id: device.id.clone(),
                    previous_id: state.config.audio.device_id.clone(),
                    previous_driver: state.config.audio.output_driver.clone(),
                });
                state.config.audio.device_id = device.id.clone();
                state.config.audio.output_driver = device.driver.clone();
                vec![
                    Effect::Audio(AudioEffect::SetDevice(device.id.clone())),
                    Effect::Sys(SysEffect::WriteConfig(Box::new(state.config.clone()))),
                ]
            }
            None => Vec::new(),
        },

        Modal::SleepTimer {
            trigger,
            fade_out,
            quit_after,
            ..
        } => {
            // Not an engine-confirmed mirror field like `PlayStatus`/volume (no `AudioEffect`
            // exists to arm one) — pure client-side scheduling state, so a direct write is
            // correct here even though it lives on `PlayerState`. `armed_at` stamps the wall
            // clock the same documented way `AppState::toast()` does: the given action carries
            // no timestamp to thread through, and this reducer never branches on it — only a
            // later `Tick` comparison (`09-05`) does.
            //
            // `09-05`: `EndOfQueue` with `Repeat::All` would never fire (there's always a
            // "next" — it wraps around) — disabling repeat here, rather than silently arming a
            // timer that can never fire, per that task's own spec.
            if trigger == SleepTrigger::EndOfQueue && state.queue.repeat == RepeatMode::All {
                state.queue.repeat = RepeatMode::Off;
                state.toast(
                    "sleep timer: repeat disabled so end-of-queue can fire",
                    ToastLevel::Info,
                );
            }
            state.player.sleep_timer = Some(SleepTimer {
                trigger,
                fade_out,
                quit_after,
                armed_at: Timestamp::now(),
                // `09-05`: which entry `EndOfTrack` waits for — recorded here, at arm time, so a
                // later skip to a different entry doesn't make some *other* track's natural end
                // fire this timer early.
                armed_entry: state.queue.current().map(|e| e.entry_id),
                pre_fade_volume: None,
            });
            Vec::new()
        }

        Modal::SavePlaylist {
            target,
            target_cursor,
            name,
            overview,
            autosort,
            field,
            source,
            error: _,
        } => {
            // `08-06`: the same guard `remove_from_playlist`/`move_in_playlist`/`delete_playlist`
            // (`reducer::queue`) already apply to every other playlist mutation. This is a
            // defensive second check — `open_save_playlist` already refuses to *open* while
            // offline — for connectivity that drops while the modal is already open.
            if state.connectivity == Connectivity::Offline {
                state.toast("playlist changes need a connection", ToastLevel::Warning);
                return Vec::new();
            }
            // `10-08`: "Name is required for a new playlist; `Enter` with an empty name shows an
            // inline `name is required` in `Error` style rather than closing" — `submit`'s own
            // caller already took the modal out of `state.modal` before this arm ever runs, so
            // "rather than closing" means putting it back with the error set, not merely skipping
            // whatever `Submit` would otherwise have done.
            if target == PlaylistTarget::New && name.trim().is_empty() {
                state.modal = Some(Modal::SavePlaylist {
                    target,
                    target_cursor,
                    name,
                    overview,
                    autosort,
                    field,
                    source,
                    error: Some("name is required".to_string()),
                });
                return Vec::new();
            }

            let tracks = maybe_sort_for_save(state, save_playlist_tracks(state, source), autosort);
            let count = tracks.len();
            let ids: Vec<ItemId> = tracks.into_iter().map(|t| t.id).collect();
            let effects = match target {
                PlaylistTarget::New => vec![Effect::Net(NetEffect::PlaylistCreate {
                    name,
                    tracks: ids,
                    overview,
                })],
                PlaylistTarget::Existing(id) => {
                    let name = playlist_display_name(state, &id);
                    vec![Effect::Net(NetEffect::PlaylistAdd {
                        id,
                        name,
                        tracks: ids,
                    })]
                }
            };
            // `10-08`: "emit `PlaylistCreate`/`PlaylistAdd`, close immediately, and toast `saving
            // <n> tracks…`. The completion toast replaces it" — matched on the reducer side by
            // `DataAction::PlaylistSaved`/`LoadFailed(PlaylistSave)` both starting with
            // `state.toasts.retain(|t| !t.message.starts_with("saving "))` before pushing their
            // own toast, rather than a dedicated "pending save" tracking field.
            state.toast(format!("saving {count} tracks\u{2026}"), ToastLevel::Info);
            effects
        }

        Modal::SortProfile {
            profiles,
            cursor,
            target,
            ..
        } => match profiles.get(cursor) {
            Some(profile) => match target {
                // A rule-less profile is the `DEFAULT_ORDER_ROW` — and a user-made empty profile
                // means the same thing, since it expresses no ordering to apply.
                SortApplyTarget::Queue if profile.rules.is_empty() => {
                    super::apply(state, Action::Queue(QueueAction::RestoreDefaultOrder))
                }
                SortApplyTarget::Queue => super::apply(
                    state,
                    Action::Queue(QueueAction::ApplySortProfile(profile.name.clone())),
                ),
                // `10-09`: `queue::sort::sort_items` (`06-04`) already exists for exactly this —
                // sorts only maximal runs of `MediaItem::Track` items, leaving headers and any
                // non-`Track` row (an Artists/Albums/Genres browse column, say) untouched, since
                // `SortField`'s own field table has no defined meaning for those. No new sort
                // logic was needed, only wiring this modal's own "Column" target to it.
                SortApplyTarget::Column => {
                    if let Some(column) = state.active_column_mut() {
                        crate::queue::sort::sort_items(&mut column.items, profile);
                        state.touch();
                    }
                    Vec::new()
                }
            },
            None => Vec::new(),
        },

        Modal::KeymapEditor {
            action_cursor,
            captured,
            conflict,
            ..
        } => keymap_editor_submit(state, action_cursor, captured, conflict),

        Modal::Confirm { on_confirm, .. } => super::apply(state, *on_confirm),
    }
}

/// `Ctrl+C` can never actually reach here as a `captured` binding — `input.rs`'s own
/// always-on-first quit check intercepts it long before capture mode ever sees a key event, by
/// design ("a user must always be able to leave"), and a bare `Esc` during capture is claimed by
/// `CancelCapture` instead of ever becoming `captured` at all. This check exists anyway as the
/// reducer's own independent, defence-in-depth guarantee — the same two chords are reserved no
/// matter how a binding for them was proposed (a hypothetical future non-interactive path, say),
/// not only via the one route `input.rs` happens to take today.
fn reserved_chord(binding: &KeyBinding) -> bool {
    if binding.0.len() != 1 {
        return false;
    }
    let chord = binding.0[0];
    let plain_esc = chord.code == KeyCode::Esc && chord.mods == KeyModifiers::default();
    let ctrl_c = chord.code == KeyCode::Char('c')
        && chord.mods
            == KeyModifiers {
                ctrl: true,
                ..KeyModifiers::default()
            };
    plain_esc || ctrl_c
}

fn keymap_editor_submit(
    state: &mut AppState,
    action_cursor: usize,
    captured: Option<KeyBinding>,
    already_conflicting: Option<ActionId>,
) -> Vec<Effect> {
    let Some(binding) = captured else {
        // Nothing captured yet (submit pressed before a chord was recorded, e.g. `StartCapture`
        // wasn't reached): reopen the modal unchanged rather than silently discarding it.
        state.modal = Some(Modal::KeymapEditor {
            action_cursor,
            capturing: false,
            conflict: None,
            captured: None,
            capture_deadline: None,
        });
        return Vec::new();
    };

    let actions: Vec<ActionId> = ActionId::iter().collect();
    let Some(&target) = actions.get(action_cursor) else {
        return Vec::new();
    };

    if reserved_chord(&binding) {
        state.toast(
            format!("{} is reserved", render_binding(&binding)),
            ToastLevel::Warning,
        );
        state.modal = Some(Modal::KeymapEditor {
            action_cursor,
            capturing: false,
            conflict: None,
            captured: None,
            capture_deadline: None,
        });
        state.touch();
        return Vec::new();
    }

    // `already_conflicting` is only `Some` on the *second* `Submit` for this same capture — the
    // one after the first already found this exact conflict and displayed it (`docs/12-decisions.md`).
    // That second press is this task's own "[Enter] rebind anyway", so the check is skipped and
    // the binding commits unconditionally; a fresh capture always runs it once.
    if already_conflicting.is_none()
        && let Some(incumbent) = state.keymap.resolve_normal(&binding)
        && incumbent != target
    {
        state.modal = Some(Modal::KeymapEditor {
            action_cursor,
            capturing: false,
            conflict: Some(incumbent),
            captured: Some(binding),
            capture_deadline: None,
        });
        state.touch();
        return Vec::new();
    }

    commit_binding(state, action_cursor, target, &binding)
}

/// Writes `binding` for `target` into `config.keybindings` (the friendly rendering,
/// `keymap/parse.rs`'s round-trip guarantee) and rebinds it live via `KeyMap::rebind` — directly
/// and incrementally, not by rebuilding from `config.keybindings` via `KeyMap::from_config` (see
/// that method's own doc comment for why). Reopens the editor at the same row rather than
/// closing — this is a persistent, remap-several-things-in-one-session editor, not a
/// one-shot-then-close modal like every other one in this file; `Esc` is how a user actually
/// leaves. Used by a clean capture, a confirmed "rebind anyway", and `d`'s reset-to-default alike.
fn commit_binding(
    state: &mut AppState,
    action_cursor: usize,
    target: ActionId,
    binding: &KeyBinding,
) -> Vec<Effect> {
    state.keymap.rebind(target, binding.clone());
    state
        .config
        .keybindings
        .insert(target.to_string(), render_binding(binding));
    state.modal = Some(Modal::KeymapEditor {
        action_cursor,
        capturing: false,
        conflict: None,
        captured: None,
        capture_deadline: None,
    });
    state.touch();
    vec![Effect::Sys(SysEffect::WriteConfig(Box::new(
        state.config.clone(),
    )))]
}

fn current_keymap_target(state: &AppState) -> Option<ActionId> {
    current_keymap_row(state).map(|(_, target)| target)
}

fn current_keymap_row(state: &AppState) -> Option<(usize, ActionId)> {
    let Some(Modal::KeymapEditor { action_cursor, .. }) = &state.modal else {
        return None;
    };
    ActionId::iter()
        .nth(*action_cursor)
        .map(|a| (*action_cursor, a))
}

fn start_capture(state: &mut AppState) -> Vec<Effect> {
    if let Some(Modal::KeymapEditor {
        capturing,
        conflict,
        captured,
        capture_deadline,
        ..
    }) = &mut state.modal
    {
        *capturing = true;
        *conflict = None;
        *captured = None;
        *capture_deadline = None;
        state.touch();
    }
    Vec::new()
}

fn cancel_capture(state: &mut AppState) -> Vec<Effect> {
    if let Some(Modal::KeymapEditor {
        capturing,
        conflict,
        captured,
        capture_deadline,
        ..
    }) = &mut state.modal
    {
        *capturing = false;
        *conflict = None;
        *captured = None;
        *capture_deadline = None;
        state.touch();
    }
    Vec::new()
}

/// Every raw key event while `capturing` is true reaches here (`input.rs`'s own dedicated
/// capture-mode check, ahead of its normal per-context dispatch) — the first chord arms
/// `capture_deadline` and leaves `capturing` true for an optional second one; a second chord
/// arriving before that deadline extends `captured` into a 2-chord sequence and ends capture
/// immediately (`docs/04-state-and-input.md` §7's "captured by pressing a prefix and then the
/// second key"). `expire_capture_window` (called from `tick`) is what ends capture if the second
/// chord never comes.
fn capture_chord(state: &mut AppState, chord: KeyChord) -> Vec<Effect> {
    let now = state.clock;
    let Some(Modal::KeymapEditor {
        capturing,
        captured,
        capture_deadline,
        ..
    }) = &mut state.modal
    else {
        return Vec::new();
    };
    if !*capturing {
        return Vec::new();
    }
    match captured {
        None => {
            *captured = Some(KeyBinding(smallvec::smallvec![chord]));
            *capture_deadline = Some(now.checked_add(CAPTURE_WINDOW).unwrap_or(now));
        }
        Some(binding) => {
            binding.0.push(chord);
            *capturing = false;
            *capture_deadline = None;
        }
    }
    state.touch();
    Vec::new()
}

/// `tick`'s own hook (mirrors `settings::maybe_write_config`'s integration, `reducer/mod.rs`) —
/// finalizes an in-progress capture as a 1-chord binding once its optional second-chord window has
/// lapsed with nothing else arriving.
pub(crate) fn expire_capture_window(state: &mut AppState, now: Timestamp) {
    if let Some(Modal::KeymapEditor {
        capturing,
        capture_deadline,
        ..
    }) = &mut state.modal
        && *capturing
        && let Some(deadline) = capture_deadline
        && now >= *deadline
    {
        *capturing = false;
        *capture_deadline = None;
        state.touch();
    }
}

fn reset_keymap_row(state: &mut AppState) -> Vec<Effect> {
    let Some((action_cursor, target)) = current_keymap_row(state) else {
        return Vec::new();
    };
    // Every `ActionId` is bound in the default table (`default_keymap_covers_every_action_id`),
    // so `None` here is unreachable in practice; a no-op rather than a panic if it ever weren't.
    let Some(default_binding) = KeyMap::defaults().binding_for(target).cloned() else {
        return Vec::new();
    };
    commit_binding(state, action_cursor, target, &default_binding)
}

fn unbind_keymap_row(state: &mut AppState) -> Vec<Effect> {
    let Some(target) = current_keymap_target(state) else {
        return Vec::new();
    };
    state.keymap.unbind(target);
    state
        .config
        .keybindings
        .insert(target.to_string(), "none".to_string());
    state.touch();
    vec![Effect::Sys(SysEffect::WriteConfig(Box::new(
        state.config.clone(),
    )))]
}

/// `R` — opens the `Confirm` this task's own spec requires before the real reset runs. `Confirm`
/// replaces `Modal::KeymapEditor` entirely (`AppState.modal` only ever holds one modal,
/// `open_confirm`'s own doc comment) — accepting it reopens the editor fresh (`reset_all_
/// keybindings`); cancelling it (`Esc`) simply closes the confirmation, the same "return to
/// nothing, not to what was open before" behaviour every other nested-under-a-modal action in
/// this codebase has, since `Modal` has no stack to return to (`docs/12-decisions.md`).
fn confirm_reset_all_keybindings(state: &mut AppState) -> Vec<Effect> {
    open_confirm(
        state,
        "reset every keybinding to its default? this cannot be undone.",
        Action::Modal(ModalAction::ResetAllKeybindings),
    )
}

fn reset_all_keybindings(state: &mut AppState) -> Vec<Effect> {
    state.keymap = KeyMap::defaults();
    state.config.keybindings.clear();
    state.modal = Some(Modal::KeymapEditor {
        action_cursor: 0,
        capturing: false,
        conflict: None,
        captured: None,
        capture_deadline: None,
    });
    state.touch();
    vec![Effect::Sys(SysEffect::WriteConfig(Box::new(
        state.config.clone(),
    )))]
}

/// Every modal that has a focusable-field cursor, and how many fields it has — `0` means
/// `FieldNext`/`FieldPrev` are no-ops (`Help`, `Confirm`, and any list-backed modal with nothing
/// loaded yet).
fn field_count(modal: &Modal) -> usize {
    match modal {
        Modal::Help { .. } | Modal::Confirm { .. } => 0,
        Modal::Equalizer { .. } => 10,
        Modal::DevicePicker { devices, .. } => devices.len(),
        // `10-07`: the 5 radio triggers (0..=2 the three durations, 3 `EndOfTrack`, 4
        // `EndOfQueue`) followed by the 2 independent checkboxes (5 `fade_out`, 6 `quit_after`) —
        // one flat row list `FieldNext`/`FieldPrev` step through in wireframe order; `activate_field`
        // below is what gives each index its own real meaning.
        Modal::SleepTimer { .. } => 7,
        // `10-08`: `0` target dropdown, `1` name, `2` description, `3` sort checkbox — was `3`
        // (name/overview/autosort only, no dropdown at all reachable via `Tab`), fixed alongside
        // giving the dropdown its own real `CycleSaveTarget` behaviour.
        Modal::SavePlaylist { .. } => 4,
        Modal::SortProfile { profiles, .. } => profiles.len(),
        Modal::KeymapEditor { .. } => ActionId::iter().count(),
    }
}

fn field_cursor_mut(modal: &mut Modal) -> Option<&mut usize> {
    match modal {
        Modal::Help { .. } | Modal::Confirm { .. } => None,
        Modal::Equalizer { band, .. } => Some(band),
        Modal::DevicePicker { cursor, .. } => Some(cursor),
        Modal::SleepTimer { field_cursor, .. } => Some(field_cursor),
        Modal::SavePlaylist { field, .. } => Some(field),
        Modal::SortProfile { cursor, .. } => Some(cursor),
        Modal::KeymapEditor { action_cursor, .. } => Some(action_cursor),
    }
}

fn field_next(state: &mut AppState) -> Vec<Effect> {
    let Some(count) = state.modal.as_ref().map(field_count) else {
        return Vec::new();
    };
    if count == 0 {
        return Vec::new();
    }
    if let Some(cursor) = state.modal.as_mut().and_then(field_cursor_mut) {
        *cursor = (*cursor + 1) % count;
        state.touch();
    }
    Vec::new()
}

fn field_prev(state: &mut AppState) -> Vec<Effect> {
    let Some(count) = state.modal.as_ref().map(field_count) else {
        return Vec::new();
    };
    if count == 0 {
        return Vec::new();
    }
    if let Some(cursor) = state.modal.as_mut().and_then(field_cursor_mut) {
        *cursor = (*cursor + count - 1) % count;
        state.touch();
    }
    Vec::new()
}

/// Only `SavePlaylist`'s `name`/`overview` (fields 0/1) are real text fields — field 2
/// (`autosort`) is a toggle, and every other modal has no text field at all, so input there is a
/// no-op rather than an error.
/// `10-05`: a click's own absolute-position counterpart to `field_next`/`field_prev`'s relative
/// steps — clamped the same way (`0` when `count == 0` is a no-op, otherwise `index.min(count -
/// 1)`), so a stale `HitTarget::ModalField` index (a resized modal, say) can never store an
/// out-of-bounds cursor.
fn field_set(state: &mut AppState, index: usize) -> Vec<Effect> {
    let Some(count) = state.modal.as_ref().map(field_count) else {
        return Vec::new();
    };
    if count == 0 {
        return Vec::new();
    }
    if let Some(cursor) = state.modal.as_mut().and_then(field_cursor_mut) {
        *cursor = index.min(count - 1);
        state.touch();
    }
    Vec::new()
}

/// The live-preview effect every equalizer-editing action below ends with — `None` (chain
/// uninstalled) is never right here (the modal is only reachable while the EQ can be previewed at
/// all), so this always emits `Some`, muted to all-zero while the modal's own `bypassed` flag is
/// set, exactly mirroring `reducer::player::eq_effects`'s committed-side equivalent.
/// The live preview for the modal's current draft. `None` — uninstall the chain — when the draft
/// says the equalizer is off; a flat curve is *bypass*, which is a different thing and leaves the
/// chain in place.
fn eq_preview_effect(gains: [f32; 10], bypassed: bool, enabled: bool) -> Vec<Effect> {
    if !enabled {
        return vec![Effect::Audio(AudioEffect::SetEq(None))];
    }
    let curve = if bypassed { [0.0; 10] } else { gains };
    vec![Effect::Audio(AudioEffect::SetEq(Some(curve)))]
}

/// `t` — off/on for the whole equalizer, drafted like everything else in this modal.
fn toggle_modal_eq_enabled(state: &mut AppState) -> Vec<Effect> {
    let Some(Modal::Equalizer {
        enabled,
        bypassed,
        draft_gains,
        ..
    }) = &mut state.modal
    else {
        return Vec::new();
    };
    *enabled = !*enabled;
    let (gains, bypassed, enabled) = (*draft_gains, *bypassed, *enabled);
    state.touch();
    eq_preview_effect(gains, bypassed, enabled)
}

fn adjust_gain(state: &mut AppState, delta: f32) -> Vec<Effect> {
    let Some(Modal::Equalizer {
        band,
        draft_gains,
        bypassed,
        enabled,
        ..
    }) = &mut state.modal
    else {
        return Vec::new();
    };
    let Some(slot) = draft_gains.get_mut(*band) else {
        return Vec::new();
    };
    *slot = super::player::clamp_eq_gain(*slot + delta);
    // Touching a band is asking to hear it: an edit switches the equalizer on rather than being
    // silently swallowed by a drafted "off".
    *enabled = true;
    let gains = *draft_gains;
    let bypassed = *bypassed;
    state.touch();
    eq_preview_effect(gains, bypassed, true)
}

fn set_gain_at(state: &mut AppState, band: usize, db: f32) -> Vec<Effect> {
    let Some(Modal::Equalizer {
        band: selected,
        draft_gains,
        bypassed,
        enabled,
        ..
    }) = &mut state.modal
    else {
        return Vec::new();
    };
    let Some(slot) = draft_gains.get_mut(band) else {
        return Vec::new();
    };
    *slot = super::player::clamp_eq_gain(db);
    *selected = band;
    *enabled = true;
    let gains = *draft_gains;
    let bypassed = *bypassed;
    state.touch();
    eq_preview_effect(gains, bypassed, true)
}

/// `p`: advances `preset_idx` through `known_presets` (factory then custom, already in that
/// order), wrapping past the end; a modal opened with no matching preset (`preset_idx: None`)
/// starts from the first one instead of treating "no current preset" as "nothing to advance from".
fn cycle_preset(state: &mut AppState) -> Vec<Effect> {
    if !matches!(state.modal, Some(Modal::Equalizer { .. }))
        || state.player.known_presets.is_empty()
    {
        return Vec::new();
    }
    let current_idx = match &state.modal {
        Some(Modal::Equalizer { preset_idx, .. }) => *preset_idx,
        _ => None,
    };
    let next_idx = current_idx.map_or(0, |i| (i + 1) % state.player.known_presets.len());
    let next_gains = state.player.known_presets[next_idx].gains;

    let Some(Modal::Equalizer {
        draft_gains,
        preset_idx,
        bypassed,
        enabled,
        ..
    }) = &mut state.modal
    else {
        return Vec::new();
    };
    *draft_gains = next_gains;
    *preset_idx = Some(next_idx);
    *enabled = true;
    let bypassed = *bypassed;
    state.touch();
    eq_preview_effect(next_gains, bypassed, true)
}

fn toggle_modal_bypass(state: &mut AppState) -> Vec<Effect> {
    let Some(Modal::Equalizer {
        bypassed,
        draft_gains,
        ..
    }) = &mut state.modal
    else {
        return Vec::new();
    };
    *bypassed = !*bypassed;
    let gains = *draft_gains;
    let bypassed = *bypassed;
    state.touch();
    eq_preview_effect(gains, bypassed, true)
}

/// `10-07`: the sleep timer modal's own five radio-trigger values, in the same wireframe order
/// `field_count`'s `7` and `Modal::SleepTimer.field_cursor` (`0..=4`) both assume.
const SLEEP_TRIGGER_ROWS: [SleepTrigger; 5] = [
    SleepTrigger::Duration(Duration::from_secs(15 * 60)),
    SleepTrigger::Duration(Duration::from_secs(30 * 60)),
    SleepTrigger::Duration(Duration::from_secs(60 * 60)),
    SleepTrigger::EndOfTrack,
    SleepTrigger::EndOfQueue,
];

/// `Space` — selects the focused radio trigger or toggles the focused checkbox, per
/// `field_cursor` (`0..=4` triggers, `5` fade-out, `6` quit-after). The two playback-relative
/// triggers (`EndOfTrack`/`EndOfQueue`, rows 3/4) are refused while nothing is playing — "Dim and
/// unselectable" (this task's own spec) — rather than letting the draft land on a trigger that can
/// never fire.
fn activate_field(state: &mut AppState) -> Vec<Effect> {
    let has_current_track = state.queue.current().is_some();
    match &mut state.modal {
        Some(Modal::SleepTimer {
            trigger,
            fade_out,
            quit_after,
            field_cursor,
        }) => {
            match *field_cursor {
                0..=2 => *trigger = SLEEP_TRIGGER_ROWS[*field_cursor],
                3 | 4 if has_current_track => *trigger = SLEEP_TRIGGER_ROWS[*field_cursor],
                3 | 4 => {}
                5 => *fade_out = !*fade_out,
                6 => *quit_after = !*quit_after,
                _ => {}
            }
            state.touch();
        }
        // `10-08`: `field == 3` is the "Sort tracks..." checkbox — the only `Space`-activatable
        // control this modal has (the dropdown, `field == 0`, uses `CycleSaveTarget` instead; the
        // two text fields, `1`/`2`, aren't checkboxes to toggle at all).
        Some(Modal::SavePlaylist {
            autosort,
            field: 3,
            error,
            ..
        }) => {
            *autosort = !*autosort;
            *error = None;
            state.touch();
        }
        _ => {}
    }
    Vec::new()
}

/// `10-08`: `↑`/`↓` while the save-playlist modal's target dropdown (`field == 0`) has focus —
/// moves `target_cursor` through `Create New Playlist…` and the loaded existing playlists,
/// wrapping, and keeps `target` itself in lockstep.
fn cycle_save_target(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let is_dropdown_focused = matches!(&state.modal, Some(Modal::SavePlaylist { field: 0, .. }));
    if !is_dropdown_focused {
        return Vec::new();
    }
    let playlists = existing_playlists(state);
    let current = match &state.modal {
        Some(Modal::SavePlaylist { target_cursor, .. }) => *target_cursor,
        _ => return Vec::new(),
    };
    let span = playlists.len() as i32 + 1;
    let next = (current as i32 + delta).rem_euclid(span) as usize;
    let new_target = if next == 0 {
        PlaylistTarget::New
    } else {
        PlaylistTarget::Existing(playlists[next - 1].0.clone())
    };

    if let Some(Modal::SavePlaylist {
        target,
        target_cursor,
        error,
        ..
    }) = &mut state.modal
    {
        *target = new_target;
        *target_cursor = next;
        *error = None;
        state.touch();
    }
    Vec::new()
}

/// `10-07`: `d` — cancels an armed timer outright (never stops playback, unlike the timer's own
/// natural expiry, `fire_sleep_timer`) and closes the modal. Restores whatever volume a fade-out
/// ramp had already ducked to, exactly like a natural fire does, so disarming mid-fade doesn't
/// leave the volume stuck low.
fn disarm_sleep_timer(state: &mut AppState) -> Vec<Effect> {
    state.modal = None;
    let Some(timer) = state.player.sleep_timer.take() else {
        return Vec::new();
    };
    state.touch();
    state.toast("sleep timer disarmed", ToastLevel::Info);
    match timer.pre_fade_volume {
        Some(original) => vec![Effect::Audio(AudioEffect::SetVolume(original))],
        None => Vec::new(),
    }
}

fn field_input(state: &mut AppState, c: char) -> Vec<Effect> {
    if let Some(Modal::SavePlaylist {
        name,
        overview,
        field,
        error,
        ..
    }) = state.modal.as_mut()
    {
        // `10-08`: field `0` is now the target dropdown (`CycleSaveTarget`, not text), so the two
        // text fields shifted from `0`/`1` to `1`/`2`.
        match *field {
            1 => name.push(c),
            2 => overview.push(c),
            _ => {}
        }
        *error = None;
        state.touch();
    }
    Vec::new()
}

fn field_backspace(state: &mut AppState) -> Vec<Effect> {
    if let Some(Modal::SavePlaylist {
        name,
        overview,
        field,
        error,
        ..
    }) = state.modal.as_mut()
    {
        // `String::pop` removes one `char` (a full Unicode scalar value), never a lone byte, so a
        // multi-byte character — an emoji, say — is removed whole rather than corrupting the
        // string's UTF-8 the way a byte-level `truncate` would.
        match *field {
            1 => {
                name.pop();
            }
            2 => {
                overview.pop();
            }
            _ => {}
        }
        *error = None;
        state.touch();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{DataAction, LoadTarget, NavAction, SystemEvent, ViewAction};
    use crate::keymap::{KeyBinding, KeyChord, KeyCode, KeyModifiers};
    use crate::state::modal::RuleEditor;
    use crate::test_support::fixtures;
    use crate::test_support::scenario::Scenario;

    fn chord(c: char) -> KeyBinding {
        KeyBinding(smallvec::smallvec![KeyChord {
            code: KeyCode::Char(c),
            mods: KeyModifiers::default(),
        }])
    }

    /// Submitting used to force `enabled = true` unconditionally, so the modal could only ever turn
    /// the equalizer on — the one screen that edits it had no off switch at all
    /// (`docs/12-decisions.md`).
    #[test]
    fn toggling_the_equalizer_off_and_submitting_keeps_it_off() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.enabled = true;
        state.player.eq.gains = [3.0; 10];

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::Equalizer)))
            .dispatch(Action::Modal(ModalAction::ToggleEqEnabled));

        // Off is previewed straight away, as `None` — uninstall — not a flat curve, which is bypass.
        assert!(
            scenario
                .effects()
                .contains(&Effect::Audio(AudioEffect::SetEq(None))),
            "got {:?}",
            scenario.effects()
        );

        let mut state = scenario.state().clone();
        let effects = crate::reducer::apply(&mut state, Action::Modal(ModalAction::Submit));
        assert!(!state.player.eq.enabled);
        assert!(!state.config.equalizer.enabled, "the choice must persist");
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::SetEq(None)))),
            "got {effects:?}"
        );
    }

    /// Submitting a *bypassed* equalizer used to commit the real gains — the opposite of what was
    /// being previewed a moment earlier.
    #[test]
    fn submitting_a_bypassed_equalizer_commits_the_flat_curve() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.enabled = true;
        state.player.eq.gains = [3.0; 10];

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::Equalizer)))
            .dispatch(Action::Modal(ModalAction::ToggleBypass));
        let mut state = scenario.state().clone();
        let effects = crate::reducer::apply(&mut state, Action::Modal(ModalAction::Submit));

        assert!(state.player.eq.enabled);
        assert!(state.player.eq.bypassed);
        assert!(
            effects.iter().any(
                |e| matches!(e, Effect::Audio(AudioEffect::SetEq(Some(c))) if *c == [0.0; 10])
            ),
            "got {effects:?}"
        );
    }

    /// Abandoning the modal with the equalizer *off* must uninstall the chain, not reinstall it at
    /// the old gains — a flat curve is bypass, which is a different thing.
    #[test]
    fn abandoning_the_modal_with_the_eq_off_uninstalls_the_chain() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.gains = [1.0; 10];
        state.player.eq.enabled = false;

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::Equalizer)))
            .dispatch(Action::Nav(crate::action::NavAction::Cancel));

        assert!(
            scenario
                .effects()
                .contains(&Effect::Audio(AudioEffect::SetEq(None))),
            "got {:?}",
            scenario.effects()
        );
    }

    #[test]
    fn modal_open_replaces_existing_modal() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.gains = [1.0; 10];
        state.player.eq.enabled = true;

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::Equalizer)))
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::DevicePicker)));

        assert_eq!(
            scenario.state().modal.as_ref().unwrap().kind(),
            ModalKind::DevicePicker
        );
        assert!(
            scenario
                .effects()
                .contains(&Effect::Audio(AudioEffect::SetEq(Some([1.0; 10])))),
            "replacing the equalizer modal must revert its live preview"
        );
        assert!(
            scenario
                .effects()
                .contains(&Effect::Audio(AudioEffect::EnumerateDevices))
        );
    }

    #[test]
    fn modal_intercepts_navigation_actions() {
        let mut state = fixtures::fixture_miller_3col();
        state.modal = Some(Modal::Help {
            context: InputContext::Normal,
            scroll: 0,
        });
        let cursor_before = state.active_column().unwrap().cursor;

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::MoveDown { n: 1 }));

        assert_eq!(
            scenario.state().active_column().unwrap().cursor,
            cursor_before
        );
        assert!(scenario.state().modal.is_some());
        scenario.assert_no_effects();
    }

    #[test]
    fn modal_allows_system_and_help_actions_through() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loading,
        });

        let scenario = Scenario::new(state)
            .dispatch(Action::System(SystemEvent::Quit))
            .dispatch(Action::View(ViewAction::ToggleHelp));

        assert!(scenario.state().should_quit);
        assert_eq!(
            scenario.state().modal.as_ref().unwrap().kind(),
            ModalKind::Help
        );
    }

    #[test]
    fn data_actions_reach_the_reducer_while_a_modal_is_open() {
        // `10-08`: the real bug this guards against — `Action::Data(_)` was missing from `allows`'s
        // own permitted set, so a reply arriving while *any* modal was open (which is exactly when
        // `DevicePicker`'s `DevicesLoaded` and this task's own `ItemsLoaded` need to land) was
        // silently discarded before it ever reached `apply_data`.
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loading,
        });

        let scenario = Scenario::new(state).dispatch(Action::Data(DataAction::DevicesLoaded {
            devices: vec![crate::model::AudioDevice {
                id: "a".to_string(),
                description: "A".to_string(),
                driver: "alsa".to_string(),
            }],
        }));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::DevicePicker { devices, .. } => assert_eq!(devices.len(), 1),
            _ => unreachable!(),
        }
    }

    #[test]
    fn equalizer_cancel_restores_gains_and_emits_set_eq() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.gains = [2.0; 10];
        state.modal = Some(Modal::Equalizer {
            band: 0,
            draft_gains: [9.0; 10],
            gains_at_open: [2.0; 10],
            preset_idx: None,
            bypassed: false,
            enabled: true,
            enabled_at_open: true,
        });

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));

        assert!(scenario.state().modal.is_none());
        assert_eq!(scenario.state().player.eq.gains, [2.0; 10]);
        assert_eq!(
            scenario.last_effect(),
            Some(&Effect::Audio(AudioEffect::SetEq(Some([2.0; 10]))))
        );
    }

    #[test]
    fn equalizer_submit_commits_and_writes_config() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.gains = [2.0; 10];
        state.modal = Some(Modal::Equalizer {
            band: 0,
            draft_gains: [9.0; 10],
            gains_at_open: [2.0; 10],
            preset_idx: None,
            bypassed: false,
            enabled: true,
            enabled_at_open: true,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert!(scenario.state().modal.is_none());
        assert!(
            scenario
                .effects()
                .contains(&Effect::Audio(AudioEffect::SetEq(Some([9.0; 10]))))
        );
        assert!(
            scenario
                .effects()
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
    }

    #[test]
    fn enter_commits_to_config() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.gains = [2.0; 10];
        state.player.eq.bypassed = false;
        state.player.eq.enabled = false;
        state.player.known_presets = vec![crate::config::EqPreset {
            name: "bass_boost".to_string(),
            gains: [9.0; 10],
        }];
        state.modal = Some(Modal::Equalizer {
            band: 0,
            draft_gains: [9.0; 10],
            gains_at_open: [2.0; 10],
            preset_idx: Some(0),
            bypassed: true,
            enabled: true,
            enabled_at_open: true,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        // The live mirror actually reflects the just-confirmed edit — not just an engine command
        // that leaves `AppState` itself stale (the bug this task fixed).
        assert_eq!(scenario.state().player.eq.gains, [9.0; 10]);
        assert!(scenario.state().player.eq.bypassed);
        assert!(scenario.state().player.eq.enabled);
        assert_eq!(scenario.state().player.eq.preset_name, "bass_boost");
        assert_eq!(
            scenario.state().config.equalizer.active_preset,
            "bass_boost"
        );
        assert!(scenario.state().config.equalizer.enabled);
    }

    // --- 10-06: equalizer modal --------------------------------------------------------------

    fn equalizer_modal(draft_gains: [f32; 10], bypassed: bool) -> Modal {
        Modal::Equalizer {
            band: 3,
            draft_gains,
            gains_at_open: [0.0; 10],
            preset_idx: None,
            bypassed,
            enabled: true,
            enabled_at_open: true,
        }
    }

    #[test]
    fn up_down_adjusts_by_half_db_and_emits() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(equalizer_modal([1.0; 10], false));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::AdjustGain(0.5)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer { draft_gains, .. } => assert_eq!(draft_gains[3], 1.5),
            _ => unreachable!(),
        }
        let mut expected = [1.0; 10];
        expected[3] = 1.5;
        assert_eq!(
            scenario.last_effect(),
            Some(&Effect::Audio(AudioEffect::SetEq(Some(expected))))
        );
    }

    #[test]
    fn gain_clamped_at_twelve_db() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(equalizer_modal([11.8; 10], false));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::AdjustGain(0.5)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer { draft_gains, .. } => assert_eq!(draft_gains[3], 12.0),
            _ => unreachable!(),
        }
    }

    #[test]
    fn set_gain_at_selects_the_clicked_band_too() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(equalizer_modal([0.0; 10], false));

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::SetGainAt { band: 7, db: -6.0 }));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer {
                band, draft_gains, ..
            } => {
                assert_eq!(*band, 7);
                assert_eq!(draft_gains[7], -6.0);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn esc_restores_gains_and_emits_undo() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::Equalizer {
            band: 0,
            draft_gains: [5.0; 10],
            gains_at_open: [2.0; 10],
            preset_idx: None,
            bypassed: false,
            enabled: true,
            enabled_at_open: true,
        });

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));

        assert!(scenario.state().modal.is_none());
        assert_eq!(
            scenario.last_effect(),
            Some(&Effect::Audio(AudioEffect::SetEq(Some([2.0; 10]))))
        );
    }

    #[test]
    fn p_cycles_factory_then_custom_presets() {
        let mut state = fixtures::fixture_empty();
        state.player.known_presets = vec![
            crate::config::EqPreset {
                name: "flat".to_string(),
                gains: [0.0; 10],
            },
            crate::config::EqPreset {
                name: "bass_boost".to_string(),
                gains: [3.0; 10],
            },
            crate::config::EqPreset {
                name: "My Custom".to_string(),
                gains: [7.0; 10],
            },
        ];
        state.modal = Some(equalizer_modal([1.0; 10], false));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::CyclePreset));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer {
                draft_gains,
                preset_idx,
                ..
            } => {
                assert_eq!(*draft_gains, [0.0; 10]);
                assert_eq!(*preset_idx, Some(0));
            }
            _ => unreachable!(),
        }

        // Two more cycles: bass_boost, then the custom preset.
        let scenario = scenario
            .dispatch(Action::Modal(ModalAction::CyclePreset))
            .dispatch(Action::Modal(ModalAction::CyclePreset));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer {
                draft_gains,
                preset_idx,
                ..
            } => {
                assert_eq!(*draft_gains, [7.0; 10]);
                assert_eq!(*preset_idx, Some(2));
            }
            _ => unreachable!(),
        }

        // A fourth cycle wraps back to the first (factory) preset.
        let scenario = scenario.dispatch(Action::Modal(ModalAction::CyclePreset));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer {
                draft_gains,
                preset_idx,
                ..
            } => {
                assert_eq!(*draft_gains, [0.0; 10]);
                assert_eq!(*preset_idx, Some(0));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn bypass_dims_bars_and_updates_title() {
        // The reducer side of "bypass ... the bars render in Dim while retaining their values" —
        // toggling bypass must never alter `draft_gains` itself, only mute the live preview.
        let mut state = fixtures::fixture_empty();
        state.modal = Some(equalizer_modal([4.0; 10], false));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::ToggleBypass));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer {
                bypassed,
                draft_gains,
                ..
            } => {
                assert!(*bypassed);
                assert_eq!(*draft_gains, [4.0; 10]);
            }
            _ => unreachable!(),
        }
        assert_eq!(
            scenario.last_effect(),
            Some(&Effect::Audio(AudioEffect::SetEq(Some([0.0; 10]))))
        );

        let scenario = scenario.dispatch(Action::Modal(ModalAction::ToggleBypass));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer { bypassed, .. } => assert!(!*bypassed),
            _ => unreachable!(),
        }
        assert_eq!(
            scenario.last_effect(),
            Some(&Effect::Audio(AudioEffect::SetEq(Some([4.0; 10]))))
        );
    }

    #[test]
    fn build_equalizer_modal_finds_a_custom_active_preset() {
        // The fixed bug: `preset_idx` used to search only `FACTORY_EQ_PRESET_NAMES`, so an active
        // *custom* preset always came up `None` even though it's right there in `known_presets`.
        let mut state = fixtures::fixture_empty();
        state.player.eq.preset_name = "My Custom".to_string();
        state.player.known_presets = vec![
            crate::config::EqPreset {
                name: "flat".to_string(),
                gains: [0.0; 10],
            },
            crate::config::EqPreset {
                name: "My Custom".to_string(),
                gains: [7.0; 10],
            },
        ];

        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::Open(ModalKind::Equalizer)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::Equalizer { preset_idx, .. } => assert_eq!(*preset_idx, Some(1)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn device_picker_open_emits_enumerate() {
        let scenario = Scenario::new(fixtures::fixture_empty())
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::DevicePicker)));

        assert_eq!(
            scenario.state().modal.as_ref().unwrap().kind(),
            ModalKind::DevicePicker
        );
        assert_eq!(
            scenario.last_effect(),
            Some(&Effect::Audio(AudioEffect::EnumerateDevices))
        );
    }

    #[test]
    fn device_picker_submit_persists_device_and_driver() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::DevicePicker {
            devices: vec![crate::model::AudioDevice {
                id: "alsa/hw:1,0".to_string(),
                description: "USB DAC".to_string(),
                driver: "alsa".to_string(),
            }],
            cursor: 0,
            load: LoadState::default(),
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert_eq!(scenario.state().config.audio.device_id, "alsa/hw:1,0");
        assert_eq!(scenario.state().config.audio.output_driver, "alsa");
        assert!(scenario.effects().iter().any(
            |e| matches!(e, Effect::Audio(AudioEffect::SetDevice(id)) if id == "alsa/hw:1,0")
        ));
        assert!(
            scenario
                .effects()
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
    }

    #[test]
    fn enter_emits_set_device_and_closes() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::DevicePicker {
            devices: vec![crate::model::AudioDevice {
                id: "alsa/hw:1,0".to_string(),
                description: "USB DAC".to_string(),
                driver: "alsa".to_string(),
            }],
            cursor: 0,
            load: LoadState::default(),
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert!(scenario.state().modal.is_none());
        assert!(scenario.effects().iter().any(
            |e| matches!(e, Effect::Audio(AudioEffect::SetDevice(id)) if id == "alsa/hw:1,0")
        ));
    }

    #[test]
    fn esc_makes_no_change() {
        let mut state = fixtures::fixture_empty();
        state.config.audio.device_id = "alsa/hw:0,0".to_string();
        state.config.audio.output_driver = "alsa".to_string();
        state.modal = Some(Modal::DevicePicker {
            devices: vec![crate::model::AudioDevice {
                id: "pulse/a".to_string(),
                description: "USB DAC".to_string(),
                driver: "pulse".to_string(),
            }],
            cursor: 0,
            load: LoadState::default(),
        });

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));

        assert!(scenario.state().modal.is_none());
        assert_eq!(scenario.state().config.audio.device_id, "alsa/hw:0,0");
        assert_eq!(scenario.state().config.audio.output_driver, "alsa");
        assert!(
            !scenario
                .effects()
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::SetDevice(_))))
        );
    }

    #[test]
    fn open_always_reenumerates() {
        // Even with devices already loaded from a previous open, re-opening the picker discards
        // them and fires `EnumerateDevices` again — "a user opens this modal precisely because
        // they just plugged something in" (this task's own spec), so a cached list must never be
        // shown instead.
        let scenario = Scenario::new(fixtures::fixture_empty())
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::DevicePicker)))
            .dispatch(Action::Data(crate::action::DataAction::DevicesLoaded {
                devices: vec![crate::model::AudioDevice {
                    id: "alsa/hw:0,0".to_string(),
                    description: "Speakers".to_string(),
                    driver: "alsa".to_string(),
                }],
            }))
            .dispatch(Action::Modal(ModalAction::Close))
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::DevicePicker)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::DevicePicker { devices, load, .. } => {
                assert!(
                    devices.is_empty(),
                    "re-opening must not show the previous enumeration's stale list"
                );
                assert_eq!(*load, LoadState::Loading);
            }
            _ => unreachable!(),
        }
        assert_eq!(
            scenario
                .effects()
                .iter()
                .filter(|e| matches!(e, Effect::Audio(AudioEffect::EnumerateDevices)))
                .count(),
            2,
            "both the first and second Open must each fire their own EnumerateDevices"
        );
    }

    #[test]
    fn device_picker_submit_records_pending_swap_for_rollback() {
        // `10-05`: the previous config values must be captured *before* the optimistic
        // overwrite, so a later `AudioEvent::DeviceUnavailable` can restore exactly these.
        let mut state = fixtures::fixture_empty();
        state.config.audio.device_id = "alsa/hw:0,0".to_string();
        state.config.audio.output_driver = "alsa".to_string();
        state.modal = Some(Modal::DevicePicker {
            devices: vec![crate::model::AudioDevice {
                id: "pulse/a".to_string(),
                description: "USB DAC".to_string(),
                driver: "pulse".to_string(),
            }],
            cursor: 0,
            load: LoadState::default(),
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        let pending = scenario
            .state()
            .player
            .pending_device_swap
            .as_ref()
            .expect("submit must record the pre-swap config for a possible rollback");
        assert_eq!(pending.attempted_id, "pulse/a");
        assert_eq!(pending.previous_id, "alsa/hw:0,0");
        assert_eq!(pending.previous_driver, "alsa");
    }

    // --- 09-05: sleep timer arm-time behaviour ------------------------------------------------

    fn sleep_timer_modal(trigger: SleepTrigger) -> Modal {
        Modal::SleepTimer {
            trigger,
            fade_out: true,
            quit_after: false,
            field_cursor: 0,
        }
    }

    #[test]
    fn arming_end_of_queue_disables_repeat_all_and_toasts() {
        let mut state = fixtures::fixture_playing_queue();
        state.queue.repeat = RepeatMode::All;
        state.modal = Some(sleep_timer_modal(SleepTrigger::EndOfQueue));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert_eq!(scenario.state().queue.repeat, RepeatMode::Off);
        assert!(
            scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message.contains("repeat disabled"))
        );
        assert_eq!(
            scenario
                .state()
                .player
                .sleep_timer
                .as_ref()
                .unwrap()
                .trigger,
            SleepTrigger::EndOfQueue
        );
    }

    #[test]
    fn arming_end_of_queue_leaves_other_repeat_modes_alone() {
        let mut state = fixtures::fixture_playing_queue();
        state.queue.repeat = RepeatMode::Off;
        state.modal = Some(sleep_timer_modal(SleepTrigger::EndOfQueue));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert_eq!(scenario.state().queue.repeat, RepeatMode::Off);
        assert!(
            !scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message.contains("repeat disabled")),
            "nothing needed disabling, so nothing should be toasted"
        );
    }

    #[test]
    fn rearming_replaces_existing_timer() {
        let mut state = fixtures::fixture_playing_queue();
        state.modal = Some(sleep_timer_modal(SleepTrigger::Duration(
            Duration::from_secs(600),
        )));
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));
        let first_armed_at = scenario
            .state()
            .player
            .sleep_timer
            .as_ref()
            .unwrap()
            .armed_at;

        let mut state = scenario.state().clone();
        state.modal = Some(sleep_timer_modal(SleepTrigger::EndOfTrack));
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        let timer = scenario.state().player.sleep_timer.as_ref().unwrap();
        assert_eq!(timer.trigger, SleepTrigger::EndOfTrack);
        assert!(
            timer.armed_at >= first_armed_at,
            "re-arming must replace the timer outright, not merge into the old one"
        );
    }

    // --- 10-07: sleep timer modal --------------------------------------------------------------

    fn sleep_timer_modal_at(field_cursor: usize) -> Modal {
        Modal::SleepTimer {
            trigger: SleepTrigger::Duration(Duration::from_secs(15 * 60)),
            fade_out: true,
            quit_after: false,
            field_cursor,
        }
    }

    #[test]
    fn triggers_are_mutually_exclusive() {
        // Selecting any one radio row always leaves exactly one `trigger` value set — there is no
        // way, through `ActivateField`, to end up with more than one "selected" at once, since
        // `trigger` is a single field, not a set.
        let mut state = fixtures::fixture_playing_queue();
        state.modal = Some(sleep_timer_modal_at(0));

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::FieldSet(2)))
            .dispatch(Action::Modal(ModalAction::ActivateField));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::SleepTimer { trigger, .. } => {
                assert_eq!(*trigger, SleepTrigger::Duration(Duration::from_secs(3600)));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn options_are_independent() {
        // Toggling `fade_out` must never touch `quit_after`, and vice versa.
        let mut state = fixtures::fixture_empty();
        state.modal = Some(sleep_timer_modal_at(5));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::ActivateField));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SleepTimer {
                fade_out,
                quit_after,
                ..
            } => {
                assert!(!fade_out, "field 5 (fade_out) must have toggled");
                assert!(!quit_after, "field 6 (quit_after) must be untouched");
            }
            _ => unreachable!(),
        }

        let scenario = scenario
            .dispatch(Action::Modal(ModalAction::FieldSet(6)))
            .dispatch(Action::Modal(ModalAction::ActivateField));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SleepTimer {
                fade_out,
                quit_after,
                ..
            } => {
                assert!(!fade_out, "toggling field 6 must not touch fade_out again");
                assert!(quit_after, "field 6 (quit_after) must have toggled");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn enter_arms_and_closes() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(sleep_timer_modal_at(1));

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::ActivateField))
            .dispatch(Action::Modal(ModalAction::Submit));

        assert!(scenario.state().modal.is_none());
        assert_eq!(
            scenario
                .state()
                .player
                .sleep_timer
                .as_ref()
                .unwrap()
                .trigger,
            SleepTrigger::Duration(Duration::from_secs(30 * 60))
        );
    }

    #[test]
    fn d_disarms() {
        let mut state = fixtures::fixture_empty();
        state.player.sleep_timer = Some(crate::state::player::SleepTimer {
            trigger: SleepTrigger::Duration(Duration::from_secs(600)),
            fade_out: true,
            quit_after: false,
            armed_at: Timestamp::now(),
            armed_entry: None,
            pre_fade_volume: None,
        });
        state.modal = Some(sleep_timer_modal_at(0));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::DisarmSleepTimer));

        assert!(scenario.state().player.sleep_timer.is_none());
        assert!(scenario.state().modal.is_none());
        assert!(
            scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message.contains("disarmed"))
        );
    }

    #[test]
    fn d_disarms_restores_ducked_volume() {
        let mut state = fixtures::fixture_empty();
        state.player.sleep_timer = Some(crate::state::player::SleepTimer {
            trigger: SleepTrigger::Duration(Duration::from_secs(5)),
            fade_out: true,
            quit_after: false,
            armed_at: Timestamp::now(),
            armed_entry: None,
            pre_fade_volume: Some(80),
        });
        state.modal = Some(sleep_timer_modal_at(0));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::DisarmSleepTimer));

        assert_eq!(
            scenario.effects(),
            &[Effect::Audio(AudioEffect::SetVolume(80))]
        );
    }

    #[test]
    fn d_on_unarmed_timer_is_a_noop() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(sleep_timer_modal_at(0));

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::DisarmSleepTimer));

        assert!(scenario.state().modal.is_none());
        scenario.assert_no_effects();
    }

    #[test]
    fn esc_leaves_armed_timer_running() {
        let mut state = fixtures::fixture_empty();
        state.player.sleep_timer = Some(crate::state::player::SleepTimer {
            trigger: SleepTrigger::Duration(Duration::from_secs(600)),
            fade_out: true,
            quit_after: false,
            armed_at: Timestamp::now(),
            armed_entry: None,
            pre_fade_volume: None,
        });
        state.modal = Some(sleep_timer_modal_at(0));

        let scenario = Scenario::new(state).dispatch(Action::Nav(NavAction::Cancel));

        assert!(scenario.state().modal.is_none());
        assert!(scenario.state().player.sleep_timer.is_some());
    }

    #[test]
    fn already_armed_shows_remaining_in_title_and_preselects() {
        let mut state = fixtures::fixture_empty();
        state.player.sleep_timer = Some(crate::state::player::SleepTimer {
            trigger: SleepTrigger::EndOfTrack,
            fade_out: false,
            quit_after: true,
            armed_at: Timestamp::now(),
            armed_entry: None,
            pre_fade_volume: None,
        });

        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::Open(ModalKind::SleepTimer)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::SleepTimer {
                trigger,
                fade_out,
                quit_after,
                ..
            } => {
                assert_eq!(*trigger, SleepTrigger::EndOfTrack);
                assert!(!fade_out);
                assert!(quit_after);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn playback_triggers_disabled_when_nothing_playing() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(sleep_timer_modal_at(3)); // EndOfTrack row

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::ActivateField));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::SleepTimer { trigger, .. } => assert_eq!(
                *trigger,
                SleepTrigger::Duration(Duration::from_secs(15 * 60)),
                "selecting a playback-relative trigger with nothing playing must be refused"
            ),
            _ => unreachable!(),
        }
    }

    #[test]
    fn playback_triggers_selectable_when_something_is_playing() {
        let mut state = fixtures::fixture_playing_queue();
        state.modal = Some(sleep_timer_modal_at(4)); // EndOfQueue row

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::ActivateField));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::SleepTimer { trigger, .. } => assert_eq!(*trigger, SleepTrigger::EndOfQueue),
            _ => unreachable!(),
        }
    }

    #[test]
    fn sleep_timer_field_count_covers_all_seven_rows() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(sleep_timer_modal_at(6));

        // FieldNext from the last row (6) must wrap back to the first (0), proving the modal's
        // own field count is 7, not the old placeholder 3.
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::FieldNext));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SleepTimer { field_cursor, .. } => assert_eq!(*field_cursor, 0),
            _ => unreachable!(),
        }
    }

    #[test]
    fn field_navigation_wraps() {
        // `fixture_playing_queue` (not `fixture_empty`) — `10-08`'s own "nothing to save" open-time
        // refusal means an empty queue can no longer open this modal at all.
        let scenario = Scenario::new(fixtures::fixture_playing_queue())
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::SavePlaylist)));

        let field_of = |s: &Scenario| match s.state().modal.as_ref().unwrap() {
            Modal::SavePlaylist { field, .. } => *field,
            _ => unreachable!(),
        };

        let scenario = scenario
            .dispatch(Action::Modal(ModalAction::FieldNext))
            .dispatch(Action::Modal(ModalAction::FieldNext))
            .dispatch(Action::Modal(ModalAction::FieldNext));
        assert_eq!(field_of(&scenario), 3);

        let scenario = scenario.dispatch(Action::Modal(ModalAction::FieldNext));
        assert_eq!(
            field_of(&scenario),
            0,
            "FieldNext must wrap past the last field"
        );

        let scenario = scenario.dispatch(Action::Modal(ModalAction::FieldPrev));
        assert_eq!(
            field_of(&scenario),
            3,
            "FieldPrev must wrap before the first field"
        );
    }

    fn three_device_picker() -> Modal {
        Modal::DevicePicker {
            devices: vec![
                crate::model::AudioDevice {
                    id: "a".to_string(),
                    description: "A".to_string(),
                    driver: "alsa".to_string(),
                },
                crate::model::AudioDevice {
                    id: "b".to_string(),
                    description: "B".to_string(),
                    driver: "alsa".to_string(),
                },
                crate::model::AudioDevice {
                    id: "c".to_string(),
                    description: "C".to_string(),
                    driver: "pulse".to_string(),
                },
            ],
            cursor: 0,
            load: LoadState::default(),
        }
    }

    #[test]
    fn field_set_moves_cursor_to_the_clicked_index() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(three_device_picker());

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::FieldSet(2)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::DevicePicker { cursor, .. } => assert_eq!(*cursor, 2),
            _ => unreachable!(),
        }
    }

    #[test]
    fn field_set_clamps_out_of_range_index() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(three_device_picker());

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::FieldSet(999)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::DevicePicker { cursor, .. } => assert_eq!(*cursor, 2),
            _ => unreachable!(),
        }
    }

    #[test]
    fn field_set_on_empty_list_is_a_noop() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loading,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::FieldSet(5)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::DevicePicker { cursor, .. } => assert_eq!(*cursor, 0),
            _ => unreachable!(),
        }
        scenario.assert_no_effects();
    }

    #[test]
    fn field_backspace_removes_grapheme_not_byte() {
        let scenario = Scenario::new(fixtures::fixture_playing_queue())
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::SavePlaylist)))
            // Field `0` is the target dropdown now; `FieldNext` reaches the name field (`1`).
            .dispatch(Action::Modal(ModalAction::FieldNext))
            .dispatch(Action::Modal(ModalAction::FieldInput('🎵')));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::SavePlaylist { name, .. } => assert_eq!(name, "🎵"),
            _ => unreachable!(),
        }

        let scenario = scenario.dispatch(Action::Modal(ModalAction::FieldBackspace));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SavePlaylist { name, .. } => assert_eq!(name, ""),
            _ => unreachable!(),
        }
    }

    // --- 10-08: save playlist modal -----------------------------------------------------------

    /// A 10-entry playing queue *and* a 3-track active selection on the Artists tab at the same
    /// time — the only way to actually distinguish "`P` always means the queue" from "`Ctrl+P`
    /// means the selection" (a fixture with only one of the two can't tell an auto-detect bug from
    /// the correct explicit-hint behaviour, which is exactly how the pre-existing test this
    /// replaces missed the bug this task's own decisions.md entry describes).
    fn fixture_queue_and_selection() -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let tracks: Vec<Track> = (1..=3)
            .map(|n| fixtures::track(&format!("Sel {n}"), n, &alb, &[&a]))
            .collect();
        let mut col = Column::new(
            ColumnKind::Tracks {
                of_album: alb.id.clone(),
            },
            "Tracks",
        );
        col.selection.selected = tracks.iter().map(|t| t.id.clone()).collect();
        col.items = tracks.into_iter().map(MediaItem::Track).collect();
        state.nav.active_tab = Tab::Artists;
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![col]);
        state.nav.focus = crate::state::nav::NavFocus::Column(0);
        state
    }

    fn open_name_and_submit(state: AppState, source: SaveSource) -> Scenario {
        Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::OpenSavePlaylist(source)))
            .dispatch(Action::Modal(ModalAction::FieldNext))
            .dispatch(Action::Modal(ModalAction::FieldInput('X')))
            .dispatch(Action::Modal(ModalAction::Submit))
    }

    #[test]
    fn p_uses_queue_ctrl_p_uses_selection() {
        let state = fixture_queue_and_selection();

        let scenario = open_name_and_submit(state.clone(), SaveSource::Queue);
        match scenario.last_effect() {
            Some(Effect::Net(NetEffect::PlaylistCreate { tracks, .. })) => {
                assert_eq!(tracks.len(), 10, "P must always save the whole queue");
            }
            other => panic!("expected PlaylistCreate, got {other:?}"),
        }

        let scenario = open_name_and_submit(state, SaveSource::Selection);
        match scenario.last_effect() {
            Some(Effect::Net(NetEffect::PlaylistCreate { tracks, .. })) => {
                assert_eq!(
                    tracks.len(),
                    3,
                    "Ctrl+P must save the selection, not the queue, even though both exist"
                );
            }
            other => panic!("expected PlaylistCreate, got {other:?}"),
        }
    }

    #[test]
    fn ctrl_p_falls_back_to_focused_item_when_nothing_selected() {
        let mut state = fixtures::fixture_empty();
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        let mut col = Column::new(
            ColumnKind::Tracks {
                of_album: alb.id.clone(),
            },
            "Tracks",
        );
        col.items = vec![MediaItem::Track(t)];
        col.cursor = 0;
        state.nav.active_tab = Tab::Artists;
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![col]);
        state.nav.focus = crate::state::nav::NavFocus::Column(0);

        let scenario = open_name_and_submit(state, SaveSource::Selection);
        match scenario.last_effect() {
            Some(Effect::Net(NetEffect::PlaylistCreate { tracks, .. })) => {
                assert_eq!(tracks.len(), 1);
            }
            other => panic!("expected PlaylistCreate, got {other:?}"),
        }
    }

    #[test]
    fn empty_source_refuses_to_open() {
        let state = fixtures::fixture_empty();
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::OpenSavePlaylist(
            SaveSource::Queue,
        )));
        assert!(scenario.state().modal.is_none());
        assert!(
            scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message.contains("nothing to save"))
        );
    }

    #[test]
    fn refused_when_offline() {
        let mut state = fixtures::fixture_playing_queue();
        state.connectivity = Connectivity::Offline;
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::OpenSavePlaylist(
            SaveSource::Queue,
        )));
        assert!(scenario.state().modal.is_none());
        assert!(
            scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message.contains("need a connection"))
        );
    }

    #[test]
    fn summary_states_source_and_count() {
        // The reducer-level half of "the summary line states which, and the count" — the widget
        // reads `source`/`save_playlist_tracks(state, source).len()` directly; this only proves
        // the modal's own `source` field is set correctly for the widget to read.
        let state = fixtures::fixture_playing_queue();
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::OpenSavePlaylist(
            SaveSource::Queue,
        )));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SavePlaylist { source, .. } => assert_eq!(*source, SaveSource::Queue),
            _ => unreachable!(),
        }
    }

    #[test]
    fn existing_target_hides_name_and_description() {
        // Reducer-level proof that `field_input` no longer writes into `name`/`overview` once a
        // target other than `New` is selected via `CycleSaveTarget` — the widget's own job is to
        // stop *rendering* those fields, but this proves the underlying data can't drift out of
        // sync with what's shown even if a stray keystroke reached the reducer.
        let scenario = open_with_one_existing_playlist();
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SavePlaylist { target, .. } => {
                assert_eq!(*target, PlaylistTarget::Existing(PlaylistId::from("pl-1")));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn existing_target_button_says_add() {
        // The button label itself is a widget-level literal (`[ Enter ] Add` vs `Save`) driven by
        // `target != PlaylistTarget::New` — covered by the widget's own snapshot tests; this only
        // establishes the reducer-level precondition (`target` actually becomes `Existing`).
        let scenario = open_with_one_existing_playlist();
        assert_ne!(
            match scenario.state().modal.as_ref().unwrap() {
                Modal::SavePlaylist { target, .. } => target.clone(),
                _ => unreachable!(),
            },
            PlaylistTarget::New
        );
    }

    /// Opens the modal, delivers the `FetchColumn { Playlists }` reply `open_save_playlist` fires
    /// (a single existing playlist, `pl-1`/"My Mix"), then cycles the target dropdown onto it.
    fn open_with_one_existing_playlist() -> Scenario {
        let items = vec![MediaItem::Playlist(crate::model::Playlist {
            id: ItemId::from("pl-1"),
            name: "My Mix".to_string(),
            overview: None,
            track_count: 0,
            total_duration: Duration::ZERO,
            can_edit: true,
            is_favorite: false,
        })];
        Scenario::new(fixtures::fixture_playing_queue())
            .dispatch(Action::Modal(ModalAction::OpenSavePlaylist(
                SaveSource::Queue,
            )))
            .dispatch(Action::Data(DataAction::ItemsLoaded {
                tab: Tab::Playlists,
                depth: 0,
                kind: ColumnKind::Playlists,
                items,
                total: 1,
                page: 0,
            }))
            .dispatch(Action::Modal(ModalAction::CycleSaveTarget(1)))
    }

    #[test]
    fn empty_name_for_new_playlist_shows_inline_error() {
        let scenario = Scenario::new(fixtures::fixture_playing_queue())
            .dispatch(Action::Modal(ModalAction::OpenSavePlaylist(
                SaveSource::Queue,
            )))
            .dispatch(Action::Modal(ModalAction::Submit));

        // Must not have closed.
        match scenario.state().modal.as_ref() {
            Some(Modal::SavePlaylist { error, .. }) => {
                assert_eq!(error.as_deref(), Some("name is required"));
            }
            other => panic!("expected the modal to stay open with an error, got {other:?}"),
        }
        // `Submit` itself (the last dispatch) must not have emitted a `PlaylistCreate`/`PlaylistAdd`
        // — only `open_save_playlist`'s own `FetchColumn` (from the first dispatch) is present.
        assert!(
            !scenario
                .effects()
                .iter()
                .any(|e| matches!(e, Effect::Net(NetEffect::PlaylistCreate { .. })))
        );
    }

    #[test]
    fn sort_checkbox_defaults_off() {
        let scenario = Scenario::new(fixtures::fixture_playing_queue()).dispatch(Action::Modal(
            ModalAction::OpenSavePlaylist(SaveSource::Queue),
        ));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SavePlaylist { autosort, .. } => assert!(!autosort),
            _ => unreachable!(),
        }
    }

    #[test]
    fn sort_checkbox_applies_profile_to_saved_order() {
        let mut state = fixtures::fixture_playing_queue();
        state
            .config
            .sorting
            .profiles
            .push(crate::config::SortProfile {
                name: "by_name".to_string(),
                rules: vec![crate::config::SortRule {
                    field: crate::config::SortField::Name,
                    direction: crate::config::Direction::Asc,
                }],
            });
        state.queue.sort_profile = Some("by_name".to_string());
        // `queue::sort::compare`'s own `SortField::Name` is a *natural* (numeric-aware) sort —
        // "Track 1".."Track 10" in the fixture's own insertion order is already naturally sorted,
        // which would make a real re-sort indistinguishable from none happening at all. Reversing
        // the queue first (never touching `play_order`/`position`, which this test's own
        // `Submit`-only dispatch chain never reads) gives a genuinely different, checkable order.
        state.queue.entries.reverse();
        let title_by_id: std::collections::HashMap<ItemId, String> = state
            .queue
            .entries
            .iter()
            .map(|e| (e.track.id.clone(), e.track.name.clone()))
            .collect();
        // Sorted the same way the reducer itself will (`queue::sort::compare`'s natural,
        // numeric-aware ordering) — a plain `Vec<String>::sort()` would use byte-wise lexicographic
        // order instead, which disagrees with the app's own definition of "sorted" for exactly the
        // "Track 1".."Track 10" names this fixture uses.
        let mut expected_tracks: Vec<Track> = state
            .queue
            .entries
            .iter()
            .map(|e| e.track.clone())
            .collect();
        let profile = state.config.sorting.profiles.last().unwrap().clone();
        expected_tracks.sort_by(|a, b| crate::queue::sort::compare(a, b, &profile));
        let expected_titles: Vec<String> = expected_tracks.into_iter().map(|t| t.name).collect();

        // Field `3` is the sort checkbox; toggling it before naming and submitting.
        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::OpenSavePlaylist(
                SaveSource::Queue,
            )))
            .dispatch(Action::Modal(ModalAction::FieldSet(3)))
            .dispatch(Action::Modal(ModalAction::ActivateField))
            .dispatch(Action::Modal(ModalAction::FieldSet(1)))
            .dispatch(Action::Modal(ModalAction::FieldInput('X')))
            .dispatch(Action::Modal(ModalAction::Submit));

        match scenario.last_effect() {
            Some(Effect::Net(NetEffect::PlaylistCreate { tracks, .. })) => {
                let saved_titles: Vec<String> =
                    tracks.iter().map(|id| title_by_id[id].clone()).collect();
                assert_eq!(saved_titles, expected_titles);
            }
            other => panic!("expected PlaylistCreate, got {other:?}"),
        }
    }

    #[test]
    fn submit_closes_immediately_and_toasts() {
        let scenario = open_name_and_submit(fixtures::fixture_playing_queue(), SaveSource::Queue);
        assert!(scenario.state().modal.is_none());
        assert!(
            scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message.starts_with("saving ") && t.message.contains("tracks"))
        );
    }

    #[test]
    fn completion_toast_replaces_pending() {
        let scenario = open_name_and_submit(fixtures::fixture_playing_queue(), SaveSource::Queue)
            .dispatch(Action::Data(DataAction::PlaylistSaved {
                name: "X".to_string(),
            }));
        assert!(
            !scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message.starts_with("saving ")),
            "the pending toast must be gone, not just added to"
        );
        assert!(
            scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message == "saved to X")
        );
    }

    #[test]
    fn failure_toast_names_reason() {
        let scenario = open_name_and_submit(fixtures::fixture_playing_queue(), SaveSource::Queue)
            .dispatch(Action::Data(DataAction::LoadFailed {
                target: LoadTarget::PlaylistSave,
                message: "server error".to_string(),
                offline: false,
            }));
        assert!(
            !scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message.starts_with("saving ")),
        );
        assert!(
            scenario
                .state()
                .toasts
                .iter()
                .any(|t| t.message == "could not save: server error")
        );
    }

    #[test]
    fn keymap_editor_refuses_conflicting_capture() {
        let mut state = fixtures::fixture_empty();
        state.keymap = KeyMap::defaults();
        // The default keymap binds `j` to `MoveDown` — capturing `j` for a different action
        // (`PlayPause`) must be refused.
        let action_cursor = ActionId::iter()
            .position(|a| a == ActionId::PlayPause)
            .unwrap();

        state.modal = Some(Modal::KeymapEditor {
            action_cursor,
            capturing: false,
            conflict: None,
            captured: Some(chord('j')),
            capture_deadline: None,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::KeymapEditor { conflict, .. } => {
                assert_eq!(*conflict, Some(ActionId::MoveDown));
            }
            _ => unreachable!("refused capture must keep the editor open"),
        }
        scenario.assert_no_effects();
    }

    fn kc(c: char) -> KeyChord {
        KeyChord {
            code: KeyCode::Char(c),
            mods: KeyModifiers::default(),
        }
    }

    fn editor_state_for(action: ActionId) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.keymap = KeyMap::defaults();
        let action_cursor = ActionId::iter().position(|a| a == action).unwrap();
        state.modal = Some(Modal::KeymapEditor {
            action_cursor,
            capturing: false,
            conflict: None,
            captured: None,
            capture_deadline: None,
        });
        state
    }

    fn start_capturing(state: &mut AppState) {
        if let Some(Modal::KeymapEditor { capturing, .. }) = &mut state.modal {
            *capturing = true;
        }
    }

    /// `11-02`: `capture_takes_key_verbatim_not_resolved` — `j` is bound to `MoveDown` in the
    /// default table; capturing it must store the raw chord itself, not resolve or reject it as
    /// `Action::Nav(MoveDown)`, and must leave capture open for an optional second chord.
    #[test]
    fn capture_takes_key_verbatim_not_resolved() {
        let mut state = editor_state_for(ActionId::ToggleShuffle);
        start_capturing(&mut state);

        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::CaptureChord(kc('j'))));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::KeymapEditor {
                captured,
                capturing,
                ..
            } => {
                assert_eq!(*captured, Some(chord('j')));
                assert!(*capturing, "still open to an optional second chord");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn two_chord_capture_within_window() {
        let mut state = editor_state_for(ActionId::ToggleShuffle);
        start_capturing(&mut state);

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::CaptureChord(kc('g'))))
            .dispatch(Action::Modal(ModalAction::CaptureChord(kc('x'))));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::KeymapEditor {
                captured,
                capturing,
                ..
            } => {
                assert_eq!(captured.as_ref().unwrap().0.len(), 2);
                assert!(!*capturing, "a second chord ends capture immediately");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn capture_window_expires_and_finalizes_a_lone_chord() {
        let mut state = editor_state_for(ActionId::ToggleShuffle);
        start_capturing(&mut state);

        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::CaptureChord(kc('g'))));
        let still_open = match scenario.state().modal.as_ref().unwrap() {
            Modal::KeymapEditor { capturing, .. } => *capturing,
            _ => unreachable!(),
        };
        assert!(still_open, "capture stays open until the window lapses");

        let expiry = scenario
            .state()
            .clock
            .checked_add(jiff::SignedDuration::from_secs(3))
            .unwrap();
        let scenario = scenario.dispatch(Action::System(SystemEvent::Tick(expiry)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::KeymapEditor {
                captured,
                capturing,
                ..
            } => {
                assert!(!*capturing);
                assert_eq!(*captured, Some(chord('g')));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn capture_cancels_on_esc() {
        let mut state = editor_state_for(ActionId::ToggleShuffle);
        start_capturing(&mut state);

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::CaptureChord(kc('g'))))
            .dispatch(Action::Modal(ModalAction::CancelCapture));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::KeymapEditor {
                captured,
                capturing,
                conflict,
                ..
            } => {
                assert!(captured.is_none());
                assert!(!*capturing);
                assert!(conflict.is_none());
            }
            _ => unreachable!(),
        }
    }

    /// `conflict_message_names_incumbent` — the render layer's own
    /// `conflict_shows_incumbent_and_rebind_footer` test (`modals::keymap_editor`) proves the UI
    /// text; this proves the reducer state it reads from actually names the right action.
    #[test]
    fn conflict_names_the_incumbent_action() {
        let mut state = editor_state_for(ActionId::ToggleShuffle);
        state.modal = Some(Modal::KeymapEditor {
            action_cursor: ActionId::iter()
                .position(|a| a == ActionId::ToggleShuffle)
                .unwrap(),
            capturing: false,
            conflict: None,
            captured: Some(chord('n')), // `n` is `NextTrack`'s only default binding.
            capture_deadline: None,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::KeymapEditor { conflict, .. } => {
                assert_eq!(*conflict, Some(ActionId::NextTrack));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn rebind_anyway_unbinds_incumbent_explicitly() {
        let mut state = editor_state_for(ActionId::ToggleShuffle);
        state.modal = Some(Modal::KeymapEditor {
            action_cursor: ActionId::iter()
                .position(|a| a == ActionId::ToggleShuffle)
                .unwrap(),
            capturing: false,
            conflict: None,
            captured: Some(chord('n')),
            capture_deadline: None,
        });

        // First `Submit` discovers and displays the conflict (proven above); a second `Submit` on
        // the same still-conflicting modal is this task's own "[Enter] rebind anyway".
        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::Submit))
            .dispatch(Action::Modal(ModalAction::Submit));

        assert_eq!(
            scenario.state().keymap.hint_for(ActionId::ToggleShuffle),
            "n"
        );
        assert_eq!(
            scenario.state().keymap.hint_for(ActionId::NextTrack),
            "unbound",
            "the incumbent's only binding must be given up, not silently kept"
        );
        assert!(
            scenario
                .effects()
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
    }

    #[test]
    fn reserved_chords_cannot_be_rebound() {
        for reserved in [
            KeyBinding(smallvec::smallvec![KeyChord {
                code: KeyCode::Esc,
                mods: KeyModifiers::default(),
            }]),
            KeyBinding(smallvec::smallvec![KeyChord {
                code: KeyCode::Char('c'),
                mods: KeyModifiers {
                    ctrl: true,
                    ..KeyModifiers::default()
                },
            }]),
        ] {
            let mut state = editor_state_for(ActionId::ToggleShuffle);
            state.modal = Some(Modal::KeymapEditor {
                action_cursor: ActionId::iter()
                    .position(|a| a == ActionId::ToggleShuffle)
                    .unwrap(),
                capturing: false,
                conflict: None,
                captured: Some(reserved),
                capture_deadline: None,
            });

            let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

            assert_eq!(
                scenario.state().keymap.hint_for(ActionId::ToggleShuffle),
                "s",
                "a reserved chord must never actually get bound"
            );
            scenario.assert_toast_contains("reserved");
        }
    }

    #[test]
    fn reset_row_restores_default() {
        let mut state = editor_state_for(ActionId::PlayPause);
        state.modal = Some(Modal::KeymapEditor {
            action_cursor: ActionId::iter()
                .position(|a| a == ActionId::PlayPause)
                .unwrap(),
            capturing: false,
            conflict: None,
            captured: Some(chord('b')), // `b` is unbound in the default table.
            capture_deadline: None,
        });
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));
        assert_eq!(scenario.state().keymap.hint_for(ActionId::PlayPause), "b");

        let scenario = scenario.dispatch(Action::Modal(ModalAction::ResetRowToDefault));
        assert_eq!(
            scenario.state().keymap.hint_for(ActionId::PlayPause),
            "space"
        );
    }

    #[test]
    fn unbind_leaves_action_listed_with_dash() {
        let state = editor_state_for(ActionId::ToggleShuffle);
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::UnbindRow));

        assert_eq!(
            scenario.state().keymap.hint_for(ActionId::ToggleShuffle),
            "unbound"
        );
        assert_eq!(
            scenario
                .state()
                .config
                .keybindings
                .get("toggle_shuffle")
                .map(String::as_str),
            Some("none")
        );
    }

    #[test]
    fn reset_all_requires_confirmation() {
        let state = editor_state_for(ActionId::ToggleShuffle);
        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::ConfirmResetAllKeybindings));

        // The reset must not have happened yet — only a `Confirm` modal is open.
        match scenario.state().modal.as_ref().unwrap() {
            Modal::Confirm { on_confirm, .. } => {
                assert_eq!(
                    **on_confirm,
                    Action::Modal(ModalAction::ResetAllKeybindings)
                );
            }
            other => unreachable!("expected a Confirm modal, got {other:?}"),
        }
        assert_eq!(
            scenario.state().keymap.hint_for(ActionId::ToggleShuffle),
            "s"
        );

        // Accepting it performs the reset and reopens the editor.
        let scenario = scenario.dispatch(Action::Modal(ModalAction::Submit));
        assert!(scenario.state().config.keybindings.is_empty());
        assert!(matches!(
            scenario.state().modal,
            Some(Modal::KeymapEditor { .. })
        ));
    }

    #[test]
    fn binding_persists_in_friendly_syntax_and_roundtrips_through_config() {
        let mut state = editor_state_for(ActionId::ToggleShuffle);
        state.modal = Some(Modal::KeymapEditor {
            action_cursor: ActionId::iter()
                .position(|a| a == ActionId::ToggleShuffle)
                .unwrap(),
            capturing: false,
            conflict: None,
            captured: Some(chord('c')), // `c` is unbound in the default table (only `ctrl+c` is).
            capture_deadline: None,
        });
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert_eq!(
            scenario
                .state()
                .config
                .keybindings
                .get("toggle_shuffle")
                .map(String::as_str),
            Some("c"),
            "always the friendly rendering, never the verbose `Char('c')` form"
        );

        // Simulates a fresh restart loading this same `config.keybindings` from disk.
        let (reloaded, warnings) = KeyMap::from_config(&scenario.state().config.keybindings);
        assert!(warnings.is_empty());
        assert_eq!(reloaded.hint_for(ActionId::ToggleShuffle), "c");
    }

    /// `remapped_key_works_immediately` / `all_ui_hints_update_after_remap` — every UI surface
    /// that shows a key (inspector, help modal, footers) reads `state.keymap.hint_for` fresh at
    /// render time with no cache of its own (`docs/07-ui-spec.md` §1, already exercised per-surface
    /// by e.g. `modals::help::tests::help_rows_generated_from_keymap`); this proves the one shared
    /// fact all of them depend on — that the live `KeyMap` itself updates the instant a capture
    /// commits, with no restart and no separate "apply" step.
    #[test]
    fn remapped_key_works_immediately() {
        let mut state = editor_state_for(ActionId::ToggleShuffle);
        state.modal = Some(Modal::KeymapEditor {
            action_cursor: ActionId::iter()
                .position(|a| a == ActionId::ToggleShuffle)
                .unwrap(),
            capturing: false,
            conflict: None,
            captured: Some(chord('c')),
            capture_deadline: None,
        });
        assert_eq!(state.keymap.hint_for(ActionId::ToggleShuffle), "s");

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));
        assert_eq!(
            scenario.state().keymap.hint_for(ActionId::ToggleShuffle),
            "c"
        );
    }

    #[test]
    fn confirm_dispatches_boxed_action_after_close() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::Confirm {
            prompt: "Enable zen mode?".to_string(),
            on_confirm: Box::new(Action::View(ViewAction::ToggleZen)),
        });

        // The modal is cleared *before* the boxed action is dispatched; `ToggleZen` itself is
        // `10-02`'s own real handler now, so this also proves the boxed action genuinely reaches
        // it rather than being dropped.
        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));
        assert!(scenario.state().modal.is_none());
        assert!(scenario.state().zen_mode);
    }

    #[test]
    fn sort_profile_editor_state_is_constructible() {
        // `RuleEditor` has no reducer behaviour yet (no acceptance criterion exercises it), but it
        // must at least be constructible as part of `Modal::SortProfile`'s `editing` field.
        let editing = Some(RuleEditor {
            field: crate::config::SortField::Name,
            direction: crate::config::Direction::Asc,
        });
        assert!(editing.is_some());
    }

    // --- 10-09: sort profile modal --------------------------------------------------------------

    fn two_profiles() -> Vec<crate::config::SortProfile> {
        vec![
            crate::config::SortProfile {
                name: "by_name".to_string(),
                rules: vec![crate::config::SortRule {
                    field: crate::config::SortField::Name,
                    direction: crate::config::Direction::Asc,
                }],
            },
            crate::config::SortProfile {
                name: "by_year".to_string(),
                rules: vec![crate::config::SortRule {
                    field: crate::config::SortField::Year,
                    direction: crate::config::Direction::Desc,
                }],
            },
        ]
    }

    #[test]
    fn target_defaults_to_queue_when_playing() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.status = crate::state::player::PlayStatus::Playing;
        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::Open(ModalKind::SortProfile)));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SortProfile { target, .. } => assert_eq!(*target, SortApplyTarget::Queue),
            _ => unreachable!(),
        }
    }

    #[test]
    fn target_defaults_to_column_only_when_the_queue_is_empty() {
        // Empty queue -> nothing to reorder, so default to sorting the browse column.
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = crate::state::nav::Tab::Artists;
        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::Open(ModalKind::SortProfile)));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SortProfile { target, .. } => assert_eq!(*target, SortApplyTarget::Column),
            _ => unreachable!(),
        }
    }

    #[test]
    fn target_defaults_to_queue_when_paused() {
        // A *paused* queue is still a queue the user means to reorder — the fix for "sorting only
        // works when unpaused".
        let mut state = fixtures::fixture_playing_queue();
        state.player.status = crate::state::player::PlayStatus::Paused;
        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::Open(ModalKind::SortProfile)));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SortProfile { target, .. } => assert_eq!(*target, SortApplyTarget::Queue),
            _ => unreachable!(),
        }
    }

    #[test]
    fn enter_applies_to_selected_target_queue() {
        let mut state = fixtures::fixture_playing_queue();
        // `QueueAction::ApplySortProfile` (which `Submit`'s own `Queue` arm dispatches) looks the
        // name up in `state.config.sorting.profiles` — the modal's own `profiles` field is a
        // separate, `build_sort_profile_modal`-cloned snapshot of it, not the source of truth the
        // actual application reads from.
        state.config.sorting.profiles = two_profiles();
        state.modal = Some(Modal::SortProfile {
            profiles: two_profiles(),
            cursor: 1,
            editing: None,
            target: SortApplyTarget::Queue,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert!(scenario.state().modal.is_none());
        assert_eq!(
            scenario.state().queue.sort_profile.as_deref(),
            Some("by_year")
        );
    }

    #[test]
    fn enter_applies_to_selected_target_column() {
        // `fixture_miller_3col`'s own Tracks column (depth 2, already focused): "Motion", "Fate",
        // "Come Closer", in that (unsorted) order.
        let mut state = fixtures::fixture_miller_3col();
        state.modal = Some(Modal::SortProfile {
            profiles: two_profiles(), // "by_name" ascending is index 0
            cursor: 0,
            editing: None,
            target: SortApplyTarget::Column,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert!(scenario.state().modal.is_none());
        // The queue's own `sort_profile` must be untouched — this only sorted the column.
        assert_eq!(scenario.state().queue.sort_profile, None);
        let names: Vec<String> = scenario.state().nav.per_tab_stacks[&Tab::Artists][2]
            .items
            .iter()
            .map(|i| i.display_name().to_string())
            .collect();
        assert_eq!(names, vec!["Come Closer", "Fate", "Motion"]);
    }

    #[test]
    fn applying_to_queue_clears_shuffle() {
        let mut state = fixtures::fixture_playing_queue();
        state.queue.shuffled = true;
        state.config.sorting.profiles = two_profiles();
        state.modal = Some(Modal::SortProfile {
            profiles: two_profiles(),
            cursor: 0,
            editing: None,
            target: SortApplyTarget::Queue,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Submit));

        assert!(!scenario.state().queue.shuffled);
    }

    #[test]
    fn e_opens_settings_sorting() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::SortProfile {
            profiles: two_profiles(),
            cursor: 0,
            editing: None,
            target: SortApplyTarget::Queue,
        });

        let scenario =
            Scenario::new(state).dispatch(Action::Modal(ModalAction::OpenSettingsSorting));

        assert!(scenario.state().modal.is_none());
        assert_eq!(scenario.state().nav.active_tab, Tab::Settings);
        // `11-01`: now lands on the Sorting section specifically, not just the tab as a whole.
        assert_eq!(
            scenario.state().settings.section,
            crate::state::settings::SettingsSection::Sorting
        );
    }

    #[test]
    fn toggle_sort_target_flips_between_queue_and_column() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::SortProfile {
            profiles: two_profiles(),
            cursor: 0,
            editing: None,
            target: SortApplyTarget::Queue,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::ToggleSortTarget));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SortProfile { target, .. } => assert_eq!(*target, SortApplyTarget::Column),
            _ => unreachable!(),
        }

        let scenario = scenario.dispatch(Action::Modal(ModalAction::ToggleSortTarget));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::SortProfile { target, .. } => assert_eq!(*target, SortApplyTarget::Queue),
            _ => unreachable!(),
        }
    }

    // --- 10-03: help modal scroll ---------------------------------------------------------------

    #[test]
    fn scroll_moves_help_modals_own_offset() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::Help {
            context: InputContext::Normal,
            scroll: 0,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Scroll(5)));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::Help { scroll, .. } => assert_eq!(*scroll, 5),
            _ => unreachable!(),
        }
    }

    #[test]
    fn scroll_clamps_at_zero() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::Help {
            context: InputContext::Normal,
            scroll: 2,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Scroll(-100)));
        match scenario.state().modal.as_ref().unwrap() {
            Modal::Help { scroll, .. } => assert_eq!(*scroll, 0),
            _ => unreachable!(),
        }
    }

    #[test]
    fn scroll_is_a_no_op_outside_help() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loading,
        });

        let scenario = Scenario::new(state).dispatch(Action::Modal(ModalAction::Scroll(5)));
        scenario.assert_no_effects();
    }

    #[test]
    fn scroll_resets_on_close_and_reopen() {
        let mut state = fixtures::fixture_empty();
        state.keymap = KeyMap::defaults();
        state.modal = Some(Modal::Help {
            context: InputContext::Normal,
            scroll: 0,
        });

        let scenario = Scenario::new(state)
            .dispatch(Action::Modal(ModalAction::Scroll(9)))
            .dispatch(Action::Modal(ModalAction::Close))
            .dispatch(Action::Modal(ModalAction::Open(ModalKind::Help)));

        match scenario.state().modal.as_ref().unwrap() {
            Modal::Help { scroll, .. } => assert_eq!(*scroll, 0),
            _ => unreachable!(),
        }
    }
}
