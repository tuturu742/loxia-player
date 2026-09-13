//! Reducer: the Settings tab (`11-01`, `docs/07-ui-spec.md` §9) — the row schema
//! (`rows_for_section`, rebuilt fresh from `&Config` every time, never cached across frames) and
//! the actual editing logic (`apply`). Sections 11-02 through 11-07 fill in the specialised
//! editors (server profiles, the keymap editor, sort profile editor, EQ preset manager, the about
//! view) that the four `SettingsRowAction` variants below stand in for; this task provides the
//! frame and the simple scalar controls only.

use std::collections::BTreeMap;

use crate::Timestamp;
use crate::action::{Action, SettingsAction};
use crate::config::{
    self, ArtProtocol, Config, QualityProfile, ReplayGainMode, ServerConfig, TargetCodec,
};
use crate::effect::{AudioEffect, CacheEffect, Effect, NetEffect, RedactedSecret, SysEffect};
use crate::model::{AudioDevice, ServerId};
use crate::state::AppState;
use crate::state::settings::{
    EqPresetEditorState, ServerDraft, ServerDraftField, ServerEditorState, SettingsSection,
    SortProfileEditorState, TextEdit,
};
use crate::state::toast::ToastLevel;

/// How long a settings edit waits before it's actually written to disk — "persistence is
/// debounced 1 second" (this task's own spec), so dragging a slider doesn't write the file on
/// every single step.
const WRITE_DEBOUNCE: jiff::SignedDuration = jiff::SignedDuration::from_secs(1);

/// One row's control — rebuilt fresh from `&Config` by [`rows_for_section`] on every render and
/// every edit, so there is never a stale copy of a value to reconcile. `get`/`set` are plain `fn`
/// pointers (not closures — none of them capture anything), matching this task's own given
/// signature.
pub enum Control {
    Toggle {
        get: fn(&Config) -> bool,
        set: fn(&mut Config, bool),
    },
    Select {
        options: Vec<String>,
        get: fn(&Config) -> String,
        set: fn(&mut Config, &str),
    },
    Slider {
        min: f64,
        max: f64,
        step: f64,
        unit: &'static str,
        get: fn(&Config) -> f64,
        set: fn(&mut Config, f64),
    },
    Text {
        get: fn(&Config) -> String,
        set: fn(&mut Config, String),
        secret: bool,
    },
    Number {
        min: i64,
        max: i64,
        get: fn(&Config) -> i64,
        set: fn(&mut Config, i64),
    },
    /// Not a `Config` field at all — "e.g. `Clear cache`, `Test connection`" (this task's own
    /// spec). Here, standing in for the four collection-valued fields (`servers`,
    /// `sorting.profiles`, `equalizer.custom_presets`, `keybindings`) whose real editors are
    /// 11-02 through 11-05's own job — activating one of these today does nothing yet
    /// (`docs/12-decisions.md`).
    Action {
        label: String,
        action: SettingsRowAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRowAction {
    ManageServers,
    ManageSortProfiles,
    ManageEqPresets,
    EditKeybindings,
    /// `11-06`: opens a `Confirm` before deleting the saved `session.json` and emptying the
    /// current queue — the one `Control::Action` row that isn't opening a sub-editor.
    ClearSavedSession,
}

pub struct SettingsRow {
    pub label: &'static str,
    /// One-line, `Dim`-styled description shown beneath the value (this task's own spec).
    pub description: &'static str,
    pub control: Control,
}

fn row(label: &'static str, description: &'static str, control: Control) -> SettingsRow {
    SettingsRow {
        label,
        description,
        control,
    }
}

/// The full row list for one section, built fresh from `cfg` — this is the single place that
/// decides which `Config` field maps to which control; `every_config_field_has_a_control`
/// cross-checks its own hand-maintained field list against this function's own coverage.
///
/// `schema_version` is deliberately absent from every section — pure schema-migration bookkeeping
/// (`docs/12-decisions.md` §5), never a user-facing setting. `logging.{level,max_files}` are
/// folded into `Interface` — this task's own section list (`docs/07-ui-spec.md` §9) names no
/// dedicated "Logging" section, and a two-field, general "how the app behaves" section is the
/// closest existing fit.
/// `devices` is `state.player.known_devices` (populated by `DataAction::DevicesLoaded`, enumerated
/// at startup) — it turns the Audio section's output driver/device rows into real dropdowns over
/// what the machine actually has, instead of free-text fields a user had to type an id into blind.
pub fn rows_for_section(
    section: SettingsSection,
    cfg: &Config,
    devices: &[AudioDevice],
) -> Vec<SettingsRow> {
    match section {
        SettingsSection::Servers => servers_rows(cfg),
        SettingsSection::Audio => audio_rows(cfg, devices),
        SettingsSection::Cache => cache_rows(),
        SettingsSection::Transcode => transcode_rows(),
        SettingsSection::Interface => interface_rows(),
        SettingsSection::Sorting => sorting_rows(cfg),
        SettingsSection::Equalizer => equalizer_rows(cfg),
        SettingsSection::Keybindings => keybindings_rows(),
        SettingsSection::About => Vec::new(),
    }
}

fn servers_rows(cfg: &Config) -> Vec<SettingsRow> {
    let options: Vec<String> = cfg.servers.iter().map(|s| s.id.clone()).collect();
    vec![
        row(
            "active server",
            "which configured server this app connects to, by id",
            Control::Select {
                options,
                get: |c| c.active_server.clone(),
                set: |c, v| c.active_server = v.to_string(),
            },
        ),
        // `11-01`'s own acceptance list names this field specifically ("secret fields
        // (`access_token`) render as ••••••••..."), so it gets a real row here rather than
        // waiting for `11-03`'s full add/edit/remove server editor — scoped to the *active*
        // server only, the one this app is actually about to connect with. A no-op `set` when
        // there is no active server (rather than panicking on an out-of-bounds lookup) — nothing
        // to write the token onto yet.
        row(
            "access token",
            "the active server's own Emby access token",
            Control::Text {
                get: |c| {
                    c.servers
                        .iter()
                        .find(|s| s.id == c.active_server)
                        .map(|s| s.access_token.clone())
                        .unwrap_or_default()
                },
                set: |c, v| {
                    if let Some(server) = c.servers.iter_mut().find(|s| s.id == c.active_server) {
                        server.access_token = v;
                    }
                },
                secret: true,
            },
        ),
        row(
            "servers",
            "add, edit, or remove configured Emby servers",
            Control::Action {
                label: format!("manage servers ({} configured)", cfg.servers.len()),
                action: SettingsRowAction::ManageServers,
            },
        ),
    ]
}

/// `"auto"` (let mpv choose) is always the first option of both output dropdowns, followed by
/// whatever was actually detected. A value already in the config that no longer matches any detected
/// device is appended too, so an unplugged/renamed device is still shown (and selectable) rather
/// than silently vanishing from its own dropdown.
fn device_options(current: &str, detected: impl Iterator<Item = String>) -> Vec<String> {
    let mut options = vec![AUTO_OPTION.to_string()];
    for value in detected {
        if !options.contains(&value) {
            options.push(value);
        }
    }
    if !options.iter().any(|o| o == current) {
        options.push(current.to_string());
    }
    options
}

const AUTO_OPTION: &str = "auto";

fn audio_rows(cfg: &Config, devices: &[AudioDevice]) -> Vec<SettingsRow> {
    vec![
        row(
            "output driver",
            "the audio backend driver, or \"auto\" to let mpv choose",
            Control::Select {
                options: device_options(
                    &cfg.audio.output_driver,
                    devices.iter().map(|d| d.driver.clone()),
                ),
                get: |c| c.audio.output_driver.clone(),
                set: |c, v| c.audio.output_driver = v.to_string(),
            },
        ),
        row(
            "output device",
            "the audio output device, or \"auto\" for the system default",
            Control::Select {
                options: device_options(&cfg.audio.device_id, devices.iter().map(|d| d.id.clone())),
                get: |c| c.audio.device_id.clone(),
                set: |c, v| c.audio.device_id = v.to_string(),
            },
        ),
        row(
            "replaygain mode",
            "which loudness tag to normalize playback against",
            Control::Select {
                options: vec!["album".to_string(), "track".to_string(), "off".to_string()],
                get: |c| replaygain_mode_to_str(c.audio.default_replaygain).to_string(),
                set: |c, v| c.audio.default_replaygain = replaygain_mode_from_str(v),
            },
        ),
        row(
            "replaygain preamp",
            "extra gain applied on top of the resolved replaygain adjustment",
            Control::Slider {
                min: -12.0,
                max: 12.0,
                step: 0.5,
                unit: "dB",
                get: |c| c.audio.replaygain_preamp_db as f64,
                set: |c, v| c.audio.replaygain_preamp_db = v as f32,
            },
        ),
        row(
            "buffer size",
            "the audio engine's own output buffer size",
            Control::Number {
                min: 100,
                max: 10_000,
                get: |c| i64::from(c.audio.buffer_size_ms),
                set: |c, v| c.audio.buffer_size_ms = v.clamp(0, u32::MAX as i64) as u32,
            },
        ),
    ]
}

fn cache_rows() -> Vec<SettingsRow> {
    vec![
        row(
            "enabled",
            "keep a rolling local cache of recently played tracks",
            Control::Toggle {
                get: |c| c.cache.enabled,
                set: |c, v| c.cache.enabled = v,
            },
        ),
        row(
            "rolling cache size",
            "maximum size of the rolling play cache before the oldest entries are evicted",
            Control::Slider {
                min: 0.0,
                max: 500.0,
                step: 0.5,
                unit: "GB",
                get: |c| c.cache.rolling_max_gb,
                set: |c, v| c.cache.rolling_max_gb = v,
            },
        ),
        row(
            "image cache size",
            "maximum size of the cached artwork store",
            Control::Number {
                min: 0,
                max: 100_000,
                get: |c| i64::try_from(c.cache.image_cache_mb).unwrap_or(i64::MAX),
                set: |c, v| c.cache.image_cache_mb = v.clamp(0, i64::from(u32::MAX)) as u64,
            },
        ),
        row(
            "prefetch on play",
            "also cache the track being played (costs double bandwidth on first play)",
            Control::Toggle {
                get: |c| c.cache.prefetch_on_play,
                set: |c, v| c.cache.prefetch_on_play = v,
            },
        ),
        row(
            "prefetch next tracks",
            "how many upcoming queue tracks to cache ahead of the one playing (0 disables)",
            Control::Number {
                min: 0,
                max: i64::from(crate::config::MAX_PREFETCH_NEXT),
                get: |c| i64::from(c.cache.prefetch_next),
                set: |c, v| {
                    c.cache.prefetch_next =
                        v.clamp(0, i64::from(crate::config::MAX_PREFETCH_NEXT)) as u8
                },
            },
        ),
        row(
            "download directory",
            "where permanently pinned downloads are stored, or \"auto\" for the platform default",
            Control::Text {
                get: |c| c.cache.download_dir.clone(),
                set: |c, v| c.cache.download_dir = v,
                secret: false,
            },
        ),
        row(
            "cache directory",
            "where the rolling cache and artwork store live, or \"auto\" for the platform default",
            Control::Text {
                get: |c| c.cache.cache_dir.clone(),
                set: |c, v| c.cache.cache_dir = v,
                secret: false,
            },
        ),
    ]
}

fn transcode_rows() -> Vec<SettingsRow> {
    vec![
        row(
            "quality",
            "stream directly, or transcode at a fixed bitrate",
            Control::Select {
                options: vec![
                    "direct".to_string(),
                    "transcode_high".to_string(),
                    "transcode_med".to_string(),
                    "transcode_low".to_string(),
                ],
                get: |c| quality_profile_to_str(c.transcode.mode).to_string(),
                set: |c, v| c.transcode.mode = quality_profile_from_str(v),
            },
        ),
        row(
            "target codec",
            "the codec a transcode is requested in",
            Control::Select {
                options: vec!["mp3".to_string(), "aac".to_string(), "opus".to_string()],
                get: |c| target_codec_to_str(c.transcode.target_codec).to_string(),
                set: |c, v| c.transcode.target_codec = target_codec_from_str(v),
            },
        ),
        row(
            "download uncompressed",
            "permanent downloads keep the original file rather than the active transcode profile",
            Control::Toggle {
                get: |c| c.transcode.download_uncompressed,
                set: |c, v| c.transcode.download_uncompressed = v,
            },
        ),
    ]
}

fn interface_rows() -> Vec<SettingsRow> {
    vec![
        row(
            "theme",
            "the colour theme applied immediately",
            Control::Select {
                options: crate::theme::BUILTIN_THEME_NAMES
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                get: |c| c.ui.theme.clone(),
                set: |c, v| c.ui.theme = v.to_string(),
            },
        ),
        row(
            "album art protocol",
            "the terminal graphics protocol used to draw album art",
            Control::Select {
                options: vec![
                    "auto".to_string(),
                    "kitty".to_string(),
                    "sixel".to_string(),
                    "halfblocks".to_string(),
                    "off".to_string(),
                ],
                get: |c| art_protocol_to_str(c.ui.album_art_protocol).to_string(),
                set: |c, v| c.ui.album_art_protocol = art_protocol_from_str(v),
            },
        ),
        row(
            "desktop notifications",
            "a native OS toast on track change",
            Control::Toggle {
                get: |c| c.ui.desktop_notifications,
                set: |c, v| c.ui.desktop_notifications = v,
            },
        ),
        row(
            "mouse",
            "click and scroll support in the terminal, applied immediately",
            Control::Toggle {
                get: |c| c.ui.enable_mouse,
                set: |c, v| c.ui.enable_mouse = v,
            },
        ),
        row(
            "websocket remote control",
            "connect to the server's own WebSocket for remote control and library-change updates",
            Control::Toggle {
                get: |c| c.ui.enable_websocket,
                set: |c, v| c.ui.enable_websocket = v,
            },
        ),
        row(
            "restore session on launch",
            "resume the queue and playback position from the last run",
            Control::Toggle {
                get: |c| c.ui.restore_session,
                set: |c, v| c.ui.restore_session = v,
            },
        ),
        row(
            "auto-play on restore",
            "resume playing immediately rather than paused",
            Control::Toggle {
                get: |c| c.ui.restore_autoplay,
                set: |c, v| c.ui.restore_autoplay = v,
            },
        ),
        row(
            "clear saved session",
            "delete the saved queue and playback position; the next launch starts empty",
            Control::Action {
                label: "clear saved session".to_string(),
                action: SettingsRowAction::ClearSavedSession,
            },
        ),
        row(
            "ascii only",
            "avoid Unicode glyphs (spinners, progress bars, borders) for terminals with limited font support",
            Control::Toggle {
                get: |c| c.ui.ascii_only,
                set: |c, v| c.ui.ascii_only = v,
            },
        ),
        row(
            "now playing format",
            "player-bar line template: {title} {artist} {album} {album_artist} {year} {track_number} {disc_number} {genre} {duration}",
            Control::Text {
                get: |c| c.ui.now_playing_format.clone(),
                set: |c, v| c.ui.now_playing_format = v,
                secret: false,
            },
        ),
        row(
            "show lyrics",
            "show the lyrics pane in Now Playing and Zen when available",
            Control::Toggle {
                get: |c| c.ui.show_lyrics,
                set: |c, v| c.ui.show_lyrics = v,
            },
        ),
        row(
            "inspector artwork",
            "show artist and album artwork above the metadata pane (off saves network and memory)",
            Control::Toggle {
                get: |c| c.ui.show_inspector_art,
                set: |c, v| c.ui.show_inspector_art = v,
            },
        ),
        row(
            "log level",
            "verbosity of the on-disk log file",
            Control::Select {
                options: vec![
                    "trace".to_string(),
                    "debug".to_string(),
                    "info".to_string(),
                    "warn".to_string(),
                    "error".to_string(),
                ],
                get: |c| c.logging.level.clone(),
                set: |c, v| c.logging.level = v.to_string(),
            },
        ),
        row(
            "log file count",
            "how many rotated log files to keep",
            Control::Number {
                min: 1,
                max: 100,
                get: |c| i64::from(c.logging.max_files),
                set: |c, v| c.logging.max_files = v.clamp(1, i64::from(u8::MAX)) as u8,
            },
        ),
    ]
}

fn sorting_rows(cfg: &Config) -> Vec<SettingsRow> {
    let options: Vec<String> = cfg
        .sorting
        .profiles
        .iter()
        .map(|p| p.name.clone())
        .collect();
    vec![
        row(
            "default queue sort",
            "the sort profile new full-context queues are ordered by",
            Control::Select {
                options,
                get: |c| c.sorting.default_queue_profile.clone(),
                set: |c, v| c.sorting.default_queue_profile = v.to_string(),
            },
        ),
        row(
            "sort profiles",
            "add, edit, or remove named sort profiles",
            Control::Action {
                label: format!(
                    "manage sort profiles ({} defined)",
                    cfg.sorting.profiles.len()
                ),
                action: SettingsRowAction::ManageSortProfiles,
            },
        ),
    ]
}

fn equalizer_rows(cfg: &Config) -> Vec<SettingsRow> {
    let mut options: Vec<String> = config::FACTORY_EQ_PRESET_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect();
    options.extend(cfg.equalizer.custom_presets.iter().map(|p| p.name.clone()));
    vec![
        row(
            "enabled",
            "apply the active preset's gains to playback",
            Control::Toggle {
                get: |c| c.equalizer.enabled,
                set: |c, v| c.equalizer.enabled = v,
            },
        ),
        row(
            "active preset",
            "the equalizer curve currently applied",
            Control::Select {
                options,
                get: |c| c.equalizer.active_preset.clone(),
                set: |c, v| c.equalizer.active_preset = v.to_string(),
            },
        ),
        row(
            "custom presets",
            "add, edit, or remove custom equalizer presets",
            Control::Action {
                label: format!(
                    "manage EQ presets ({} custom)",
                    cfg.equalizer.custom_presets.len()
                ),
                action: SettingsRowAction::ManageEqPresets,
            },
        ),
    ]
}

fn keybindings_rows() -> Vec<SettingsRow> {
    vec![row(
        "keybindings",
        "remap any action's key; conflicts are shown below",
        Control::Action {
            label: "edit keybindings".to_string(),
            action: SettingsRowAction::EditKeybindings,
        },
    )]
}

fn replaygain_mode_to_str(mode: ReplayGainMode) -> &'static str {
    match mode {
        ReplayGainMode::Album => "album",
        ReplayGainMode::Track => "track",
        ReplayGainMode::Off => "off",
    }
}

fn replaygain_mode_from_str(s: &str) -> ReplayGainMode {
    match s {
        "track" => ReplayGainMode::Track,
        "off" => ReplayGainMode::Off,
        _ => ReplayGainMode::Album,
    }
}

fn quality_profile_to_str(profile: QualityProfile) -> &'static str {
    match profile {
        QualityProfile::Direct => "direct",
        QualityProfile::TranscodeHigh => "transcode_high",
        QualityProfile::TranscodeMed => "transcode_med",
        QualityProfile::TranscodeLow => "transcode_low",
    }
}

fn quality_profile_from_str(s: &str) -> QualityProfile {
    match s {
        "transcode_high" => QualityProfile::TranscodeHigh,
        "transcode_med" => QualityProfile::TranscodeMed,
        "transcode_low" => QualityProfile::TranscodeLow,
        _ => QualityProfile::Direct,
    }
}

fn target_codec_to_str(codec: TargetCodec) -> &'static str {
    match codec {
        TargetCodec::Mp3 => "mp3",
        TargetCodec::Aac => "aac",
        TargetCodec::Opus => "opus",
    }
}

fn target_codec_from_str(s: &str) -> TargetCodec {
    match s {
        "aac" => TargetCodec::Aac,
        "opus" => TargetCodec::Opus,
        _ => TargetCodec::Mp3,
    }
}

fn art_protocol_to_str(protocol: ArtProtocol) -> &'static str {
    match protocol {
        ArtProtocol::Auto => "auto",
        ArtProtocol::Kitty => "kitty",
        ArtProtocol::Sixel => "sixel",
        ArtProtocol::Halfblocks => "halfblocks",
        ArtProtocol::Off => "off",
    }
}

