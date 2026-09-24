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

<!--
  Per-test classification of the loxia-tui snapshot suite (render::tests::layout_snapshot_*,
  views::now_playing::tests::now_playing_snapshot_history,
  widgets::header::tests::header_snapshot_offline_with_downloads) and the investigation that
  went into diagnosing them belongs in the PR description, not here — these are release notes,
  not an engineering log. See PR_DESCRIPTION.md in this same change for that writeup. Nothing is
  recorded here under `### Fixed` yet because `cargo test -p loxia-tui --lib` has not actually
  been observed to pass 0-failures from this session (see PR_DESCRIPTION.md, "Status"); a
  `### Fixed` line will be added once that run has actually happened, not before.
-->
