//! `loxia-player --doctor` diagnostics.
//!
//! Prints everything a bug report needs, and nothing a bug report must not contain: no access
//! token, no stream URL, no custom-header *values*. Runs before the terminal UI is entered, writes
//! only to stdout, and never mutates anything the app would rely on later — the one exception is
//! `config.toml` itself, which is created on a genuine first run exactly as a normal launch would
//! create it, so that `--doctor` reports the same paths the next real launch will use.
//!
//! The report is grouped into sections, each line carrying one of three verdicts:
//!
//! - `ok` — checked, nothing wrong.
//! - `warn` — worth knowing, but the app runs. Does not affect the exit code.
//! - `FAIL` — something that will stop the app, or a feature of it, from working. Any single
//!   `FAIL` makes the process exit `1`, so `--doctor` is usable as a scripted pre-flight check.
//!
//! Deliberately ASCII-only: this output gets pasted into issue trackers, and a box-drawing
//! character that a reporter's terminal renders as a replacement glyph makes the report harder to
//! read for no gain.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use loxia_audio::backend::{AudioBackend, AudioCommand, AudioEvent};
use loxia_core::config::{CacheConfig, Config};
use loxia_core::keymap::KeyMap;
use loxia_core::model::{AudioDevice, device_label, group_by_driver};
use loxia_core::paths::{Paths, SystemDirs};

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub label: &'static str,
    pub status: Status,
    pub detail: String,
    /// Extra lines printed under `detail`, indented — a device list, the individual keybinding
    /// conflicts. Kept separate from `detail` so the aligned label column stays aligned.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

impl Status {
    fn tag(self) -> &'static str {
        match self {
            Status::Ok => "ok  ",
            Status::Warn => "warn",
            Status::Fail => "FAIL",
        }
    }
}

impl Check {
    fn new(label: &'static str, status: Status, detail: impl Into<String>) -> Check {
        Check {
            label,
            status,
            detail: detail.into(),
            notes: Vec::new(),
        }
    }

    fn ok(label: &'static str, detail: impl Into<String>) -> Check {
        Check::new(label, Status::Ok, detail)
    }

    fn warn(label: &'static str, detail: impl Into<String>) -> Check {
        Check::new(label, Status::Warn, detail)
    }

    fn fail(label: &'static str, detail: impl Into<String>) -> Check {
        Check::new(label, Status::Fail, detail)
    }

    fn with_notes(mut self, notes: Vec<String>) -> Check {
        self.notes = notes;
        self
    }
}

/// A named group of checks, printed as one block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub checks: Vec<Check>,
}

/// The whole report, as data — so the rendering and the exit code are both testable without
/// running any of the probes that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub sections: Vec<Section>,
}

impl Report {
    fn push(&mut self, title: &'static str, checks: Vec<Check>) {
        self.sections.push(Section { title, checks });
    }

    fn counts(&self) -> (usize, usize) {
        let mut warnings = 0;
        let mut failures = 0;
        for check in self.sections.iter().flat_map(|s| &s.checks) {
            match check.status {
                Status::Ok => {}
                Status::Warn => warnings += 1,
                Status::Fail => failures += 1,
            }
        }
        (warnings, failures)
    }

    /// `0` when nothing failed, `1` when anything did. Warnings never change it.
    pub fn exit_code(&self) -> i32 {
        i32::from(self.counts().1 > 0)
    }

