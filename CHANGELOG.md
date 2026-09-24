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
  - **Previous round:** `header_snapshot_offline_with_downloads.snap` was hand-corrected
    against the failing test's own reported output — only its clock digits, `23:13` →
    `22:13` — with no other byte in the file touched.
  - **This round:** no `.snap` file was changed. The next fixture queued for correction,
    `render::tests::layout_snapshot_80x24` (`loxia_tui__render__tests__layout_snapshot_80x24.snap`),
    could not be safely fixed because the copy of that fixture available in this round's
    working context was truncated mid-line (cut off inside the buffer's `styles` list,
    before its closing brackets) — not a full, verbatim copy of the file in the repository.
    Given this task's own governing rule ("a fixture that is short by one line fails exactly
    like a fixture that is wrong"), reconstructing the missing style spans by inference
    instead of by copying them was judged more likely to reintroduce exactly the corruption
    this rework was opened to fix, so no edit was made. The next round should re-read the
    fixture in full from the repository (not from a possibly-truncated context snippet)
    before changing its clock digits.
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
    the `[env]` pin without touching this file's tests.
