//! Config load, session restore, server connect, worker spawn.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use loxia_core::Timestamp;
use loxia_core::config::{Config, ConfigWarning, ServerConfig, Severity};
use loxia_core::effect::{Effect, NetEffect};
use loxia_core::model::{ItemId, ServerId};
use loxia_core::paths::Paths;
use loxia_core::state::nav::{Column, ColumnKind, LoadState, Tab};
use loxia_core::state::toast::ToastLevel;
use loxia_core::state::{AppState, Connectivity, ServerSession};
use loxia_emby::client::EmbyClient;
use loxia_emby::endpoints::items;

const FIRST_RUN_HEADER: &str = r#"# loxia-player configuration file.
# See docs/user/configuration.md in the project repository for the full reference.
#
# Example server profile (uncomment and fill in to connect):
#
# [[servers]]
# id = "remote"
# name = "My Emby Server"
# url = "https://emby.example.com"
# user_id = "..."
# access_token = "..."
#
# [servers.custom_headers]
# "CF-Access-Client-Id" = "..."
# "CF-Access-Client-Secret" = "..."
#
# Further addresses for the SAME server, tried in order when the one above cannot be reached —
# a LAN address at home, the external one when away. Same account, same library, and the same
# cache and downloads: storage is keyed on the server's own id, not on the address.
#
# [[servers.fallbacks]]
# url = "http://192.168.1.10:8096"

"#;

/// Loads `config.toml`, creating a first-run default if it is absent and quarantining it if it
/// fails to parse. Never fails outright — a malformed config must never prevent the app starting.
pub fn load(paths: &Paths) -> (Config, Vec<ConfigWarning>) {
    load_at(&paths.config_file())
}

/// As `load`, but at an explicit path — this is what `--config <PATH>` (task `01-06`) overrides
/// with, bypassing the path `Paths` would otherwise compute.
pub fn load_at(path: &Path) -> (Config, Vec<ConfigWarning>) {
    if !path.exists() {
        let cfg = Config::default();
        let mut warnings = vec![ConfigWarning {
            field: "config".to_string(),
            message: format!("created a new config file at {}", path.display()),
            severity: Severity::Info,
        }];
        if let Err(e) = write_first_run(path, &cfg) {
            warnings.push(ConfigWarning {
                field: "config".to_string(),
                message: format!("could not write the initial config file: {e}"),
                severity: Severity::Warning,
            });
        }
        return (cfg, warnings);
    }

    let mut warnings = Vec::new();
    check_permissions(path, &mut warnings);

    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            // Unreadable is treated the same as unparseable: quarantine and start clean rather
            // than refusing to run.
            return quarantine_and_reset(path, &format!("could not be read: {e}"), warnings);
        }
    };

    match loxia_core::config::parse_and_validate(&text) {
        Ok((cfg, mut parse_warnings)) => {
            warnings.append(&mut parse_warnings);
            (cfg, warnings)
        }
        Err(e) => quarantine_and_reset(path, &e.to_string(), warnings),
    }
}

fn quarantine_and_reset(
    path: &Path,
    reason: &str,
    mut warnings: Vec<ConfigWarning>,
) -> (Config, Vec<ConfigWarning>) {
    let bad_path = {
        let mut p = path.as_os_str().to_os_string();
        p.push(".bad");
        std::path::PathBuf::from(p)
    };
    let _ = fs::rename(path, &bad_path);

    let cfg = Config::default();
    if let Err(e) = write_first_run(path, &cfg) {
        warnings.push(ConfigWarning {
            field: "config".to_string(),
            message: format!("could not write a fresh config after quarantining the old one: {e}"),
            severity: Severity::Warning,
        });
    }
    warnings.push(ConfigWarning {
        field: "config".to_string(),
        message: format!(
            "config.toml {reason}; moved it to {} and started with defaults",
            bad_path.display()
        ),
        severity: Severity::Warning,
    });
    (cfg, warnings)
}

/// Unix-only: an existing config readable by group or other gets a warning, since it contains the
/// access token. The mode is never changed silently — the user may have deliberate reasons.
#[cfg(unix)]
fn check_permissions(path: &Path, warnings: &mut Vec<ConfigWarning>) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = fs::metadata(path) {
        let mode = meta.permissions().mode();
        if mode & 0o077 != 0 {
            warnings.push(ConfigWarning {
                field: "config".to_string(),
                message: "config.toml is readable by other users; it contains your access token"
                    .to_string(),
                severity: Severity::Warning,
            });
        }
    }
}

#[cfg(not(unix))]
fn check_permissions(_path: &Path, _warnings: &mut Vec<ConfigWarning>) {}

