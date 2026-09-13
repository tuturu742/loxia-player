//! loxia binary: CLI, bootstrap, and the main event loop wiring the other five crates together.
//! See docs/01-architecture.md §3.6.

mod bootstrap;
mod dispatch;
mod doctor;
mod input;
mod runtime;
mod terminal;
mod workers;

use std::path::{Path, PathBuf};

use clap::Parser;
use loxia_core::config::ConfigWarning;
use loxia_core::keymap::KeyMap;
use loxia_core::paths::{Paths, SystemDirs};
use loxia_core::state::AppState;
use terminal::TerminalGuard;
use tracing_appender::non_blocking::WorkerGuard;

/// The log file's own name, and the prefix `tracing-appender`'s daily rotation adds a date to.
/// Kept beside the binary's name (`loxia_core::paths::APP_DIR`) rather than derived from it, since
/// the rotation prefix must match what the retention sweep looks for.
const LOG_FILE: &str = "loxia-player.log";
const LOG_PREFIX: &str = "loxia-player.log.";

#[derive(Parser, Debug)]
#[command(
    name = "loxia-player",
    version,
    about = "A terminal music client for Emby"
)]
struct Cli {
    /// Override the config file location.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Override `active_server` for this run.
    #[arg(long)]
    server: Option<String>,

    /// Override `logging.level`; also settable via `LOXIA_LOG`.
    #[arg(long = "log-level")]
    log_level: Option<String>,

    /// Use the mock audio backend instead of libmpv2.
    #[arg(long = "no-audio")]
    no_audio: bool,

    /// Run diagnostics and exit (implemented in task 12-08).
    #[arg(long)]
    doctor: bool,

    /// Panic after entering the terminal, to manually verify panic-safe restore.
    #[arg(long = "panic-test", hide = true)]
    panic_test: bool,

    /// Load a URL directly at startup, bypassing the queue — for exercising the audio engine
    /// before the queue exists (`05-06`).
    #[arg(long = "play-url", hide = true)]
    play_url: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.doctor {
        println!("loxia-player --doctor: not yet implemented (see task 12-08)");
        std::process::exit(0);
    }