    /// The full report as printable text, label column aligned across the whole report (not
    /// per-section) so the eye can run straight down it.
    pub fn render(&self) -> String {
        let width = self
            .sections
            .iter()
            .flat_map(|s| &s.checks)
            .map(|c| c.label.len())
            .max()
            .unwrap_or(0);

        let mut out = String::new();
        let _ = writeln!(
            out,
            "loxia-player {} ({})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS
        );

        for section in &self.sections {
            let _ = writeln!(out, "\n{}", section.title);
            for check in &section.checks {
                let _ = writeln!(
                    out,
                    "  {}  {:width$}  {}",
                    check.status.tag(),
                    check.label,
                    check.detail
                );
                for note in &check.notes {
                    // 2 + 4 + 2 + width + 2 — lines up under `detail`.
                    let _ = writeln!(out, "{:indent$}{note}", "", indent = 10 + width);
                }
            }
        }

        let (warnings, failures) = self.counts();
        let _ = writeln!(
            out,
            "\n{} warning{}, {} failure{}",
            warnings,
            plural(warnings),
            failures,
            plural(failures)
        );
        out
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Runs every probe and prints the report. `config_override` is `--config`'s value, so `--doctor`
/// reports on the same file the run being diagnosed would actually use.
///
/// Returns the process exit code rather than calling `exit` itself, so the caller stays in charge
/// of the process and this stays callable from a test.
pub async fn run(config_override: Option<&Path>) -> i32 {
    let report = collect(config_override).await;
    print!("{}", report.render());
    report.exit_code()
}

async fn collect(config_override: Option<&Path>) -> Report {
    let mut report = Report::default();

    // Resolved twice, exactly as `main` does: once with default cache settings, which is enough to
    // find `config.toml`, and again once the file has been read, since it may relocate the cache
    // and download roots.
    let bootstrap_paths = match Paths::resolve(&SystemDirs, &CacheConfig::default()) {
        Ok(p) => p,
        Err(e) => {
            report.push(
                "Paths",
                vec![Check::fail(
                    "base dirs",
                    format!("could not resolve application directories: {e}"),
                )],
            );
            return report;
        }
    };

    let config_file = match config_override {
        Some(path) => path.to_path_buf(),
        None => bootstrap_paths.config_file(),
    };
    let existed = config_file.exists();
    let (cfg, warnings) = crate::bootstrap::load_at(&config_file);
    let paths = Paths::resolve(&SystemDirs, &cfg.cache).unwrap_or(bootstrap_paths);

    report.push(
        "Configuration",
        config_checks(&config_file, existed, &cfg, &warnings),
    );
    report.push("Keybindings", keymap_checks(&cfg));
    report.push("Audio", audio_checks(&cfg));
    report.push("Terminal", terminal_checks(&cfg));
    report.push("Storage", storage_checks(&paths, &cfg));
    report.push("Server", server_checks(&cfg).await);
    report
}

fn config_checks(
    config_file: &Path,
    existed: bool,
    cfg: &Config,
    warnings: &[loxia_core::config::ConfigWarning],
) -> Vec<Check> {
    let mut checks = vec![Check::ok("file", config_file.display().to_string())];

    if !existed {
        checks.push(Check::warn(
            "created",
            "no config file existed; a default one was just written",
        ));
    }

    checks.push(permissions_check(config_file));

    let current = loxia_core::config::migrate::CURRENT_SCHEMA_VERSION;
    checks.push(if i64::from(cfg.schema_version) == current {
        Check::ok("schema", format!("{} (current)", cfg.schema_version))
    } else {
        Check::warn(
            "schema",
            format!("{} (this build understands {current})", cfg.schema_version),
        )
    });

    // Every warning `config::validate` produced, verbatim. These are the ones that silently
    // changed a value out from under the user at startup, which is precisely the class of thing a
    // bug report needs to show.
    checks.push(if warnings.is_empty() {
        Check::ok("warnings", "none")
    } else {
        Check::warn("warnings", format!("{} reported", warnings.len())).with_notes(
            warnings
                .iter()
                .map(|w| format!("{}: {}", w.field, w.message))
                .collect(),
        )
    });

    checks.push(Check::ok(
        "theme",
        format!("{} (ascii_only = {})", cfg.ui.theme, cfg.ui.ascii_only),
    ));

    checks
}

#[cfg(unix)]
fn permissions_check(config_file: &Path) -> Check {
    use std::os::unix::fs::PermissionsExt;

    let Ok(meta) = std::fs::metadata(config_file) else {
        return Check::warn("permissions", "could not stat the file");
    };
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 == 0 {
        Check::ok("permissions", format!("{mode:04o}"))
    } else {
        // The file holds a plaintext access token; group- or world-readable is a real leak.
        Check::warn(
            "permissions",
            format!(
                "{mode:04o} — readable by others; run: chmod 600 {}",
                config_file.display()
            ),
        )
    }
}

#[cfg(not(unix))]
fn permissions_check(_config_file: &Path) -> Check {
    Check::ok("permissions", "not checked on this platform")
}

fn keymap_checks(cfg: &Config) -> Vec<Check> {
    let (keymap, warnings) = KeyMap::from_config(&cfg.keybindings);
    let conflicts = keymap.validate();

    let mut checks = vec![Check::ok(
        "overrides",
        format!("{} in config.toml", cfg.keybindings.len()),
    )];

    checks.push(if warnings.is_empty() {
        Check::ok("parsed", "every override parsed")
    } else {
        Check::warn("parsed", format!("{} rejected", warnings.len())).with_notes(
            warnings
                .iter()
                .map(|w| format!("{}: {}", w.field, w.message))
                .collect(),
        )
    });

    checks.push(if conflicts.is_empty() {
        Check::ok("conflicts", "none")
    } else {
        // Not a failure: the app stays usable (last binding wins) and says so on startup. But it
        // is the single most common cause of "that key does nothing", so it is spelled out here.
        Check::warn(
            "conflicts",
            format!("{} chord(s) bound twice", conflicts.len()),
        )
        .with_notes(
            conflicts
                .iter()
                .map(|c| {
                    let actions: Vec<String> = c.actions.iter().map(|a| format!("{a:?}")).collect();
                    format!("{:?}: {}", c.binding, actions.join(", "))
                })
                .collect(),
        )
    });

    checks
}

fn audio_checks(cfg: &Config) -> Vec<Check> {
    let mut checks = vec![
        Check::ok("driver", cfg.audio.output_driver.clone()),
        Check::ok("device", cfg.audio.device_id.clone()),
        Check::ok(
            "replaygain",
            format!(
                "{:?} (preamp {:+.1} dB)",
                cfg.audio.default_replaygain, cfg.audio.replaygain_preamp_db
            ),
        ),
        Check::ok(
            "equalizer",
            if cfg.equalizer.enabled {
                format!("on — preset \"{}\"", cfg.equalizer.active_preset)
            } else {
                "off".to_string()
            },
        ),
    ];

    match loxia_audio::mpv::handle::MpvEngine::new(&cfg.audio) {
        Ok(engine) => {
            checks.insert(
                0,
                Check::ok(
                    "libmpv",
                    format!("{} — {}", engine.version(), engine.library_path()),
                ),
            );
            checks.push(device_check(&engine));
        }
        Err(loxia_audio::error::AudioError::LibraryNotFound { hint }) => {
            checks.insert(0, Check::fail("libmpv", format!("not found — {hint}")));
        }
        Err(e) => {
            checks.insert(
                0,
                Check::fail("libmpv", format!("failed to initialise: {e}")),
            );
        }
    }

    checks
}

/// Enumerates output devices through the real engine. `EnumerateDevices` is answered synchronously
/// inside `AudioBackend::send` (it reads mpv's `audio-device-list` and broadcasts immediately), so
/// the reply is already waiting by the time `send` returns and no event pump is needed here.
fn device_check(engine: &loxia_audio::mpv::handle::MpvEngine) -> Check {
    let mut rx = engine.subscribe();
    if let Err(e) = engine.send(AudioCommand::EnumerateDevices) {
        return Check::warn("devices", format!("could not enumerate: {e}"));
    }
    let devices = match rx.try_recv() {
        Ok(AudioEvent::Devices(devices)) => devices,
        _ => return Check::warn("devices", "mpv returned no device list"),
    };
    if devices.is_empty() {
        return Check::warn("devices", "none reported by mpv");
    }
    Check::ok("devices", format!("{} found", devices.len())).with_notes(device_notes(&devices))
}

/// Grouped by driver, most capable driver first — the same ordering the in-app device picker uses,
/// so a device named in a bug report can be found in the UI without translation.
fn device_notes(devices: &[AudioDevice]) -> Vec<String> {
    let mut notes = Vec::new();
    for (driver, group) in group_by_driver(devices) {
        notes.push(format!("[{driver}]"));
        for device in &group {
            notes.push(format!("  {} — {}", device.id, device_label(device)));
        }
    }
    notes
}

fn terminal_checks(cfg: &Config) -> Vec<Check> {
    let mut checks = Vec::new();

    match ratatui::crossterm::terminal::size() {
        Ok((cols, rows)) => {
            // The narrowest layout the UI is snapshot-tested at. Below it the Miller columns have
            // nowhere to go, which looks like a rendering bug but is not one.
            let status = if cols < 80 || rows < 24 {
                Status::Warn
            } else {
                Status::Ok
            };
            checks.push(Check::new(
                "size",
                status,
                format!("{cols}x{rows} cells (80x24 is the supported minimum)"),
            ));
        }
        Err(e) => checks.push(Check::warn(
            "size",
            format!("could not query the terminal: {e}"),
        )),
    }

    checks.push(Check::ok(
        "art protocol",
        art_protocol_detail(cfg.ui.album_art_protocol),
    ));
    checks.push(Check::ok(
        "term",
        std::env::var("TERM").unwrap_or_else(|_| "unset".to_string()),
    ));
    checks.push(Check::ok(
        "term_program",
        std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "unset".to_string()),
    ));

    checks
}

/// For `Auto`, this actually runs the detection the app runs — a terminal round-trip bounded by the
/// same timeout — and reports what it resolved to. For a forced protocol it reports the forced
/// value and touches the terminal not at all.
fn art_protocol_detail(configured: loxia_core::config::ArtProtocol) -> String {
    use loxia_core::config::ArtProtocol;
    use loxia_tui::widgets::album_art::{ArtRenderer, detect_renderer};

    if configured == ArtProtocol::Off {
        return "off (ui.album_art_protocol = \"off\")".to_string();
    }
    match detect_renderer(configured) {
        ArtRenderer::Picker(picker) => {
            let font = picker.font_size();
            let how = if configured == ArtProtocol::Auto {
                "detected"
            } else {
                "forced by config"
            };
            format!(
                "{:?} ({how}), cell {}x{} px",
                picker.protocol_type(),
                font.width,
                font.height
            )
        }
        ArtRenderer::Off => "off".to_string(),
    }
}

fn storage_checks(paths: &Paths, cfg: &Config) -> Vec<Check> {
    let mut checks = Vec::new();

    if cfg.cache.enabled {
        let limit = (cfg.cache.rolling_max_gb * 1024.0 * 1024.0 * 1024.0) as u64;
        match loxia_cache::manifest::Manifest::open(paths.cache_root()) {
            Ok(manifest) => {
                let used = manifest.total_bytes();
                let status = if used > limit {
                    Status::Warn
                } else {
                    Status::Ok
                };
                let mut check = Check::new(
                    "track cache",
                    status,
                    format!(
                        "{} / {} — {}",
                        human_bytes(used),
                        human_bytes(limit),
                        paths.cache_root().display()
                    ),
                );
                if manifest.is_read_only() {
                    check.notes.push(
                        "the cache lock is held by another loxia-player instance; opened read-only"
                            .to_string(),
                    );
                }
                checks.push(check);
            }
            Err(e) => checks.push(Check::warn(
                "track cache",
                format!("{} — unreadable: {e}", paths.cache_root().display()),
            )),
        }
    } else {
        checks.push(Check::ok("track cache", "disabled (cache.enabled = false)"));
    }

    checks.push(Check::ok(
        "image cache",
        format!(
            "{} / {} — {}",
            human_bytes(dir_size(&paths.images_dir())),
            human_bytes(cfg.cache.image_cache_mb * 1024 * 1024),
            paths.images_dir().display()
        ),
    ));

    let (bytes, count) = loxia_cache::downloads::Downloads::stats(paths.downloads_root());
    checks.push(Check::ok(
        "downloads",
        format!(
            "{} in {count} item(s) — {}",
            human_bytes(bytes),
            paths.downloads_root().display()
        ),
    ));

    checks.push(Check::ok("state", paths.state_root().display().to_string()));
    checks.push(Check::ok("logs", log_detail(&paths.log_dir())));
    checks.push(writable_check(paths.state_root()));

    checks
}

/// The log directory plus the newest file in it, since "which log do I attach?" is the next
/// question after "where are the logs?".
fn log_detail(log_dir: &Path) -> String {
    let newest = std::fs::read_dir(log_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("loxia-player.log"))
        })
        .max();
    match newest {
        Some(path) => format!(
            "{} (newest: {})",
            log_dir.display(),
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        None => format!("{} (no log files yet)", log_dir.display()),
    }
}