fn write_first_run(path: &Path, cfg: &Config) -> anyhow::Result<()> {
    let body = loxia_core::config::serialize(cfg)?;
    let contents = format!("{FIRST_RUN_HEADER}{body}");
    save_text(path, &contents)
}

/// Writes `config.toml` atomically: `.tmp` in the same directory, `fsync`, `0600` on Unix, then
/// rename over the target. Setting the mode before the rename means the token is never briefly
/// world-readable.
///
/// Called from `runtime::shutdown` (`03-08`) as the quit-time save; the debounced-write trigger
/// on settings changes (task `11-01`) is its other real caller.
pub fn save(paths: &Paths, cfg: &Config) -> anyhow::Result<()> {
    let text = loxia_core::config::serialize(cfg)?;
    save_text(&paths.config_file(), &text)
}

fn save_text(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = {
        let mut p = path.as_os_str().to_os_string();
        p.push(".tmp");
        std::path::PathBuf::from(p)
    };

    let file = fs::File::create(&tmp_path)?;
    use std::io::Write;
    {
        let mut writer = std::io::BufWriter::new(&file);
        writer.write_all(contents.as_bytes())?;
        writer.flush()?;
    }
    file.sync_all()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// What `connect` returns on success — the client and the browsing scope, ready to hand to
/// `workers::Workers::spawn`.
pub struct Connected {
    pub client: Arc<EmbyClient>,
    /// The library every browsing list is scoped to, or `None` for "every music library on the
    /// server". See [`library_scope`].
    pub library: Option<ItemId>,
    /// Where everything stored per server goes — see [`storage_id`].
    pub storage: ServerId,
}

/// Which library the Artists/Album Artists/Albums/Genres lists should cover.
///
/// A server with more than one music library gets `None` — no `ParentId` at all, so the lists span
/// all of them. Pinning to `libraries[0]` is what made a second library invisible everywhere but
/// Folders: a user with 2145 artists in one library and 2531 in another saw only the first
/// (`docs/12-decisions.md`). Emby de-duplicates the union itself, so an artist in both libraries
/// still appears once.
///
/// With exactly one music library the scope stays explicit rather than becoming "the whole
/// server": it is equivalent for the lists themselves, and it keeps a server that also holds
/// non-music audio from leaking into them.
fn library_scope(libraries: Vec<ItemId>) -> Option<ItemId> {
    match libraries.len() {
        1 => libraries.into_iter().next(),
        _ => None,
    }
}

/// Authenticates against `state.config`'s active server profile, resolves the browsing scope over
/// the server's music libraries ([`library_scope`]), and updates `state.server`/`state.connectivity`.
/// Never fails outright: a missing server profile, an invalid token, or an unreachable server all
/// fall back to `Tab::Settings` with an explanatory toast instead of exiting — a bad token must
/// not make the app unusable.
pub async fn connect(state: &mut AppState) -> Option<Connected> {
    let Some(server_cfg) = state
        .config
        .servers
        .iter()
        .find(|s| s.id == state.config.active_server)
        .cloned()
    else {
        fail(state, "no server configured — add one in Settings");
        return None;
    };

    // `reach_server` has already toasted whatever went wrong on every address it tried.
    let Reached { client, identity } = reach_server(state, &server_cfg).await?;

    let library = match items::music_libraries(&client).await {
        Ok(libs) if libs.is_empty() => {
            fail(state, "server has no music library");
            return None;
        }
        Ok(libs) => {
            // Logged because this decision is made **once** per connection and then held for the
            // whole session: if this call ever returns a partial view of the server, every
            // browsing list is silently mis-scoped until the next restart, with nothing on screen
            // to say so. A user hit exactly that and it cleared on restart, at which point there
            // was no record of what had been decided (`docs/12-decisions.md`).
            let names: Vec<&str> = libs.iter().map(|lib| lib.name.as_str()).collect();
            let scope = library_scope(libs.iter().map(|lib| lib.id.clone()).collect());
            match &scope {
                Some(id) => tracing::info!(
                    libraries = ?names,
                    scoped_to = %id,
                    "one music library: browsing lists are scoped to it"
                ),
                None => tracing::info!(
                    libraries = ?names,
                    count = names.len(),
                    "several music libraries: browsing lists span the whole server"
                ),
            }
            scope
        }
        Err(e) => {
            fail(state, &format!("could not list libraries: {e}"));
            return None;
        }
    };

    state.server = ServerSession {
        user_id: Some(client.user_id().clone()),
        server_name: Some(server_cfg.name.clone()),
        connected_at: Some(Timestamp::now()),
    };
    state.connectivity = Connectivity::Online;

    let storage = storage_id(state, identity);
    Some(Connected {
        client,
        library,
        storage,
    })
}

struct Reached {
    client: Arc<EmbyClient>,
    /// The server's own id, if it reported one — already checked against the profile's.
    identity: Option<String>,
}

/// Tries every address this profile knows, primary first, and returns the first that answers.
///
/// A server can be reachable on more than one path — a LAN `http://` address at home and an
/// external `https://` one behind a proxy when away — and which of them works depends on where the
/// machine happens to be, not on anything the user should have to reconfigure
/// (`docs/12-decisions.md`).
///
/// **Any** failure moves on to the next address, not just an unreachable one: an endpoint fronted
/// by Cloudflare Access answers `403` when its service token is stale while the LAN address beside
/// it is perfectly fine, so "the server said no" is no more conclusive than "no route to host". If
/// every address fails, the *last* error is what the user is shown.
///
/// Each candidate must also prove it is the right server before it is used. The profile's recorded
/// `server_id` is what everything stored per server is keyed on, so an address that answers as a
/// *different* installation — stale DNS, a config copied to another machine — would otherwise
/// attach this profile's cache, downloads and session to somebody else's library. Such an endpoint
/// is skipped, loudly.
async fn reach_server(state: &mut AppState, server_cfg: &ServerConfig) -> Option<Reached> {
    let expected = server_cfg.server_id.clone();
    let endpoints = server_cfg.endpoints();
    let mut last_error = String::new();

    for (i, endpoint) in endpoints.iter().enumerate() {
        let candidate = server_cfg.at(endpoint);
        let client = match EmbyClient::new(&candidate) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                last_error = format!("invalid server configuration: {e}");
                tracing::warn!(url = %endpoint.url, error = %e, "endpoint unusable");
                continue;
            }
        };

        if let Err(e) = loxia_emby::auth::validate_token(&client).await {
            last_error = format!("could not connect: {e}");
            tracing::info!(url = %endpoint.url, error = %e, "endpoint did not answer; trying the next");
            continue;
        }

        let identity = match loxia_emby::endpoints::probe::identity(&client).await {
            Ok(info) if !info.id.is_empty() => Some(info.id),
            Ok(_) => {
                tracing::warn!("server reports no id; storage stays keyed to the profile");
                None
            }
            Err(error) => {
                tracing::warn!(%error, "could not read the server's identity");
                None
            }
        };

        if let (false, Some(found)) = (expected.is_empty(), identity.as_deref())
            && found != expected
        {
            last_error = "that address answers as a different server".to_string();
            tracing::warn!(
                url = %endpoint.url,
                expected = %expected,
                found = %found,
                "refusing an endpoint that is not this server"
            );
            continue;
        }

        if i > 0 {
            tracing::info!(url = %endpoint.url, "primary address unreachable; using a fallback");
            state.toast(
                format!("connected via fallback address ({})", endpoint.url),
                ToastLevel::Info,
            );
        }
        state.active_endpoint = endpoint.url.clone();
        return Some(Reached { client, identity });
    }

    fail(state, &last_error);
    None
}