    let paths = match Paths::resolve(&SystemDirs, &loxia_core::config::CacheConfig::default()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("loxia-player: could not resolve application directories: {e}");
            std::process::exit(1);
        }
    };

    let (mut cfg, mut warnings) = match &cli.config {
        Some(path) => bootstrap::load_at(path),
        None => bootstrap::load(&paths),
    };

    if let Some(server) = &cli.server {
        cfg.active_server = server.clone();
    }

    let level = resolve_log_level(
        cli.log_level.as_deref(),
        std::env::var("LOXIA_LOG").ok().as_deref(),
        &cfg.logging.level,
    );

    // The WorkerGuard must live for the rest of the process — dropping it flushes the
    // non-blocking writer, so it stays a local in `main` rather than being dropped early.
    let _log_guard = init_logging(&level, &paths.log_dir(), cfg.logging.max_files);
    log_startup(&paths, &warnings);

    if cli.no_audio {
        tracing::info!("--no-audio: using the mock audio backend");
    }

    terminal::install_panic_hook();

    let mut guard = match TerminalGuard::enter(cfg.ui.enable_mouse) {
        Ok(g) => g,
        Err(e) => {
            terminal::restore_terminal_raw();
            eprintln!("loxia-player: could not start the terminal UI: {e}");
            std::process::exit(1);
        }
    };

    if cli.panic_test {
        panic!("--panic-test: deliberate panic to verify terminal restore");
    }

    let selected_backend = match select_audio_backend(cli.no_audio, &cfg.audio) {
        Ok(selected) => selected,
        Err(hint) => {
            let _ = guard.restore();
            eprintln!("{}", library_not_found_message(&hint));
            std::process::exit(1);
        }
    };
    // `11-07`: the About view's own libmpv line — captured from `selected_backend` before
    // `into_boxed` erases which concrete engine it was.
    let about_libmpv = about_libmpv_status(&selected_backend, cli.no_audio);
    let audio_backend = selected_backend.into_boxed();

    let (keymap, keymap_warnings) = KeyMap::from_config(&cfg.keybindings);
    warnings.extend(keymap_warnings);

    let mut state = AppState {
        config: cfg,
        keymap,
        config_warnings: warnings,
        // `11-07`: `Paths::resolve` performs no I/O of its own (`paths.rs`'s own doc comment) —
        // storing the already-resolved value is no different from any other plain-data field; the
        // About view is its only reader today.
        paths: Some(paths.clone()),
        about: loxia_core::state::AboutInfo {
            libmpv: about_libmpv,
        },
        ..AppState::default()
    };
    state.touch();

    // A real gap found in the field: falling back to the mock audio backend (libmpv found, but
    // some other init error — the `LibraryNotFound` case above already exits with a clear stderr
    // message before the terminal UI even starts) was previously silent but for a `tracing::warn!`
    // line in `select_audio_backend` — from inside the running app, playback looked completely
    // normal (the queue plays, the position advances, since the mock engine simulates all of it)
    // while producing no actual sound at all, with nothing on screen ever explaining why
    // (`docs/12-decisions.md`).
    if let Some(message) = audio_fallback_toast(&state.about.libmpv) {
        state.toast(message, loxia_core::state::toast::ToastLevel::Warning);
    }

    // `09-03`: factory presets (parsed from `assets/eq_presets.toml` by `loxia_audio::eq`, since
    // `loxia-core` cannot parse that asset itself) merged with the user's own custom presets —
    // fired once, here, rather than on some later event, since there is nothing to wait for.
    let presets = loxia_audio::eq::all_presets(&state.config.equalizer.custom_presets);
    let _ = loxia_core::reducer::apply(
        &mut state,
        loxia_core::action::Action::Data(loxia_core::action::DataAction::PresetsLoaded { presets }),
    );

    // Applies the persisted config onto the runtime mirror the app actually reads — theme, quality
    // profile, ReplayGain mode and EQ. Must follow `PresetsLoaded` above: resolving the active EQ
    // preset's gains needs `known_presets`. Its effects go out with `restore_effects` below, once
    // `workers` exists to dispatch them (`docs/12-decisions.md`).
    let mut startup_effects = loxia_core::reducer::hydrate_from_config(&mut state);

    // `11-06`: restored **before** connecting — the UI is usable immediately, and a slow or
    // failed connection still leaves the user with their queue rather than an empty screen
    // (`docs/06-cache-and-offline.md` §8). Neither history nor the session snapshot need a live
    // connection; `bootstrap::restore_session` only ever touches `AppState`/`Paths`. Any effects
    // the restore itself produces (a real `Effect::Audio(Load)` for `ui.restore_autoplay`) need
    // `workers` to dispatch through, which doesn't exist yet — collected here, sent once it does.
    startup_effects.extend(bootstrap::restore_session(&mut state, &paths));

    let workers = match bootstrap::connect(&mut state).await {
        Some(bootstrap::Connected {
            client,
            library,
            storage,
        }) => {
            let cache = bootstrap::open_cache(
                &paths,
                // The Emby installation's own id, so two profiles pointing at one server share a
                // cache and a download tree instead of keeping two copies of everything
                // (`bootstrap::storage_id`).
                storage.clone(),
                state.config.transcode.target_codec,
                // Honours the config again. It was forced off after a live user's playback died with
                // `http: ... Immediate exit requested`, blamed on this re-fetching the exact track
                // mpv was streaming. Measured against a real server since: a concurrent fetch of the
                // *same* item, on the same device id, in both direct and transcode profiles, never
                // interrupted a held-open stream (`docs/12-decisions.md`). It still costs double
                // bandwidth on first play, which is why it now defaults off and is opt-in.
                state.config.cache.prefetch_on_play,
            );
            // `11-07`: the About view's own cache/download stats — read once, here, from the
            // structures `open_cache` (rolling cache manifest) and `loxia_cache::downloads::
            // Downloads::stats` (a read-only parse, no `Fetcher` needed) already give a synchronous
            // answer for; nothing keeps this live afterward (`loxia-core` has no I/O of its own to
            // do so, `docs/12-decisions.md`).
            if let Some((cache_arc, _)) = &cache {
                state.cache_stats.audio_cache_bytes = cache_arc.lock().await.total_bytes();
            }
            state.cache_stats.image_cache_bytes = dir_size(&paths.images_dir());
            let (download_bytes, download_count) =
                loxia_cache::downloads::Downloads::stats(paths.downloads_root());
            state.cache_stats.download_bytes = download_bytes;
            state.cache_stats.pinned_count = download_count;
            // The cache worker announces the authoritative set (and totals) as soon as it starts —
            // this is only so the very first frame does not show `d` as un-pinned on something that
            // is already downloaded.
            // `08-06`'s connectivity state machine is what will actually flip `offline` and
            // populate the index; until then this is an inert default (`docs/12-decisions.md`).
            let offline = workers::network::OfflineHandle {
                offline: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                index: std::sync::Arc::new(std::sync::Mutex::new(None)),
            };
            let workers = workers::Workers::spawn(
                client,
                library,
                audio_backend,
                cache,
                offline,
                state.config.ui.enable_websocket,
            );
            // `11-06`: generalized from "always seed Artists" to "whichever tab the (possibly
            // just-restored) `state.nav.active_tab` actually is" — and skipped outright if that
            // tab already has columns.
            if let Some(seed) = bootstrap::seed_active_tab_column(&mut state) {
                let _ = workers.network.send(seed);
            }
            workers
        }
        None => workers::Workers::spawn_stubs(audio_backend),
    };

    for effect in startup_effects {
        dispatch::dispatch(&effect, &workers);
    }

    // Enumerate audio devices once at startup so `player.known_devices` is populated before anything
    // needs it. Previously the *only* emitter was the device picker opening, so `known_devices` was
    // empty until the user happened to press `O`, leaving the Settings device/driver dropdowns with
    // nothing to offer (`docs/12-decisions.md`).
    let _ = workers.audio.send(loxia_core::effect::Effect::Audio(
        loxia_core::effect::AudioEffect::EnumerateDevices,
    ));

    if let Some(url) = cli.play_url {
        let load = loxia_core::effect::AudioEffect::Load {
            url: loxia_core::effect::RedactedUrl::new(url),
            headers: std::collections::BTreeMap::new(),
            start_at: std::time::Duration::ZERO,
            gain_db: None,
        };
        let _ = workers.audio.send(loxia_core::effect::Effect::Audio(load));
    }

    let result = runtime::run_terminal(state, &mut guard, workers, &paths).await;

    // Explicit before the process exits; `Drop` would also do this, but doing it here means a
    // loop error is reported to stderr on an already-restored terminal, not a raw one.
    let _ = guard.restore();

    if let Err(e) = result {
        eprintln!("loxia-player: {e}");
        std::process::exit(1);
    }
}

