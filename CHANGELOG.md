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
    - `render::tests::layout_snapshot_80x24` — stale fixture (header clock shifted to
      `00:00` under UTC).
    - `render::tests::layout_snapshot_120x30` — stale fixture (same header clock).
    - `render::tests::layout_snapshot_200x50` — stale fixture (same header clock).
    - `views::now_playing::tests::now_playing_snapshot_history` — stale fixture (the three
      history rows shifted by exactly one hour, e.g. `23:15` → `22:15`).
    - `widgets::header::tests::header_snapshot_offline_with_downloads` — stale fixture
      (`23:13` → `22:13`).
  - All five fixtures have been regenerated against the pinned `TZ=UTC` (via `cargo test -p
    loxia-tui --lib` followed by `cargo insta accept`, not hand-edited) and are committed in
    this change: `layout_snapshot_80x24.snap`, `layout_snapshot_120x30.snap`,
    `layout_snapshot_200x50.snap`, `now_playing_snapshot_history.snap`, and
    `header_snapshot_offline_with_downloads.snap`. `cargo test -p loxia-tui --lib` passes
    with 0 failures on this branch, and no `.snap.new`/`.pending-snap` files are committed.
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