/// The namespace for everything stored per server: the rolling cache, permanent downloads, the
/// session snapshot, the scrobble buffer.
///
/// This is the Emby installation's own GUID, **not** the local profile id. Those are different
/// questions — a profile is one *address*, and one server can have several (a LAN `http://` one and
/// an external `https://` one behind a proxy). Keying storage on the profile gave each address its
/// own cache and its own download tree, so the same music was fetched and kept twice
/// (`docs/12-decisions.md`).
///
/// Learned once, on the first successful connection, and written back to the profile — after which
/// it is known offline too, which matters because offline is exactly when the cache has to be
/// found. Falls back to the profile id only until that first connection succeeds.
///
/// A profile that comes back with a *different* id than the one recorded is not the server it was
/// before (a reused address, a restored backup, a copied config). Its storage moves to the new id
/// rather than being merged into the old namespace, and it says so in the log.
fn storage_id(state: &mut AppState, identity: Option<String>) -> ServerId {
    let profile_id = state.config.active_server.clone();
    let stored = state
        .config
        .servers
        .iter()
        .find(|s| s.id == profile_id)
        .map(|s| s.server_id.clone())
        .unwrap_or_default();
    debug_assert_eq!(
        state.config.storage_server_id(),
        if stored.is_empty() {
            profile_id.clone()
        } else {
            stored.clone()
        },
        "this must agree with `Config::storage_server_id`, which every other reader uses"
    );

    let Some(discovered) = identity.filter(|id| !id.is_empty()) else {
        return ServerId::from(if stored.is_empty() {
            profile_id
        } else {
            stored
        });
    };

    if !stored.is_empty() && stored != discovered {
        tracing::warn!(
            previous = %stored,
            now = %discovered,
            "this profile now reaches a different server; its storage moves with it"
        );
    }
    if stored != discovered
        && let Some(server) = state.config.servers.iter_mut().find(|s| s.id == profile_id)
    {
        server.server_id = discovered.clone();
        state.touch();
    }
    ServerId::from(discovered)
}