/// The message printed to stderr — after the terminal has been restored — when `MpvEngine::new`
/// reports `AudioError::LibraryNotFound`. A pure function so its exact shape is testable without
/// needing to actually make libmpv unavailable to this process (`05-05`'s own manual acceptance
/// step — renaming libmpv temporarily on a real machine and checking the output by eye — still
/// applies; this covers the message text deterministically alongside it).
fn library_not_found_message(hint: &str) -> String {
    format!(
        "loxia-player requires libmpv, which was not found.\n\n  {hint}\n\n\
         See {}#installing-mpv for details.\n\n\
         Pass --no-audio to browse the library without playback in the meantime.",
        env!("CARGO_PKG_REPOSITORY")
    )
}

/// A concrete (non-trait-object) result of `select_audio_backend`, so tests can distinguish which
/// backend was actually chosen (`no_audio_flag_selects_mock`) — a `Box<dyn AudioBackend>` alone
/// can't be introspected from outside without a downcast the trait doesn't offer.
enum SelectedBackend {
    Mock(loxia_audio::mock::MockEngine),
    Real(loxia_audio::mpv::handle::MpvEngine),
}

impl SelectedBackend {
    fn into_boxed(self) -> Box<dyn loxia_audio::backend::AudioBackend> {
        match self {
            SelectedBackend::Mock(e) => Box::new(e),
            SelectedBackend::Real(e) => Box::new(e),
        }
    }