fn art_protocol_from_str(s: &str) -> ArtProtocol {
    match s {
        "kitty" => ArtProtocol::Kitty,
        "sixel" => ArtProtocol::Sixel,
        "halfblocks" => ArtProtocol::Halfblocks,
        "off" => ArtProtocol::Off,
        _ => ArtProtocol::Auto,
    }
}

/// One control's own current value, tagged by shape — what `AdjustValue`/text-edit-commit builds
/// to hand to [`try_apply`].
enum EditValue {
    Bool(bool),
    Str(String),
    F64(f64),
    I64(i64),
}

/// Applies `value` to `control` on a clone of `cfg`, then runs it through `config::validate` —
/// if validation corrected the very field just written (compared via the control's own `get`,
/// not a whole-`Config` diff, since validate touches unrelated fields too, e.g. device ids), the
/// edit is rejected: `Err(message)` from the most specific warning validate produced, and the
/// caller must not commit the draft. "The previous value is retained" (this task's own spec).
fn try_apply(cfg: &Config, control: &Control, value: EditValue) -> Result<Config, String> {
    let mut draft = cfg.clone();
    match (control, &value) {
        (Control::Toggle { set, .. }, EditValue::Bool(v)) => set(&mut draft, *v),
        (Control::Select { set, .. }, EditValue::Str(v)) => set(&mut draft, v),
        (Control::Slider { set, .. }, EditValue::F64(v)) => set(&mut draft, *v),
        (Control::Number { set, .. }, EditValue::I64(v)) => set(&mut draft, *v),
        (Control::Text { set, .. }, EditValue::Str(v)) => set(&mut draft, v.clone()),
        _ => return Err("this row cannot be edited this way".to_string()),
    }
    let warnings = config::validate(&mut draft);

    let accepted = match (control, &value) {
        (Control::Toggle { get, .. }, EditValue::Bool(v)) => get(&draft) == *v,
        (Control::Select { get, .. }, EditValue::Str(v)) => &get(&draft) == v,
        (Control::Slider { get, .. }, EditValue::F64(v)) => (get(&draft) - v).abs() < 1e-9,
        (Control::Number { get, .. }, EditValue::I64(v)) => get(&draft) == *v,
        (Control::Text { get, .. }, EditValue::Str(v)) => &get(&draft) == v,
        _ => false,
    };

    if accepted {
        Ok(draft)
    } else {
        Err(warnings
            .last()
            .map(|w| w.message.clone())
            .unwrap_or_else(|| "value rejected".to_string()))
    }
}

/// The `SystemEvent::ConfigChanged` handler (`reducer::mod`'s own dispatch table already named
/// this module as its owner, before this task ever gave it a real implementation) — commits
/// `new_cfg` as the live config: live-applies theme/mouse (the two that need more than a fresh
/// read at point-of-use — `docs/12-decisions.md`), arms the debounced write, and clears any stale
/// inline error. Every settings edit funnels through this same function, whether it came from
/// `SettingsAction` (this module's own dispatch below) or, in principle, anywhere else that ever
/// wants to change the live config wholesale.
pub(crate) fn on_config_changed(state: &mut AppState, new_cfg: Config) -> Vec<Effect> {
    let mut effects = Vec::new();
    let theme_changed = new_cfg.ui.theme != state.config.ui.theme;
    let ascii_changed = new_cfg.ui.ascii_only != state.config.ui.ascii_only;
    let mouse_changed = new_cfg.ui.enable_mouse != state.config.ui.enable_mouse;
    let quality_changed = new_cfg.transcode.mode != state.config.transcode.mode;
    let replay_gain_changed =
        new_cfg.audio.default_replaygain != state.config.audio.default_replaygain;
    let eq_changed = new_cfg.equalizer.enabled != state.config.equalizer.enabled
        || new_cfg.equalizer.active_preset != state.config.equalizer.active_preset;

    state.config = new_cfg;
    state.settings.error = None;

    if theme_changed {
        // `validate_theme` already guarantees `ui.theme` names a real builtin by the time it
        // reaches here (an unknown name is corrected before `try_apply` would ever call this
        // `accepted`), so this always succeeds.
        if let Some(theme) = crate::theme::Theme::builtin(&state.config.ui.theme) {
            state.theme = theme;
        }
    }
    // Re-derived from the theme's own baseline, so turning the setting back off restores whatever
    // the theme itself asked for rather than leaving it stuck on.
    if (ascii_changed || theme_changed)
        && let Some(theme) = crate::theme::Theme::builtin(&state.config.ui.theme)
    {
        state.theme.ascii_only = theme.ascii_only || state.config.ui.ascii_only;
    }
    if mouse_changed {
        effects.push(Effect::Sys(SysEffect::SetMouseCapture(
            state.config.ui.enable_mouse,
        )));
    }

    // The audio settings below have a *runtime* mirror on `state.player` that the rest of the app
    // actually reads — `hydrate_from_config` establishes it at startup, and changing the config
    // without updating it here is why the Settings rows for quality, ReplayGain and the equalizer
    // saved correctly but changed nothing until the next launch (`docs/12-decisions.md`). Each is
    // the same edit `q`/`r`/`e` already make from the keyboard.
    if quality_changed {
        state.player.quality_profile = state.config.transcode.mode;
        // A locally-served track is whatever file is already on disk; re-fetching it at a new
        // profile would mean nothing. The preference still applies from the next track on, which
        // is why this is a skipped reload rather than `cycle_quality`'s outright refusal — a
        // *setting* the user typed into Settings must not be silently discarded.
        let served_locally = state.queue.current().is_some_and(|e| {
            matches!(
                e.availability,
                crate::state::queue::Availability::Cached
                    | crate::state::queue::Availability::Downloaded
            )
        });
        if !served_locally {
            effects.extend(super::player::reload_at_current_profile(state));
        }
    }
    if replay_gain_changed {
        state.player.replay_gain = state.config.audio.default_replaygain;
        super::player::apply_replay_gain(state);
        effects.push(Effect::Audio(crate::effect::AudioEffect::SetReplayGain(
            state.player.replay_gain,
        )));
    }
    if eq_changed {
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
        effects.push(Effect::Audio(crate::effect::AudioEffect::SetEq(
            super::player::eq_curve(&state.player.eq),
        )));
    }

    state.settings_write_debounce_until = state
        .clock
        .checked_add(WRITE_DEBOUNCE)
        .ok()
        .or(Some(state.clock));
    state.touch();
    effects
}