fn fail(state: &mut AppState, message: &str) {
    state.nav.active_tab = Tab::Settings;
    state.connectivity = Connectivity::Offline;
    state.toast(message, ToastLevel::Warning);
}

/// Opens the rolling cache and builds its worker config — a nicety, never a hard requirement for
/// playback (`docs/06-cache-and-offline.md` §9's own "disk full: stop caching, keep playing" rule
/// extends naturally to "cache root unusable at all: same"): a failure to open it is logged, not
/// fatal, and leaves the caller to run with `None` (a stub cache worker; `EnsureCached` then
/// silently gets no reply, which the reducer already treats as "playback proceeds from the
/// network" either way). `11-03`: factored out of `main`'s own startup sequence so `runtime`'s
/// server-switch reconnect can call it a second time with a different `server` — each call opens
/// its own fresh `RollingCache` over the same `cache_root` rather than reusing the previous
/// connection's instance; the manifest they share is a single, not per-server, LRU index, so a
/// switch's own brief overlap (the outgoing cache worker finishing whatever was already queued
/// while the new one starts) could in principle let one instance's manifest write clobber the
/// other's — accepted as a narrow, low-severity race (stale LRU bookkeeping, never a lost or
/// corrupted audio file, since the two instances' actual track writes are already
/// path-disjoint by server id) rather than threading a shared `Arc<Mutex<RollingCache>>` across
/// the whole reconnect path just to close it (`docs/12-decisions.md`).
pub fn open_cache(
    paths: &Paths,
    server: loxia_core::model::ServerId,
    target_codec: loxia_core::config::TargetCodec,
    prefetch_on_play: bool,
) -> Option<(
    Arc<tokio::sync::Mutex<loxia_cache::lru::RollingCache>>,
    crate::workers::CacheWorkerConfig,
)> {
    match loxia_cache::lru::RollingCache::open(paths.cache_root()) {
        Ok(mut cache) => {
            // Startup reconciliation (`docs/06-cache-and-offline.md` §4) was written, tested and
            // never called: nothing in the binary invoked it, so stale `.part` files and files with
            // no manifest row accumulated forever. It also flushes the index, which is how an
            // empty-but-clean cache stops reporting a stale size.
            match cache.reconcile() {
                Ok(report) => tracing::debug!(?report, "cache reconciled"),
                Err(e) => tracing::warn!(error = %e, "cache reconciliation failed"),
            }
            Some((
                Arc::new(tokio::sync::Mutex::new(cache)),
                crate::workers::CacheWorkerConfig {
                    server,
                    target_codec,
                    prefetch_on_play,
                    paths: paths.clone(),
                },
            ))
        }
        Err(e) => {
            tracing::warn!(error = %e, "cache unavailable; running without it");
            None
        }
    }
}

/// Seeds the Artists column so it starts loading in the background before the user ever switches
/// to that tab, and returns the fetch effect for the caller to send on `Workers::network` —
/// mirrors `reducer::nav::seed_column_for_tab`'s own `Artists` case, but runs before the runtime
/// loop (and so before any `Action`/`reducer::apply` round trip) exists. Still used by a live
/// in-app server switch (`runtime::reconnect_server`, `11-03`) — unlike startup, that path always
/// lands back on whatever tab the user was already on, which is never the concern this function
/// exists for.
pub fn seed_artists_column(state: &mut AppState) -> Effect {
    let mut column = Column::new(ColumnKind::Artists, "Artists");
    column.load = LoadState::Loading;
    state.nav.per_tab_stacks.insert(Tab::Artists, vec![column]);
    Effect::Net(NetEffect::FetchColumn {
        tab: Tab::Artists,
        depth: 0,
        kind: ColumnKind::Artists,
        page: 0,
    })
}