    #[cfg(test)]
    fn is_mock(&self) -> bool {
        matches!(self, SelectedBackend::Mock(_))
    }
}

/// `--no-audio` selects `MockEngine` outright, never even attempting `MpvEngine::new` — a machine
/// with no libmpv at all must not risk touching it. Otherwise the real engine; a missing libmpv
/// (`AudioError::LibraryNotFound`, the common, expected failure) is returned as `Err(hint)` for
/// the caller to turn into `library_not_found_message` and exit — this function itself never
/// calls `process::exit`, so it stays callable from a test. Any *other* init failure is rarer and
/// unexpected — falls back to `MockEngine` with a warning rather than making the whole app
/// unusable over it.
fn select_audio_backend(
    no_audio: bool,
    cfg: &loxia_core::config::AudioConfig,
) -> Result<SelectedBackend, String> {
    if no_audio {
        let (engine, _control) = loxia_audio::mock::MockEngine::new();
        return Ok(SelectedBackend::Mock(engine));
    }
    match loxia_audio::mpv::handle::MpvEngine::new(cfg) {
        Ok(engine) => Ok(SelectedBackend::Real(engine)),
        Err(loxia_audio::error::AudioError::LibraryNotFound { hint }) => Err(hint),
        Err(e) => {
            tracing::warn!(error = %e, "audio engine failed to initialise; using the mock backend");
            let (engine, _control) = loxia_audio::mock::MockEngine::new();
            Ok(SelectedBackend::Mock(engine))
        }
    }
}

/// `11-07`: total on-disk size of `path`'s contents, recursively — the image cache has no
/// manifest of its own the way the rolling audio cache does (`RollingCache::total_bytes`), so this
/// is a plain directory walk instead. A one-time boot-time read; not something this crate needs to
/// keep live, and `image_cache_mb`'s own config cap keeps it bounded in practice. Errors (a missing
/// directory on first run, a permission problem) are silently `0`, matching every other "a
/// cache-adjacent read failing is not fatal" posture in this codebase.
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

/// `11-07`: the About view's own libmpv status line. `Mock` covers both `--no-audio` (the literal
/// `"not loaded (--no-audio)"` this task's own spec names) and the rarer fallback where the real
/// engine's `new()` failed for some other reason (`select_audio_backend`'s own "logs a warning and
/// uses the mock engine" path) — a different, more specific reason string, since claiming
/// `--no-audio` there would be simply false.
fn about_libmpv_status(
    selected: &SelectedBackend,
    no_audio_flag: bool,
) -> loxia_core::state::LibmpvStatus {
    use loxia_core::state::LibmpvStatus;
    match selected {
        SelectedBackend::Mock(_) if no_audio_flag => {
            LibmpvStatus::NotLoaded("--no-audio".to_string())
        }
        SelectedBackend::Mock(_) => LibmpvStatus::NotLoaded(
            "libmpv failed to initialise; using the mock engine".to_string(),
        ),
        SelectedBackend::Real(engine) => LibmpvStatus::Loaded {
            version: engine.version(),
            path: engine.library_path(),
        },
    }
}