pub fn apply(state: &mut AppState, action: SettingsAction) -> Vec<Effect> {
    match action {
        SettingsAction::SetSection(section) => {
            state.settings.section = section;
            state.settings.cursor = 0;
            state.settings.editing = None;
            state.settings.reveal_secret = false;
            state.settings.error = None;
            state.touch();
            Vec::new()
        }
        SettingsAction::NextSection => {
            state.settings.section = state.settings.section.next();
            reset_row_cursor(state);
            Vec::new()
        }
        SettingsAction::PrevSection => {
            state.settings.section = state.settings.section.prev();
            reset_row_cursor(state);
            Vec::new()
        }
        SettingsAction::FocusSectionList => {
            // Reached from the rows (`Esc`) or from the tab sidebar (`→`/`Enter`) — either way the
            // section list is now the sole focused level.
            state.settings.section_list_focused = true;
            state.nav.sidebar_focused = false;
            state.touch();
            Vec::new()
        }
        SettingsAction::FocusRows => {
            state.settings.section_list_focused = false;
            state.touch();
            Vec::new()
        }
        SettingsAction::LeaveToTabSidebar => {
            state.settings.section_list_focused = false;
            state.nav.sidebar_focused = true;
            state.touch();
            Vec::new()
        }
        SettingsAction::MoveRow(delta) => {
            if state.settings.server_editor.is_some() {
                return server_editor_move(state, delta);
            }
            if state.settings.sort_profile_editor.is_some() {
                return sort_editor_move(state, delta);
            }
            if state.settings.eq_preset_editor.is_some() {
                return eq_editor_move(state, delta);
            }
            let rows = rows_for_section(
                state.settings.section,
                &state.config,
                &state.player.known_devices,
            );
            if rows.is_empty() {
                return Vec::new();
            }
            let new_cursor = (state.settings.cursor as i64 + i64::from(delta))
                .clamp(0, rows.len() as i64 - 1) as usize;
            state.settings.cursor = new_cursor;
            state.settings.reveal_secret = false;
            state.settings.error = None;
            state.touch();
            Vec::new()
        }
        SettingsAction::SetRow(index) => {
            if state.settings.server_editor.is_some() {
                return server_editor_set_row(state, index);
            }
            let rows = rows_for_section(
                state.settings.section,
                &state.config,
                &state.player.known_devices,
            );
            if index < rows.len() {
                state.settings.cursor = index;
                state.settings.reveal_secret = false;
                state.settings.error = None;
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::AdjustValue(delta) => adjust_value(state, delta),
        SettingsAction::StartTextEdit => {
            if state.settings.server_editor.is_some() {
                return server_editor_start_text_edit(state);
            }
            let rows = rows_for_section(
                state.settings.section,
                &state.config,
                &state.player.known_devices,
            );
            if let Some(SettingsRow {
                control: Control::Text { get, .. },
                ..
            }) = rows.get(state.settings.cursor)
            {
                state.settings.editing = Some(TextEdit::new(get(&state.config)));
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::TextInput(c) => {
            if let Some(buf) = active_text_buffer(state) {
                buf.insert(c);
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::TextBackspace => {
            if let Some(buf) = active_text_buffer(state) {
                buf.backspace();
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::TextDeleteForward => {
            if let Some(buf) = active_text_buffer(state) {
                buf.delete_forward();
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::TextCursorLeft => {
            if let Some(buf) = active_text_buffer(state) {
                buf.move_left();
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::TextCursorRight => {
            if let Some(buf) = active_text_buffer(state) {
                buf.move_right();
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::TextCursorHome => {
            if let Some(buf) = active_text_buffer(state) {
                buf.move_home();
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::TextCursorEnd => {
            if let Some(buf) = active_text_buffer(state) {
                buf.move_end();
                state.touch();
            }
            Vec::new()
        }
        SettingsAction::CommitTextEdit => {
            if state.settings.server_editor.is_some() {
                return server_editor_commit_text_edit(state);
            }
            if state.settings.sort_profile_editor.is_some() {
                return sort_editor_commit_name(state);
            }
            if state.settings.eq_preset_editor.is_some() {
                return eq_editor_commit_name(state);
            }
            commit_text_edit(state)
        }
        SettingsAction::CancelTextEdit => {
            if let Some(editor) = &mut state.settings.server_editor
                && let Some(draft) = &mut editor.editing
            {
                draft.text_buf = None;
                state.touch();
                return Vec::new();
            }
            if let Some(editor) = &mut state.settings.sort_profile_editor
                && editor.name_buf.is_some()
            {
                editor.name_buf = None;
                editor.creating = false;
                state.touch();
                return Vec::new();
            }
            if let Some(editor) = &mut state.settings.eq_preset_editor
                && editor.name_buf.is_some()
            {
                editor.name_buf = None;
                editor.creating = false;
                editor.captured_gains = None;
                state.touch();
                return Vec::new();
            }
            state.settings.editing = None;
            state.touch();
            Vec::new()
        }
        SettingsAction::ToggleRevealSecret => {
            if state.settings.server_editor.is_some() {
                return server_editor_toggle_reveal(state);
            }
            state.settings.reveal_secret = !state.settings.reveal_secret;
            state.touch();
            Vec::new()
        }
        SettingsAction::ActivateRow => activate_row(state),
        SettingsAction::ServerEditorAddNew => server_editor_add_new(state),
        SettingsAction::ServerEditorEdit => server_editor_edit(state),
        SettingsAction::ServerEditorCycleProtocol(delta) => {
            server_editor_cycle_protocol(state, delta)
        }
        SettingsAction::ServerEditorRemove => server_editor_remove(state),
        SettingsAction::ServerEditorToggleDeleteData => server_editor_toggle_delete_data(state),
        SettingsAction::ServerEditorSwitch => server_editor_switch(state),
        SettingsAction::ServerEditorSwitchConfirmed(id) => {
            server_editor_switch_confirmed(state, id)
        }
        SettingsAction::ServerEditorAddEndpoint => server_editor_add_endpoint(state),
        SettingsAction::ServerEditorRemoveEndpoint => server_editor_remove_endpoint(state),
        SettingsAction::ServerEditorAddHeader => server_editor_add_header(state),
        SettingsAction::ServerEditorRemoveFocusedHeader => {
            server_editor_remove_focused_header(state)
        }
        SettingsAction::ServerEditorTestConnection => server_editor_test_connection(state, false),
        SettingsAction::ServerEditorSave => server_editor_test_connection(state, true),
        SettingsAction::ServerEditorClose => server_editor_close(state),
        SettingsAction::SortEditorNew => sort_editor_new(state),
        SettingsAction::SortEditorRename => sort_editor_rename(state),
        SettingsAction::SortEditorDeleteProfile => sort_editor_delete_profile(state),
        SettingsAction::SortEditorDeleteProfileConfirmed => {
            sort_editor_delete_profile_confirmed(state)
        }
        SettingsAction::SortEditorRestoreDefaults => sort_editor_restore_defaults(state),
        SettingsAction::SortEditorAddRule => sort_editor_add_rule(state),
        SettingsAction::SortEditorDeleteRule => sort_editor_delete_rule(state),
        SettingsAction::SortEditorReorderRule(delta) => sort_editor_reorder_rule(state, delta),
        SettingsAction::SortEditorCycleField(delta) => sort_editor_cycle_field(state, delta),
        SettingsAction::SortEditorToggleDirection => sort_editor_toggle_direction(state),
        SettingsAction::SortEditorApply => sort_editor_apply(state),
        SettingsAction::SortEditorClose => sort_editor_close(state),
        SettingsAction::EqEditorSaveCurrent => eq_editor_save_current(state),
        SettingsAction::EqEditorRename => eq_editor_rename(state),
        SettingsAction::EqEditorDelete => eq_editor_delete(state),
        SettingsAction::EqEditorOpenEqualizer => eq_editor_open_equalizer(state),
        SettingsAction::EqEditorClose => eq_editor_close(state),
        SettingsAction::ClearSavedSessionConfirmed => clear_saved_session_confirmed(state),
        SettingsAction::AboutToggleLicences => about_toggle_licences(state),
        SettingsAction::AboutLicencesScroll(delta) => about_licences_scroll(state, delta),
        SettingsAction::AboutCopyDiagnostics => about_copy_diagnostics(state),
    }
}

fn about_toggle_licences(state: &mut AppState) -> Vec<Effect> {
    state.settings.about_licences_open = !state.settings.about_licences_open;
    state.settings.about_licences_scroll = 0;
    state.touch();
    Vec::new()
}

fn about_licences_scroll(state: &mut AppState, delta: i32) -> Vec<Effect> {
    state.settings.about_licences_scroll = state
        .settings
        .about_licences_scroll
        .saturating_add_signed(delta as isize);
    state.touch();
    Vec::new()
}

/// `11-07`: the `[d]` diagnostics dump — version, licence, libmpv status, the keybinding conflict
/// count (the same count the Keybindings row's own badge and section already show, `04-05`), and
/// the active config's non-secret values. `Config`'s own hand-written `Debug`
/// (`config::schema::ServerConfig`) already redacts `access_token` and every custom header
/// *value* (fixed alongside this task after finding it hadn't, for `custom_headers`) — reused
/// directly rather than hand-picking fields a second time, which would only risk the two lists
/// drifting apart.
fn about_diagnostics_text(state: &AppState) -> String {
    let conflicts = state.keymap.validate().len();
    format!(
        "loxia {}\nLicence GPL-3.0-or-later\nlibmpv {:?}\nKeybinding conflicts: {conflicts}\n\nConfig:\n{:#?}",
        env!("CARGO_PKG_VERSION"),
        state.about.libmpv,
        state.config,
    )
}

fn about_copy_diagnostics(state: &mut AppState) -> Vec<Effect> {
    let text = about_diagnostics_text(state);
    state.toast("diagnostics copied to clipboard", ToastLevel::Info);
    state.touch();
    vec![Effect::Sys(SysEffect::CopyToClipboard(text))]
}

/// `11-02`: `EditKeybindings` is the first of the four `Control::Action` rows (`11-01`'s own doc
/// comment on `SettingsRowAction`) to get real behaviour — opens the keymap editor exactly like
/// any other `ModalAction::Open` would. `ManageServers`/`ManageSortProfiles`/`ManageEqPresets`
/// remain documented no-ops until `11-03`/`11-04`/`11-05` (`docs/12-decisions.md`).
fn activate_row(state: &mut AppState) -> Vec<Effect> {
    let rows = rows_for_section(
        state.settings.section,
        &state.config,
        &state.player.known_devices,
    );
    match rows.get(state.settings.cursor).map(|r| &r.control) {
        Some(Control::Action {
            action: SettingsRowAction::EditKeybindings,
            ..
        }) => {
            crate::reducer::modal::open_modal(state, crate::state::modal::ModalKind::KeymapEditor)
        }
        // `11-03`: the second of the four `Control::Action` rows (`11-01`'s own doc comment on
        // `SettingsRowAction`) to get real behaviour — opens the profile-management sub-view
        // instead of the section's normal row list.
        Some(Control::Action {
            action: SettingsRowAction::ManageServers,
            ..
        }) => {
            state.settings.server_editor = Some(ServerEditorState::default());
            state.touch();
            Vec::new()
        }
        // `11-04`: the third — opens the sort-profile management sub-view.
        Some(Control::Action {
            action: SettingsRowAction::ManageSortProfiles,
            ..
        }) => {
            state.settings.sort_profile_editor = Some(SortProfileEditorState::default());
            state.touch();
            Vec::new()
        }
        // `11-05`: the fourth and last — opens the EQ-preset management sub-view. Every
        // `SettingsRowAction` variant now has real behaviour; none of the four is a documented
        // no-op any more (`docs/12-decisions.md`).
        Some(Control::Action {
            action: SettingsRowAction::ManageEqPresets,
            ..
        }) => {
            state.settings.eq_preset_editor = Some(EqPresetEditorState::default());
            state.touch();
            Vec::new()
        }
        // `11-06`: the one `Control::Action` row that isn't a sub-editor — opens a `Confirm`
        // rather than acting immediately, since deleting the saved session is destructive and, per
        // this task's own spec, must ask first.
        Some(Control::Action {
            action: SettingsRowAction::ClearSavedSession,
            ..
        }) => crate::reducer::modal::open_confirm(
            state,
            "clear the saved session? the current queue and playback position will be deleted.",
            Action::Settings(SettingsAction::ClearSavedSessionConfirmed),
        ),
        _ => Vec::new(),
    }
}

/// The `Confirm`'s own `on_confirm` payload — reuses `queue::clear` verbatim (the same "stop
/// playback, empty the queue" the server-switch confirmation already relies on) and adds the one
/// piece that flow doesn't need: actually deleting `session.json` from disk, so a relaunch doesn't
/// just restore the very queue that was just cleared.
fn clear_saved_session_confirmed(state: &mut AppState) -> Vec<Effect> {
    let mut effects = crate::reducer::queue::clear(state);
    effects.push(Effect::Cache(Box::new(CacheEffect::DeleteSession)));
    effects
}

fn reset_row_cursor(state: &mut AppState) {
    state.settings.cursor = 0;
    state.settings.editing = None;
    state.settings.reveal_secret = false;
    state.settings.error = None;
    state.touch();
}

fn adjust_value(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let rows = rows_for_section(
        state.settings.section,
        &state.config,
        &state.player.known_devices,
    );
    let Some(row) = rows.get(state.settings.cursor) else {
        return Vec::new();
    };
    let row_index = state.settings.cursor;

    let value = match &row.control {
        Control::Toggle { get, .. } => EditValue::Bool(!get(&state.config)),
        Control::Select { options, get, .. } => {
            if options.is_empty() {
                return Vec::new();
            }
            let current = get(&state.config);
            let idx = options.iter().position(|o| *o == current).unwrap_or(0) as i64;
            let len = options.len() as i64;
            let next = (idx + i64::from(delta)).rem_euclid(len) as usize;
            EditValue::Str(options[next].clone())
        }
        Control::Slider { min, max, step, .. } => {
            let current = match &row.control {
                Control::Slider { get, .. } => get(&state.config),
                _ => unreachable!(),
            };
            EditValue::F64((current + f64::from(delta) * step).clamp(*min, *max))
        }
        Control::Number { min, max, .. } => {
            let current = match &row.control {
                Control::Number { get, .. } => get(&state.config),
                _ => unreachable!(),
            };
            EditValue::I64((current + i64::from(delta)).clamp(*min, *max))
        }
        Control::Text { .. } | Control::Action { .. } => return Vec::new(),
    };

    match try_apply(&state.config, &row.control, value) {
        Ok(new_cfg) => on_config_changed(state, new_cfg),
        Err(message) => {
            state.settings.error = Some((row_index, message));
            state.touch();
            Vec::new()
        }
    }
}

/// Whichever of the four Settings text buffers is currently under edit, if any — the one place
/// `TextInput`/`TextBackspace`/`TextDeleteForward`/the cursor-movement actions all reach through,
/// so none of them need to know *which* of the four owns it, only that one does. Priority order
/// (server draft field, then sort-profile name, then EQ-preset name, then the plain row editor)
/// matches every pre-existing branch this replaces — at most one is ever `Some` at a time in
/// practice, since only one sub-editor can be open at once.
fn active_text_buffer(state: &mut AppState) -> Option<&mut TextEdit> {
    if let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
        && draft.text_buf.is_some()
    {
        return draft.text_buf.as_mut();
    }
    if let Some(editor) = state.settings.sort_profile_editor.as_mut()
        && editor.name_buf.is_some()
    {
        return editor.name_buf.as_mut();
    }
    if let Some(editor) = state.settings.eq_preset_editor.as_mut()
        && editor.name_buf.is_some()
    {
        return editor.name_buf.as_mut();
    }
    state.settings.editing.as_mut()
}

fn commit_text_edit(state: &mut AppState) -> Vec<Effect> {
    let Some(text) = state.settings.editing.take() else {
        return Vec::new();
    };
    let rows = rows_for_section(
        state.settings.section,
        &state.config,
        &state.player.known_devices,
    );
    let row_index = state.settings.cursor;
    let Some(row) = rows.get(row_index) else {
        return Vec::new();
    };
    if !matches!(row.control, Control::Text { .. }) {
        return Vec::new();
    }
    match try_apply(&state.config, &row.control, EditValue::Str(text.text)) {
        Ok(new_cfg) => on_config_changed(state, new_cfg),
        Err(message) => {
            state.settings.error = Some((row_index, message));
            state.touch();
            Vec::new()
        }
    }
}

/// `reducer::tick`'s own half of "persistence is debounced 1 second" — fires the actual write
/// once the deadline set by `commit` above has passed, mirroring `nav::maybe_fire_search`'s
/// identical shape for its own 250ms debounce.
pub(crate) fn maybe_write_config(state: &mut AppState, now: Timestamp) -> Vec<Effect> {
    let Some(deadline) = state.settings_write_debounce_until else {
        return Vec::new();
    };
    if now < deadline {
        return Vec::new();
    }
    state.settings_write_debounce_until = None;
    vec![Effect::Sys(SysEffect::WriteConfig(Box::new(
        state.config.clone(),
    )))]
}

// ---------------------------------------------------------------------------------------------
// `11-03`: server profiles (`docs/02-data-model.md` §8, `docs/03-emby-api.md` §2).
// ---------------------------------------------------------------------------------------------

/// Custom-header names loxia itself always sets — duplicated from `loxia_emby::client::
/// RESERVED_HEADERS` rather than imported, since `loxia-core` cannot depend on `loxia-emby`
/// (`docs/01-architecture.md` §3.2); the inline "cannot be overridden" check this task's own spec
/// asks for needs the list before a profile is ever saved, let alone turned into a real
/// `EmbyClient`. Kept in sync by hand — both lists are small, fixed, and change essentially never.
const RESERVED_HEADER_NAMES: [&str; 4] = [
    "authorization",
    "x-emby-authorization",
    "host",
    "content-length",
];

/// The add/edit form's own field order — header rows are spliced in between `Password` and
/// `NewHeaderName`, one per `draft.headers` entry, so the total count (and therefore every field
/// index after `Password`) depends on how many headers exist. `pub` (not `pub(crate)`): both
/// `crates/loxia`'s own input layer and `crates/loxia-tui`'s renderer need this same layout.
pub fn server_draft_fields(draft: &ServerDraft) -> Vec<ServerDraftField> {
    let mut fields = vec![
        ServerDraftField::Name,
        ServerDraftField::Endpoint,
        ServerDraftField::AddEndpointAction,
        ServerDraftField::Protocol,
        ServerDraftField::Host,
        ServerDraftField::Port,
        ServerDraftField::Username,
        ServerDraftField::Password,
    ];
    fields.extend((0..draft.headers.len()).map(ServerDraftField::Header));
    fields.push(ServerDraftField::NewHeaderName);
    fields.push(ServerDraftField::NewHeaderValue);
    fields.push(ServerDraftField::AddHeaderAction);
    fields.push(ServerDraftField::TestConnectionAction);
    fields.push(ServerDraftField::SaveAction);
    fields
}

pub fn server_draft_field_count(draft: &ServerDraft) -> usize {
    // `Name`/`Endpoint`/`AddEndpointAction`/`Protocol`/`Host`/`Port`/`Username`/`Password` (8)
    // + one per header +
    // `NewHeaderName`/`NewHeaderValue`/`AddHeaderAction`/`TestConnectionAction`/`SaveAction` (5) —
    // computed directly rather than calling `server_draft_fields(draft).len()` so counting doesn't
    // allocate a throwaway `Vec`.
    8 + draft.headers.len() + 5
}

pub fn server_draft_field_at(draft: &ServerDraft, index: usize) -> Option<ServerDraftField> {
    server_draft_fields(draft).get(index).copied()
}

fn server_editor_move(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.server_editor else {
        return Vec::new();
    };
    if let Some(draft) = &mut editor.editing {
        let count = server_draft_field_count(draft);
        if count == 0 {
            return Vec::new();
        }
        draft.field = (draft.field as i64 + i64::from(delta)).clamp(0, count as i64 - 1) as usize;
        draft.reveal_password = false;
        draft.reveal_header = None;
    } else {
        let count = state.config.servers.len();
        if count == 0 {
            return Vec::new();
        }
        editor.cursor =
            (editor.cursor as i64 + i64::from(delta)).clamp(0, count as i64 - 1) as usize;
        editor.error = None;
    }
    state.touch();
    Vec::new()
}

fn server_editor_set_row(state: &mut AppState, index: usize) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.server_editor else {
        return Vec::new();
    };
    if let Some(draft) = &mut editor.editing {
        if index < server_draft_field_count(draft) {
            draft.field = index;
            draft.reveal_password = false;
            draft.reveal_header = None;
        }
    } else if index < state.config.servers.len() {
        editor.cursor = index;
        editor.error = None;
    }
    state.touch();
    Vec::new()
}

fn server_editor_start_text_edit(state: &mut AppState) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    let current = match server_draft_field_at(draft, draft.field) {
        Some(ServerDraftField::Name) => draft.name.clone(),
        Some(ServerDraftField::Host) => draft.host.clone(),
        Some(ServerDraftField::Port) => draft.port.clone(),
        Some(ServerDraftField::Username) => draft.username.clone(),
        Some(ServerDraftField::Password) => draft.password.clone(),
        Some(ServerDraftField::NewHeaderName) => draft.new_header_name.clone(),
        Some(ServerDraftField::NewHeaderValue) => draft.new_header_value.clone(),
        // A header row, an action row, or `Protocol` (cycled, never typed) has nothing to "enter"
        // as text.
        _ => return Vec::new(),
    };
    draft.text_buf = Some(TextEdit::new(current));
    state.touch();
    Vec::new()
}

fn server_editor_commit_text_edit(state: &mut AppState) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    let Some(text) = draft.text_buf.take().map(|e| e.text) else {
        return Vec::new();
    };
    let field = server_draft_field_at(draft, draft.field);
    match field {
        Some(ServerDraftField::Name) => draft.name = text,
        Some(ServerDraftField::Host) => draft.host = text,
        Some(ServerDraftField::Port) => draft.port = text,
        Some(ServerDraftField::Username) => draft.username = text,
        Some(ServerDraftField::Password) => draft.password = text,
        Some(ServerDraftField::NewHeaderName) => draft.new_header_name = text,
        Some(ServerDraftField::NewHeaderValue) => draft.new_header_value = text,
        _ => {}
    }
    // Editing any connection field invalidates whatever `Test connection` last found — a stale
    // "connected to ..." next to a host that has since changed would be actively misleading.
    // `Protocol` gets the same treatment directly from `server_editor_cycle_protocol`, since it
    // never passes through here.
    if matches!(
        field,
        Some(
            ServerDraftField::Name
                | ServerDraftField::Host
                | ServerDraftField::Port
                | ServerDraftField::Username
                | ServerDraftField::Password
        )
    ) {
        draft.test_result = None;
    }
    state.touch();
    Vec::new()
}

/// `11-03`(field extension): the two values `[←→]`/`Enter` cycles `ServerDraftField::Protocol`
/// through — always exactly one of these two, never free text.
const URL_PROTOCOLS: [&str; 2] = ["http", "https"];

/// `[←→]`, which means "cycle whatever this row offers": the protocol on the protocol row, and the
/// selected address on the endpoint row.
fn server_editor_cycle_protocol(state: &mut AppState, delta: i32) -> Vec<Effect> {
    if server_editor_focused_field(state) == Some(ServerDraftField::Endpoint) {
        return server_editor_cycle_endpoint(state, delta);
    }
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    if server_draft_field_at(draft, draft.field) != Some(ServerDraftField::Protocol) {
        return Vec::new();
    }
    let idx = URL_PROTOCOLS
        .iter()
        .position(|p| *p == draft.protocol)
        .unwrap_or(0) as i64;
    let next = (idx + i64::from(delta)).rem_euclid(URL_PROTOCOLS.len() as i64) as usize;
    draft.protocol = URL_PROTOCOLS[next].to_string();
    draft.test_result = None;
    state.touch();
    Vec::new()
}

fn server_editor_focused_field(state: &AppState) -> Option<ServerDraftField> {
    let draft = state.settings.server_editor.as_ref()?.editing.as_ref()?;
    server_draft_field_at(draft, draft.field)
}

fn server_editor_cycle_endpoint(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    let count = draft.endpoints.len() as i64;
    if count <= 1 {
        return Vec::new();
    }
    let next = (draft.endpoint as i64 + i64::from(delta)).rem_euclid(count) as usize;
    draft.select_endpoint(next);
    draft.test_result = None;
    state.touch();
    Vec::new()
}

/// `+` on the endpoint row — a further address for this same server, selected immediately so the
/// fields below are about it.
fn server_editor_add_endpoint(state: &mut AppState) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    draft.stash_endpoint();
    draft
        .endpoints
        .push(crate::state::settings::ServerEndpointDraft {
            // The address is what differs; the headers are almost always the *reason* a second address
            // exists (a proxy's own token), so a new one starts empty rather than copying the primary's.
            protocol: "https".to_string(),
            ..Default::default()
        });
    let last = draft.endpoints.len() - 1;
    draft.select_endpoint(last);
    draft.test_result = None;
    state.touch();
    Vec::new()
}

/// `x` on the endpoint row. The primary cannot be removed — a profile without an address is not a
/// profile — so this only ever drops a fallback.
fn server_editor_remove_endpoint(state: &mut AppState) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    if draft.endpoint == 0 || draft.endpoint >= draft.endpoints.len() {
        return Vec::new();
    }
    draft.endpoints.remove(draft.endpoint);
    let previous = draft.endpoint - 1;
    draft.endpoint = previous;
    // Not `select_endpoint`: the values on screen belong to the endpoint that has just been
    // deleted, so they must be discarded rather than stashed into its neighbour.
    let selected = draft.endpoints[previous].clone();
    draft.protocol = selected.protocol;
    draft.host = selected.host;
    draft.port = selected.port;
    draft.headers = selected.headers;
    draft.text_buf = None;
    draft.test_result = None;
    state.touch();
    Vec::new()
}

/// Builds the single `http(s)://host[:port]` string `ServerConfig.url`/`Effect::Net(
/// TestServerConnection)` actually need, from the draft's own three fields — Emby's own base URL
/// is never more than scheme + host + an optional port (no path, query, or userinfo), so this
/// avoids needing a real URL-parsing dependency in `loxia-core` (which cannot depend on `url`/
/// `reqwest` at all, the same boundary `loxia_emby::client::parse_base_url` sits on the other side
/// of) for something this simple.
pub(crate) fn build_server_url(protocol: &str, host: &str, port: &str) -> String {
    let host = host.trim();
    let port = port.trim();
    if port.is_empty() {
        format!("{protocol}://{host}")
    } else {
        format!("{protocol}://{host}:{port}")
    }
}

/// The inverse — splits an existing `ServerConfig.url` into `(protocol, host, port)` for the
/// add/edit form to prefill when editing an existing profile. Deliberately simple, the same way
/// `build_server_url` is: this only ever needs to round-trip what that function itself produces,
/// plus whatever a user might have hand-typed into `config.toml`'s own `url` field. A bracketed
/// IPv6 literal host (`[::1]:8096`) is a known, accepted gap — `rsplit_once(':')` would split
/// inside the brackets rather than before them — nobody using this feature is expected to run an
/// Emby server on a raw IPv6 address in practice (`docs/12-decisions.md`).
pub(crate) fn split_server_url(url: &str) -> (String, String, String) {
    let (protocol, rest) = url.split_once("://").unwrap_or(("https", url));
    let rest = rest.split('/').next().unwrap_or(rest); // strip any accidental path or trailing slash
    match rest.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            (protocol.to_string(), host.to_string(), port.to_string())
        }
        _ => (protocol.to_string(), rest.to_string(), String::new()),
    }
}

fn server_editor_toggle_reveal(state: &mut AppState) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    match server_draft_field_at(draft, draft.field) {
        Some(ServerDraftField::Password) => draft.reveal_password = !draft.reveal_password,
        Some(ServerDraftField::Header(i)) => {
            draft.reveal_header = if draft.reveal_header == Some(i) {
                None
            } else {
                Some(i)
            };
        }
        _ => {}
    }
    state.touch();
    Vec::new()
}

fn server_editor_add_new(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.server_editor else {
        return Vec::new();
    };
    editor.editing = Some(ServerDraft {
        // Stamped immediately, not deferred to a later `config::validate()` fill-in — "generated
        // once per profile" (this task's own spec), and there is no more natural "once" than the
        // moment the profile is conceived.
        device_id: config::generate_uuid_v4(),
        // `https` is the more common real-world default (a cloud/reverse-proxied Emby, or any
        // server reachable outside the local network) — `http` is one `[←→]` away.
        protocol: "https".to_string(),
        endpoints: vec![crate::state::settings::ServerEndpointDraft {
            protocol: "https".to_string(),
            ..Default::default()
        }],
        ..ServerDraft::default()
    });
    editor.error = None;
    state.touch();
    Vec::new()
}

/// `user_id`/`access_token` are deliberately left blank in the draft (`docs/12-decisions.md`:
/// "derived, not typed") — editing any *other* field of an existing profile and saving again
/// re-authenticates from scratch, requiring the password to be re-entered. This is the direct,
/// intended consequence of never presenting a token field at all, not an oversight.
fn server_editor_edit(state: &mut AppState) -> Vec<Effect> {
    let Some(cursor) = state.settings.server_editor.as_ref().map(|e| e.cursor) else {
        return Vec::new();
    };
    let Some(server) = state.config.servers.get(cursor).cloned() else {
        return Vec::new();
    };
    let Some(editor) = &mut state.settings.server_editor else {
        return Vec::new();
    };
    let (protocol, host, port) = split_server_url(&server.url);
    let mut endpoints = vec![crate::state::settings::ServerEndpointDraft {
        protocol: protocol.clone(),
        host: host.clone(),
        port: port.clone(),
        headers: server.custom_headers.clone().into_iter().collect(),
    }];
    endpoints.extend(server.fallbacks.iter().map(|endpoint| {
        let (protocol, host, port) = split_server_url(&endpoint.url);
        crate::state::settings::ServerEndpointDraft {
            protocol,
            host,
            port,
            headers: endpoint.custom_headers.clone().into_iter().collect(),
        }
    }));
    editor.editing = Some(ServerDraft {
        id: Some(server.id),
        device_id: server.device_id,
        name: server.name,
        protocol,
        host,
        port,
        headers: server.custom_headers.into_iter().collect(),
        endpoints,
        ..ServerDraft::default()
    });
    editor.error = None;
    state.touch();
    Vec::new()
}

fn server_editor_remove(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.settings.server_editor else {
        return Vec::new();
    };
    if editor.editing.is_some() {
        return Vec::new(); // only meaningful from the plain profile list
    }
    let Some(server) = state.config.servers.get(editor.cursor).cloned() else {
        return Vec::new();
    };

    if server.id == state.config.active_server {
        if let Some(editor) = &mut state.settings.server_editor {
            editor.error =
                Some("switch to another server before removing the active one".to_string());
        }
        state.touch();
        return Vec::new();
    }

    let delete_data = editor.delete_data_on_remove;
    state.config.servers.retain(|s| s.id != server.id);
    if let Some(editor) = &mut state.settings.server_editor {
        editor.cursor = editor
            .cursor
            .min(state.config.servers.len().saturating_sub(1));
        editor.error = None;
        editor.delete_data_on_remove = false;
    }
    state.touch();

    let mut effects = vec![Effect::Sys(SysEffect::WriteConfig(Box::new(
        state.config.clone(),
    )))];
    if delete_data {
        effects.push(Effect::Cache(Box::new(CacheEffect::DeleteServerData(
            ServerId::from(server.id),
        ))));
    }
    effects
}

fn server_editor_toggle_delete_data(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.server_editor else {
        return Vec::new();
    };
    editor.delete_data_on_remove = !editor.delete_data_on_remove;
    state.touch();
    Vec::new()
}

/// Opens the `Confirm` this task's own spec requires ("switching is disruptive and must be
/// explicit"). A no-op if the focused profile is already active — there is nothing to switch to,
/// and showing a confirmation for that would be actively confusing.
fn server_editor_switch(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.settings.server_editor else {
        return Vec::new();
    };
    if editor.editing.is_some() {
        return Vec::new();
    }
    let Some(server) = state.config.servers.get(editor.cursor).cloned() else {
        return Vec::new();
    };
    if server.id == state.config.active_server {
        return Vec::new();
    }
    crate::reducer::modal::open_confirm(
        state,
        format!(
            "switch to '{}'? the current queue and session will be cleared.",
            server.name
        ),
        Action::Settings(SettingsAction::ServerEditorSwitchConfirmed(ServerId::from(
            server.id,
        ))),
    )
}

fn build_session_snapshot(state: &AppState) -> crate::state::SessionSnapshot {
    crate::state::SessionSnapshot {
        schema_version: crate::state::SESSION_SCHEMA_VERSION,
        server_id: ServerId::from(state.config.active_server.clone()),
        queue: state.queue.clone(),
        position_secs: state.player.position.as_secs_f64(),
        active_tab: state.nav.active_tab,
        zen_mode: state.zen_mode,
        volume: state.player.volume,
        quality_profile: state.player.quality_profile,
        eq: state.player.eq.clone(),
        saved_at: Timestamp::now(),
    }
}

/// The `Confirm`'s own `on_confirm` payload. Order matters: the outgoing snapshot is captured
/// (and the queue/history cleared) *before* `active_server` changes, since both read `state`
/// as "the server we're leaving," not the one we're headed to.
fn server_editor_switch_confirmed(state: &mut AppState, new_server: ServerId) -> Vec<Effect> {
    let outgoing = ServerId::from(state.config.active_server.clone());
    let snapshot = build_session_snapshot(state);

    let mut effects = crate::reducer::queue::clear(state);
    state.history.clear();

    state.config.active_server = new_server.as_str().to_string();
    state.settings.server_editor = None;
    state.touch();

    effects.push(Effect::Cache(Box::new(
        CacheEffect::PersistSessionForServer(outgoing, Box::new(snapshot)),
    )));
    effects.push(Effect::Sys(SysEffect::WriteConfig(Box::new(
        state.config.clone(),
    ))));
    effects.push(Effect::Sys(SysEffect::ReconnectServer(new_server)));
    effects
}

fn server_editor_add_header(state: &mut AppState) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    let name = draft.new_header_name.trim().to_string();
    let value = draft.new_header_value.clone();

    if name.is_empty() {
        draft.header_error = Some("header name cannot be empty".to_string());
        state.touch();
        return Vec::new();
    }
    if RESERVED_HEADER_NAMES.contains(&name.to_ascii_lowercase().as_str()) {
        draft.header_error = Some(format!("{name} is set by loxia and cannot be overridden"));
        state.touch();
        return Vec::new();
    }

    // Re-adding an existing name replaces its value rather than appending a duplicate row.
    draft
        .headers
        .retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
    draft.headers.push((name, value));
    draft.new_header_name.clear();
    draft.new_header_value.clear();
    draft.header_error = None;
    state.touch();
    Vec::new()
}

fn server_editor_remove_focused_header(state: &mut AppState) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    // `x` means "remove what is focused"; on the endpoint row that is the address, not a header.
    if matches!(
        server_draft_field_at(draft, draft.field),
        Some(ServerDraftField::Endpoint)
    ) {
        return server_editor_remove_endpoint(state);
    }
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    if let Some(ServerDraftField::Header(i)) = server_draft_field_at(draft, draft.field) {
        draft.headers.remove(i);
        let new_count = server_draft_field_count(draft);
        draft.field = draft.field.min(new_count.saturating_sub(1));
        draft.reveal_header = None;
        state.touch();
    }
    Vec::new()
}

/// Dispatches `Effect::Net(TestServerConnection)` against the draft's own (unsaved)
/// `url`/`headers`/`username`/`password` — "must be possible before saving" (this task's own
/// spec), so this reads only from the draft, never from `config.servers`. `is_save` marks the
/// in-flight request as a `Save` rather than a plain `Test connection`, read back by
/// `server_editor_test_succeeded` to decide whether a successful reply should also commit the
/// draft.
fn server_editor_test_connection(state: &mut AppState, is_save: bool) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    if draft.testing {
        return Vec::new(); // no cancellation mechanism for an in-flight request; refuse a second
    }
    draft.testing = true;
    draft.saving = is_save;
    draft.test_result = None;
    let headers: BTreeMap<String, String> = draft.headers.iter().cloned().collect();
    let url = build_server_url(&draft.protocol, &draft.host, &draft.port);
    let device_id = draft.device_id.clone();
    let username = draft.username.clone();
    let password = draft.password.clone();
    state.touch();
    vec![Effect::Net(NetEffect::TestServerConnection {
        url,
        headers,
        device_id,
        username,
        password: RedactedSecret::new(password),
    })]
}

/// `{base}_2`, `{base}_3`, ... — the first name-derived slug that doesn't already collide with an
/// existing profile id. There is no user-facing "id" field in the add/edit form (`docs/02-data-model.md`
/// §8's own `id` doc: "config-assigned local server id" — assigned by loxia here, not typed).
fn slugify_server_name(name: &str) -> String {
    let mut slug: String = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "server".to_string()
    } else {
        slug.to_string()
    }
}

fn generate_server_id(existing: &[ServerConfig], name: &str) -> String {
    let base = slugify_server_name(name);
    if !existing.iter().any(|s| s.id == base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if !existing.iter().any(|s| s.id == candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// The `Effect::Net(TestServerConnection)` reply on success (`reducer::nav::apply_data`) — always
/// updates `test_result` (a plain "Test connection" stops here); if this reply was for a `Save`,
/// it also commits the draft into `config.servers`, using the freshly-authenticated
/// `user_id`/`access_token` this reply carries rather than anything the user typed.
pub(crate) fn server_editor_test_succeeded(
    state: &mut AppState,
    user_id: String,
    access_token: String,
    server_name: String,
    version: String,
) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new(); // the editor (or this draft) has since been closed; drop the reply
    };
    if !draft.testing {
        return Vec::new(); // stale reply, e.g. from a draft that was closed and reopened
    }
    draft.testing = false;
    draft.test_result = Some(Ok(format!("connected to {server_name} (v{version})")));

    if !draft.saving {
        state.touch();
        return Vec::new();
    }
    draft.saving = false;

    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| generate_server_id(&state.config.servers, &draft.name));
    let is_new = draft.id.is_none();
    // The fields on screen belong to whichever address is *selected*, so they are written back
    // before the endpoint list is read — otherwise the edit in progress would be dropped — and
    // every address below is then taken from that list, never from the live fields.
    //
    // Reading the primary's url straight off the live fields is precisely the bug a user hit:
    // saving while a fallback was selected wrote the fallback's address into `url` as well as into
    // `fallbacks[0]`, so both came back showing the last address added, and deleting the fallback
    // left the primary holding its values (`docs/12-decisions.md`).
    draft.stash_endpoint();
    let endpoint_url = |e: &crate::state::settings::ServerEndpointDraft| {
        build_server_url(&e.protocol, &e.host, &e.port)
    };
    // Every production path populates `endpoints` (`add_new`/`edit`), but a draft built any other
    // way would otherwise save an empty `https://` url and wipe the profile — falling back to the
    // live fields is exactly what this used to do for every save.
    let primary = draft.endpoints.first().cloned().unwrap_or_else(|| {
        crate::state::settings::ServerEndpointDraft {
            protocol: draft.protocol.clone(),
            host: draft.host.clone(),
            port: draft.port.clone(),
            headers: draft.headers.clone(),
        }
    });
    let fallbacks: Vec<crate::config::ServerEndpoint> = draft
        .endpoints
        .iter()
        .skip(1)
        .map(|e| crate::config::ServerEndpoint {
            url: endpoint_url(e),
            custom_headers: e.headers.iter().cloned().collect(),
        })
        .collect();
    let server = ServerConfig {
        id: id.clone(),
        name: draft.name.clone(),
        url: endpoint_url(&primary),
        user_id,
        access_token,
        device_id: draft.device_id.clone(),
        custom_headers: primary.headers.iter().cloned().collect(),
        // Carried over when editing an existing profile, so changing its address does not orphan
        // the cache and downloads already stored under that server's identity. A brand-new profile
        // learns it on its first connection.
        server_id: state
            .config
            .servers
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.server_id.clone())
            .unwrap_or_default(),
        fallbacks,
    };

    if let Some(existing) = state.config.servers.iter_mut().find(|s| s.id == id) {
        *existing = server;
    } else {
        state.config.servers.push(server);
    }
    // A fresh install's very first saved profile becomes active automatically — otherwise
    // `bootstrap::connect` would have nothing configured to connect to at all.
    if is_new && state.config.active_server.is_empty() {
        state.config.active_server = id;
    }
    if let Some(editor) = &mut state.settings.server_editor {
        editor.editing = None;
    }
    state.touch();
    vec![Effect::Sys(SysEffect::WriteConfig(Box::new(
        state.config.clone(),
    )))]
}

pub(crate) fn server_editor_test_failed(state: &mut AppState, message: String) -> Vec<Effect> {
    let Some(draft) = state
        .settings
        .server_editor
        .as_mut()
        .and_then(|e| e.editing.as_mut())
    else {
        return Vec::new();
    };
    if !draft.testing {
        return Vec::new();
    }
    draft.testing = false;
    draft.saving = false;
    draft.test_result = Some(Err(message));
    state.touch();
    Vec::new()
}

fn server_editor_close(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.server_editor else {
        return Vec::new();
    };
    if editor.editing.is_some() {
        editor.editing = None;
    } else {
        state.settings.server_editor = None;
    }
    state.touch();
    Vec::new()
}

// ---------------------------------------------------------------------------------------------
// `11-04`: sort profiles (`docs/02-data-model.md` §8).
// ---------------------------------------------------------------------------------------------

/// Declared order — also the order `SortEditorCycleField` steps through. `pub(crate)`: the
/// renderer (`crates/loxia-tui`) needs the same fixed order to label a rule row's own field.
pub const SORT_FIELD_ORDER: [config::SortField; 8] = [
    config::SortField::Name,
    config::SortField::Artist,
    config::SortField::AlbumArtist,
    config::SortField::Album,
    config::SortField::Year,
    config::SortField::TrackNumber,
    config::SortField::Genre,
    config::SortField::DateAdded,
];

fn sort_field_index(field: config::SortField) -> usize {
    SORT_FIELD_ORDER
        .iter()
        .position(|&f| f == field)
        .unwrap_or(0)
}

fn current_sort_profile(state: &AppState) -> Option<&config::SortProfile> {
    let editor = state.settings.sort_profile_editor.as_ref()?;
    state.config.sorting.profiles.get(editor.profile_cursor)
}

/// Steps `(profile_cursor, rule_cursor)` by `delta` through the flat visual list — the focused
/// profile's own header, then each of its rules, then the next profile's header, and so on.
/// Landing on a different profile always resets `rule_cursor` to `None` (its header is what's
/// now focused), matching the wireframe's single always-expanded profile.
fn sort_editor_move(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let profiles = state.config.sorting.profiles.clone();
    let Some(editor) = &mut state.settings.sort_profile_editor else {
        return Vec::new();
    };
    if profiles.is_empty() {
        return Vec::new();
    }
    editor.profile_cursor = editor.profile_cursor.min(profiles.len() - 1);

    // Flatten into `(profile_index, rule_index_or_none)` for every step the current profile's own
    // rules contribute, then step `delta` times.
    let mut flat: Vec<(usize, Option<usize>)> = Vec::new();
    for (i, profile) in profiles.iter().enumerate() {
        flat.push((i, None));
        if i == editor.profile_cursor {
            for r in 0..profile.rules.len() {
                flat.push((i, Some(r)));
            }
        }
    }
    let current_pos = flat
        .iter()
        .position(|&(p, r)| p == editor.profile_cursor && r == editor.rule_cursor)
        .unwrap_or(0);
    let new_pos = (current_pos as i64 + i64::from(delta)).clamp(0, flat.len() as i64 - 1) as usize;
    let (new_profile, new_rule) = flat[new_pos];
    editor.profile_cursor = new_profile;
    editor.rule_cursor = new_rule;
    editor.error = None;
    state.touch();
    Vec::new()
}

fn sort_editor_new(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.sort_profile_editor else {
        return Vec::new();
    };
    editor.name_buf = Some(TextEdit::default());
    editor.creating = true;
    editor.error = None;
    state.touch();
    Vec::new()
}

fn sort_editor_rename(state: &mut AppState) -> Vec<Effect> {
    let Some(current_name) = current_sort_profile(state).map(|p| p.name.clone()) else {
        return Vec::new();
    };
    let Some(editor) = &mut state.settings.sort_profile_editor else {
        return Vec::new();
    };
    editor.name_buf = Some(TextEdit::new(current_name));
    editor.creating = false;
    editor.error = None;
    state.touch();
    Vec::new()
}

/// Validates and commits `name_buf` — a brand-new profile (`creating`) is appended; a rename
/// updates the focused profile in place and, per this task's own spec, updates
/// `default_queue_profile` too if it named the profile being renamed. Both paths go through
/// `config::validate` (matching every other config-writing path in this file) so a rename that
/// happens to leave `default_queue_profile` pointing nowhere is still auto-corrected the same way
/// a bad TOML edit would be.
fn sort_editor_commit_name(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.sort_profile_editor else {
        return Vec::new();
    };
    let Some(name) = editor.name_buf.take().map(|e| e.text) else {
        return Vec::new();
    };
    let creating = editor.creating;
    editor.creating = false;
    let name = name.trim().to_string();

    if name.is_empty() {
        if let Some(editor) = &mut state.settings.sort_profile_editor {
            editor.error = Some("a profile name is required".to_string());
        }
        state.touch();
        return Vec::new();
    }

    let profile_cursor = editor.profile_cursor;
    let duplicate = state
        .config
        .sorting
        .profiles
        .iter()
        .enumerate()
        .any(|(i, p)| p.name == name && (creating || i != profile_cursor));
    if duplicate {
        if let Some(editor) = &mut state.settings.sort_profile_editor {
            editor.error = Some("a profile with that name exists".to_string());
        }
        state.touch();
        return Vec::new();
    }

    let mut draft = state.config.clone();
    if creating {
        draft.sorting.profiles.push(config::SortProfile {
            name,
            rules: Vec::new(),
        });
        if let Some(editor) = &mut state.settings.sort_profile_editor {
            editor.profile_cursor = draft.sorting.profiles.len() - 1;
            editor.rule_cursor = None;
        }
    } else if let Some(profile) = draft.sorting.profiles.get_mut(profile_cursor) {
        let old_name = profile.name.clone();
        profile.name = name.clone();
        if draft.sorting.default_queue_profile == old_name {
            draft.sorting.default_queue_profile = name;
        }
    }
    config::validate(&mut draft);
    on_config_changed(state, draft)
}

/// `x` — asks first. A live user deleted their sort profiles by accident, so removal now routes
/// through a `Confirm` modal naming the profile (`docs/12-decisions.md`).
fn sort_editor_delete_profile(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.settings.sort_profile_editor else {
        return Vec::new();
    };
    let Some(profile) = state.config.sorting.profiles.get(editor.profile_cursor) else {
        return Vec::new();
    };
    let name = profile.name.clone();
    crate::reducer::modal::open_confirm(
        state,
        format!("delete sort profile '{name}'?"),
        Action::Settings(SettingsAction::SortEditorDeleteProfileConfirmed),
    )
}

/// `R` — re-adds the built-in profiles a user may have deleted. Existing names are left untouched
/// (never overwrites the user's own edits); a cleared default is repointed at the built-in default.
fn sort_editor_restore_defaults(state: &mut AppState) -> Vec<Effect> {
    if state.settings.sort_profile_editor.is_none() {
        return Vec::new();
    }
    let defaults = config::SortingConfig::default();
    let mut draft = state.config.clone();
    let mut added = 0;
    for profile in defaults.profiles {
        if !draft
            .sorting
            .profiles
            .iter()
            .any(|p| p.name == profile.name)
        {
            draft.sorting.profiles.push(profile);
            added += 1;
        }
    }
    if draft.sorting.default_queue_profile.is_empty() {
        draft.sorting.default_queue_profile = defaults.default_queue_profile;
    }
    config::validate(&mut draft);
    if let Some(editor) = &mut state.settings.sort_profile_editor {
        editor.error = None;
        editor.rule_cursor = None;
        editor.profile_cursor = editor
            .profile_cursor
            .min(draft.sorting.profiles.len().saturating_sub(1));
    }
    state.toast(
        if added == 0 {
            "default sort profiles already present".to_string()
        } else {
            format!(
                "restored {added} default sort profile{}",
                if added == 1 { "" } else { "s" }
            )
        },
        ToastLevel::Info,
    );
    on_config_changed(state, draft)
}

fn sort_editor_delete_profile_confirmed(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.settings.sort_profile_editor else {
        return Vec::new();
    };
    let cursor = editor.profile_cursor;
    if state.config.sorting.profiles.get(cursor).is_none() {
        return Vec::new();
    }

    let mut draft = state.config.clone();
    draft.sorting.profiles.remove(cursor);
    // `deleting_default_profile_clears_default_and_warns` — `config::validate`'s own
    // `resolve_default_queue_profile` already does exactly this (reassigns to whatever profile
    // remains first, or clears to empty if none do) and pushes the matching `ConfigWarning`; no
    // bespoke clear-and-warn logic is needed here, only routing the deletion through it.
    let warnings = config::validate(&mut draft);
    state.config_warnings.extend(warnings);

    if let Some(editor) = &mut state.settings.sort_profile_editor {
        editor.profile_cursor = editor
            .profile_cursor
            .min(draft.sorting.profiles.len().saturating_sub(1));
        editor.rule_cursor = None;
        editor.error = None;
    }
    on_config_changed(state, draft)
}

fn sort_editor_add_rule(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.settings.sort_profile_editor else {
        return Vec::new();
    };
    let cursor = editor.profile_cursor;
    let Some(profile) = state.config.sorting.profiles.get(cursor) else {
        return Vec::new();
    };
    if profile.rules.len() >= config::MAX_SORT_RULES {
        if let Some(editor) = &mut state.settings.sort_profile_editor {
            editor.error = Some(format!("maximum {} rules", config::MAX_SORT_RULES));
        }
        state.touch();
        return Vec::new();
    }

    let mut draft = state.config.clone();
    draft.sorting.profiles[cursor].rules.push(config::SortRule {
        field: config::SortField::Name,
        direction: config::Direction::Asc,
    });
    let new_rule_index = draft.sorting.profiles[cursor].rules.len() - 1;
    if let Some(editor) = &mut state.settings.sort_profile_editor {
        editor.rule_cursor = Some(new_rule_index);
        editor.error = None;
    }
    on_config_changed(state, draft)
}

fn sort_editor_delete_rule(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.settings.sort_profile_editor else {
        return Vec::new();
    };
    let profile_cursor = editor.profile_cursor;
    let Some(rule_index) = editor.rule_cursor else {
        return Vec::new();
    };
    let Some(profile) = state.config.sorting.profiles.get(profile_cursor) else {
        return Vec::new();
    };
    if rule_index >= profile.rules.len() {
        return Vec::new();
    }

    let mut draft = state.config.clone();
    draft.sorting.profiles[profile_cursor]
        .rules
        .remove(rule_index);
    let remaining = draft.sorting.profiles[profile_cursor].rules.len();
    if let Some(editor) = &mut state.settings.sort_profile_editor {
        editor.rule_cursor = if remaining == 0 {
            None
        } else {
            Some(rule_index.min(remaining - 1))
        };
    }
    on_config_changed(state, draft)
}

fn sort_editor_reorder_rule(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let Some(editor) = &state.settings.sort_profile_editor else {
        return Vec::new();
    };
    let profile_cursor = editor.profile_cursor;
    let Some(rule_index) = editor.rule_cursor else {
        return Vec::new();
    };
    let Some(profile) = state.config.sorting.profiles.get(profile_cursor) else {
        return Vec::new();
    };
    let len = profile.rules.len();
    if len == 0 {
        return Vec::new();
    }
    let new_index = (rule_index as i64 + i64::from(delta)).clamp(0, len as i64 - 1) as usize;
    if new_index == rule_index {
        return Vec::new();
    }

    let mut draft = state.config.clone();
    let rule = draft.sorting.profiles[profile_cursor]
        .rules
        .remove(rule_index);
    draft.sorting.profiles[profile_cursor]
        .rules
        .insert(new_index, rule);
    if let Some(editor) = &mut state.settings.sort_profile_editor {
        editor.rule_cursor = Some(new_index);
    }
    on_config_changed(state, draft)
}

fn sort_editor_cycle_field(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let Some(editor) = &state.settings.sort_profile_editor else {
        return Vec::new();
    };
    let profile_cursor = editor.profile_cursor;
    let Some(rule_index) = editor.rule_cursor else {
        return Vec::new();
    };
    let Some(rule) = state
        .config
        .sorting
        .profiles
        .get(profile_cursor)
        .and_then(|p| p.rules.get(rule_index))
    else {
        return Vec::new();
    };
    let len = SORT_FIELD_ORDER.len() as i64;
    let idx = sort_field_index(rule.field) as i64;
    let next = (idx + i64::from(delta)).rem_euclid(len) as usize;

    let mut draft = state.config.clone();
    draft.sorting.profiles[profile_cursor].rules[rule_index].field = SORT_FIELD_ORDER[next];
    on_config_changed(state, draft)
}

fn sort_editor_toggle_direction(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.settings.sort_profile_editor else {
        return Vec::new();
    };
    let profile_cursor = editor.profile_cursor;
    let Some(rule_index) = editor.rule_cursor else {
        return Vec::new();
    };
    let Some(rule) = state
        .config
        .sorting
        .profiles
        .get(profile_cursor)
        .and_then(|p| p.rules.get(rule_index))
    else {
        return Vec::new();
    };
    let flipped = match rule.direction {
        config::Direction::Asc => config::Direction::Desc,
        config::Direction::Desc => config::Direction::Asc,
    };

    let mut draft = state.config.clone();
    draft.sorting.profiles[profile_cursor].rules[rule_index].direction = flipped;
    on_config_changed(state, draft)
}

/// `Enter` on a profile's own header row — "Apply is a separate `Action` control" (this task's
/// own spec): reuses the exact same `QueueAction::ApplySortProfile` the quick-apply
/// `Modal::SortProfile` (`10-09`) already dispatches, so both surfaces stay behaviourally
/// identical rather than this task re-deriving its own apply logic.
fn sort_editor_apply(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.settings.sort_profile_editor else {
        return Vec::new();
    };
    if editor.rule_cursor.is_some() {
        return Vec::new(); // `Enter` on a rule row means something else (`ToggleDirection`)
    }
    let Some(name) = current_sort_profile(state).map(|p| p.name.clone()) else {
        return Vec::new();
    };
    crate::reducer::queue::apply_sort_profile(state, &name)
}

fn sort_editor_close(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.sort_profile_editor else {
        return Vec::new();
    };
    if editor.name_buf.is_some() {
        editor.name_buf = None;
        editor.creating = false;
        editor.error = None;
    } else {
        state.settings.sort_profile_editor = None;
    }
    state.touch();
    Vec::new()
}

// ---------------------------------------------------------------------------------------------
// `11-05`: EQ presets (`docs/05-audio-engine.md` §5).
// ---------------------------------------------------------------------------------------------

/// Every preset the editor lists, in the same order `equalizer_rows`'s own active-preset dropdown
/// already uses: the fixed factory names first, then `config.equalizer.custom_presets` — paired
/// with whether each one is a factory preset (read-only) or a custom one. `pub` (not
/// `pub(crate)`): `crates/loxia-tui`'s own renderer needs the same row list.
pub fn eq_preset_rows(cfg: &Config) -> Vec<(String, bool)> {
    let mut rows: Vec<(String, bool)> = config::FACTORY_EQ_PRESET_NAMES
        .iter()
        .map(|s| (s.to_string(), true))
        .collect();
    rows.extend(
        cfg.equalizer
            .custom_presets
            .iter()
            .map(|p| (p.name.clone(), false)),
    );
    rows
}

fn eq_editor_move(state: &mut AppState, delta: i32) -> Vec<Effect> {
    let rows = eq_preset_rows(&state.config);
    let Some(editor) = &mut state.settings.eq_preset_editor else {
        return Vec::new();
    };
    if rows.is_empty() {
        return Vec::new();
    }
    editor.cursor =
        (editor.cursor as i64 + i64::from(delta)).clamp(0, rows.len() as i64 - 1) as usize;
    editor.error = None;
    state.touch();
    Vec::new()
}

/// "Captures the current live gains" (this task's own spec) — read once, right now, into
/// `captured_gains`, not re-read later at commit time.
fn eq_editor_save_current(state: &mut AppState) -> Vec<Effect> {
    let gains = state.player.eq.gains;
    let Some(editor) = &mut state.settings.eq_preset_editor else {
        return Vec::new();
    };
    editor.name_buf = Some(TextEdit::default());
    editor.creating = true;
    editor.captured_gains = Some(gains);
    editor.error = None;
    state.touch();
    Vec::new()
}

fn eq_editor_rename(state: &mut AppState) -> Vec<Effect> {
    let rows = eq_preset_rows(&state.config);
    let Some(editor) = &mut state.settings.eq_preset_editor else {
        return Vec::new();
    };
    let Some((name, is_factory)) = rows.get(editor.cursor).cloned() else {
        return Vec::new();
    };
    if is_factory {
        editor.error = Some("factory presets cannot be modified".to_string());
        state.touch();
        return Vec::new();
    }
    editor.name_buf = Some(TextEdit::new(name));
    editor.creating = false;
    editor.error = None;
    state.touch();
    Vec::new()
}

fn eq_editor_delete(state: &mut AppState) -> Vec<Effect> {
    let rows = eq_preset_rows(&state.config);
    let Some(editor) = &state.settings.eq_preset_editor else {
        return Vec::new();
    };
    let Some((name, is_factory)) = rows.get(editor.cursor).cloned() else {
        return Vec::new();
    };
    if is_factory {
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.error = Some("factory presets cannot be modified".to_string());
        }
        state.touch();
        return Vec::new();
    }

    let was_active = state.config.equalizer.active_preset == name;
    let mut draft = state.config.clone();
    draft.equalizer.custom_presets.retain(|p| p.name != name);
    if was_active {
        draft.equalizer.active_preset = "flat".to_string();
    }
    if let Some(editor) = &mut state.settings.eq_preset_editor {
        let new_len = eq_preset_rows(&draft).len();
        editor.cursor = editor.cursor.min(new_len.saturating_sub(1));
        editor.error = None;
    }

    let mut effects = on_config_changed(state, draft);
    if was_active {
        // "The audio does not keep applying a curve the user just deleted" (this task's own
        // spec) — applies `flat` immediately, the same live-mirror exception `player.eq` already
        // documents for every other in-modal EQ commit (`10-06`).
        let flat_gains = state
            .player
            .known_presets
            .iter()
            .find(|p| p.name == "flat")
            .map(|p| p.gains)
            .unwrap_or([0.0; 10]);
        state.player.eq.gains = flat_gains;
        state.player.eq.preset_name = "flat".to_string();
        state.touch();
        effects.push(Effect::Audio(AudioEffect::SetEq(Some(flat_gains))));
    }
    effects
}

/// `e` — opens the equalizer modal (`10-06`) prefilled with the *focused* preset's own gains,
/// which may not be whatever is currently playing.
fn eq_editor_open_equalizer(state: &mut AppState) -> Vec<Effect> {
    let rows = eq_preset_rows(&state.config);
    let Some(editor) = &state.settings.eq_preset_editor else {
        return Vec::new();
    };
    let Some((name, is_factory)) = rows.get(editor.cursor).cloned() else {
        return Vec::new();
    };
    let gains = if is_factory {
        state
            .player
            .known_presets
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.gains)
    } else {
        state
            .config
            .equalizer
            .custom_presets
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.gains)
    };
    let Some(gains) = gains else {
        return Vec::new(); // not found in `known_presets` yet (e.g. never loaded) — nothing to prefill
    };
    let preset_idx = state
        .player
        .known_presets
        .iter()
        .position(|p| p.name == name);
    crate::reducer::modal::open_equalizer_with_gains(state, gains, preset_idx)
}

