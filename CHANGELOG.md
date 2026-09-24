# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Workspace scaffolding: the `loxia-core`, `loxia-emby`, `loxia-audio`, `loxia-cache`,
  `loxia-tui`, and `loxia-player` crates wired together under a single Cargo workspace,
  with shared `[workspace.package]` metadata (edition, MSRV, licence) and a pinned
  `rust-toolchain.toml` (`00-01`).
- Locked dependency set: every workspace dependency pinned in `Cargo.toml`, resolved into
  `Cargo.lock`, checked against the `cargo-deny` policy in `deny.toml`, and inventoried in
  `THIRD_PARTY_LICENSES.md` (`00-02`, `00-05`, `12-05`).

### Fixed

- loxia-tui's clock and history-timestamp snapshot tests no longer depend on the host
  machine's local timezone: a workspace-level `.cargo/config.toml` now pins `TZ=UTC` for
  every process cargo itself launches, and the affected fixtures
  (`layout_snapshot_80x24`, `layout_snapshot_120x30`, `layout_snapshot_200x50`,
  `now_playing_snapshot_history`, and `header_snapshot_offline_with_downloads`) were
  regenerated against that pinned timezone.
