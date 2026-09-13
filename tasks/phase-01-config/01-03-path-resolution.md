# 01-03 · Path resolution

**Phase:** 01 — Config · **Agent:** A · **Size:** S
**Prerequisites:** `01-01`
**Reference:** `docs/02-data-model.md` §9, `docs/06-cache-and-offline.md` §1

## Goal
Compute every path loxia uses, per platform, as a **pure function** of an injected directory
provider. Testability here is what makes the Windows CI job meaningful without a Windows dev machine.

## Files
- `crates/loxia-core/src/paths.rs`

## Specification

```
pub trait BaseDirs {
    fn config_dir(&self) -> Option<PathBuf>;
    fn cache_dir(&self)  -> Option<PathBuf>;
    fn data_dir(&self)   -> Option<PathBuf>;
    fn state_dir(&self)  -> Option<PathBuf>;
}

pub struct SystemDirs;              // backed by the `dirs` crate
pub struct FixedDirs { .. }         // test double, constructed from four paths

pub struct Paths { /* resolved, absolute */ }

impl Paths {
    pub fn resolve(dirs: &impl BaseDirs, cfg: &CacheConfig) -> Result<Paths, PathError>;
    pub fn config_file(&self) -> PathBuf;      // <config>/loxia/config.toml
    pub fn cache_root(&self) -> &Path;
    pub fn tracks_dir(&self) -> PathBuf;
    pub fn images_dir(&self) -> PathBuf;
    pub fn downloads_root(&self) -> &Path;
    pub fn state_root(&self) -> &Path;
    pub fn session_file(&self) -> PathBuf;
    pub fn history_file(&self) -> PathBuf;
    pub fn scrobbles_file(&self) -> PathBuf;
    pub fn cache_index_file(&self) -> PathBuf;
    pub fn log_dir(&self) -> PathBuf;
}
```

Resolution rules:
- Every path is under a `loxia-player` subdirectory of the platform base.
- `cfg.cache_dir` / `cfg.download_dir` equal to `"auto"` use the platform base; any other value is
  taken as an absolute path override. A **relative** override is an error (`PathError::Relative`) —
  silently resolving it against the CWD would scatter caches wherever the user happened to launch.
- On platforms where `state_dir()` is `None` (macOS, Windows), state falls back to the data dir.
- A `None` from a required base directory is `PathError::NoBaseDir`.

`resolve` performs **no I/O**. Directory creation belongs to the callers in `loxia-cache` and the
binary, which is what keeps this module unit-testable.

## Acceptance
Tests in `paths.rs`, all using `FixedDirs`:
- `linux_layout_matches_spec` — asserts every accessor against the XDG paths in
  `docs/06-cache-and-offline.md` §1.
- `macos_state_falls_back_to_data_dir`
- `windows_state_falls_back_to_data_dir`
- `auto_resolves_to_platform_base`
- `absolute_override_is_honoured`
- `relative_override_is_rejected`
- `missing_base_dir_is_an_error`
- `resolve_performs_no_io` — run against a `FixedDirs` pointing at a non-existent path; `resolve`
  still succeeds.

## Done when
The global DoD in `tasks/README.md` is satisfied.
