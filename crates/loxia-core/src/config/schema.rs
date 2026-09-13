//! Serde structs mirroring `config.toml` (`docs/02-data-model.md` §8).
//!
//! Every struct is `#[serde(default)]` at the container level with a hand-written `Default` impl
//! giving the documented value for each field — this is what makes a missing or empty config file
//! (or a config file with only a handful of keys set) deserialize into a fully working `Config`:
//! serde fills in any field absent from a present section from `Default::default()` of the whole
//! container, not from a per-field zero value.
//!
//! `audio.crossfade_sec` and `ui.show_spectrum_analyzer` are deliberately absent — both features
//! were cut (`docs/12-decisions.md` §3).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub active_server: String,
    pub servers: Vec<ServerConfig>,
    pub audio: AudioConfig,
    pub cache: CacheConfig,
    pub transcode: TranscodeConfig,
    pub ui: UiConfig,
    pub logging: LoggingConfig,
    pub sorting: SortingConfig,
    pub equalizer: EqConfig,
    pub keybindings: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            schema_version: 1,
            active_server: String::new(),
            servers: Vec::new(),
            audio: AudioConfig::default(),
            cache: CacheConfig::default(),
            transcode: TranscodeConfig::default(),
            ui: UiConfig::default(),
            logging: LoggingConfig::default(),
            sorting: SortingConfig::default(),
            equalizer: EqConfig::default(),
            keybindings: BTreeMap::new(),
        }
    }
}

impl Config {
    /// The namespace everything stored per server belongs to: the rolling cache, permanent
    /// downloads, the session snapshot, the scrobble buffer.
    ///
    /// The Emby installation's own id ([`ServerConfig::server_id`]) once known, and the profile id
    /// until then. Two profiles pointing at the same server therefore share all of it rather than
    /// keeping two copies — which is the whole point of telling *addresses* apart from *servers*
    /// (`docs/12-decisions.md`).
    ///
    /// Readable from config alone, with no network round trip, because the id is written back to
    /// the profile on the first successful connection. That matters: the session is restored before
    /// the app connects, and the cache has to be findable while offline.
    pub fn storage_server_id(&self) -> String {
        self.servers
            .iter()
            .find(|s| s.id == self.active_server)
            .map(|s| s.server_id.clone())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| self.active_server.clone())
    }
}

// Manual Debug: `access_token` inside `servers` must never be printed. `ServerConfig` already
// redacts itself (below), so deriving through it here would be safe, but the derive is skipped
// anyway to make the intent explicit at the point anyone reads a `Config` in a log line.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("schema_version", &self.schema_version)
            .field("active_server", &self.active_server)
            .field("servers", &self.servers)
            .field("audio", &self.audio)
            .field("cache", &self.cache)
            .field("transcode", &self.transcode)
            .field("ui", &self.ui)
            .field("logging", &self.logging)
            .field("sorting", &self.sorting)
            .field("equalizer", &self.equalizer)
            .field("keybindings", &self.keybindings)
            .finish()
    }
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub user_id: String,
    pub access_token: String,
    /// Empty means "not yet generated" — a UUID v4 is generated on first use and persisted
    /// (`docs/12-decisions.md` §5); this must be stable across restarts for Emby session identity.
    pub device_id: String,
    pub custom_headers: BTreeMap<String, String>,
    /// The Emby installation's own GUID (`System/Info/Public` → `Id`), learned on the first
    /// successful connection and written back here.
    ///
    /// **Not** the same thing as `id`, which names this *profile*. Everything stored per server —
    /// the rolling cache, permanent downloads, the session snapshot, the scrobble buffer — is
    /// namespaced by this, so two profiles pointing at one server (a LAN address and an external
    /// one behind a proxy, say) share it all instead of keeping two copies. Empty until the first
    /// connection succeeds, which is the only reason the profile id is still used as a fallback
    /// (`docs/12-decisions.md`).
    #[serde(default)]
    pub server_id: String,
    /// Further addresses for the **same** server, tried in order after the primary
    /// (`ServerConfig::url`) when it cannot be reached.
    ///
    /// Each is verified against `server_id` before it is used, so a stale DNS entry or a copied
    /// config that happens to answer can never quietly attach this profile's cache and downloads to
    /// somebody else's library (`docs/12-decisions.md`).
    #[serde(default)]
    pub fallbacks: Vec<ServerEndpoint>,
}

