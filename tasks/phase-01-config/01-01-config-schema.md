# 01-01 · Config schema

**Phase:** 01 — Config · **Agent:** A · **Size:** M
**Prerequisites:** `00-02`
**Reference:** `docs/02-data-model.md` §8, `docs/12-decisions.md` §5

## Goal
Define the complete `config.toml` serde schema in `loxia-core`. Afterwards the whole configuration
surface exists as types, and later tasks only add behaviour that reads it.

## Files
- `crates/loxia-core/src/config/schema.rs`
- `crates/loxia-core/src/config/mod.rs`

## Specification

All structs derive `Debug, Clone, PartialEq, Serialize, Deserialize` and mark **every** field
`#[serde(default)]` with a `Default` impl giving the documented value. A missing or empty file must
deserialize into a fully working `Config`.

```
Config {
    schema_version: u32,                 // default 1
    active_server: String,               // default ""
    servers: Vec<ServerConfig>,          // default vec![]
    audio: AudioConfig,
    cache: CacheConfig,
    transcode: TranscodeConfig,
    ui: UiConfig,
    logging: LoggingConfig,
    sorting: SortingConfig,
    equalizer: EqConfig,
    keybindings: BTreeMap<String, String>,   // ActionId -> binding string; empty = all defaults
}
```

| Struct | Fields and defaults |
| :-- | :-- |
| `ServerConfig` | `id: String`, `name: String`, `url: String`, `user_id: String`, `access_token: String`, `device_id: String` (empty → generated on first use and persisted), `custom_headers: BTreeMap<String,String>` |
| `AudioConfig` | `output_driver: String = "auto"`, `device_id: String = "auto"`, `bit_perfect: bool = false`, `default_replaygain: ReplayGainMode = Album`, `replaygain_preamp_db: f32 = 0.0`, `buffer_size_ms: u32 = 2000` |
| `CacheConfig` | `enabled: bool = true`, `rolling_max_gb: f64 = 5.0`, `image_cache_mb: u64 = 200`, `prefetch_on_play: bool = true`, `download_dir: String = "auto"`, `cache_dir: String = "auto"` |
| `TranscodeConfig` | `mode: QualityProfile = Direct`, `target_codec: TargetCodec = Mp3`, `download_uncompressed: bool = true` |
| `UiConfig` | `theme: String = "default_terminal"`, `album_art_protocol: ArtProtocol = Auto`, `desktop_notifications: bool = true`, `enable_mouse: bool = true`, `enable_websocket: bool = true`, `restore_session: bool = true`, `restore_autoplay: bool = false`, `ascii_only: bool = false`, `show_lyrics: bool = true` |
| `LoggingConfig` | `level: String = "info"`, `max_files: u8 = 5` |
| `SortingConfig` | `default_queue_profile: String = "chronological_discog"`, `profiles: Vec<SortProfile>` — defaults to the two profiles in `design_overview` §7 |
| `EqConfig` | `enabled: bool = false`, `active_preset: String = "flat"`, `custom_presets: Vec<EqPreset>` |
| `EqPreset` | `name: String`, `gains: [f32; 10]` |
| `SortProfile` | `name: String`, `rules: Vec<SortRule>` |
| `SortRule` | `field: SortField`, `direction: Direction` |

Enums, all `#[serde(rename_all = "snake_case")]`:
- `ReplayGainMode` = `Album | Track | Off`
- `QualityProfile` = `Direct | TranscodeHigh | TranscodeMed | TranscodeLow`
- `TargetCodec` = `Mp3 | Aac | Opus`
- `ArtProtocol` = `Auto | Kitty | Sixel | Halfblocks | Off`
- `SortField` = `Name | Artist | AlbumArtist | Album | Year | TrackNumber | Genre | DateAdded`
- `Direction` = `Asc | Desc`

**Do not add `audio.crossfade_sec` or `ui.show_spectrum_analyzer`** — those features are cut
(`docs/12-decisions.md` §3).

`ServerConfig` must implement `Debug` **manually** so `access_token` renders as `"<redacted>"`.
The derived impl would leak the token into every log line that formats a config.

## Acceptance
Tests in `config/schema.rs`:
- `empty_toml_deserializes_to_defaults` — `toml::from_str::<Config>("")` succeeds and matches
  `Config::default()`.
- `design_overview_sample_parses` — the `[servers]`/`[audio]`/`[ui]`/`[sorting]`/`[equalizer]`
  sample from `design_overview` §7, minus the removed keys, parses with no error.
- `server_debug_redacts_token` — `format!("{:?}", server)` contains `<redacted>` and does **not**
  contain the token value.
- `config_roundtrips` (proptest) — arbitrary valid `Config` → TOML → `Config` is the identity.

## Done when
The global DoD in `tasks/README.md` is satisfied.