/// The startup toast for a silent audio-engine fallback — `None` for a real, working engine, and
/// `None` for `--no-audio` too, since that's an intentional choice, not a failure, and deserves no
/// warning. A real gap found in the field: without this, "audio playback failed to start" had
/// nowhere to go but a log line nobody watching the actual running app would ever see.
fn audio_fallback_toast(libmpv: &loxia_core::state::LibmpvStatus) -> Option<String> {
    match libmpv {
        loxia_core::state::LibmpvStatus::NotLoaded(reason) if reason != "--no-audio" => {
            Some(format!("audio playback failed to start: {reason}"))
        }
        _ => None,
    }
}

/// `--log-level` beats `LOXIA_LOG` beats `config.logging.level`. Takes the environment value as a
/// parameter rather than reading `std::env::var` itself so it stays a pure, deterministic
/// function callers can test without touching real process environment state.
fn resolve_log_level(cli: Option<&str>, env: Option<&str>, cfg_level: &str) -> String {
    cli.map(str::to_string)
        .or_else(|| env.map(str::to_string))
        .unwrap_or_else(|| cfg_level.to_string())
}

/// Builds the tracing subscriber: a daily-rolling, non-blocking file writer under `log_dir`,
/// pruned to `max_files`, filtered by `level`. Never touches stdout or stderr — those belong to
/// the TUI. Returns the `WorkerGuard` the caller must keep alive for the process lifetime.
fn init_logging(level: &str, log_dir: &Path, max_files: u8) -> WorkerGuard {
    let (subscriber, guard) = build_subscriber(level, log_dir, max_files);
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        eprintln!(
            "loxia-player: a tracing subscriber was already installed; logging may be incomplete"
        );
    }
    guard
}

fn build_subscriber(
    level: &str,
    log_dir: &Path,
    max_files: u8,
) -> (
    impl tracing::Subscriber + Send + Sync + 'static,
    WorkerGuard,
) {
    let _ = std::fs::create_dir_all(log_dir);
    prune_old_logs(log_dir, max_files as usize);

    let file_appender = tracing_appender::rolling::daily(log_dir, LOG_FILE);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = tracing_subscriber::EnvFilter::try_new(level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let subscriber = tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_env_filter(filter)
        .finish();

    (subscriber, guard)
}

/// `tracing-appender`'s rolling file writer has no built-in retention limit — it rolls forever.
/// This keeps only the newest `keep` `loxia-player.log.*` files in `dir`, run once at startup before the
/// current run's file is created.
fn prune_old_logs(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(LOG_PREFIX))
        })
        .collect();
    logs.sort(); // date-suffixed names sort chronologically
    if logs.len() > keep {
        for old in &logs[..logs.len() - keep] {
            let _ = std::fs::remove_file(old);
        }
    }
}

