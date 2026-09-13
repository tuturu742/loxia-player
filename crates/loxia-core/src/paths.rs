//! Per-OS path resolution, pure function of an injected provider.
//!
//! Layout mirrors `docs/06-cache-and-offline.md` §1. `#[cfg(target_os = ...)]` is confined to this
//! module and to `loxia-audio::device` (`docs/01-architecture.md` §7) — the one piece of genuinely
//! platform-specific behaviour here is that the Windows cache root nests one level deeper
//! (`%LOCALAPPDATA%\loxia-player\cache`, not `%LOCALAPPDATA%\loxia`), since `%LOCALAPPDATA%` is shared
//! with other local app data on that platform in a way `~/.cache` and `~/Library/Caches` are not.

use std::path::{Path, PathBuf};

use crate::config::CacheConfig;

/// The directory name loxia claims under every platform base directory, and the stem of its log
/// file. Deliberately the **binary's** name rather than the project's: `loxia` is a common enough
/// word to collide with unrelated tools on `PATH`, so the shipped command is `loxia-player` and its
/// on-disk footprint matches it (`docs/12-decisions.md`).
///
/// A single constant because a half-renamed identity is worse than either name: config read from
/// one directory and cache written to another would look like data loss.
pub const APP_DIR: &str = "loxia-player";

/// The four platform base directories loxia needs. Implemented by `SystemDirs` (backed by the
/// `dirs` crate) for real use and by `FixedDirs` for tests — this is what makes path resolution
/// unit-testable for all three platforms from a single Linux CI runner (except the one
/// `#[cfg(target_os = "windows")]` branch, which only runs on the real Windows CI job).
pub trait BaseDirs {
    fn config_dir(&self) -> Option<PathBuf>;
    fn cache_dir(&self) -> Option<PathBuf>;
    fn data_dir(&self) -> Option<PathBuf>;
    fn state_dir(&self) -> Option<PathBuf>;
}

pub struct SystemDirs;

impl BaseDirs for SystemDirs {
    fn config_dir(&self) -> Option<PathBuf> {
        dirs::config_dir()
    }
    fn cache_dir(&self) -> Option<PathBuf> {
        dirs::cache_dir()
    }
    fn data_dir(&self) -> Option<PathBuf> {
        dirs::data_dir()
    }
    fn state_dir(&self) -> Option<PathBuf> {
        dirs::state_dir()
    }
}

/// A test double constructed from four explicit (possibly absent) paths.
pub struct FixedDirs {
    pub config: Option<PathBuf>,
    pub cache: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub state: Option<PathBuf>,
}

impl FixedDirs {
    pub fn new(
        config: Option<PathBuf>,
        cache: Option<PathBuf>,
        data: Option<PathBuf>,
        state: Option<PathBuf>,
    ) -> Self {
        FixedDirs {
            config,
            cache,
            data,
            state,
        }
    }
}

impl BaseDirs for FixedDirs {
    fn config_dir(&self) -> Option<PathBuf> {
        self.config.clone()
    }
    fn cache_dir(&self) -> Option<PathBuf> {
        self.cache.clone()
    }
    fn data_dir(&self) -> Option<PathBuf> {
        self.data.clone()
    }
    fn state_dir(&self) -> Option<PathBuf> {
        self.state.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("required base directory is unavailable on this system")]
    NoBaseDir,
    #[error("path override must be absolute, got a relative path: {0}")]
    Relative(String),
}

/// Every path loxia uses, fully resolved. Construction performs **no I/O** — directory creation
/// belongs to the callers in `loxia-cache` and the binary, which is what keeps this testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    config_dir: PathBuf,
    cache_root: PathBuf,
    data_root: PathBuf,
    state_root: PathBuf,
}

