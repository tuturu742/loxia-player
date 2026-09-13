//! Config load/validate — parsing only, callers do I/O.

pub mod migrate;
pub mod schema;

pub use schema::{
    ArtProtocol, AudioConfig, CacheConfig, Config, Direction, EQ_BANDS_HZ, EqConfig, EqPreset,
    FACTORY_EQ_PRESET_NAMES, LoggingConfig, MAX_PREFETCH_NEXT, QualityProfile, ReplayGainMode,
    ServerConfig, ServerEndpoint, SortField, SortProfile, SortRule, SortingConfig, TargetCodec,
    TranscodeConfig, UiConfig,
};

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::theme::BUILTIN_THEME_NAMES;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigWarning {
    /// Dotted path, e.g. `"ui.theme"`.
    pub field: String,
    /// User-facing, one sentence.
    pub message: String,
    pub severity: Severity,
}

impl ConfigWarning {
    pub(crate) fn new(field: &str, message: impl Into<String>, severity: Severity) -> Self {
        ConfigWarning {
            field: field.to_string(),
            message: message.into(),
            severity,
        }
    }
}

/// A config file failed to parse as TOML, or its structure didn't deserialize into `Config`. The
/// underlying `toml` error is captured as a message only — `loxia-core` owns the `toml` dependency
/// (`docs/13-dependencies.md`), so this type lets `loxia` (the binary) handle a parse failure
/// without depending on `toml` itself.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ConfigParseError(String);

/// Parses `text` as TOML, migrates it, deserializes it into a `Config`, and validates it — the
/// full pipeline from raw file contents to a usable, warning-annotated `Config`. This is the one
/// place `loxia-core` touches the `toml` crate's own error types, so callers never need to.
pub fn parse_and_validate(text: &str) -> Result<(Config, Vec<ConfigWarning>), ConfigParseError> {
    let mut raw: toml::Value = toml::from_str(text).map_err(|e| ConfigParseError(e.to_string()))?;
    let mut warnings = migrate::migrate(&mut raw);
    let mut cfg: Config = raw
        .try_into()
        .map_err(|e: toml::de::Error| ConfigParseError(e.to_string()))?;
    warnings.extend(validate(&mut cfg));
    Ok((cfg, warnings))
}

/// Serializes a `Config` back to TOML text.
pub fn serialize(cfg: &Config) -> Result<String, ConfigParseError> {
    toml::to_string_pretty(cfg).map_err(|e| ConfigParseError(e.to_string()))
}

const RESERVED_HEADER_NAMES: &[&str] = &[
    "authorization",
    "x-emby-authorization",
    "host",
    "content-length",
];

const VALID_LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];

/// `pub(crate)` (`11-04`): the sort-profile editor's own "the fourth rule disables `a`" refusal
/// reuses this exact limit rather than a second, duplicated `4` literal.
pub(crate) const MAX_SORT_RULES: usize = 4;
const MIN_ROLLING_MAX_GB: f64 = 0.0;
const DEFAULT_ROLLING_MAX_GB: f64 = 5.0;
const EQ_GAIN_MIN_DB: f32 = -12.0;
const EQ_GAIN_MAX_DB: f32 = 12.0;

/// Mutates `cfg` into a usable state and reports what changed. Never fails — a malformed config
/// must never prevent the app from starting. Keybinding validation (parse failures, conflicts) is
/// deliberately **not** done here — see task `01-02`'s note; it lives in `keymap::KeyMap::from_config`
/// (`03-05`), which is the first point in the crate's build-out where a binding-string parser
/// exists, and its warnings join the same `Vec<ConfigWarning>` this function returns.
pub fn validate(cfg: &mut Config) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    dedup_server_ids(cfg, &mut warnings);
    for server in &mut cfg.servers {
        validate_server_url(server, &mut warnings);
        ensure_device_id(server, &mut warnings);
        strip_reserved_headers(server, &mut warnings);
    }
    resolve_active_server(cfg, &mut warnings);
    validate_theme(cfg, &mut warnings);
    truncate_sort_rules(cfg, &mut warnings);
    resolve_default_queue_profile(cfg, &mut warnings);
    validate_rolling_max_gb(cfg, &mut warnings);
    clamp_prefetch_next(cfg, &mut warnings);
    dedup_eq_preset_names(cfg, &mut warnings);
    clamp_eq_gains(cfg, &mut warnings);
    validate_log_level(cfg, &mut warnings);

    warnings
}