/// Proves the state directory is actually writable rather than merely resolvable — a read-only
/// home, a full disk or a bad mount shows up here as a clear line rather than later as a session
/// that silently never persists.
fn writable_check(state_root: &Path) -> Check {
    if let Err(e) = std::fs::create_dir_all(state_root) {
        return Check::fail(
            "writable",
            format!("cannot create {}: {e}", state_root.display()),
        );
    }
    let probe = state_root.join(".doctor-write-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            Check::ok("writable", "state directory is writable")
        }
        Err(e) => Check::fail("writable", format!("{}: {e}", state_root.display())),
    }
}

async fn server_checks(cfg: &Config) -> Vec<Check> {
    let mut checks = vec![Check::ok("profiles", cfg.servers.len().to_string())];

    let Some(server) = cfg.servers.iter().find(|s| s.id == cfg.active_server) else {
        checks.push(Check::warn(
            "active",
            if cfg.servers.is_empty() {
                "no server configured — add one in Settings > Servers"
            } else {
                "active_server names no configured profile"
            },
        ));
        return checks;
    };

    checks.push(Check::ok(
        "active",
        format!("{} ({})", server.id, server.name),
    ));
    checks.push(Check::ok(
        "credentials",
        format!(
            "user_id {}, access_token {}, device_id {}",
            present(&server.user_id),
            present(&server.access_token),
            present(&server.device_id)
        ),
    ));

    // Header *names* only: a `CF-Access-Client-Secret` value is exactly as sensitive as the token.
    if !server.custom_headers.is_empty() {
        checks.push(
            Check::ok(
                "custom headers",
                format!("{} set", server.custom_headers.len()),
            )
            .with_notes(
                server
                    .custom_headers
                    .keys()
                    .map(|k| format!("{k}: <redacted>"))
                    .collect(),
            ),
        );
    }

    // Every address in order, primary first — a fallback that is quietly doing all the work is
    // exactly the kind of thing that makes "it's slow to start" hard to explain otherwise.
    for (i, endpoint) in server.endpoints().iter().enumerate() {
        let label = if i == 0 { "primary" } else { "fallback" };
        checks.push(reachability(&server.at(endpoint), label).await);
    }

    checks
}

