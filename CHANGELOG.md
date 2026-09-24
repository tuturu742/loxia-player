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
  launches (build scripts, `cargo test`, `cargo run`). `local_hour_minute` resolves the zone
  via `jiff::tz::TimeZone::system()` on every call (it does not cache the zone at build
  time), and jiff's system-timezone lookup honours `TZ` at process-launch time, so this pin
  is effective for the whole `cargo test` process tree, including `cargo test -p loxia-tui
  --lib`. This requires no change to the production code path — an installed `loxia-player`
  binary launched directly (not through `cargo run`) still resolves and shows the user's
  real local time.
  - `render::tests::layout_snapshot_80x24`, `render::tests::layout_snapshot_120x30`,
    `render::tests::layout_snapshot_200x50`,
    `views::now_playing::tests::now_playing_snapshot_history`, and
    `widgets::header::tests::header_snapshot_offline_with_downloads` are the five fixtures
    affected by the timezone-dependent clock.
  - **Status of this round:** only the header-clock `HH:MM` digits in
    `layout_snapshot_120x30.snap` and `layout_snapshot_200x50.snap` were corrected by hand
    against the values `cargo test` reported for this run (`01:00`); no other byte in either
    file was touched. `layout_snapshot_80x24.snap`,
    `now_playing_snapshot_history.snap`, and `header_snapshot_offline_with_downloads.snap`
    were **not** touched in this round and still need their own single-fixture pass once
    their exact failing-test diff is available — regenerating any of them without that diff
    in hand risks the same corruption this fix set already suffered from once, so they are
    deliberately left alone rather than guessed at.
  - **Why a process-wide `TZ` pin instead of test-level injection:** a per-test seam (making
    `local_hour_minute`'s UTC offset an explicit, test-overridable parameter) was considered,
    since it would leave `cargo run`'s displayed clock untouched. It was rejected for this
    fix set because `local_hour_minute` is a `loxia-core` free function shared by every
    caller that turns a `Timestamp` into a local hour/minute, not just these two widgets;
    giving it a test-injectable time source is a real, separate change to that function's
    signature and every call site, not a one-line fix for a stale-fixture bug. The
    `.cargo/config.toml` pin is scoped to processes cargo itself launches (test binaries,
    build scripts, `cargo run`) and never touches an installed `loxia-player` binary invoked
    directly, so its dev-experience cost is limited to `cargo run` during local development
    — not the shipped product. A follow-up task can introduce the test-level seam and drop
    the `[env]` pin without touching these fixtures again.