fn dedup_server_ids(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    let mut seen = HashSet::new();
    let mut had_duplicate = false;
    cfg.servers.retain(|s| {
        if seen.insert(s.id.clone()) {
            true
        } else {
            had_duplicate = true;
            false
        }
    });
    if had_duplicate {
        warnings.push(ConfigWarning::new(
            "servers",
            "duplicate server ids found; kept the first occurrence of each",
            Severity::Warning,
        ));
    }
}

fn validate_server_url(server: &ServerConfig, warnings: &mut Vec<ConfigWarning>) {
    let ok = server.url.starts_with("http://") || server.url.starts_with("https://");
    if !ok {
        warnings.push(ConfigWarning::new(
            "servers.url",
            format!("server \"{}\" has an empty or non-http(s) url", server.id),
            Severity::Warning,
        ));
    }
    // A fallback address is held to the same rule as the primary — an unusable one would otherwise
    // only be discovered at the moment it is needed, which is when the primary is already down.
    for (i, endpoint) in server.fallbacks.iter().enumerate() {
        if !endpoint.url.starts_with("http://") && !endpoint.url.starts_with("https://") {
            warnings.push(ConfigWarning::new(
                "servers.fallbacks.url",
                format!(
                    "server \"{}\" fallback {i} has an empty or non-http(s) url",
                    server.id
                ),
                Severity::Warning,
            ));
        }
    }
}

fn ensure_device_id(server: &mut ServerConfig, warnings: &mut Vec<ConfigWarning>) {
    if server.device_id.is_empty() {
        server.device_id = generate_uuid_v4();
        warnings.push(ConfigWarning::new(
            "servers.device_id",
            format!("generated a new device id for server \"{}\"", server.id),
            Severity::Info,
        ));
    }
}

fn strip_reserved_headers(server: &mut ServerConfig, warnings: &mut Vec<ConfigWarning>) {
    // Every endpoint's headers, not just the primary's: a fallback that overrode `X-Emby-Token`
    // would break authentication precisely when it is the only way in.
    let mut header_sets: Vec<&mut std::collections::BTreeMap<String, String>> =
        vec![&mut server.custom_headers];
    header_sets.extend(server.fallbacks.iter_mut().map(|e| &mut e.custom_headers));

    for headers in header_sets {
        let reserved: Vec<String> = headers
            .keys()
            .filter(|k| RESERVED_HEADER_NAMES.contains(&k.to_ascii_lowercase().as_str()))
            .cloned()
            .collect();
        for key in reserved {
            headers.remove(&key);
            warnings.push(ConfigWarning::new(
                "servers.custom_headers",
                format!("header \"{key}\" is set by loxia and cannot be overridden; removed"),
                Severity::Warning,
            ));
        }
    }
}

fn resolve_active_server(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    let names_an_entry = cfg.servers.iter().any(|s| s.id == cfg.active_server);
    if names_an_entry {
        return;
    }
    if cfg.servers.len() == 1 {
        cfg.active_server = cfg.servers[0].id.clone();
        warnings.push(ConfigWarning::new(
            "active_server",
            "active_server did not match any configured server; selected the only one available",
            Severity::Info,
        ));
    } else if !cfg.active_server.is_empty() || !cfg.servers.is_empty() {
        cfg.active_server = String::new();
        warnings.push(ConfigWarning::new(
            "active_server",
            "active_server did not match any configured server; cleared",
            Severity::Warning,
        ));
    }
}