/// `11-06`: generalizes `seed_artists_column` above from "always the Artists tab" to "whichever
/// tab is actually active" — session restore (unlike the plain startup default of `Tab::NowPlaying`)
/// can leave any Miller-column tab active, and that tab is the one that should start loading in
/// the background, not always Artists. Mirrors `reducer::nav::seed_column_for_tab`'s own
/// `Tab -> (ColumnKind, title)` mapping (duplicated here rather than exposed from `loxia-core`,
/// since that function is private and this is the only caller outside the reducer itself — the
/// same narrow, accepted duplication `seed_artists_column` above already established). Returns
/// `None` — seeding nothing — for a tab with no Miller column of its own (`NowPlaying`/
/// `Favourites`/`Search`/`Settings`) or one that already has columns (a vacant-entry check,
/// exactly `reducer::nav::set_tab`'s own "a tab visited for the first time gets a seed column;
/// a previously-visited tab's stack is preserved untouched" — here, "the restored tab has no
/// columns" is what decides "first time").
pub fn seed_active_tab_column(state: &mut AppState) -> Option<Effect> {
    let tab = state.nav.active_tab;
    let (kind, title) = match tab {
        Tab::Artists => (ColumnKind::Artists, "Artists"),
        Tab::AlbumArtists => (ColumnKind::AlbumArtists, "Album Artists"),
        Tab::Albums => (ColumnKind::Albums { of_artist: None }, "Albums"),
        Tab::Genres => (ColumnKind::Genres, "Genres"),
        Tab::Folders => (ColumnKind::Folders { of_parent: None }, "Folders"),
        Tab::Playlists => (ColumnKind::Playlists, "Playlists"),
        Tab::NowPlaying | Tab::Favourites | Tab::Search | Tab::Settings => return None,
    };
    let std::collections::hash_map::Entry::Vacant(entry) = state.nav.per_tab_stacks.entry(tab)
    else {
        return None;
    };
    let mut column = Column::new(kind.clone(), title);
    column.load = LoadState::Loading;
    entry.insert(vec![column]);
    Some(Effect::Net(NetEffect::FetchColumn {
        tab,
        depth: 0,
        kind,
        page: 0,
    }))
}