impl ServerConfig {
    /// Every address to try, primary first.
    pub fn endpoints(&self) -> Vec<ServerEndpoint> {
        let mut all = vec![ServerEndpoint {
            url: self.url.clone(),
            custom_headers: self.custom_headers.clone(),
        }];
        all.extend(self.fallbacks.iter().cloned());
        all
    }

    /// This profile as if `endpoint` were its address — same account, same token, same device id,
    /// different path in. `EmbyClient::new` reads only those fields, so substituting them here is
    /// all it takes to build a client for a fallback.
    pub fn at(&self, endpoint: &ServerEndpoint) -> ServerConfig {
        ServerConfig {
            url: endpoint.url.clone(),
            custom_headers: endpoint.custom_headers.clone(),
            ..self.clone()
        }
    }
}

/// One address a server can be reached on. A profile has a primary (its own `url`/`custom_headers`)
/// plus any number of these.
///
/// The point is that a LAN `http://` address and an external `https://` one behind a proxy are two
/// *endpoints of one server*, not two servers: the same account, the same library, and — since
/// storage is keyed on the server's own id ([`Config::storage_server_id`]) — the same cache,
/// downloads and session. Only the address and whatever headers that path needs differ, which is
/// why those are the only two fields here (`docs/12-decisions.md`).
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerEndpoint {
    pub url: String,
    /// Headers this path needs and the others do not — a Cloudflare Access service token on the
    /// external address, say, which would be meaningless on the LAN one.
    pub custom_headers: BTreeMap<String, String>,
}

/// Redacts header *values* for the same reason [`ServerConfig`] does: a `CF-Access-Client-Secret`
/// is exactly as sensitive as an access token, and these end up in log lines.
impl fmt::Debug for ServerEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted: BTreeMap<&String, &str> = self
            .custom_headers
            .keys()
            .map(|k| (k, "<redacted>"))
            .collect();
        f.debug_struct("ServerEndpoint")
            .field("url", &self.url)
            .field("custom_headers", &redacted)
            .finish()
    }
}