fn validate_theme(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    if !BUILTIN_THEME_NAMES.contains(&cfg.ui.theme.as_str()) {
        warnings.push(ConfigWarning::new(
            "ui.theme",
            format!("unknown theme \"{}\"; using default_terminal", cfg.ui.theme),
            Severity::Warning,
        ));
        cfg.ui.theme = "default_terminal".to_string();
    }
}

fn truncate_sort_rules(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    for profile in &mut cfg.sorting.profiles {
        if profile.rules.len() > MAX_SORT_RULES {
            profile.rules.truncate(MAX_SORT_RULES);
            warnings.push(ConfigWarning::new(
                "sorting.profiles.rules",
                format!(
                    "sort profile \"{}\" had more than {MAX_SORT_RULES} rules; truncated",
                    profile.name
                ),
                Severity::Warning,
            ));
        }
    }
}

fn resolve_default_queue_profile(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    let names_a_profile = cfg
        .sorting
        .profiles
        .iter()
        .any(|p| p.name == cfg.sorting.default_queue_profile);
    if names_a_profile {
        return;
    }
    let fallback = cfg
        .sorting
        .profiles
        .first()
        .map(|p| p.name.clone())
        .unwrap_or_default();
    if cfg.sorting.default_queue_profile != fallback {
        cfg.sorting.default_queue_profile = fallback;
        warnings.push(ConfigWarning::new(
            "sorting.default_queue_profile",
            "default_queue_profile did not match any profile; reset",
            Severity::Warning,
        ));
    }
}

fn validate_rolling_max_gb(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    if cfg.cache.rolling_max_gb <= MIN_ROLLING_MAX_GB {
        warnings.push(ConfigWarning::new(
            "cache.rolling_max_gb",
            format!("rolling_max_gb must be positive; reset to {DEFAULT_ROLLING_MAX_GB}"),
            Severity::Warning,
        ));
        cfg.cache.rolling_max_gb = DEFAULT_ROLLING_MAX_GB;
    }
}

/// Each prefetched track is a whole file's worth of transfer, so an accidental `prefetch_next =
/// 500` would saturate a connection for no benefit the rolling cache can even keep.
fn clamp_prefetch_next(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    if cfg.cache.prefetch_next > MAX_PREFETCH_NEXT {
        warnings.push(ConfigWarning::new(
            "cache.prefetch_next",
            format!(
                "prefetch_next must be at most {MAX_PREFETCH_NEXT}; clamped from {}",
                cfg.cache.prefetch_next
            ),
            Severity::Warning,
        ));
        cfg.cache.prefetch_next = MAX_PREFETCH_NEXT;
    }
}

fn dedup_eq_preset_names(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    for preset in &mut cfg.equalizer.custom_presets {
        if FACTORY_EQ_PRESET_NAMES.contains(&preset.name.as_str()) {
            let renamed = format!("{} (custom)", preset.name);
            warnings.push(ConfigWarning::new(
                "equalizer.custom_presets",
                format!(
                    "custom preset \"{}\" collides with a factory preset; renamed to \"{renamed}\"",
                    preset.name
                ),
                Severity::Warning,
            ));
            preset.name = renamed;
        }
    }
}

fn clamp_eq_gains(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    for preset in &mut cfg.equalizer.custom_presets {
        let mut clamped = false;
        for gain in &mut preset.gains {
            let new_gain = gain.clamp(EQ_GAIN_MIN_DB, EQ_GAIN_MAX_DB);
            if new_gain != *gain {
                *gain = new_gain;
                clamped = true;
            }
        }
        if clamped {
            warnings.push(ConfigWarning::new(
                "equalizer.custom_presets.gains",
                format!(
                    "preset \"{}\" had gains outside ±{EQ_GAIN_MAX_DB} dB; clamped",
                    preset.name
                ),
                Severity::Warning,
            ));
        }
    }
}