/// Logs the version, config path, and every collected warning at `warn` — regardless of the
/// warning's own `Severity`, per task 01-06's spec.
fn log_startup(paths: &Paths, warnings: &[ConfigWarning]) {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %paths.config_file().display(),
        "starting loxia-player"
    );
    for w in warnings {
        tracing::warn!(field = %w.field, "{}", w.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_audio_flag_selects_mock() {
        let selected =
            select_audio_backend(true, &loxia_core::config::AudioConfig::default()).unwrap();
        assert!(
            selected.is_mock(),
            "--no-audio must never even attempt MpvEngine::new"
        );
    }

    #[test]
    fn audio_fallback_toast_warns_on_a_real_init_failure() {
        let libmpv = loxia_core::state::LibmpvStatus::NotLoaded(
            "libmpv failed to initialise; using the mock engine".to_string(),
        );
        let message = audio_fallback_toast(&libmpv).expect("must toast on a real failure");
        assert!(message.contains("audio playback failed to start"));
    }

    #[test]
    fn audio_fallback_toast_is_silent_for_deliberate_no_audio() {
        let libmpv = loxia_core::state::LibmpvStatus::NotLoaded("--no-audio".to_string());
        assert!(audio_fallback_toast(&libmpv).is_none());
    }

    #[test]
    fn audio_fallback_toast_is_silent_when_loaded() {
        let libmpv = loxia_core::state::LibmpvStatus::Loaded {
            version: "0.35.1".to_string(),
            path: "/usr/lib/libmpv.so.2".to_string(),
        };
        assert!(audio_fallback_toast(&libmpv).is_none());
    }

    #[test]
    fn library_not_found_message_is_actionable() {
        let message = library_not_found_message("install mpv from your package manager.");
        assert!(message.contains("libmpv"));
        assert!(message.contains("install mpv from your package manager."));
        assert!(message.contains("--no-audio"));
        assert!(message.contains("#installing-mpv"));
        assert!(!message.contains("panic"));
        assert!(!message.contains("backtrace"));
    }

    #[test]
    fn log_level_precedence() {
        assert_eq!(
            resolve_log_level(Some("debug"), Some("trace"), "info"),
            "debug"
        );
        assert_eq!(resolve_log_level(None, Some("trace"), "info"), "trace");
        assert_eq!(resolve_log_level(None, None, "info"), "info");
    }

    #[test]
    fn logging_never_writes_to_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        let (subscriber, guard) = build_subscriber("info", tmp.path(), 5);
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!("boom");
        });
        drop(guard);

        // The subscriber's only writer is the file appender constructed above — never
        // `std::io::stdout()` or `std::io::stderr()` — so nothing to capture there; the
        // meaningful assertion is that the message actually landed in the log file.
        let mut found = false;
        for entry in std::fs::read_dir(tmp.path()).unwrap() {
            let path = entry.unwrap().path();
            if let Ok(contents) = std::fs::read_to_string(&path)
                && contents.contains("boom")
            {
                found = true;
            }
        }
        assert!(found, "log file did not contain the emitted message");
    }

    #[test]
    fn config_warnings_are_logged() {
        let tmp = tempfile::tempdir().unwrap();
        let (subscriber, guard) = build_subscriber("info", tmp.path(), 5);

        let dirs = loxia_core::paths::FixedDirs::new(
            Some(tmp.path().join("config")),
            Some(tmp.path().join("cache")),
            Some(tmp.path().join("data")),
            Some(tmp.path().join("state")),
        );
        let paths = Paths::resolve(&dirs, &loxia_core::config::CacheConfig::default()).unwrap();
        let warnings = vec![ConfigWarning {
            field: "ui.theme".to_string(),
            message: "unknown theme \"bogus\"; using default_terminal".to_string(),
            severity: loxia_core::config::Severity::Warning,
        }];

        tracing::subscriber::with_default(subscriber, || {
            log_startup(&paths, &warnings);
        });
        drop(guard);

        let mut found = false;
        for entry in std::fs::read_dir(tmp.path()).unwrap() {
            let path = entry.unwrap().path();
            if let Ok(contents) = std::fs::read_to_string(&path)
                && contents.contains("ui.theme")
            {
                found = true;
            }
        }
        assert!(found, "log file did not contain the ui.theme warning");
    }

    #[test]
    fn prune_keeps_only_newest_files() {
        let tmp = tempfile::tempdir().unwrap();
        for day in ["2026-01-01", "2026-01-02", "2026-01-03", "2026-01-04"] {
            std::fs::write(tmp.path().join(format!("{LOG_PREFIX}{day}")), "x").unwrap();
        }
        prune_old_logs(tmp.path(), 2);
        let mut remaining: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        remaining.sort();
        assert_eq!(
            remaining,
            vec![
                format!("{LOG_PREFIX}2026-01-03"),
                format!("{LOG_PREFIX}2026-01-04")
            ]
        );
    }
}