fn eq_editor_commit_name(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.eq_preset_editor else {
        return Vec::new();
    };
    let Some(name) = editor.name_buf.take().map(|e| e.text) else {
        return Vec::new();
    };
    let creating = editor.creating;
    let captured_gains = editor.captured_gains.take();
    editor.creating = false;
    let name = name.trim().to_string();
    let cursor = editor.cursor;

    if name.is_empty() {
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.error = Some("a preset name is required".to_string());
        }
        state.touch();
        return Vec::new();
    }

    let rows = eq_preset_rows(&state.config);
    let renaming_row = if creating {
        None
    } else {
        rows.get(cursor).cloned()
    };
    let collides = rows.iter().any(|(existing, _)| {
        *existing == name && renaming_row.as_ref().map(|(old, _)| old) != Some(existing)
    });
    if collides {
        let is_factory_collision = config::FACTORY_EQ_PRESET_NAMES.contains(&name.as_str());
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.error = Some(if is_factory_collision {
                "that name is used by a factory preset".to_string()
            } else {
                "a preset with that name exists".to_string()
            });
        }
        state.touch();
        return Vec::new();
    }

    let mut draft = state.config.clone();
    if creating {
        draft.equalizer.custom_presets.push(config::EqPreset {
            name,
            gains: captured_gains.unwrap_or(state.player.eq.gains),
        });
    } else if let Some((old_name, _)) = renaming_row
        && let Some(preset) = draft
            .equalizer
            .custom_presets
            .iter_mut()
            .find(|p| p.name == old_name)
    {
        preset.name = name.clone();
        if draft.equalizer.active_preset == old_name {
            draft.equalizer.active_preset = name.clone();
        }
        if state.player.eq.preset_name == old_name {
            state.player.eq.preset_name = name;
        }
    }
    on_config_changed(state, draft)
}