fn validate_log_level(cfg: &mut Config, warnings: &mut Vec<ConfigWarning>) {
    if !VALID_LOG_LEVELS.contains(&cfg.logging.level.to_ascii_lowercase().as_str()) {
        warnings.push(ConfigWarning::new(
            "logging.level",
            format!("unknown log level \"{}\"; using info", cfg.logging.level),
            Severity::Warning,
        ));
        cfg.logging.level = "info".to_string();
    }
}

/// A hand-rolled UUID v4 generator (RFC 4122) — `loxia-core` may not depend on the `uuid` crate
/// (see `docs/13-dependencies.md`: only `loxia-emby` does), so this uses `rand` directly, which is
/// already an allowed dependency, and sets the version/variant bits manually. `pub(crate)`
/// (`11-03`): the server-profile editor stamps a new profile's `device_id` immediately at creation
/// time, rather than leaving it for `ensure_device_id`'s own lazy fill on the next `validate()` —
/// "generated once per profile," and this is the same generator either way.
pub(crate) fn generate_uuid_v4() -> String {
    let bits: u128 = rand::random();
    let mut bytes = bits.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0F) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3F) | 0x80; // variant 10xx

    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str) -> ServerConfig {
        ServerConfig {
            id: id.to_string(),
            url: "https://example.com".to_string(),
            ..ServerConfig::default()
        }
    }

    #[test]
    fn parse_and_validate_roundtrips_defaults() {
        let text = serialize(&Config::default()).unwrap();
        let (cfg, warnings) = parse_and_validate(&text).unwrap();
        assert_eq!(cfg, Config::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn parse_and_validate_reports_parse_errors() {
        let err = parse_and_validate("not valid = [[[ toml").unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn unknown_theme_falls_back_with_warning() {
        let mut cfg = Config {
            ui: UiConfig {
                theme: "not_a_theme".to_string(),
                ..UiConfig::default()
            },
            ..Config::default()
        };
        let warnings = validate(&mut cfg);
        assert_eq!(cfg.ui.theme, "default_terminal");
        assert!(warnings.iter().any(|w| w.field == "ui.theme"));
    }

    #[test]
    fn reserved_custom_header_is_stripped() {
        let mut s = server("s1");
        s.custom_headers
            .insert("Authorization".to_string(), "x".to_string());
        s.custom_headers
            .insert("X-Custom".to_string(), "y".to_string());
        let mut cfg = Config {
            servers: vec![s],
            ..Config::default()
        };
        let warnings = validate(&mut cfg);
        assert_eq!(cfg.servers[0].custom_headers.len(), 1);
        assert!(cfg.servers[0].custom_headers.contains_key("X-Custom"));
        assert!(warnings.iter().any(|w| w.field == "servers.custom_headers"));
    }

    #[test]
    fn duplicate_server_ids_deduped_keeping_first() {
        let mut first = server("dup");
        first.name = "First".to_string();
        let mut second = server("dup");
        second.name = "Second".to_string();
        let mut cfg = Config {
            servers: vec![first, second],
            ..Config::default()
        };
        let warnings = validate(&mut cfg);
        assert_eq!(cfg.servers.len(), 1);
        assert_eq!(cfg.servers[0].name, "First");
        assert!(warnings.iter().any(|w| w.field == "servers"));
    }

    #[test]
    fn sort_profile_truncated_to_four_rules() {
        use schema::{Direction, SortField, SortRule};
        let mut cfg = Config::default();
        cfg.sorting.profiles[0].rules = vec![
            SortRule {
                field: SortField::Name,
                direction: Direction::Asc,
            },
            SortRule {
                field: SortField::Artist,
                direction: Direction::Asc,
            },
            SortRule {
                field: SortField::Album,
                direction: Direction::Asc,
            },
            SortRule {
                field: SortField::Year,
                direction: Direction::Asc,
            },
            SortRule {
                field: SortField::Genre,
                direction: Direction::Asc,
            },
        ];
        let warnings = validate(&mut cfg);
        assert_eq!(cfg.sorting.profiles[0].rules.len(), 4);
        assert!(warnings.iter().any(|w| w.field == "sorting.profiles.rules"));
    }

    #[test]
    fn empty_device_id_is_generated_and_stable() {
        let mut cfg = Config {
            servers: vec![server("s1")],
            ..Config::default()
        };
        validate(&mut cfg);
        let first_id = cfg.servers[0].device_id.clone();
        assert!(!first_id.is_empty());
        assert_eq!(first_id.len(), 36); // 32 hex + 4 dashes
        validate(&mut cfg);
        assert_eq!(cfg.servers[0].device_id, first_id);
    }

    #[test]
    fn future_schema_version_warns_but_proceeds() {
        let mut raw: toml::Value = toml::from_str("schema_version = 999").unwrap();
        let warnings = migrate::migrate(&mut raw);
        assert!(warnings.iter().any(|w| w.field == "schema_version"));
        let cfg: Config = raw.try_into().unwrap();
        assert_eq!(cfg.schema_version, 999);
    }

    #[test]
    fn rolling_max_gb_reset_when_non_positive() {
        let mut cfg = Config {
            cache: CacheConfig {
                rolling_max_gb: -1.0,
                ..CacheConfig::default()
            },
            ..Config::default()
        };
        validate(&mut cfg);
        assert_eq!(cfg.cache.rolling_max_gb, DEFAULT_ROLLING_MAX_GB);
    }

    #[test]
    fn eq_gains_are_clamped() {
        let mut cfg = Config::default();
        cfg.equalizer.custom_presets.push(EqPreset {
            name: "loud".to_string(),
            gains: [20.0, -20.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        });
        let warnings = validate(&mut cfg);
        assert_eq!(cfg.equalizer.custom_presets[0].gains[0], 12.0);
        assert_eq!(cfg.equalizer.custom_presets[0].gains[1], -12.0);
        assert!(
            warnings
                .iter()
                .any(|w| w.field == "equalizer.custom_presets.gains")
        );
    }

    #[test]
    fn custom_preset_colliding_with_factory_is_renamed() {
        let mut cfg = Config::default();
        cfg.equalizer.custom_presets.push(EqPreset {
            name: "flat".to_string(),
            gains: [0.0; 10],
        });
        let warnings = validate(&mut cfg);
        assert_eq!(cfg.equalizer.custom_presets[0].name, "flat (custom)");
        assert!(
            warnings
                .iter()
                .any(|w| w.field == "equalizer.custom_presets")
        );
    }

    #[test]
    fn single_server_selected_as_active_when_unset() {
        let mut cfg = Config {
            servers: vec![server("only")],
            ..Config::default()
        };
        validate(&mut cfg);
        assert_eq!(cfg.active_server, "only");
    }

    proptest::proptest! {
        #[test]
        fn validate_never_errors(
            theme in ".{0,20}",
            rolling_max_gb in -1000.0f64..1000.0,
            n_rules in 0usize..10,
        ) {
            use schema::{Direction, SortField, SortRule};
            let mut cfg = Config::default();
            cfg.ui.theme = theme;
            cfg.cache.rolling_max_gb = rolling_max_gb;
            cfg.sorting.profiles[0].rules =
                vec![SortRule { field: SortField::Name, direction: Direction::Asc }; n_rules];

            let _warnings = validate(&mut cfg);

            proptest::prop_assert!(BUILTIN_THEME_NAMES.contains(&cfg.ui.theme.as_str()));
            proptest::prop_assert!(cfg.cache.rolling_max_gb > 0.0);
            proptest::prop_assert!(cfg.sorting.profiles[0].rules.len() <= MAX_SORT_RULES);
        }
    }
}