impl Paths {
    pub fn resolve(dirs: &impl BaseDirs, cfg: &CacheConfig) -> Result<Paths, PathError> {
        let config_dir = dirs.config_dir().ok_or(PathError::NoBaseDir)?.join(APP_DIR);

        let cache_base = resolve_override(&cfg.cache_dir, || dirs.cache_dir())?;
        let cache_root = default_cache_root(cache_base);

        let data_root = resolve_override(&cfg.download_dir, || dirs.data_dir())?;

        // state_dir() is None on macOS and Windows — fall back to the data root, which is what
        // makes session.json/history.json/scrobbles.json/loxia-player.log live alongside downloads/
        // there instead of erroring.
        let state_root = match dirs.state_dir() {
            Some(base) => base.join(APP_DIR),
            None => data_root.clone(),
        };

        Ok(Paths {
            config_dir,
            cache_root,
            data_root,
            state_root,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    pub fn tracks_dir(&self) -> PathBuf {
        self.cache_root.join("tracks")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.cache_root.join("images")
    }

    pub fn cache_index_file(&self) -> PathBuf {
        self.cache_root.join("cache_index.json")
    }

    pub fn downloads_root(&self) -> &Path {
        &self.data_root
    }

    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    pub fn session_file(&self) -> PathBuf {
        self.state_root.join("session.json")
    }

    /// `11-03`: "persist the outgoing session under its own server id" — a switch away from
    /// server `server` must not clobber `session_file()`'s single slot (which the *newly active*
    /// server's own restore-on-reconnect reads), so each server gets its own file instead.
    pub fn session_file_for(&self, server: &crate::model::ServerId) -> PathBuf {
        self.state_root.join(format!("session-{server}.json"))
    }

    pub fn history_file(&self) -> PathBuf {
        self.state_root.join("history.json")
    }

    pub fn scrobbles_file(&self) -> PathBuf {
        self.state_root.join("scrobbles.json")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.state_root.clone()
    }
}

/// `"auto"` uses the platform base (via `fallback`); anything else is an absolute-path override.
/// A relative override is rejected rather than silently resolved against the current working
/// directory, which would scatter caches wherever the user happened to launch loxia from.
fn resolve_override(
    value: &str,
    fallback: impl FnOnce() -> Option<PathBuf>,
) -> Result<PathBuf, PathError> {
    if value == "auto" {
        return fallback()
            .ok_or(PathError::NoBaseDir)
            .map(|p| p.join(APP_DIR));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(PathError::Relative(value.to_string()));
    }
    Ok(path)
}

#[cfg(target_os = "windows")]
fn default_cache_root(base: PathBuf) -> PathBuf {
    base.join("cache")
}

#[cfg(not(target_os = "windows"))]
fn default_cache_root(base: PathBuf) -> PathBuf {
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linux_dirs() -> FixedDirs {
        FixedDirs::new(
            Some(PathBuf::from("/home/u/.config")),
            Some(PathBuf::from("/home/u/.cache")),
            Some(PathBuf::from("/home/u/.local/share")),
            Some(PathBuf::from("/home/u/.local/state")),
        )
    }

    #[test]
    fn linux_layout_matches_spec() {
        let dirs = linux_dirs();
        let cfg = CacheConfig::default();
        let paths = Paths::resolve(&dirs, &cfg).unwrap();

        assert_eq!(
            paths.config_file(),
            PathBuf::from("/home/u/.config/loxia-player/config.toml")
        );
        assert_eq!(paths.cache_root(), Path::new("/home/u/.cache/loxia-player"));
        assert_eq!(
            paths.tracks_dir(),
            PathBuf::from("/home/u/.cache/loxia-player/tracks")
        );
        assert_eq!(
            paths.images_dir(),
            PathBuf::from("/home/u/.cache/loxia-player/images")
        );
        assert_eq!(
            paths.cache_index_file(),
            PathBuf::from("/home/u/.cache/loxia-player/cache_index.json")
        );
        assert_eq!(
            paths.downloads_root(),
            Path::new("/home/u/.local/share/loxia-player")
        );
        assert_eq!(
            paths.state_root(),
            Path::new("/home/u/.local/state/loxia-player")
        );
        assert_eq!(
            paths.session_file(),
            PathBuf::from("/home/u/.local/state/loxia-player/session.json")
        );
        assert_eq!(
            paths.history_file(),
            PathBuf::from("/home/u/.local/state/loxia-player/history.json")
        );
        assert_eq!(
            paths.scrobbles_file(),
            PathBuf::from("/home/u/.local/state/loxia-player/scrobbles.json")
        );
        assert_eq!(
            paths.log_dir(),
            PathBuf::from("/home/u/.local/state/loxia-player")
        );
    }

    /// `state_dir()` is `None` on macOS; state must fall back to the data root.
    #[test]
    fn macos_state_falls_back_to_data_dir() {
        let dirs = FixedDirs::new(
            Some(PathBuf::from("/Users/u/Library/Application Support")),
            Some(PathBuf::from("/Users/u/Library/Caches")),
            Some(PathBuf::from("/Users/u/Library/Application Support")),
            None,
        );
        let paths = Paths::resolve(&dirs, &CacheConfig::default()).unwrap();
        assert_eq!(paths.state_root(), paths.downloads_root());
    }

    /// `state_dir()` is `None` on Windows too; same fallback, different base paths.
    #[test]
    fn windows_state_falls_back_to_data_dir() {
        let dirs = FixedDirs::new(
            Some(PathBuf::from(r"C:\Users\u\AppData\Roaming")),
            Some(PathBuf::from(r"C:\Users\u\AppData\Local")),
            Some(PathBuf::from(r"C:\Users\u\AppData\Roaming")),
            None,
        );
        let paths = Paths::resolve(&dirs, &CacheConfig::default()).unwrap();
        assert_eq!(paths.state_root(), paths.downloads_root());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_cache_root_nests_under_an_extra_cache_segment() {
        let dirs = FixedDirs::new(
            Some(PathBuf::from(r"C:\Users\u\AppData\Roaming")),
            Some(PathBuf::from(r"C:\Users\u\AppData\Local")),
            Some(PathBuf::from(r"C:\Users\u\AppData\Roaming")),
            None,
        );
        let paths = Paths::resolve(&dirs, &CacheConfig::default()).unwrap();
        assert_eq!(
            paths.cache_root(),
            Path::new(r"C:\Users\u\AppData\Local\loxia\cache")
        );
    }

    #[test]
    fn auto_resolves_to_platform_base() {
        let dirs = linux_dirs();
        let cfg = CacheConfig {
            cache_dir: "auto".to_string(),
            ..CacheConfig::default()
        };
        let paths = Paths::resolve(&dirs, &cfg).unwrap();
        assert_eq!(paths.cache_root(), Path::new("/home/u/.cache/loxia-player"));
    }

    #[test]
    fn absolute_override_is_honoured() {
        let dirs = linux_dirs();
        let cfg = CacheConfig {
            cache_dir: "/mnt/bigdisk/loxia-cache".to_string(),
            ..CacheConfig::default()
        };
        let paths = Paths::resolve(&dirs, &cfg).unwrap();
        assert_eq!(paths.cache_root(), Path::new("/mnt/bigdisk/loxia-cache"));
    }

    #[test]
    fn relative_override_is_rejected() {
        let dirs = linux_dirs();
        let cfg = CacheConfig {
            cache_dir: "relative/path".to_string(),
            ..CacheConfig::default()
        };
        let err = Paths::resolve(&dirs, &cfg).unwrap_err();
        assert_eq!(err, PathError::Relative("relative/path".to_string()));
    }

    #[test]
    fn missing_base_dir_is_an_error() {
        let dirs = FixedDirs::new(
            None,
            Some(PathBuf::from("/x")),
            Some(PathBuf::from("/y")),
            None,
        );
        let err = Paths::resolve(&dirs, &CacheConfig::default()).unwrap_err();
        assert_eq!(err, PathError::NoBaseDir);
    }

    #[test]
    fn resolve_performs_no_io() {
        // These paths do not exist on disk; resolve() must still succeed since it never touches
        // the filesystem.
        let dirs = FixedDirs::new(
            Some(PathBuf::from("/does/not/exist/config")),
            Some(PathBuf::from("/does/not/exist/cache")),
            Some(PathBuf::from("/does/not/exist/data")),
            Some(PathBuf::from("/does/not/exist/state")),
        );
        let paths = Paths::resolve(&dirs, &CacheConfig::default()).unwrap();
        assert_eq!(
            paths.cache_root(),
            Path::new("/does/not/exist/cache/loxia-player")
        );
    }
}
