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
  machine's local timezone. Both the header clock (`widgets/header.rs::format_clock`) and
  the Now Playing history timestamps (`views/now_playing.rs`) read the host's *system*
  timezone via `loxia_core::local_hour_minute`, so the same fixed test-fixture clock used
  to render a different hour depending on which machine last regenerated the snapshot. A
  new workspace-level `.cargo/config.toml` now pins `TZ=UTC` for every process cargo itself
  launches (build scripts, `cargo test`, `cargo run`), which is deterministic and requires
  no change to the production code path — an installed `loxia-player` binary launched
  directly still resolves and shows the user's real local time.
  - All five previously-failing snapshots were **stale fixtures, not rendering
    regressions**: in every case the only diff was the HH:MM string itself (by exactly the
    offset between the capturing machine's timezone and UTC); buffer layout, borders, and
    styles were byte-identical.
    - `render::tests::layout_snapshot_80x24` — stale fixture (header clock `01:00` → `00:00`
      under UTC).
    - `render::tests::layout_snapshot_120x30` — stale fixture (same header clock).
    - `render::tests::layout_snapshot_200x50` — stale fixture (same header clock).
    - `views::now_playing::tests::now_playing_snapshot_history` — stale fixture (the three
      history rows shifted by exactly one hour, e.g. `23:15` → `22:15`).
    - `widgets::header::tests::header_snapshot_offline_with_downloads` — stale fixture
      (`23:13` → `22:13`).
  - The `header_snapshot_offline_with_downloads` fixture has been regenerated in this
    change. The three `render::tests::layout_snapshot_*` fixtures and
    `now_playing_snapshot_history` still need a `cargo insta accept` run against this
    `.cargo/config.toml` to pick up their corresponding one-hour shift before `cargo test -p
    loxia-tui --lib` is fully green.