/// `access_token` renders as `<redacted>` — the derived `Debug` would leak the token into every
/// log line and panic message that formats a config or a server profile. `custom_headers`' own
/// *values* are redacted the same way (`11-07`, found while building the About view's "copy
/// diagnostics" action): a header like `CF-Access-Client-Secret` is exactly as sensitive as the
/// token itself, and the original derive-adjacent version of this impl printed those values
/// verbatim — a real leak this hand-written `Debug` exists specifically to prevent. Header *names*
/// stay visible; knowing which headers are configured is useful for debugging and carries no
/// secret.
impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted_headers: BTreeMap<&String, &str> = self
            .custom_headers
            .keys()
            .map(|k| (k, "<redacted>"))
            .collect();
        f.debug_struct("ServerConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("url", &self.url)
            .field("user_id", &self.user_id)
            .field("access_token", &"<redacted>")
            .field("device_id", &self.device_id)
            .field("custom_headers", &redacted_headers)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayGainMode {
    #[default]
    Album,
    Track,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub output_driver: String,
    pub device_id: String,
    pub default_replaygain: ReplayGainMode,
    pub replaygain_preamp_db: f32,
    pub buffer_size_ms: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        AudioConfig {
            output_driver: "auto".to_string(),
            device_id: "auto".to_string(),
            default_replaygain: ReplayGainMode::default(),
            replaygain_preamp_db: 0.0,
            buffer_size_ms: 2000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    pub enabled: bool,
    pub rolling_max_gb: f64,
    pub image_cache_mb: u64,
    /// Cache the track that is **currently playing**, alongside streaming it.
    ///
    /// Off by default: it re-fetches the exact bytes mpv is already pulling, so a first play costs
    /// double the bandwidth and the copy only pays off on a later replay or offline. Turn it on to
    /// have everything you listen to land in the cache as you go. (It was force-disabled in code
    /// for a while after being blamed for killing playback; that did not reproduce when measured —
    /// `docs/12-decisions.md`.)
    #[serde(default)]
    pub prefetch_on_play: bool,
    /// How many *upcoming* queue entries to pull into the rolling cache alongside the one being
    /// played, so a skip forward — or a connection that drops mid-album — finds them already
    /// local. `0` disables it. Ignored entirely when `enabled` is false: there is nowhere to put
    /// them. Capped by [`MAX_PREFETCH_NEXT`], since each one is a whole track's worth of transfer.
    #[serde(default = "default_prefetch_next")]
    pub prefetch_next: u8,
    pub download_dir: String,
    pub cache_dir: String,
}

fn default_true() -> bool {
    true
}

/// One track ahead: enough that an ordinary listen-through never waits on the network, without
/// turning a browse through an album into a bulk download.
///
/// Defaulting this on was gated on the 2026-08-02 failure, where a background fetch alongside
/// playback appeared to kill the stream. Measured against the live server: holding a stream open
/// while fetching another track concurrently — on the same device id, in both direct and transcode
/// profiles, and for the same item as well as a different one — never interrupted playback
/// (`docs/12-decisions.md`).
fn default_prefetch_next() -> u8 {
    1
}

/// An upper bound on `cache.prefetch_next` — beyond this a "prefetch" is really a bulk download,
/// which is what the Downloads feature is for.
pub const MAX_PREFETCH_NEXT: u8 = 10;

impl Default for CacheConfig {
    fn default() -> Self {
        CacheConfig {
            enabled: true,
            rolling_max_gb: 5.0,
            image_cache_mb: 200,
            prefetch_on_play: false,
            prefetch_next: default_prefetch_next(),
            download_dir: "auto".to_string(),
            cache_dir: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityProfile {
    #[default]
    Direct,
    TranscodeHigh,
    TranscodeMed,
    TranscodeLow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetCodec {
    #[default]
    Mp3,
    Aac,
    Opus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TranscodeConfig {
    pub mode: QualityProfile,
    pub target_codec: TargetCodec,
    pub download_uncompressed: bool,
}

impl Default for TranscodeConfig {
    fn default() -> Self {
        TranscodeConfig {
            mode: QualityProfile::default(),
            target_codec: TargetCodec::default(),
            download_uncompressed: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtProtocol {
    #[default]
    Auto,
    Kitty,
    Sixel,
    Halfblocks,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub album_art_protocol: ArtProtocol,
    pub desktop_notifications: bool,
    pub enable_mouse: bool,
    pub enable_websocket: bool,
    pub restore_session: bool,
    pub restore_autoplay: bool,
    pub ascii_only: bool,
    pub show_lyrics: bool,
    /// Whether the inspector draws artwork above the metadata (artist images in the Artists and
    /// Album Artists lists, covers in Albums and on a track). Off skips the fetch entirely, not
    /// just the drawing, and gives the reserved rows back to the metadata — the point is to spend
    /// no network or memory on it at all.
    #[serde(default = "default_true")]
    pub show_inspector_art: bool,
    /// The player bar's own "what's playing" line, as a template. Supported placeholders:
    /// `{title}`, `{artist}`, `{album}`, `{album_artist}`, `{year}`, `{track_number}`,
    /// `{disc_number}`, `{genre}`, `{duration}`. An unknown placeholder is left verbatim so a typo
    /// is visible rather than silently blanking the line (`docs/12-decisions.md`).
    pub now_playing_format: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            theme: "default_terminal".to_string(),
            album_art_protocol: ArtProtocol::default(),
            desktop_notifications: true,
            enable_mouse: true,
            enable_websocket: true,
            restore_session: true,
            restore_autoplay: false,
            ascii_only: false,
            now_playing_format: "{title} — {artist} ({album})".to_string(),
            show_lyrics: true,
            show_inspector_art: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub max_files: u8,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            level: "info".to_string(),
            max_files: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    Name,
    Artist,
    AlbumArtist,
    Album,
    Year,
    TrackNumber,
    Genre,
    DateAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortRule {
    pub field: SortField,
    pub direction: Direction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortProfile {
    pub name: String,
    pub rules: Vec<SortRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SortingConfig {
    pub default_queue_profile: String,
    pub profiles: Vec<SortProfile>,
}

impl Default for SortingConfig {
    /// The two profiles from `design_overview` §7.
    fn default() -> Self {
        use Direction::{Asc, Desc};
        use SortField::{Album, AlbumArtist, TrackNumber, Year};

        SortingConfig {
            default_queue_profile: "chronological_discog".to_string(),
            profiles: vec![
                SortProfile {
                    name: "chronological_discog".to_string(),
                    rules: vec![
                        SortRule {
                            field: AlbumArtist,
                            direction: Asc,
                        },
                        SortRule {
                            field: Year,
                            direction: Asc,
                        },
                        SortRule {
                            field: Album,
                            direction: Asc,
                        },
                        SortRule {
                            field: TrackNumber,
                            direction: Asc,
                        },
                    ],
                },
                SortProfile {
                    name: "release_chronology".to_string(),
                    rules: vec![
                        SortRule {
                            field: Year,
                            direction: Desc,
                        },
                        SortRule {
                            field: Album,
                            direction: Asc,
                        },
                        SortRule {
                            field: TrackNumber,
                            direction: Asc,
                        },
                    ],
                },
            ],
        }
    }
}

/// The factory equalizer preset names (`docs/05-audio-engine.md` §5, `assets/eq_presets.toml`).
/// Defined here — not in `loxia-audio` — because `config::validate` (task `01-02`) needs to check
/// a custom preset name for collisions before `loxia-audio::eq` exists; task `09-03` reuses this
/// constant when it embeds `assets/eq_presets.toml` rather than redefining the list.
pub const FACTORY_EQ_PRESET_NAMES: &[&str] = &[
    "flat",
    "darkwave_ebm",
    "bass_boost",
    "vocal",
    "acoustic",
    "night_listening_warm",
    "loudness",
    "classical",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqPreset {
    pub name: String,
    pub gains: [f32; 10],
}

/// The ten ISO band centre frequencies every `EqPreset`/`EqState.gains` entry corresponds to,
/// index-for-index (`docs/05-audio-engine.md` §5). Moved here from `loxia_audio::backend`
/// (`05-04`, re-exported back for that module's own existing callers) because it's pure data with
/// no OS-specific behaviour — `loxia-tui`'s equalizer modal (`10-06`) needs the same frequency
/// labels `loxia-audio`'s own mpv filter-string builder does, but cannot depend on `loxia-audio` to
/// reach them. Same shape as `group_by_driver`/`device_label`'s own move, `10-05`.
pub const EQ_BANDS_HZ: [u32; 10] = [31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqConfig {
    pub enabled: bool,
    pub active_preset: String,
    pub custom_presets: Vec<EqPreset>,
}

impl Default for EqConfig {
    fn default() -> Self {
        EqConfig {
            enabled: false,
            active_preset: "flat".to_string(),
            custom_presets: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_toml_deserializes_to_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn partial_section_fills_remaining_fields_from_default() {
        let cfg: Config = toml::from_str("[audio]\nbuffer_size_ms = 500\n").unwrap();
        assert_eq!(cfg.audio.buffer_size_ms, 500);
        assert_eq!(cfg.audio.output_driver, "auto");
    }

    #[test]
    fn design_overview_sample_parses() {
        let sample = r#"
active_server = "remote_proxy"

[[servers]]
id = "remote_proxy"
name = "Remote Emby Server"
url = "https://emby.yourdomain.com"
user_id = "e8837bc1-ad67-520e-8cd2-f629e3155721"
access_token = "9a8b7c6d5e4f3a2b1c"

[servers.custom_headers]
"CF-Access-Client-Id" = "97e2aac120121f902df8.access"
"CF-Access-Client-Secret" = "e5f6g7h8i9j0123456789abcdef0123456789abcdef0123456789abcdef01234"
"X-Custom-Auth" = "secret_value"

[[servers]]
id = "local_lan"
name = "Local LAN Server"
url = "http://192.168.1.100:8096"
user_id = "a1122334-bb55-6677-8899-00aabbccdd"
access_token = "1a2b3c4d5e6f"

[servers.custom_headers]

[audio]
output_driver = "auto"
device_id = "auto"
default_replaygain = "album"
buffer_size_ms = 2000

[cache]
enabled = true
rolling_max_gb = 5.0
download_dir = "auto"
cache_dir = "auto"

[transcode]
mode = "direct"
target_codec = "mp3"
download_uncompressed = true

[ui]
theme = "cyberpunk_neon"
album_art_protocol = "kitty"
desktop_notifications = true
enable_mouse = true
restore_session = true

[sorting]
default_queue_profile = "chronological_discog"

[[sorting.profiles]]
name = "chronological_discog"
rules = [
  { field = "album_artist", direction = "asc" },
  { field = "year", direction = "asc" },
  { field = "album", direction = "asc" },
  { field = "track_number", direction = "asc" }
]

[[sorting.profiles]]
name = "release_chronology"
rules = [
  { field = "year", direction = "desc" },
  { field = "album", direction = "asc" },
  { field = "track_number", direction = "asc" }
]

[equalizer]
enabled = true
active_preset = "darkwave_ebm"

[[equalizer.custom_presets]]
name = "night_listening_warm"
gains = [2.0, 2.0, 1.0, 0.0, 0.0, -1.0, -1.5, -2.0, -3.0, -4.0]

[keybindings]
play_pause = "Space"
next_track = "Char('n')"
prev_track = "Char('p')"
"#;
        let cfg: Config = toml::from_str(sample).expect("sample must parse");
        assert_eq!(cfg.active_server, "remote_proxy");
        assert_eq!(cfg.servers.len(), 2);
        assert_eq!(cfg.servers[0].custom_headers.len(), 3);
        assert_eq!(cfg.audio.buffer_size_ms, 2000);
        assert_eq!(cfg.ui.theme, "cyberpunk_neon");
        assert_eq!(cfg.sorting.profiles.len(), 2);
        assert_eq!(cfg.equalizer.custom_presets[0].name, "night_listening_warm");
        assert_eq!(cfg.keybindings.get("play_pause").unwrap(), "Space");
    }

    #[test]
    fn server_debug_redacts_token() {
        let server = ServerConfig {
            access_token: "super-secret-token".to_string(),
            ..ServerConfig::default()
        };
        let debug = format!("{server:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("super-secret-token"));
    }

    #[test]
    fn config_debug_redacts_nested_token() {
        let mut cfg = Config::default();
        cfg.servers.push(ServerConfig {
            access_token: "super-secret-token".to_string(),
            ..ServerConfig::default()
        });
        let debug = format!("{cfg:?}");
        assert!(!debug.contains("super-secret-token"));
    }

    /// `11-07`: a real gap found while building the About view's "copy diagnostics" action — this
    /// hand-written `Debug` redacted `access_token` but printed `custom_headers`' own values
    /// verbatim, and a header like `CF-Access-Client-Secret` is exactly as sensitive as the token.
    /// Header *names* must stay visible (useful for debugging, no secret in a name).
    #[test]
    fn server_debug_redacts_custom_header_values_not_names() {
        let mut headers = BTreeMap::new();
        headers.insert(
            "CF-Access-Client-Secret".to_string(),
            "super-secret-header-value".to_string(),
        );
        let server = ServerConfig {
            custom_headers: headers,
            ..ServerConfig::default()
        };
        let debug = format!("{server:?}");
        assert!(!debug.contains("super-secret-header-value"));
        assert!(debug.contains("CF-Access-Client-Secret"));
        assert!(debug.contains("<redacted>"));
    }

    fn arbitrary_config() -> impl proptest::strategy::Strategy<Value = Config> {
        use proptest::prelude::*;
        (
            any::<bool>(),
            ".{0,20}",
            ".{0,20}",
            any::<f64>().prop_map(|f| f.abs() % 1000.0),
        )
            .prop_map(|(_unused, active_server, theme, rolling_max_gb)| Config {
                active_server,
                audio: AudioConfig::default(),
                ui: UiConfig {
                    theme,
                    ..UiConfig::default()
                },
                cache: CacheConfig {
                    rolling_max_gb,
                    ..CacheConfig::default()
                },
                ..Config::default()
            })
    }

    proptest::proptest! {
        #[test]
        fn config_roundtrips(cfg in arbitrary_config()) {
            let toml_str = toml::to_string(&cfg).unwrap();
            let back: Config = toml::from_str(&toml_str).unwrap();
            proptest::prop_assert_eq!(cfg, back);
        }
    }
}