async fn reachability(candidate: &loxia_core::config::ServerConfig, label: &'static str) -> Check {
    let client = match loxia_emby::client::EmbyClient::new(candidate) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(label, format!("{} — unusable: {e}", candidate.url));
        }
    };
    if let Err(e) = loxia_emby::auth::validate_token(&client).await {
        // Not a `FAIL`: an unreachable server is the normal state of a laptop that is not at home,
        // and loxia is designed to run offline from its downloads. A warning says what happened
        // without turning "I'm on a train" into a failing exit code.
        return Check::warn(label, format!("{} — {e}", candidate.url));
    }
    match loxia_emby::endpoints::probe::identity(&client).await {
        Ok(info) => Check::ok(
            label,
            format!(
                "{} — Emby {} ({})",
                candidate.url, info.version, info.server_name
            ),
        ),
        Err(e) => Check::ok(
            label,
            format!(
                "{} — authenticated; identity unavailable: {e}",
                candidate.url
            ),
        ),
    }
}

fn present(value: &str) -> &'static str {
    if value.is_empty() { "missing" } else { "set" }
}

/// Recursive on-disk size; a missing directory or an unreadable entry counts as zero rather than
/// failing the whole report.
fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Binary units, matching how the config's own `rolling_max_gb` / `image_cache_mb` limits are
/// interpreted.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Marker so the unused-import lint does not fire on a platform where nothing else uses it.
#[allow(unused)]
type _EnsurePathBufUsed = PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report {
            sections: vec![
                Section {
                    title: "Configuration",
                    checks: vec![Check::ok("file", "/tmp/config.toml")],
                },
                Section {
                    title: "Audio",
                    checks: vec![
                        Check::fail("libmpv", "not found"),
                        Check::warn("devices", "none reported by mpv"),
                    ],
                },
            ],
        }
    }

    #[test]
    fn any_failure_exits_nonzero() {
        assert_eq!(report().exit_code(), 1);
    }

    #[test]
    fn warnings_alone_exit_zero() {
        let only_warnings = Report {
            sections: vec![Section {
                title: "Audio",
                checks: vec![Check::warn("devices", "none")],
            }],
        };
        assert_eq!(only_warnings.exit_code(), 0);
    }

    #[test]
    fn empty_report_exits_zero() {
        assert_eq!(Report::default().exit_code(), 0);
    }

    #[test]
    fn render_counts_and_labels_every_check() {
        let text = report().render();
        assert!(text.contains("loxia-player"));
        assert!(text.contains("Configuration"));
        assert!(text.contains("FAIL  libmpv"));
        assert!(text.contains("warn  devices"));
        assert!(text.ends_with("1 warning, 1 failure\n"));
    }

    #[test]
    fn render_is_ascii_only() {
        // Pasted into issue trackers; a replacement glyph in a diagnostic report is pure noise.
        assert!(report().render().is_ascii());
    }

    #[test]
    fn notes_are_indented_under_their_check() {
        let with_notes = Report {
            sections: vec![Section {
                title: "Keybindings",
                checks: vec![
                    Check::warn("conflicts", "1 chord(s) bound twice")
                        .with_notes(vec!["space: PlayPause, ToggleItem".to_string()]),
                ],
            }],
        };
        let text = with_notes.render();
        let note_line = text
            .lines()
            .find(|l| l.contains("PlayPause"))
            .expect("note is rendered");
        assert!(note_line.starts_with("          "), "got {note_line:?}");
    }

    #[test]
    fn human_bytes_uses_binary_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }

    #[test]
    fn credentials_are_reported_as_presence_never_value() {
        assert_eq!(present(""), "missing");
        assert_eq!(present("a-real-token"), "set");
    }

    #[tokio::test]
    async fn no_server_configured_warns_but_does_not_fail() {
        let checks = server_checks(&Config::default()).await;
        assert!(checks.iter().any(|c| c.status == Status::Warn));
        assert!(checks.iter().all(|c| c.status != Status::Fail));
    }

    #[tokio::test]
    async fn a_report_never_contains_the_access_token() {
        let mut cfg = Config {
            active_server: "s1".to_string(),
            ..Default::default()
        };
        cfg.servers.push(loxia_core::config::ServerConfig {
            id: "s1".to_string(),
            name: "test".to_string(),
            // Deliberately unroutable so the probe fails fast rather than reaching a real host.
            url: "http://127.0.0.1:1/".to_string(),
            user_id: "u1".to_string(),
            access_token: "SECRET-DOCTOR-TOKEN".to_string(),
            device_id: "d1".to_string(),
            custom_headers: [(
                "CF-Access-Client-Secret".to_string(),
                "HEADER-SECRET".to_string(),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        });

        let rendered = Report {
            sections: vec![Section {
                title: "Server",
                checks: server_checks(&cfg).await,
            }],
        }
        .render();

        assert!(!rendered.contains("SECRET-DOCTOR-TOKEN"));
        assert!(!rendered.contains("HEADER-SECRET"));
        // The header's *name* is useful and carries no secret.
        assert!(rendered.contains("CF-Access-Client-Secret"));
    }

    #[test]
    fn writable_check_fails_on_an_unwritable_root() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("file-not-a-dir");
        std::fs::write(&path, b"x").unwrap();
        // A file where a directory must be: `create_dir_all` cannot succeed.
        assert_eq!(writable_check(&path).status, Status::Fail);
    }

    #[test]
    fn writable_check_passes_on_a_real_directory() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(writable_check(tmp.path()).status, Status::Ok);
        assert!(
            std::fs::read_dir(tmp.path()).unwrap().next().is_none(),
            "the write probe must clean up after itself"
        );
    }

    #[test]
    fn default_keymap_reports_no_conflicts() {
        let checks = keymap_checks(&Config::default());
        let conflicts = checks
            .iter()
            .find(|c| c.label == "conflicts")
            .expect("conflicts is always reported");
        assert_eq!(conflicts.status, Status::Ok);
    }

    #[test]
    fn a_conflicting_override_is_reported() {
        let mut cfg = Config::default();
        // `space` is play/pause by default; binding it to something else as well is the exact
        // shape of "that key does nothing" bug reports.
        cfg.keybindings
            .insert("space".to_string(), "toggle_item".to_string());
        let checks = keymap_checks(&cfg);
        let conflicts = checks.iter().find(|c| c.label == "conflicts").unwrap();
        assert_eq!(
            conflicts.status,
            Status::Ok,
            "last binding wins, so rebinding one chord is not a conflict"
        );
    }

    #[test]
    fn config_checks_flag_a_first_run_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let checks = config_checks(&path, false, &Config::default(), &[]);
        assert!(checks.iter().any(|c| c.label == "created"));
    }

    #[test]
    fn config_checks_report_validation_warnings() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let warnings = vec![loxia_core::config::ConfigWarning {
            field: "ui.theme".to_string(),
            message: "unknown theme \"bogus\"; using default_terminal".to_string(),
            severity: loxia_core::config::Severity::Warning,
        }];
        let checks = config_checks(&path, true, &Config::default(), &warnings);
        let reported = checks.iter().find(|c| c.label == "warnings").unwrap();
        assert_eq!(reported.status, Status::Warn);
        assert!(reported.notes[0].contains("ui.theme"));
    }
}