/// `11-06`: history (unconditional) and the session snapshot (gated on `ui.restore_session`),
/// applied **before** `connect` — the UI is usable immediately, and a slow or failed connection
/// still leaves the user with their queue rather than an empty screen
/// (`docs/06-cache-and-offline.md` §8). Neither half needs a live connection at all: `reducer::
/// session::restore` only ever reads/writes `AppState` and `state.config`, never the network
/// (`docs/12-decisions.md`). Any effects the restore itself produces (`Effect::Audio(Load)` for
/// `ui.restore_autoplay`) are returned for the caller to dispatch once `Workers` exists — this
/// function itself needs none.
pub fn restore_session(state: &mut AppState, paths: &Paths) -> Vec<Effect> {
    state.history = loxia_cache::session::load_history(paths).unwrap_or_default();
    if !state.config.ui.restore_session {
        return Vec::new();
    }
    let Ok(Some(snapshot)) = loxia_cache::session::load(paths) else {
        return Vec::new();
    };
    loxia_core::reducer::apply(
        state,
        loxia_core::action::Action::System(loxia_core::action::SystemEvent::SessionRestored(
            Box::new(snapshot),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::config::ServerConfig;
    use loxia_core::paths::FixedDirs;

    fn paths_in(dir: &Path) -> Paths {
        let dirs = FixedDirs::new(
            Some(dir.join("config")),
            Some(dir.join("cache")),
            Some(dir.join("data")),
            Some(dir.join("state")),
        );
        Paths::resolve(&dirs, &loxia_core::config::CacheConfig::default()).unwrap()
    }

    #[test]
    fn first_run_creates_config_and_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        let (cfg, warnings) = load(&paths);
        assert_eq!(cfg, Config::default());
        assert!(paths.config_file().exists());
        assert!(warnings.iter().any(|w| w.severity == Severity::Info));
    }

    #[test]
    fn first_run_file_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        load(&paths);
        let text = fs::read_to_string(paths.config_file()).unwrap();
        let (cfg, _warnings) = loxia_core::config::parse_and_validate(&text).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn corrupt_config_is_quarantined_and_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        fs::create_dir_all(paths.config_file().parent().unwrap()).unwrap();
        fs::write(paths.config_file(), "not valid = [[[ toml").unwrap();

        let (cfg, warnings) = load(&paths);
        assert_eq!(cfg, Config::default());
        let bad_path = {
            let mut p = paths.config_file().as_os_str().to_os_string();
            p.push(".bad");
            std::path::PathBuf::from(p)
        };
        assert!(bad_path.exists());
        assert!(warnings.iter().any(|w| w.severity == Severity::Warning));
    }

    #[test]
    fn save_is_atomic() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        save(&paths, &Config::default()).unwrap();
        let mut tmp_path = paths.config_file().as_os_str().to_os_string();
        tmp_path.push(".tmp");
        assert!(!Path::new(&tmp_path).exists());
        assert!(paths.config_file().exists());
    }

    #[test]
    #[cfg(unix)]
    fn save_sets_0600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        save(&paths, &Config::default()).unwrap();
        let mode = fs::metadata(paths.config_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    #[cfg(unix)]
    fn world_readable_config_warns() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        save(&paths, &Config::default()).unwrap();
        fs::set_permissions(paths.config_file(), fs::Permissions::from_mode(0o644)).unwrap();

        let (_cfg, warnings) = load(&paths);
        assert!(
            warnings
                .iter()
                .any(|w| w.message.contains("readable by other users"))
        );
    }

    #[test]
    fn save_then_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        let mut cfg = Config::default();
        // A dangling active_server with no matching entry would be legitimately cleared by
        // validate() on the way back in, so give it a real server to round-trip cleanly.
        cfg.servers.push(ServerConfig {
            id: "my-server".to_string(),
            url: "https://example.com".to_string(),
            device_id: "11111111-1111-4111-8111-111111111111".to_string(),
            ..ServerConfig::default()
        });
        cfg.active_server = "my-server".to_string();
        save(&paths, &cfg).unwrap();

        let (loaded, _warnings) = load(&paths);
        assert_eq!(loaded, cfg);
    }

    fn server_cfg(url: &str) -> ServerConfig {
        ServerConfig {
            id: "srv".to_string(),
            name: "Test".to_string(),
            url: url.to_string(),
            user_id: "user-1".to_string(),
            access_token: "bad-token".to_string(),
            device_id: "dev".to_string(),
            ..ServerConfig::default()
        }
    }

    fn state_with_server(cfg: ServerConfig) -> AppState {
        let config = Config {
            active_server: cfg.id.clone(),
            servers: vec![cfg],
            ..Config::default()
        };
        AppState {
            config,
            ..AppState::default()
        }
    }

    /// A server that answers `validate_token` and reports `id` — the two calls `reach_server` makes
    /// before it will use an endpoint.
    async fn mock_identity(server: &wiremock::MockServer, id: &str) {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/emby/Users/user-1"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "Id": "user-1",
                    "Name": "test",
                })),
            )
            .mount(server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/System/Info/Public"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "ServerName": "Sanji",
                    "Version": "4.9.5.0",
                    "Id": id,
                })),
            )
            .mount(server)
            .await;
    }

    /// The point of a fallback address: the LAN one works at home and the external one works away,
    /// and which applies is a fact about where the machine is, not something a user should have to
    /// reconfigure (`docs/12-decisions.md`).
    #[tokio::test]
    async fn an_unreachable_primary_falls_through_to_the_next_address() {
        let good = wiremock::MockServer::start().await;
        mock_identity(&good, "emby-guid").await;

        let mut cfg = server_cfg("http://127.0.0.1:1"); // nothing listens there
        cfg.server_id = "emby-guid".to_string();
        cfg.fallbacks = vec![loxia_core::config::ServerEndpoint {
            url: good.uri(),
            custom_headers: Default::default(),
        }];
        let mut state = state_with_server(cfg.clone());

        let reached = reach_server(&mut state, &cfg)
            .await
            .expect("the fallback must be used");
        assert_eq!(reached.identity.as_deref(), Some("emby-guid"));
        assert_eq!(state.active_endpoint, good.uri());
        assert!(
            state.toasts.iter().any(|t| t.message.contains("fallback")),
            "silently changing which address is in use would hide a real network problem"
        );
    }

    /// An address that answers as a *different* Emby must never be used: everything stored per
    /// server is keyed on that id, so accepting it would attach this profile's cache, downloads and
    /// session to somebody else's library.
    #[tokio::test]
    async fn an_endpoint_that_is_a_different_server_is_refused() {
        let impostor = wiremock::MockServer::start().await;
        mock_identity(&impostor, "somebody-elses-guid").await;

        let mut cfg = server_cfg(&impostor.uri());
        cfg.server_id = "emby-guid".to_string();
        let mut state = state_with_server(cfg.clone());

        assert!(reach_server(&mut state, &cfg).await.is_none());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("different server"))
        );
    }

    /// A profile that has never connected has nothing to check against, so the first address that
    /// answers is taken at its word — and its id recorded, which is what makes every *later*
    /// connection checkable.
    #[tokio::test]
    async fn a_profile_with_no_recorded_identity_accepts_the_first_answer() {
        let server = wiremock::MockServer::start().await;
        mock_identity(&server, "fresh-guid").await;

        let cfg = server_cfg(&server.uri());
        assert!(cfg.server_id.is_empty(), "fixture assumption");
        let mut state = state_with_server(cfg.clone());

        let reached = reach_server(&mut state, &cfg).await.expect("accepted");
        assert_eq!(reached.identity.as_deref(), Some("fresh-guid"));
    }

    /// Everything stored per server is namespaced by the **server's** own id, not the profile's —
    /// so a LAN profile and an external one pointing at the same Emby share one cache and one
    /// download tree instead of keeping two copies (`docs/12-decisions.md`).
    #[tokio::test]
    async fn storage_follows_the_server_identity_and_is_remembered() {
        let server = wiremock::MockServer::start().await;
        mock_identity(&server, "d7b2231244f0465cb3812bd64a38c447").await;

        let mut state = state_with_server(server_cfg(&server.uri()));
        let cfg = state.config.servers[0].clone();
        let Some(reached) = reach_server(&mut state, &cfg).await else {
            panic!("the primary address must be usable");
        };

        let id = storage_id(&mut state, reached.identity);
        assert_eq!(id.as_str(), "d7b2231244f0465cb3812bd64a38c447");
        assert_eq!(
            state.config.servers[0].server_id, "d7b2231244f0465cb3812bd64a38c447",
            "it must be written back, so it is known offline too — when the cache matters most"
        );
    }

    /// A server that cannot be reached, or is too old to report an id, must still start: the
    /// profile id stands in until a connection succeeds, and a *previously learned* id always
    /// wins over it.
    #[tokio::test]
    async fn storage_falls_back_without_losing_a_known_identity() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/System/Info/Public"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let mut state = state_with_server(server_cfg(&server.uri()));
        assert_eq!(storage_id(&mut state, None).as_str(), "srv");

        state.config.servers[0].server_id = "already-known".to_string();
        assert_eq!(
            storage_id(&mut state, None).as_str(),
            "already-known",
            "an unreachable server must not orphan the cache it already has"
        );
    }

    #[tokio::test]
    async fn invalid_token_opens_settings_with_toast() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/emby/Users/user-1"))
            .respond_with(wiremock::ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let mut state = state_with_server(server_cfg(&server.uri()));
        let connected = connect(&mut state).await;

        assert!(connected.is_none());
        assert_eq!(state.nav.active_tab, Tab::Settings);
        assert_eq!(state.connectivity, Connectivity::Offline);
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.level == ToastLevel::Warning && t.message.contains("could not connect"))
        );
    }

    #[tokio::test]
    async fn missing_server_config_opens_settings_with_toast() {
        let mut state = AppState::default();
        let connected = connect(&mut state).await;

        assert!(connected.is_none());
        assert_eq!(state.nav.active_tab, Tab::Settings);
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("no server configured"))
        );
    }

    /// A second music library used to be invisible everywhere but Folders: every browsing list was
    /// pinned to `music_libraries()[0]`, so a user with 2145 artists in one library and 2531 in
    /// another only ever saw the first (`docs/12-decisions.md`). Several libraries now mean "no
    /// `ParentId`", which Emby answers with the de-duplicated union.
    #[test]
    fn several_music_libraries_scope_to_the_whole_server() {
        assert_eq!(
            library_scope(vec![ItemId::from("lib-1"), ItemId::from("lib-2")]),
            None
        );
        assert_eq!(
            library_scope(vec![ItemId::from("lib-1")]),
            Some(ItemId::from("lib-1")),
            "one library stays explicitly scoped, so non-music audio can't leak in"
        );
    }

    #[tokio::test]
    async fn successful_connect_scopes_to_a_lone_music_library() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/emby/Users/user-1"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "Id": "user-1", "Name": "Test User" })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/emby/Users/user-1/Views"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "Items": [
                        { "Id": "lib-1", "Name": "Music", "CollectionType": "music" },
                    ]
                })),
            )
            .mount(&server)
            .await;

        let mut state = state_with_server(server_cfg(&server.uri()));
        let connected = connect(&mut state).await.expect("should connect");

        assert_eq!(
            connected.library.as_ref().map(ItemId::as_str),
            Some("lib-1"),
            "a server with a single music library stays scoped to it"
        );
        assert_eq!(state.connectivity, Connectivity::Online);
        assert!(state.server.user_id.is_some());

        let effect = seed_artists_column(&mut state);
        assert_eq!(
            effect,
            Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Artists,
                depth: 0,
                kind: ColumnKind::Artists,
                page: 0,
            })
        );
        assert!(state.nav.per_tab_stacks.contains_key(&Tab::Artists));
    }

    fn sample_snapshot(server_id: &str) -> loxia_core::state::SessionSnapshot {
        let a = loxia_core::test_support::fixtures::artist("Boy Harsher");
        let alb = loxia_core::test_support::fixtures::album("Care", 2019, &a);
        let t = loxia_core::test_support::fixtures::track("Motion", 1, &alb, &[&a]);
        let mut queue = loxia_core::state::queue::QueueState::default();
        queue.entries.push(loxia_core::state::queue::QueueEntry {
            entry_id: loxia_core::model::QueueEntryId(0),
            track: t,
            source: loxia_core::state::queue::QueueSource::Manual,
            availability: loxia_core::state::queue::Availability::Remote,
        });
        queue.play_order.push(0);
        loxia_core::state::SessionSnapshot {
            schema_version: loxia_core::state::SESSION_SCHEMA_VERSION,
            server_id: loxia_core::model::ServerId::from(server_id),
            queue,
            position_secs: 84.0,
            active_tab: Tab::Playlists,
            zen_mode: false,
            volume: 60,
            quality_profile: loxia_core::config::QualityProfile::Direct,
            eq: loxia_core::state::player::EqState::default(),
            saved_at: loxia_core::test_support::fixtures::fixed_epoch(),
        }
    }

    #[test]
    fn restore_disabled_by_config() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        let mut state = state_with_server(server_cfg("http://example.invalid"));
        state.config.ui.restore_session = false;
        loxia_cache::session::save(&paths, &sample_snapshot(&state.config.active_server)).unwrap();

        let effects = restore_session(&mut state, &paths);
        assert!(effects.is_empty());
        assert!(state.queue.entries.is_empty());
    }

    #[test]
    fn restore_happens_before_connect() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        let mut state = state_with_server(server_cfg("http://example.invalid"));
        loxia_cache::session::save(&paths, &sample_snapshot(&state.config.active_server)).unwrap();

        // `connect` is never called here at all — proof that restoring the queue/tab/position has
        // no dependency on a server connection ever having succeeded, which is exactly what makes
        // it safe for `main` to run this before `connect`.
        restore_session(&mut state, &paths);
        assert_eq!(state.queue.entries.len(), 1);
        assert!(state.server.user_id.is_none(), "connect() was never called");
    }

    #[tokio::test]
    async fn failed_connection_still_leaves_queue_restored() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        let mut state = state_with_server(server_cfg("http://127.0.0.1:1"));
        loxia_cache::session::save(&paths, &sample_snapshot(&state.config.active_server)).unwrap();

        restore_session(&mut state, &paths);
        assert_eq!(state.queue.entries.len(), 1);

        let connected = connect(&mut state).await;
        assert!(connected.is_none());
        assert_eq!(state.connectivity, Connectivity::Offline);
        assert_eq!(
            state.queue.entries.len(),
            1,
            "a failed connect must not clear the already-restored queue"
        );
    }

    #[test]
    fn seed_active_tab_column_seeds_the_restored_tab_not_always_artists() {
        let mut state = AppState::default();
        state.nav.active_tab = Tab::Playlists;

        let effect = seed_active_tab_column(&mut state).expect("Playlists needs a seed column");
        assert_eq!(
            effect,
            Effect::Net(NetEffect::FetchColumn {
                tab: Tab::Playlists,
                depth: 0,
                kind: ColumnKind::Playlists,
                page: 0,
            })
        );
        assert!(state.nav.per_tab_stacks.contains_key(&Tab::Playlists));
    }

    #[test]
    fn seed_active_tab_column_is_none_for_tabs_with_no_column() {
        // `AppState::default()`'s own active tab, `NowPlaying`, has no Miller column at all.
        let mut state = AppState::default();
        assert!(seed_active_tab_column(&mut state).is_none());
    }

    #[test]
    fn seed_skipped_when_restored_tab_has_columns() {
        let mut state = AppState::default();
        state.nav.active_tab = Tab::Playlists;
        // As if something else already seeded it (or the restored session file, in principle,
        // ever grows a column-carrying field of its own) — either way, an existing entry must
        // never be replaced.
        state.nav.per_tab_stacks.insert(
            Tab::Playlists,
            vec![Column::new(ColumnKind::Playlists, "Playlists")],
        );

        assert!(seed_active_tab_column(&mut state).is_none());
    }
}