fn eq_editor_close(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &mut state.settings.eq_preset_editor else {
        return Vec::new();
    };
    if editor.name_buf.is_some() {
        editor.name_buf = None;
        editor.creating = false;
        editor.captured_gains = None;
        editor.error = None;
    } else {
        state.settings.eq_preset_editor = None;
    }
    state.touch();
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixtures;
    use jiff::SignedDuration;

    fn row_labels(section: SettingsSection, cfg: &Config) -> Vec<&'static str> {
        rows_for_section(section, cfg, &[])
            .into_iter()
            .map(|r| r.label)
            .collect()
    }

    fn cursor_at(state: &mut AppState, section: SettingsSection, label: &str) {
        state.settings.section = section;
        let rows = rows_for_section(section, &state.config, &[]);
        state.settings.cursor = rows
            .iter()
            .position(|r| r.label == label)
            .unwrap_or_else(|| panic!("no row labelled {label:?} in {section:?}"));
    }

    /// `11-01`: `every_config_field_has_a_control` — hand-maintained against `Config`'s own field
    /// list (`docs/12-decisions.md`): `schema_version` is excluded (migration-only, never a
    /// user-facing setting) and `logging.{level,max_files}` are folded into `Interface` (this
    /// task's own section list, `docs/07-ui-spec.md` §9, names no dedicated "Logging" section).
    fn audio_device(id: &str, driver: &str) -> AudioDevice {
        AudioDevice {
            id: id.to_string(),
            description: format!("{id} description"),
            driver: driver.to_string(),
        }
    }

    /// The output driver/device rows are dropdowns over what was actually detected — they used to be
    /// free-text fields a user had to type a device id into blind (`docs/12-decisions.md`).
    #[test]
    fn output_rows_are_dropdowns_over_detected_devices() {
        let cfg = Config::default();
        let devices = [
            audio_device("alsa/hw:0,0", "alsa"),
            audio_device("pulse/default", "pulse"),
        ];
        let rows = rows_for_section(SettingsSection::Audio, &cfg, &devices);

        let driver = rows.iter().find(|r| r.label == "output driver").unwrap();
        match &driver.control {
            Control::Select { options, .. } => {
                assert_eq!(
                    options,
                    &["auto", "alsa", "pulse"],
                    "auto first, then detected"
                );
            }
            _ => panic!("expected a Select control"),
        }

        let device = rows.iter().find(|r| r.label == "output device").unwrap();
        match &device.control {
            Control::Select { options, .. } => {
                assert_eq!(options, &["auto", "alsa/hw:0,0", "pulse/default"]);
            }
            _ => panic!("expected a Select control"),
        }
    }

    #[test]
    fn every_config_field_has_a_control() {
        let cfg = Config::default();
        let expected = [
            (SettingsSection::Servers, 3), // active_server, servers[].access_token, servers
            (SettingsSection::Audio, 5), // output_driver, device_id, default_replaygain, replaygain_preamp_db, buffer_size_ms
            // enabled, rolling_max_gb, image_cache_mb, prefetch_on_play, prefetch_next,
            // download_dir, cache_dir
            (SettingsSection::Cache, 7),
            (SettingsSection::Transcode, 3), // mode, target_codec, download_uncompressed
            // ui's own 10 fields + logging's 2 + `11-06`'s "clear saved session" action row (not a
            // `Config` field at all — deleting on-disk state, like the other three `Action` rows)
            (SettingsSection::Interface, 14),
            (SettingsSection::Sorting, 2), // default_queue_profile, profiles
            (SettingsSection::Equalizer, 3), // enabled, active_preset, custom_presets
            (SettingsSection::Keybindings, 1), // keybindings
            (SettingsSection::About, 0),
        ];
        for (section, count) in expected {
            assert_eq!(
                rows_for_section(section, &cfg, &[]).len(),
                count,
                "{section:?} row count"
            );
        }
    }

    fn select_options<'a>(rows: &'a [SettingsRow], label: &str) -> &'a [String] {
        let row = rows.iter().find(|r| r.label == label).unwrap();
        match &row.control {
            Control::Select { options, .. } => options,
            _ => panic!("{label} is not a Select"),
        }
    }

    /// `11-01`: `enum_valued_selects_cover_all_variants`.
    #[test]
    fn enum_valued_selects_cover_all_variants() {
        let cfg = Config::default();
        assert_eq!(
            select_options(
                &rows_for_section(SettingsSection::Audio, &cfg, &[]),
                "replaygain mode"
            )
            .len(),
            3 // Album, Track, Off
        );
        let transcode_rows = rows_for_section(SettingsSection::Transcode, &cfg, &[]);
        assert_eq!(
            select_options(&transcode_rows, "quality").len(),
            4 // Direct, TranscodeHigh, TranscodeMed, TranscodeLow
        );
        assert_eq!(
            select_options(&transcode_rows, "target codec").len(),
            3 // Mp3, Aac, Opus
        );
        assert_eq!(
            select_options(
                &rows_for_section(SettingsSection::Interface, &cfg, &[]),
                "album art protocol"
            )
            .len(),
            5 // Auto, Kitty, Sixel, Halfblocks, Off
        );
    }

    #[test]
    fn live_apply_theme_updates_state_theme_immediately() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Interface, "theme");
        let before = state.theme.clone();
        apply(&mut state, SettingsAction::AdjustValue(1));
        assert_ne!(
            state.theme, before,
            "state.theme must be rebuilt, not just config.ui.theme"
        );
        assert_eq!(
            crate::theme::Theme::builtin(&state.config.ui.theme).unwrap(),
            state.theme
        );
    }

    #[test]
    fn live_apply_mouse_emits_set_mouse_capture_effect() {
        let mut state = fixtures::fixture_empty();
        assert!(state.config.ui.enable_mouse, "default is enabled");
        cursor_at(&mut state, SettingsSection::Interface, "mouse");
        let effects = apply(&mut state, SettingsAction::AdjustValue(1));
        assert!(!state.config.ui.enable_mouse);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::SetMouseCapture(false))))
        );
    }

    /// `11-01`: `live_apply_for_theme_mouse_notifications_lyrics` — the notifications/lyrics half:
    /// neither needs an effect at all, since both are already read fresh from `state.config` at
    /// the point of use every time (`reducer::queue::notify_track_change`, `widgets::lyrics`) —
    /// "live" here just means the config field itself updates immediately, which every edit
    /// already does regardless of section.
    #[test]
    fn live_apply_notifications_and_lyrics_update_config_immediately() {
        let mut state = fixtures::fixture_empty();
        cursor_at(
            &mut state,
            SettingsSection::Interface,
            "desktop notifications",
        );
        apply(&mut state, SettingsAction::AdjustValue(1));
        assert!(!state.config.ui.desktop_notifications);

        cursor_at(&mut state, SettingsSection::Interface, "show lyrics");
        apply(&mut state, SettingsAction::AdjustValue(1));
        assert!(!state.config.ui.show_lyrics);
    }

    /// The Settings row for transcode quality wrote `config.transcode.mode` and stopped there,
    /// leaving `player.quality_profile` — the field the stream URL is actually built from — at its
    /// old value until the next launch. So `q` changed the quality and the identical Settings row
    /// appeared to do nothing at all (`docs/12-decisions.md`).
    #[test]
    fn changing_transcode_quality_in_settings_applies_it_live() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.quality_profile = QualityProfile::Direct;
        state.config.transcode.mode = QualityProfile::Direct;

        cursor_at(&mut state, SettingsSection::Transcode, "quality");
        let effects = apply(&mut state, SettingsAction::AdjustValue(1));

        assert_ne!(state.config.transcode.mode, QualityProfile::Direct);
        assert_eq!(
            state.player.quality_profile, state.config.transcode.mode,
            "the runtime mirror must follow the config"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Load { .. }))),
            "the loaded track must be re-fetched at the new profile: {effects:?}"
        );
    }

    /// A cached or downloaded file is whatever is already on disk, so there is nothing to re-fetch
    /// — but unlike `q`'s outright refusal, a *setting* the user typed in must still stick and
    /// take effect from the next track on.
    #[test]
    fn changing_quality_on_a_local_track_saves_without_reloading() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.quality_profile = QualityProfile::Direct;
        state.config.transcode.mode = QualityProfile::Direct;
        for entry in &mut state.queue.entries {
            entry.availability = crate::state::queue::Availability::Downloaded;
        }

        cursor_at(&mut state, SettingsSection::Transcode, "quality");
        let effects = apply(&mut state, SettingsAction::AdjustValue(1));

        assert_eq!(state.player.quality_profile, state.config.transcode.mode);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Load { .. }))),
            "a local file must not be re-fetched: {effects:?}"
        );
    }

    /// ReplayGain and the equalizer have the same runtime-mirror shape as quality, and were live
    /// -applied by their keybindings only.
    #[test]
    fn changing_replaygain_in_settings_applies_it_live() {
        let mut state = fixtures::fixture_empty();
        state.player.replay_gain = state.config.audio.default_replaygain;

        cursor_at(&mut state, SettingsSection::Audio, "replaygain mode");
        let effects = apply(&mut state, SettingsAction::AdjustValue(1));

        assert_eq!(
            state.player.replay_gain,
            state.config.audio.default_replaygain
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::SetReplayGain(_)))),
            "the engine must be told: {effects:?}"
        );
    }

    /// Turning the equalizer off must uninstall the filter chain — `SetEq(None)`, not silence.
    #[test]
    fn disabling_the_equalizer_in_settings_clears_the_chain() {
        let mut state = fixtures::fixture_empty();
        state.config.equalizer.enabled = true;
        state.player.eq.enabled = true;

        cursor_at(&mut state, SettingsSection::Equalizer, "enabled");
        let effects = apply(&mut state, SettingsAction::AdjustValue(1));

        assert!(!state.config.equalizer.enabled);
        assert!(!state.player.eq.enabled, "the runtime mirror must follow");
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::SetEq(None)))),
            "the chain must be uninstalled: {effects:?}"
        );
    }

    /// `11-01`: `persistence_debounced_one_second`.
    #[test]
    fn persistence_debounced_one_second() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Cache, "enabled");

        let effects = apply(&mut state, SettingsAction::AdjustValue(1));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_)))),
            "must not write immediately"
        );
        let deadline = state
            .settings_write_debounce_until
            .expect("a write must be armed");

        let too_early = deadline
            .checked_sub(SignedDuration::from_millis(1))
            .unwrap();
        assert!(maybe_write_config(&mut state, too_early).is_empty());

        let after = deadline
            .checked_add(SignedDuration::from_millis(1))
            .unwrap();
        let effects = maybe_write_config(&mut state, after);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
        assert!(
            state.settings_write_debounce_until.is_none(),
            "consumed once fired"
        );
    }

    /// `11-01`: `invalid_value_shows_inline_error_and_retains_previous`.
    #[test]
    fn invalid_value_shows_inline_error_and_retains_previous() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Cache, "rolling cache size");
        state.config.cache.rolling_max_gb = 0.5; // one step (0.5) above the rejection boundary

        let effects = apply(&mut state, SettingsAction::AdjustValue(-1));

        assert_eq!(
            state.config.cache.rolling_max_gb, 0.5,
            "the previous value must be retained"
        );
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
        let (row_index, message) = state.settings.error.as_ref().expect("an inline error");
        assert_eq!(*row_index, state.settings.cursor);
        assert!(!message.is_empty());
    }

    #[test]
    fn reveal_secret_toggles_and_resets_on_row_move() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Servers, "access token");
        apply(&mut state, SettingsAction::ToggleRevealSecret);
        assert!(state.settings.reveal_secret);
        apply(&mut state, SettingsAction::MoveRow(1));
        assert!(
            !state.settings.reveal_secret,
            "moving to a different row must hide it again"
        );
    }

    #[test]
    fn text_edit_round_trip_commits_the_value() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Cache, "download directory");
        apply(&mut state, SettingsAction::StartTextEdit);
        assert_eq!(
            state.settings.editing.as_ref().map(|e| e.text.as_str()),
            Some("auto")
        );

        apply(&mut state, SettingsAction::TextBackspace);
        apply(&mut state, SettingsAction::TextBackspace);
        apply(&mut state, SettingsAction::TextBackspace);
        apply(&mut state, SettingsAction::TextBackspace);
        for c in "pulse".chars() {
            apply(&mut state, SettingsAction::TextInput(c));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert_eq!(state.config.cache.download_dir, "pulse");
        assert!(state.settings.editing.is_none());
    }

    #[test]
    fn cancel_text_edit_discards_the_buffer() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Cache, "download directory");
        apply(&mut state, SettingsAction::StartTextEdit);
        apply(&mut state, SettingsAction::TextInput('x'));
        apply(&mut state, SettingsAction::CancelTextEdit);
        assert!(state.settings.editing.is_none());
        assert_eq!(state.config.cache.download_dir, "auto");
    }

    /// A real gap found in the field: without cursor movement, a typo could only ever be fixed by
    /// erasing everything after it. This is the reducer half — `active_text_buffer` — exercised
    /// against the plain row editor; `typing_into_*_reaches_the_reducer` in `crates/loxia`'s own
    /// `input.rs` covers the same actions reaching the reducer for the other three buffers.
    #[test]
    fn cursor_moves_and_mid_string_edits_work() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Cache, "download directory");
        apply(&mut state, SettingsAction::StartTextEdit); // "auto", cursor at 4 (the end)

        apply(&mut state, SettingsAction::TextCursorHome);
        assert_eq!(state.settings.editing.as_ref().unwrap().cursor, 0);
        apply(&mut state, SettingsAction::TextCursorRight);
        apply(&mut state, SettingsAction::TextInput('X')); // "a|uto" -> "aXuto"
        assert_eq!(state.settings.editing.as_ref().unwrap().text, "aXuto");
        assert_eq!(state.settings.editing.as_ref().unwrap().cursor, 2);

        apply(&mut state, SettingsAction::TextCursorEnd);
        assert_eq!(state.settings.editing.as_ref().unwrap().cursor, 5);
        apply(&mut state, SettingsAction::TextCursorLeft);
        apply(&mut state, SettingsAction::TextDeleteForward); // removes the trailing "o"
        assert_eq!(state.settings.editing.as_ref().unwrap().text, "aXut");

        apply(&mut state, SettingsAction::TextCursorHome);
        apply(&mut state, SettingsAction::TextDeleteForward); // removes the leading "a"
        assert_eq!(state.settings.editing.as_ref().unwrap().text, "Xut");
        assert_eq!(state.settings.editing.as_ref().unwrap().cursor, 0);
    }

    #[test]
    fn cursor_left_at_start_and_delete_forward_at_end_are_no_ops() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Cache, "download directory");
        apply(&mut state, SettingsAction::StartTextEdit);
        apply(&mut state, SettingsAction::TextCursorHome);
        apply(&mut state, SettingsAction::TextCursorLeft);
        assert_eq!(state.settings.editing.as_ref().unwrap().cursor, 0);

        apply(&mut state, SettingsAction::TextCursorEnd);
        apply(&mut state, SettingsAction::TextDeleteForward);
        assert_eq!(state.settings.editing.as_ref().unwrap().text, "auto");
    }

    #[test]
    fn activate_row_on_manage_eq_presets_opens_the_editor() {
        // `11-05`: the last of the four `Control::Action` rows to get real behaviour — none of
        // them is a documented no-op any more.
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Equalizer, "custom presets");
        let effects = apply(&mut state, SettingsAction::ActivateRow);
        assert!(effects.is_empty());
        assert!(state.settings.eq_preset_editor.is_some());
    }

    /// `11-03`: `ManageServers` itself now opens the profile-management sub-view.
    #[test]
    fn activate_row_on_manage_servers_opens_the_editor() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Servers, "servers");
        let effects = apply(&mut state, SettingsAction::ActivateRow);
        assert!(effects.is_empty());
        assert!(state.settings.server_editor.is_some());
    }

    /// `11-04`: `ManageSortProfiles` opens the sort-profile management sub-view.
    #[test]
    fn activate_row_on_manage_sort_profiles_opens_the_editor() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Sorting, "sort profiles");
        let effects = apply(&mut state, SettingsAction::ActivateRow);
        assert!(effects.is_empty());
        assert!(state.settings.sort_profile_editor.is_some());
    }

    /// `11-06`: `clear_saved_session_requires_confirm_and_deletes_file` — the reducer-level half
    /// (`workers::cache`'s own test covers the on-disk deletion the emitted effect requests).
    #[test]
    fn clear_saved_session_requires_confirm_and_deletes_file() {
        let mut state = fixtures::fixture_empty();
        state.queue.entries.push(crate::state::queue::QueueEntry {
            entry_id: crate::model::QueueEntryId(0),
            track: fixtures::track(
                "Motion",
                1,
                &fixtures::album("Care", 2019, &fixtures::artist("Boy Harsher")),
                &[&fixtures::artist("Boy Harsher")],
            ),
            source: crate::state::queue::QueueSource::Manual,
            availability: crate::state::queue::Availability::Remote,
        });
        state.queue.play_order.push(0);
        cursor_at(
            &mut state,
            SettingsSection::Interface,
            "clear saved session",
        );

        let effects = apply(&mut state, SettingsAction::ActivateRow);
        assert!(
            effects.is_empty(),
            "activating the row must only open a Confirm, never act immediately"
        );
        assert!(!state.queue.entries.is_empty(), "not cleared yet");
        let Some(crate::state::modal::Modal::Confirm { on_confirm, .. }) = &state.modal else {
            panic!("expected a Confirm modal, got {:?}", state.modal);
        };
        assert_eq!(
            **on_confirm,
            Action::Settings(SettingsAction::ClearSavedSessionConfirmed)
        );

        let effects = apply(&mut state, SettingsAction::ClearSavedSessionConfirmed);
        assert!(state.queue.entries.is_empty());
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Cache(c) if **c == CacheEffect::DeleteSession))
        );
    }

    #[test]
    fn about_copy_diagnostics_emits_toast_and_effect() {
        let mut state = fixtures::fixture_empty();
        let effects = about_copy_diagnostics(&mut state);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::CopyToClipboard(_))))
        );
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("copied to clipboard"))
        );
    }

    #[test]
    fn diagnostics_redacts_token() {
        let mut state = fixtures::fixture_empty();
        state.config.servers.push(ServerConfig {
            id: "srv1".to_string(),
            access_token: "super-secret-token".to_string(),
            ..ServerConfig::default()
        });
        let text = about_diagnostics_text(&state);
        assert!(!text.contains("super-secret-token"));
    }

    #[test]
    fn diagnostics_redacts_custom_header_values() {
        let mut state = fixtures::fixture_empty();
        let mut headers = BTreeMap::new();
        headers.insert(
            "CF-Access-Client-Secret".to_string(),
            "super-secret-header-value".to_string(),
        );
        state.config.servers.push(ServerConfig {
            id: "srv1".to_string(),
            custom_headers: headers,
            ..ServerConfig::default()
        });
        let text = about_diagnostics_text(&state);
        assert!(!text.contains("super-secret-header-value"));
        assert!(text.contains("CF-Access-Client-Secret"));
    }

    #[test]
    fn diagnostics_includes_conflict_count() {
        let mut state = fixtures::fixture_empty();
        // "n" defaults to `NextTrack`; overriding `toggle_shuffle` onto it too is a real,
        // detectable conflict (`keymap::validate`'s own insertion-log check), the same technique
        // `keymap::tests::conflicting_overrides_last_wins_with_warning` uses.
        let mut overrides = BTreeMap::new();
        overrides.insert("toggle_shuffle".to_string(), "n".to_string());
        let (keymap, _warnings) = crate::keymap::KeyMap::from_config(&overrides);
        assert_eq!(
            keymap.validate().len(),
            1,
            "the override must actually conflict"
        );
        state.keymap = keymap;

        let text = about_diagnostics_text(&state);
        assert!(text.contains("Keybinding conflicts: 1"), "{text}");
    }

    #[test]
    fn move_row_clamps_at_both_ends() {
        let mut state = fixtures::fixture_empty();
        state.settings.section = SettingsSection::Transcode;
        apply(&mut state, SettingsAction::MoveRow(-5));
        assert_eq!(state.settings.cursor, 0);
        apply(&mut state, SettingsAction::MoveRow(50));
        let last = row_labels(SettingsSection::Transcode, &state.config).len() - 1;
        assert_eq!(state.settings.cursor, last);
    }

    #[test]
    fn set_row_moves_the_cursor_directly_and_ignores_out_of_range() {
        let mut state = fixtures::fixture_empty();
        state.settings.section = SettingsSection::Transcode;
        apply(&mut state, SettingsAction::SetRow(2));
        assert_eq!(state.settings.cursor, 2);
        apply(&mut state, SettingsAction::SetRow(999));
        assert_eq!(state.settings.cursor, 2, "out-of-range index is ignored");
    }

    #[test]
    fn next_and_prev_section_reset_the_cursor() {
        let mut state = fixtures::fixture_empty();
        state.settings.section = SettingsSection::Servers;
        state.settings.cursor = 2;
        apply(&mut state, SettingsAction::NextSection);
        assert_eq!(state.settings.section, SettingsSection::Audio);
        assert_eq!(state.settings.cursor, 0);
    }

    #[test]
    fn settings_focus_steps_out_rows_to_sections_to_tab_sidebar_and_back() {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = crate::state::nav::Tab::Settings;
        assert!(!state.settings.section_list_focused);
        assert!(!state.nav.sidebar_focused);

        // Rows -> section list.
        apply(&mut state, SettingsAction::FocusSectionList);
        assert!(state.settings.section_list_focused);
        assert!(!state.nav.sidebar_focused);

        // Section list -> tab sidebar.
        apply(&mut state, SettingsAction::LeaveToTabSidebar);
        assert!(!state.settings.section_list_focused);
        assert!(state.nav.sidebar_focused);

        // Tab sidebar -> section list (steps back in, clearing the sidebar level).
        apply(&mut state, SettingsAction::FocusSectionList);
        assert!(state.settings.section_list_focused);
        assert!(!state.nav.sidebar_focused);

        // Section list -> rows.
        apply(&mut state, SettingsAction::FocusRows);
        assert!(!state.settings.section_list_focused);
    }

    #[test]
    fn toggle_flips_a_bool_field() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Cache, "prefetch on play");
        assert!(
            !state.config.cache.prefetch_on_play,
            "opt-in, so off by default"
        );
        apply(&mut state, SettingsAction::AdjustValue(1));
        assert!(state.config.cache.prefetch_on_play);
        apply(&mut state, SettingsAction::AdjustValue(1));
        assert!(!state.config.cache.prefetch_on_play);
    }

    #[test]
    fn select_cycles_forward_and_wraps() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Transcode, "target codec");
        assert_eq!(state.config.transcode.target_codec, TargetCodec::Mp3);
        apply(&mut state, SettingsAction::AdjustValue(1));
        assert_eq!(state.config.transcode.target_codec, TargetCodec::Aac);
        apply(&mut state, SettingsAction::AdjustValue(1));
        assert_eq!(state.config.transcode.target_codec, TargetCodec::Opus);
        apply(&mut state, SettingsAction::AdjustValue(1));
        assert_eq!(
            state.config.transcode.target_codec,
            TargetCodec::Mp3,
            "must wrap back to the first option"
        );
    }

    #[test]
    fn number_control_steps_and_clamps() {
        let mut state = fixtures::fixture_empty();
        cursor_at(&mut state, SettingsSection::Audio, "buffer size");
        assert_eq!(state.config.audio.buffer_size_ms, 2000);
        apply(&mut state, SettingsAction::AdjustValue(-1900));
        assert_eq!(state.config.audio.buffer_size_ms, 100, "clamped at min");
    }

    #[test]
    fn config_changed_system_event_goes_through_the_same_live_apply() {
        let mut state = fixtures::fixture_empty();
        let mut new_cfg = state.config.clone();
        new_cfg.ui.theme = crate::theme::BUILTIN_THEME_NAMES[1].to_string();
        let effects = crate::reducer::apply(
            &mut state,
            crate::action::Action::System(crate::action::SystemEvent::ConfigChanged(Box::new(
                new_cfg.clone(),
            ))),
        );
        assert_eq!(state.config.ui.theme, new_cfg.ui.theme);
        assert_eq!(
            state.theme,
            crate::theme::Theme::builtin(&new_cfg.ui.theme).unwrap()
        );
        let _ = effects;
    }

    // -----------------------------------------------------------------------------------------
    // `11-03`: server profiles.
    // -----------------------------------------------------------------------------------------

    fn server(id: &str, name: &str) -> ServerConfig {
        ServerConfig {
            id: id.to_string(),
            name: name.to_string(),
            url: format!("https://{id}.example.com"),
            user_id: "user-1".to_string(),
            access_token: "old-token".to_string(),
            device_id: format!("device-{id}"),
            custom_headers: BTreeMap::new(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }
    }

    /// Moves the draft's cursor to `target` by searching the field list, rather than counting rows
    /// — adding a row to the form must not silently re-point every test at its neighbour.
    fn focus_field(state: &mut AppState, target: ServerDraftField) {
        let draft = state
            .settings
            .server_editor
            .as_mut()
            .unwrap()
            .editing
            .as_mut()
            .unwrap();
        draft.field = server_draft_fields(draft)
            .iter()
            .position(|f| *f == target)
            .expect("field is in the form");
    }

    fn open_editor(state: &mut AppState) {
        state.settings.section = SettingsSection::Servers;
        state.settings.server_editor = Some(ServerEditorState::default());
    }

    #[test]
    fn add_new_stamps_a_device_id_immediately() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        let draft = state
            .settings
            .server_editor
            .unwrap()
            .editing
            .expect("form should be open");
        assert!(!draft.device_id.is_empty());
        // RFC 4122 v4: `8-4-4-4-12` hex groups.
        assert_eq!(draft.device_id.len(), 36);
    }

    /// `device_id_generated_once_per_profile` — editing an existing profile must reuse its own
    /// `device_id`, never regenerate one.
    #[test]
    fn editing_an_existing_profile_keeps_its_device_id() {
        let mut state = fixtures::fixture_empty();
        state.config.servers.push(server("srv", "My Server"));
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorEdit);
        let draft = state
            .settings
            .server_editor
            .unwrap()
            .editing
            .expect("form should be open");
        assert_eq!(draft.device_id, "device-srv");
    }

    /// `token_is_not_an_editable_field` — the draft has no field a user could type a token into;
    /// `access_token`/`user_id` only ever arrive via a successful `ServerTestSucceeded` reply.
    #[test]
    fn editing_an_existing_profile_never_exposes_its_token() {
        let mut state = fixtures::fixture_empty();
        state.config.servers.push(server("srv", "My Server"));
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorEdit);
        let draft = state
            .settings
            .server_editor
            .unwrap()
            .editing
            .expect("form should be open");
        // Every field on `ServerDraft` is accounted for here; none of them is a token.
        assert_eq!(draft.username, "");
        assert_eq!(draft.password, "");
    }

    #[test]
    fn saving_writes_only_the_freshly_authenticated_token_never_anything_typed() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        {
            let draft = state
                .settings
                .server_editor
                .as_mut()
                .unwrap()
                .editing
                .as_mut()
                .unwrap();
            draft.name = "My Server".to_string();
            draft.protocol = "https".to_string();
            draft.host = "example.com".to_string();
            draft.saving = true;
            draft.testing = true;
        }
        server_editor_test_succeeded(
            &mut state,
            "user-42".to_string(),
            "fresh-token".to_string(),
            "Home Library".to_string(),
            "4.8.0.80".to_string(),
        );
        let saved = state
            .config
            .servers
            .iter()
            .find(|s| s.name == "My Server")
            .expect("profile should have been saved");
        assert_eq!(saved.user_id, "user-42");
        assert_eq!(saved.access_token, "fresh-token");
    }

    #[test]
    fn build_server_url_omits_port_when_empty() {
        assert_eq!(
            build_server_url("https", "emby.example.com", ""),
            "https://emby.example.com"
        );
        assert_eq!(
            build_server_url("http", "192.168.1.5", "8096"),
            "http://192.168.1.5:8096"
        );
    }

    #[test]
    fn split_server_url_round_trips_build_server_url() {
        assert_eq!(
            split_server_url("https://emby.example.com:8096"),
            (
                "https".to_string(),
                "emby.example.com".to_string(),
                "8096".to_string()
            )
        );
        assert_eq!(
            split_server_url("http://192.168.1.5"),
            ("http".to_string(), "192.168.1.5".to_string(), String::new())
        );
    }

    #[test]
    fn split_server_url_ignores_a_non_numeric_trailing_colon_segment() {
        // Not a port — the whole thing is the host.
        assert_eq!(
            split_server_url("https://emby.example.com"),
            (
                "https".to_string(),
                "emby.example.com".to_string(),
                String::new()
            )
        );
    }

    #[test]
    fn cycle_protocol_wraps_both_ways_and_only_acts_on_the_protocol_field() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        // Freshly added: protocol defaults to "https", field 0 (Name) is focused.
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(1));
        let protocol = |state: &AppState| {
            state
                .settings
                .server_editor
                .as_ref()
                .unwrap()
                .editing
                .as_ref()
                .unwrap()
                .protocol
                .clone()
        };
        assert_eq!(protocol(&state), "https", "wrong field focused: no-op");

        focus_field(&mut state, ServerDraftField::Protocol);
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(1));
        assert_eq!(protocol(&state), "http");
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(1));
        assert_eq!(protocol(&state), "https", "wraps forward");
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(-1));
        assert_eq!(protocol(&state), "http", "wraps backward");
    }

    /// Fallback addresses had no UI at all — they could only be written into `config.toml` by
    /// hand. The form now edits *whichever address is selected*, so everything below the selector
    /// (protocol, host, port, headers, and `Test connection`) applies to it
    /// (`docs/12-decisions.md`).
    #[test]
    fn the_editor_adds_a_second_address_and_saves_it_as_a_fallback() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);

        // The primary.
        set_field(&mut state, ServerDraftField::Host, "192.168.1.10");
        set_field(&mut state, ServerDraftField::Port, "8096");
        focus_field(&mut state, ServerDraftField::Protocol);
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(1)); // -> http

        // A second address, with the header only that path needs.
        focus_field(&mut state, ServerDraftField::AddEndpointAction);
        apply(&mut state, SettingsAction::ServerEditorAddEndpoint);
        set_field(&mut state, ServerDraftField::Host, "music.example.com");
        {
            let draft = draft_mut(&mut state);
            draft.new_header_name = "CF-Access-Client-Id".to_string();
            draft.new_header_value = "abc".to_string();
        }
        apply(&mut state, SettingsAction::ServerEditorAddHeader);

        // Switching back must not lose either side's values.
        focus_field(&mut state, ServerDraftField::Endpoint);
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(-1));
        {
            let draft = draft_mut(&mut state);
            assert_eq!(draft.endpoint, 0);
            assert_eq!(draft.host, "192.168.1.10");
            assert_eq!(draft.protocol, "http");
            assert!(
                draft.headers.is_empty(),
                "the fallback's header belongs to the fallback"
            );
        }

        let draft = draft_mut(&mut state);
        draft.stash_endpoint();
        assert_eq!(draft.endpoints.len(), 2);
        assert_eq!(draft.endpoints[1].host, "music.example.com");
        assert_eq!(draft.endpoints[1].headers.len(), 1);
    }

    /// The primary is not removable — a profile with no address is not a profile.
    #[test]
    fn removing_an_address_only_ever_drops_a_fallback() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        focus_field(&mut state, ServerDraftField::AddEndpointAction);
        apply(&mut state, SettingsAction::ServerEditorAddEndpoint);
        assert_eq!(draft_mut(&mut state).endpoints.len(), 2);

        focus_field(&mut state, ServerDraftField::Endpoint);
        apply(&mut state, SettingsAction::ServerEditorRemoveFocusedHeader); // `x`
        assert_eq!(draft_mut(&mut state).endpoints.len(), 1);
        assert_eq!(draft_mut(&mut state).endpoint, 0);

        // Again, on the primary: refused.
        apply(&mut state, SettingsAction::ServerEditorRemoveFocusedHeader);
        assert_eq!(draft_mut(&mut state).endpoints.len(), 1);
    }

    /// Types into a field the way the UI does — `StartTextEdit`, characters, commit — rather than
    /// assigning the struct field directly. The direct assignment is what let a real bug through:
    /// it skips `text_buf` entirely, and `text_buf` is where the in-progress edit lives.
    fn type_into(state: &mut AppState, field: ServerDraftField, text: &str) {
        focus_field(state, field);
        apply(state, SettingsAction::StartTextEdit);
        for c in text.chars() {
            apply(state, SettingsAction::TextInput(c));
        }
        apply(state, SettingsAction::CommitTextEdit);
    }

    /// As `each_address_keeps_its_own_values`, but navigating with `MoveRow` exactly as the UI
    /// does rather than placing the cursor directly.
    #[test]
    fn each_address_keeps_its_own_values_when_navigating_by_row() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);

        let move_to = |state: &mut AppState, target: ServerDraftField| {
            for _ in 0..40 {
                let draft = draft_mut(state);
                if server_draft_field_at(draft, draft.field) == Some(target) {
                    return;
                }
                apply(state, SettingsAction::MoveRow(1));
            }
            panic!("never reached {target:?}");
        };

        move_to(&mut state, ServerDraftField::Host);
        apply(&mut state, SettingsAction::StartTextEdit);
        for c in "192.168.1.10".chars() {
            apply(&mut state, SettingsAction::TextInput(c));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        // Back up to the add-address row and press it.
        for _ in 0..40 {
            let draft = draft_mut(&mut state);
            if server_draft_field_at(draft, draft.field)
                == Some(ServerDraftField::AddEndpointAction)
            {
                break;
            }
            apply(&mut state, SettingsAction::MoveRow(-1));
        }
        apply(&mut state, SettingsAction::ServerEditorAddEndpoint);

        move_to(&mut state, ServerDraftField::Host);
        apply(&mut state, SettingsAction::StartTextEdit);
        for c in "music.example.com".chars() {
            apply(&mut state, SettingsAction::TextInput(c));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        for _ in 0..40 {
            let draft = draft_mut(&mut state);
            if server_draft_field_at(draft, draft.field) == Some(ServerDraftField::Endpoint) {
                break;
            }
            apply(&mut state, SettingsAction::MoveRow(-1));
        }
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(1));

        assert_eq!(draft_mut(&mut state).endpoint, 0);
        assert_eq!(draft_mut(&mut state).host, "192.168.1.10");
    }

    /// Saving while a *fallback* was the selected address wrote its values into the profile's own
    /// `url`/`custom_headers` as well as into `fallbacks[0]` — so both addresses came back as the
    /// last one added, and removing the fallback left the primary holding its values. Reported from
    /// live use (`docs/12-decisions.md`).
    #[test]
    fn saving_while_a_fallback_is_selected_does_not_overwrite_the_primary() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        type_into(&mut state, ServerDraftField::Name, "Sanji");
        type_into(&mut state, ServerDraftField::Host, "192.168.1.10");
        type_into(&mut state, ServerDraftField::Port, "8096");

        focus_field(&mut state, ServerDraftField::AddEndpointAction);
        apply(&mut state, SettingsAction::ServerEditorAddEndpoint);
        type_into(&mut state, ServerDraftField::Host, "music.example.com");

        // Saved with the fallback still selected — the natural thing to do right after adding one.
        assert_eq!(draft_mut(&mut state).endpoint, 1, "fixture assumption");
        draft_mut(&mut state).testing = true;
        draft_mut(&mut state).saving = true;
        server_editor_test_succeeded(
            &mut state,
            "user-1".to_string(),
            "token".to_string(),
            "Sanji".to_string(),
            "4.9".to_string(),
        );

        let saved = &state.config.servers[0];
        assert!(
            saved.url.contains("192.168.1.10"),
            "the primary must keep its own address, got {}",
            saved.url
        );
        assert_eq!(saved.fallbacks.len(), 1);
        assert!(saved.fallbacks[0].url.contains("music.example.com"));
    }

    /// The exact sequence a user reported: fill in the primary, add a second address, fill that in,
    /// then look at them. Both showed the values of the one added last, and removing the fallback
    /// left the primary holding *its* values (`docs/12-decisions.md`).
    #[test]
    fn each_address_keeps_its_own_values() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        type_into(&mut state, ServerDraftField::Host, "192.168.1.10");

        focus_field(&mut state, ServerDraftField::AddEndpointAction);
        apply(&mut state, SettingsAction::ServerEditorAddEndpoint);
        type_into(&mut state, ServerDraftField::Host, "music.example.com");

        // Back to the primary.
        focus_field(&mut state, ServerDraftField::Endpoint);
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(-1));
        assert_eq!(draft_mut(&mut state).endpoint, 0);
        assert_eq!(
            draft_mut(&mut state).host,
            "192.168.1.10",
            "the primary must still be the primary"
        );

        // And forward to the fallback again.
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(1));
        assert_eq!(draft_mut(&mut state).host, "music.example.com");

        // Removing the fallback must leave the primary exactly as it was.
        apply(&mut state, SettingsAction::ServerEditorRemoveFocusedHeader); // `x`
        assert_eq!(draft_mut(&mut state).endpoints.len(), 1);
        assert_eq!(draft_mut(&mut state).host, "192.168.1.10");
    }

    fn draft_mut(state: &mut AppState) -> &mut crate::state::settings::ServerDraft {
        state
            .settings
            .server_editor
            .as_mut()
            .unwrap()
            .editing
            .as_mut()
            .unwrap()
    }

    fn set_field(state: &mut AppState, field: ServerDraftField, text: &str) {
        focus_field(state, field);
        let draft = draft_mut(state);
        match field {
            ServerDraftField::Host => draft.host = text.to_string(),
            ServerDraftField::Port => draft.port = text.to_string(),
            ServerDraftField::Name => draft.name = text.to_string(),
            other => panic!("{other:?} is not a plain text field"),
        }
    }

    #[test]
    fn cycle_protocol_invalidates_stale_test_result() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        focus_field(&mut state, ServerDraftField::Protocol);
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.test_result = Some(Ok("connected to Home Library (v4.8.0)".to_string()));
        }
        apply(&mut state, SettingsAction::ServerEditorCycleProtocol(1));
        let draft = state.settings.server_editor.unwrap().editing.unwrap();
        assert!(draft.test_result.is_none());
    }

    #[test]
    fn editing_an_existing_profile_splits_its_url_into_three_fields() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        state.config.servers.push(ServerConfig {
            id: "srv1".to_string(),
            name: "Home".to_string(),
            url: "http://192.168.1.5:8096".to_string(),
            ..ServerConfig::default()
        });
        apply(&mut state, SettingsAction::ServerEditorEdit);
        let draft = state.settings.server_editor.unwrap().editing.unwrap();
        assert_eq!(draft.protocol, "http");
        assert_eq!(draft.host, "192.168.1.5");
        assert_eq!(draft.port, "8096");
    }

    #[test]
    fn test_connection_works_before_save() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        {
            let draft = state
                .settings
                .server_editor
                .as_mut()
                .unwrap()
                .editing
                .as_mut()
                .unwrap();
            draft.protocol = "https".to_string();
            draft.host = "example.com".to_string();
            draft.username = "alice".to_string();
            draft.password = "s3cr3t-pw".to_string();
        }
        let effects = apply(&mut state, SettingsAction::ServerEditorTestConnection);
        assert!(state.config.servers.is_empty(), "testing must never save");
        match effects.as_slice() {
            [Effect::Net(NetEffect::TestServerConnection { url, username, .. })] => {
                assert_eq!(url, "https://example.com");
                assert_eq!(username, "alice");
            }
            other => panic!("expected exactly one TestServerConnection effect, got {other:?}"),
        }
    }

    #[test]
    fn test_connection_reports_server_name_and_version() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.testing = true;
        }
        server_editor_test_succeeded(
            &mut state,
            "user-1".to_string(),
            "tok".to_string(),
            "Home Library".to_string(),
            "4.8.0.80".to_string(),
        );
        let draft = state
            .settings
            .server_editor
            .unwrap()
            .editing
            .expect("a plain test must not close the form");
        assert_eq!(
            draft.test_result,
            Some(Ok("connected to Home Library (v4.8.0.80)".to_string()))
        );
    }

    /// `test_connection_maps_error_to_message` — the mapped `EmbyError` message (already computed
    /// by `loxia-emby`/`workers::network`, which this reducer cannot itself depend on to redo) is
    /// stored verbatim; three representative messages stand in for unauthorized/offline/invalid
    /// URL, matching this task's own table test.
    #[test]
    fn test_connection_maps_error_to_message() {
        for message in [
            "the server rejected these credentials.",
            "the server could not be reached.",
            "\"not a url\" is not a valid server URL.",
        ] {
            let mut state = fixtures::fixture_empty();
            open_editor(&mut state);
            apply(&mut state, SettingsAction::ServerEditorAddNew);
            if let Some(draft) = state
                .settings
                .server_editor
                .as_mut()
                .and_then(|e| e.editing.as_mut())
            {
                draft.testing = true;
            }
            server_editor_test_failed(&mut state, message.to_string());
            let draft = state
                .settings
                .server_editor
                .unwrap()
                .editing
                .expect("a failed test must not close the form");
            assert_eq!(draft.test_result, Some(Err(message.to_string())));
        }
    }

    #[test]
    fn reserved_header_rejected_inline() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.new_header_name = "X-Emby-Authorization".to_string();
            draft.new_header_value = "whatever".to_string();
        }
        apply(&mut state, SettingsAction::ServerEditorAddHeader);
        let draft = state.settings.server_editor.unwrap().editing.unwrap();
        assert!(draft.headers.is_empty());
        assert!(
            draft
                .header_error
                .as_deref()
                .unwrap()
                .contains("cannot be overridden")
        );
    }

    #[test]
    fn non_reserved_header_is_added() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.new_header_name = "CF-Access-Client-Id".to_string();
            draft.new_header_value = "abc123".to_string();
        }
        apply(&mut state, SettingsAction::ServerEditorAddHeader);
        let draft = state.settings.server_editor.unwrap().editing.unwrap();
        assert_eq!(
            draft.headers,
            vec![("CF-Access-Client-Id".to_string(), "abc123".to_string())]
        );
        assert!(draft.header_error.is_none());
    }

    #[test]
    fn switching_requires_confirmation() {
        let mut state = fixtures::fixture_empty();
        state.config.servers.push(server("a", "Server A"));
        state.config.servers.push(server("b", "Server B"));
        state.config.active_server = "a".to_string();
        open_editor(&mut state);
        state.settings.server_editor.as_mut().unwrap().cursor = 1; // "Server B", not active

        let effects = apply(&mut state, SettingsAction::ServerEditorSwitch);
        assert!(effects.is_empty());
        assert_eq!(
            state.config.active_server, "a",
            "must not switch before confirmation"
        );
        assert!(matches!(
            state.modal,
            Some(crate::state::modal::Modal::Confirm { .. })
        ));
    }

    #[test]
    fn switching_to_the_already_active_server_is_a_no_op() {
        let mut state = fixtures::fixture_empty();
        state.config.servers.push(server("a", "Server A"));
        state.config.active_server = "a".to_string();
        open_editor(&mut state);

        let effects = apply(&mut state, SettingsAction::ServerEditorSwitch);
        assert!(effects.is_empty());
        assert!(state.modal.is_none());
    }

    #[test]
    fn switching_clears_queue_and_persists_outgoing_session() {
        let mut state = fixtures::fixture_playing_queue();
        state.config.servers.push(server("a", "Server A"));
        state.config.servers.push(server("b", "Server B"));
        state.config.active_server = "a".to_string();
        state.history.push_front(crate::state::queue::HistoryEntry {
            track: fixtures::track(
                "Motion",
                1,
                &fixtures::album("Care", 2019, &fixtures::artist("Boy Harsher")),
                &[&fixtures::artist("Boy Harsher")],
            ),
            played_at: fixtures::fixed_epoch(),
            completed: true,
        });
        assert!(!state.queue.entries.is_empty());

        let effects = apply(
            &mut state,
            SettingsAction::ServerEditorSwitchConfirmed(ServerId::from("b")),
        );

        assert!(state.queue.entries.is_empty());
        assert!(state.history.is_empty());
        assert_eq!(state.config.active_server, "b");

        let persisted = effects.iter().find_map(|e| match e {
            Effect::Cache(boxed) => match boxed.as_ref() {
                CacheEffect::PersistSessionForServer(server_id, snapshot) => {
                    Some((server_id.clone(), snapshot.clone()))
                }
                _ => None,
            },
            _ => None,
        });
        let (persisted_server, snapshot) =
            persisted.expect("the outgoing session must be persisted");
        assert_eq!(persisted_server, ServerId::from("a"));
        assert_eq!(snapshot.server_id, ServerId::from("a"));

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::WriteConfig(_))))
        );
        assert!(effects.iter().any(
            |e| matches!(e, Effect::Sys(SysEffect::ReconnectServer(id)) if *id == ServerId::from("b"))
        ));
    }

    #[test]
    fn removing_active_profile_is_disabled_with_reason() {
        let mut state = fixtures::fixture_empty();
        state.config.servers.push(server("a", "Server A"));
        state.config.active_server = "a".to_string();
        open_editor(&mut state);

        let effects = apply(&mut state, SettingsAction::ServerEditorRemove);
        assert!(effects.is_empty());
        assert_eq!(state.config.servers.len(), 1, "must not be removed");
        assert!(
            state
                .settings
                .server_editor
                .unwrap()
                .error
                .unwrap()
                .contains("switch to another server")
        );
    }

    #[test]
    fn removing_profile_offers_data_deletion_defaulting_to_no() {
        let mut state = fixtures::fixture_empty();
        state.config.servers.push(server("a", "Server A"));
        state.config.servers.push(server("b", "Server B"));
        state.config.active_server = "a".to_string();
        open_editor(&mut state);
        state.settings.server_editor.as_mut().unwrap().cursor = 1; // "Server B"

        let effects = apply(&mut state, SettingsAction::ServerEditorRemove);
        assert_eq!(state.config.servers.len(), 1);
        assert!(!state.config.servers.iter().any(|s| s.id == "b"));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Cache(boxed) if matches!(boxed.as_ref(), CacheEffect::DeleteServerData(_)))),
            "must default to *not* deleting cached data"
        );
    }

    #[test]
    fn removing_profile_deletes_data_when_toggled_on() {
        let mut state = fixtures::fixture_empty();
        state.config.servers.push(server("a", "Server A"));
        state.config.servers.push(server("b", "Server B"));
        state.config.active_server = "a".to_string();
        open_editor(&mut state);
        state.settings.server_editor.as_mut().unwrap().cursor = 1;
        apply(&mut state, SettingsAction::ServerEditorToggleDeleteData);

        let effects = apply(&mut state, SettingsAction::ServerEditorRemove);
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Cache(boxed) if matches!(boxed.as_ref(), CacheEffect::DeleteServerData(id) if *id == ServerId::from("b"))
        )));
    }

    #[test]
    fn tab_cycles_fields_and_new_header_is_added_between_password_and_new_header_inputs() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        let draft = state
            .settings
            .server_editor
            .as_ref()
            .unwrap()
            .editing
            .as_ref()
            .unwrap();
        assert_eq!(server_draft_field_count(draft), 8 + 5); // no headers yet
        assert_eq!(
            server_draft_field_at(draft, 8),
            Some(ServerDraftField::NewHeaderName)
        );

        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft
                .headers
                .push(("X-Custom".to_string(), "v".to_string()));
        }
        let draft = state
            .settings
            .server_editor
            .as_ref()
            .unwrap()
            .editing
            .as_ref()
            .unwrap();
        assert_eq!(
            server_draft_field_at(draft, 8),
            Some(ServerDraftField::Header(0))
        );
        assert_eq!(
            server_draft_field_at(draft, 9),
            Some(ServerDraftField::NewHeaderName)
        );
    }

    #[test]
    fn remove_focused_header_only_acts_on_a_header_row() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft
                .headers
                .push(("X-Custom".to_string(), "v".to_string()));
        }
        focus_field(&mut state, ServerDraftField::Header(0));
        apply(&mut state, SettingsAction::ServerEditorRemoveFocusedHeader);
        let draft = state.settings.server_editor.unwrap().editing.unwrap();
        assert!(draft.headers.is_empty());
    }

    #[test]
    fn esc_from_form_returns_to_list_esc_from_list_closes_editor() {
        let mut state = fixtures::fixture_empty();
        open_editor(&mut state);
        apply(&mut state, SettingsAction::ServerEditorAddNew);
        assert!(
            state
                .settings
                .server_editor
                .as_ref()
                .unwrap()
                .editing
                .is_some()
        );

        apply(&mut state, SettingsAction::ServerEditorClose);
        assert!(
            state
                .settings
                .server_editor
                .as_ref()
                .unwrap()
                .editing
                .is_none()
        );
        assert!(state.settings.server_editor.is_some());

        apply(&mut state, SettingsAction::ServerEditorClose);
        assert!(state.settings.server_editor.is_none());
    }

    // -----------------------------------------------------------------------------------------
    // `11-04`: sort profiles.
    // -----------------------------------------------------------------------------------------

    fn sort_profile(
        name: &str,
        rules: Vec<(config::SortField, config::Direction)>,
    ) -> config::SortProfile {
        config::SortProfile {
            name: name.to_string(),
            rules: rules
                .into_iter()
                .map(|(field, direction)| config::SortRule { field, direction })
                .collect(),
        }
    }

    /// `Config::default()` seeds two real sort profiles (`design_overview` §7's own
    /// `chronological_discog`/`release_chronology`) — cleared here so every test below starts
    /// from a genuinely blank slate and can assert on exact indices/counts of its own profiles.
    fn open_sort_editor(state: &mut AppState) {
        state.config.sorting.profiles.clear();
        state.config.sorting.default_queue_profile.clear();
        state.settings.section = SettingsSection::Sorting;
        state.settings.sort_profile_editor = Some(SortProfileEditorState::default());
    }

    #[test]
    fn add_new_profile_from_empty_via_name_buffer() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state); // clears all profiles

        apply(&mut state, SettingsAction::SortEditorNew);
        assert!(
            state
                .settings
                .sort_profile_editor
                .as_ref()
                .unwrap()
                .name_buf
                .is_some(),
            "SortEditorNew opens the name buffer"
        );
        for c in "my mix".chars() {
            apply(&mut state, SettingsAction::TextInput(c));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert_eq!(
            state
                .config
                .sorting
                .profiles
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["my mix"],
            "the new profile is added"
        );
    }

    #[test]
    fn add_rule_up_to_four() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("p", vec![]));

        for _ in 0..4 {
            apply(&mut state, SettingsAction::SortEditorAddRule);
        }
        assert_eq!(state.config.sorting.profiles[0].rules.len(), 4);
    }

    #[test]
    fn fifth_rule_refused_with_inline_reason() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state.config.sorting.profiles.push(sort_profile(
            "p",
            vec![(config::SortField::Name, config::Direction::Asc); 4],
        ));

        let effects = apply(&mut state, SettingsAction::SortEditorAddRule);
        assert!(effects.is_empty());
        assert_eq!(state.config.sorting.profiles[0].rules.len(), 4);
        assert_eq!(
            state.settings.sort_profile_editor.unwrap().error.as_deref(),
            Some("maximum 4 rules")
        );
    }

    #[test]
    fn delete_rule() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state.config.sorting.profiles.push(sort_profile(
            "p",
            vec![
                (config::SortField::Name, config::Direction::Asc),
                (config::SortField::Year, config::Direction::Desc),
            ],
        ));
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .rule_cursor = Some(0);

        apply(&mut state, SettingsAction::SortEditorDeleteRule);
        assert_eq!(state.config.sorting.profiles[0].rules.len(), 1);
        assert_eq!(
            state.config.sorting.profiles[0].rules[0].field,
            config::SortField::Year
        );
    }

    #[test]
    fn reorder_rules() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state.config.sorting.profiles.push(sort_profile(
            "p",
            vec![
                (config::SortField::Name, config::Direction::Asc),
                (config::SortField::Year, config::Direction::Desc),
            ],
        ));
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .rule_cursor = Some(0);

        apply(&mut state, SettingsAction::SortEditorReorderRule(1));
        assert_eq!(
            state.config.sorting.profiles[0].rules[0].field,
            config::SortField::Year
        );
        assert_eq!(
            state.config.sorting.profiles[0].rules[1].field,
            config::SortField::Name
        );
        assert_eq!(
            state.settings.sort_profile_editor.unwrap().rule_cursor,
            Some(1)
        );
    }

    #[test]
    fn direction_toggle() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state.config.sorting.profiles.push(sort_profile(
            "p",
            vec![(config::SortField::Name, config::Direction::Asc)],
        ));
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .rule_cursor = Some(0);

        apply(&mut state, SettingsAction::SortEditorToggleDirection);
        assert_eq!(
            state.config.sorting.profiles[0].rules[0].direction,
            config::Direction::Desc
        );
        apply(&mut state, SettingsAction::SortEditorToggleDirection);
        assert_eq!(
            state.config.sorting.profiles[0].rules[0].direction,
            config::Direction::Asc
        );
    }

    #[test]
    fn cycle_field_wraps() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state.config.sorting.profiles.push(sort_profile(
            "p",
            vec![(config::SortField::DateAdded, config::Direction::Asc)], // last in SORT_FIELD_ORDER
        ));
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .rule_cursor = Some(0);

        apply(&mut state, SettingsAction::SortEditorCycleField(1));
        assert_eq!(
            state.config.sorting.profiles[0].rules[0].field,
            config::SortField::Name
        );
        apply(&mut state, SettingsAction::SortEditorCycleField(-1));
        assert_eq!(
            state.config.sorting.profiles[0].rules[0].field,
            config::SortField::DateAdded
        );
    }

    #[test]
    fn duplicate_name_rejected() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("existing", vec![]));
        apply(&mut state, SettingsAction::SortEditorNew);
        if let Some(editor) = &mut state.settings.sort_profile_editor {
            editor.name_buf = Some(TextEdit::new("existing"));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert_eq!(state.config.sorting.profiles.len(), 1);
        assert_eq!(
            state.settings.sort_profile_editor.unwrap().error.as_deref(),
            Some("a profile with that name exists")
        );
    }

    #[test]
    fn empty_name_rejected() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        apply(&mut state, SettingsAction::SortEditorNew);
        if let Some(editor) = &mut state.settings.sort_profile_editor {
            editor.name_buf = Some(TextEdit::new("   "));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert!(state.config.sorting.profiles.is_empty());
        assert_eq!(
            state.settings.sort_profile_editor.unwrap().error.as_deref(),
            Some("a profile name is required")
        );
    }

    #[test]
    fn new_profile_is_created_with_no_rules() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        apply(&mut state, SettingsAction::SortEditorNew);
        if let Some(editor) = &mut state.settings.sort_profile_editor {
            editor.name_buf = Some(TextEdit::new("fresh"));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert_eq!(state.config.sorting.profiles.len(), 1);
        assert_eq!(state.config.sorting.profiles[0].name, "fresh");
        assert!(state.config.sorting.profiles[0].rules.is_empty());
    }

    #[test]
    fn rename_updates_default_reference() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("old_name", vec![]));
        state.config.sorting.default_queue_profile = "old_name".to_string();
        apply(&mut state, SettingsAction::SortEditorRename);
        if let Some(editor) = &mut state.settings.sort_profile_editor {
            editor.name_buf = Some(TextEdit::new("new_name"));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert_eq!(state.config.sorting.profiles[0].name, "new_name");
        assert_eq!(state.config.sorting.default_queue_profile, "new_name");
    }

    #[test]
    fn deleting_default_profile_clears_default_and_warns() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("only", vec![]));
        state.config.sorting.default_queue_profile = "only".to_string();

        apply(&mut state, SettingsAction::SortEditorDeleteProfileConfirmed);
        assert!(state.config.sorting.profiles.is_empty());
        assert_eq!(state.config.sorting.default_queue_profile, "");
        assert!(
            state
                .config_warnings
                .iter()
                .any(|w| w.message.contains("default_queue_profile"))
        );
    }

    #[test]
    fn deleting_default_profile_falls_back_to_another_remaining_one() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("a", vec![]));
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("b", vec![]));
        state.config.sorting.default_queue_profile = "a".to_string();
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .profile_cursor = 0;

        apply(&mut state, SettingsAction::SortEditorDeleteProfileConfirmed);
        assert_eq!(state.config.sorting.profiles.len(), 1);
        assert_eq!(state.config.sorting.default_queue_profile, "b");
    }

    #[test]
    fn restore_defaults_readds_builtins_from_empty() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state); // clears all profiles and the default

        apply(&mut state, SettingsAction::SortEditorRestoreDefaults);

        let names: Vec<&str> = state
            .config
            .sorting
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["chronological_discog", "release_chronology"]);
        assert_eq!(
            state.config.sorting.default_queue_profile, "chronological_discog",
            "a cleared default is repointed at the built-in default"
        );
    }

    #[test]
    fn restore_defaults_never_overwrites_an_existing_same_named_profile() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        // A user's own profile that happens to share a built-in name — must survive untouched.
        state.config.sorting.profiles.push(sort_profile(
            "chronological_discog",
            vec![(config::SortField::Name, config::Direction::Asc)],
        ));

        apply(&mut state, SettingsAction::SortEditorRestoreDefaults);

        // The existing one kept its single (custom) rule; only the missing built-in was added.
        assert_eq!(state.config.sorting.profiles[0].rules.len(), 1);
        let names: Vec<&str> = state
            .config
            .sorting
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["chronological_discog", "release_chronology"]);
    }

    #[test]
    fn deleting_a_profile_asks_for_confirmation_first() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("keepme", vec![]));

        // `x` opens a Confirm modal naming the profile; nothing is deleted yet.
        apply(&mut state, SettingsAction::SortEditorDeleteProfile);
        match state.modal.as_ref() {
            Some(crate::state::modal::Modal::Confirm { prompt, .. }) => {
                assert!(prompt.contains("keepme"), "names the profile: {prompt}");
            }
            other => panic!("expected a Confirm modal, got {other:?}"),
        }
        assert_eq!(state.config.sorting.profiles.len(), 1, "not deleted yet");

        // Confirming (Submit) runs the actual deletion.
        crate::reducer::apply(
            &mut state,
            Action::Modal(crate::action::ModalAction::Submit),
        );
        assert!(state.config.sorting.profiles.is_empty());
    }

    #[test]
    fn deleting_last_profile_allowed() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("only", vec![]));

        apply(&mut state, SettingsAction::SortEditorDeleteProfileConfirmed);
        assert!(state.config.sorting.profiles.is_empty());
        // "Changes persist through the standard 1-second debounce" (this task's own spec) — via
        // `on_config_changed`, the same path every other config-writing edit in this module uses;
        // no *immediate* `WriteConfig` effect is expected here.
        assert!(state.settings_write_debounce_until.is_some());
    }

    #[test]
    fn editing_does_not_reorder_queue() {
        let mut state = fixtures::fixture_playing_queue();
        open_sort_editor(&mut state);
        state.config.sorting.profiles.push(sort_profile(
            "p",
            vec![(config::SortField::Name, config::Direction::Asc)],
        ));
        let before = state.queue.clone();
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .rule_cursor = Some(0);

        apply(&mut state, SettingsAction::SortEditorToggleDirection);
        apply(&mut state, SettingsAction::SortEditorCycleField(1));
        apply(&mut state, SettingsAction::SortEditorAddRule);

        assert_eq!(
            state.queue, before,
            "editing a profile must never touch the live queue"
        );
    }

    #[test]
    fn apply_reorders_the_queue() {
        let mut state = fixtures::fixture_playing_queue();
        open_sort_editor(&mut state);
        state.config.sorting.profiles.push(sort_profile(
            "by_name",
            vec![(config::SortField::Name, config::Direction::Asc)],
        ));
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .profile_cursor = 0;
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .rule_cursor = None;

        let before = state.queue.clone();
        apply(&mut state, SettingsAction::SortEditorApply);
        // `06-04`'s own `apply_sort_profile` is exercised in full elsewhere (`reducer::queue`);
        // this only proves the editor's own `Enter` on a header row actually reaches it.
        assert_eq!(state.queue.sort_profile.as_deref(), Some("by_name"));
        let _ = before;
    }

    #[test]
    fn navigation_moves_between_profiles_and_into_their_own_rules() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        state.config.sorting.profiles.push(sort_profile(
            "a",
            vec![(config::SortField::Name, config::Direction::Asc)],
        ));
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("b", vec![]));

        apply(&mut state, SettingsAction::MoveRow(1)); // "a"'s own rule 0
        {
            let editor = state.settings.sort_profile_editor.as_ref().unwrap();
            assert_eq!(editor.profile_cursor, 0);
            assert_eq!(editor.rule_cursor, Some(0));
        }
        apply(&mut state, SettingsAction::MoveRow(1)); // profile "b"'s own header
        let editor = state.settings.sort_profile_editor.unwrap();
        assert_eq!(editor.profile_cursor, 1);
        assert_eq!(editor.rule_cursor, None);
    }

    #[test]
    fn esc_from_name_edit_returns_to_editor_esc_from_editor_closes_it() {
        let mut state = fixtures::fixture_empty();
        open_sort_editor(&mut state);
        apply(&mut state, SettingsAction::SortEditorNew);
        assert!(
            state
                .settings
                .sort_profile_editor
                .as_ref()
                .unwrap()
                .name_buf
                .is_some()
        );

        apply(&mut state, SettingsAction::SortEditorClose);
        assert!(
            state
                .settings
                .sort_profile_editor
                .as_ref()
                .unwrap()
                .name_buf
                .is_none()
        );
        assert!(state.settings.sort_profile_editor.is_some());

        apply(&mut state, SettingsAction::SortEditorClose);
        assert!(state.settings.sort_profile_editor.is_none());
    }

    // -----------------------------------------------------------------------------------------
    // `11-05`: EQ presets.
    // -----------------------------------------------------------------------------------------

    fn eq_preset(name: &str, gain: f32) -> config::EqPreset {
        config::EqPreset {
            name: name.to_string(),
            gains: [gain; 10],
        }
    }

    fn open_eq_editor(state: &mut AppState) {
        state.settings.section = SettingsSection::Equalizer;
        state.settings.eq_preset_editor = Some(EqPresetEditorState::default());
    }

    fn eq_cursor_at(state: &AppState, name: &str) -> usize {
        eq_preset_rows(&state.config)
            .iter()
            .position(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no preset named {name:?}"))
    }

    #[test]
    fn factory_presets_are_readonly_with_reason() {
        let mut state = fixtures::fixture_empty();
        open_eq_editor(&mut state);
        state.settings.eq_preset_editor.as_mut().unwrap().cursor = eq_cursor_at(&state, "flat");

        let effects = apply(&mut state, SettingsAction::EqEditorRename);
        assert!(effects.is_empty());
        assert_eq!(
            state
                .settings
                .eq_preset_editor
                .as_ref()
                .unwrap()
                .error
                .as_deref(),
            Some("factory presets cannot be modified")
        );
        assert!(
            state
                .settings
                .eq_preset_editor
                .as_ref()
                .unwrap()
                .name_buf
                .is_none()
        );

        let effects = apply(&mut state, SettingsAction::EqEditorDelete);
        assert!(effects.is_empty());
        assert_eq!(
            state.settings.eq_preset_editor.unwrap().error.as_deref(),
            Some("factory presets cannot be modified")
        );
    }

    #[test]
    fn save_captures_current_live_gains() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.gains = [3.5; 10];
        open_eq_editor(&mut state);

        apply(&mut state, SettingsAction::EqEditorSaveCurrent);
        assert_eq!(
            state
                .settings
                .eq_preset_editor
                .as_ref()
                .unwrap()
                .captured_gains,
            Some([3.5; 10])
        );

        // Even if the live gains change afterwards (say, from tweaking the modal further), the
        // *captured* value at the moment `s` was pressed is what gets saved.
        state.player.eq.gains = [9.0; 10];
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.name_buf = Some(TextEdit::new("my preset"));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);
        assert_eq!(state.config.equalizer.custom_presets[0].gains, [3.5; 10]);
    }

    #[test]
    fn save_prompts_for_name() {
        let mut state = fixtures::fixture_empty();
        open_eq_editor(&mut state);
        apply(&mut state, SettingsAction::EqEditorSaveCurrent);
        let editor = state.settings.eq_preset_editor.unwrap();
        assert!(editor.name_buf.is_some());
        assert!(editor.creating);
    }

    #[test]
    fn name_collision_with_factory_rejected() {
        let mut state = fixtures::fixture_empty();
        open_eq_editor(&mut state);
        apply(&mut state, SettingsAction::EqEditorSaveCurrent);
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.name_buf = Some(TextEdit::new("flat"));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert!(state.config.equalizer.custom_presets.is_empty());
        assert_eq!(
            state.settings.eq_preset_editor.unwrap().error.as_deref(),
            Some("that name is used by a factory preset")
        );
    }

    #[test]
    fn name_collision_with_custom_rejected() {
        let mut state = fixtures::fixture_empty();
        state
            .config
            .equalizer
            .custom_presets
            .push(eq_preset("mine", 1.0));
        open_eq_editor(&mut state);
        apply(&mut state, SettingsAction::EqEditorSaveCurrent);
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.name_buf = Some(TextEdit::new("mine"));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert_eq!(state.config.equalizer.custom_presets.len(), 1);
        assert_eq!(
            state.settings.eq_preset_editor.unwrap().error.as_deref(),
            Some("a preset with that name exists")
        );
    }

    #[test]
    fn empty_name_rejected_for_eq_preset() {
        let mut state = fixtures::fixture_empty();
        open_eq_editor(&mut state);
        apply(&mut state, SettingsAction::EqEditorSaveCurrent);
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.name_buf = Some(TextEdit::new("   "));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert!(state.config.equalizer.custom_presets.is_empty());
        assert_eq!(
            state.settings.eq_preset_editor.unwrap().error.as_deref(),
            Some("a preset name is required")
        );
    }

    #[test]
    fn rename_custom_preset() {
        let mut state = fixtures::fixture_empty();
        state
            .config
            .equalizer
            .custom_presets
            .push(eq_preset("old", 2.0));
        state.config.equalizer.active_preset = "old".to_string();
        open_eq_editor(&mut state);
        state.settings.eq_preset_editor.as_mut().unwrap().cursor = eq_cursor_at(&state, "old");

        apply(&mut state, SettingsAction::EqEditorRename);
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.name_buf = Some(TextEdit::new("new"));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert_eq!(state.config.equalizer.custom_presets[0].name, "new");
        assert_eq!(state.config.equalizer.active_preset, "new");
    }

    #[test]
    fn delete_custom_preset() {
        let mut state = fixtures::fixture_empty();
        state
            .config
            .equalizer
            .custom_presets
            .push(eq_preset("mine", 1.0));
        open_eq_editor(&mut state);
        state.settings.eq_preset_editor.as_mut().unwrap().cursor = eq_cursor_at(&state, "mine");

        apply(&mut state, SettingsAction::EqEditorDelete);
        assert!(state.config.equalizer.custom_presets.is_empty());
    }

    #[test]
    fn deleting_active_preset_falls_back_to_flat_and_applies() {
        let mut state = fixtures::fixture_empty();
        state
            .config
            .equalizer
            .custom_presets
            .push(eq_preset("mine", 5.0));
        state.config.equalizer.active_preset = "mine".to_string();
        state.player.eq.gains = [5.0; 10];
        state.player.eq.preset_name = "mine".to_string();
        state.player.known_presets = vec![eq_preset("flat", 0.0), eq_preset("mine", 5.0)];
        open_eq_editor(&mut state);
        state.settings.eq_preset_editor.as_mut().unwrap().cursor = eq_cursor_at(&state, "mine");

        let effects = apply(&mut state, SettingsAction::EqEditorDelete);
        assert_eq!(state.config.equalizer.active_preset, "flat");
        assert_eq!(state.player.eq.gains, [0.0; 10]);
        assert_eq!(state.player.eq.preset_name, "flat");
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Audio(AudioEffect::SetEq(Some(gains))) if *gains == [0.0; 10]
        )));
    }

    #[test]
    fn e_opens_equalizer_prefilled() {
        let mut state = fixtures::fixture_empty();
        state
            .config
            .equalizer
            .custom_presets
            .push(eq_preset("mine", 4.0));
        state.player.known_presets = vec![eq_preset("flat", 0.0), eq_preset("mine", 4.0)];
        state.player.eq.gains = [0.0; 10]; // currently playing something else
        open_eq_editor(&mut state);
        state.settings.eq_preset_editor.as_mut().unwrap().cursor = eq_cursor_at(&state, "mine");

        let effects = apply(&mut state, SettingsAction::EqEditorOpenEqualizer);
        match state.modal.as_ref().unwrap() {
            crate::state::modal::Modal::Equalizer {
                draft_gains,
                gains_at_open,
                ..
            } => {
                assert_eq!(*draft_gains, [4.0; 10]);
                assert_eq!(*gains_at_open, [0.0; 10]);
            }
            other => panic!("expected Modal::Equalizer, got {other:?}"),
        }
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Audio(AudioEffect::SetEq(Some(gains))) if *gains == [4.0; 10]
        )));
    }

    #[test]
    fn presets_persist_to_config() {
        let mut state = fixtures::fixture_empty();
        state.player.eq.gains = [1.0; 10];
        open_eq_editor(&mut state);
        apply(&mut state, SettingsAction::EqEditorSaveCurrent);
        if let Some(editor) = &mut state.settings.eq_preset_editor {
            editor.name_buf = Some(TextEdit::new("persisted"));
        }
        apply(&mut state, SettingsAction::CommitTextEdit);

        assert_eq!(state.config.equalizer.custom_presets[0].name, "persisted");
        // "Through the standard debounce" (this task's own spec) — armed, not written immediately.
        assert!(state.settings_write_debounce_until.is_some());
    }

    #[test]
    fn esc_from_name_edit_returns_to_eq_editor_esc_from_editor_closes_it() {
        let mut state = fixtures::fixture_empty();
        open_eq_editor(&mut state);
        apply(&mut state, SettingsAction::EqEditorSaveCurrent);
        assert!(
            state
                .settings
                .eq_preset_editor
                .as_ref()
                .unwrap()
                .name_buf
                .is_some()
        );

        apply(&mut state, SettingsAction::EqEditorClose);
        assert!(
            state
                .settings
                .eq_preset_editor
                .as_ref()
                .unwrap()
                .name_buf
                .is_none()
        );
        assert!(state.settings.eq_preset_editor.is_some());

        apply(&mut state, SettingsAction::EqEditorClose);
        assert!(state.settings.eq_preset_editor.is_none());
    }
}
