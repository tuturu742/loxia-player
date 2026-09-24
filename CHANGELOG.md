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
  workspace-level `.cargo/config.toml` now pins `TZ=UTC` for every process cargo itself
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
  - **Round 1:** `.cargo/config.toml` added, pinning `TZ=UTC`; the five affected fixtures
    were identified from the pre-pin failure output.
  - **Round 2:** `header_snapshot_offline_with_downloads.snap` was hand-corrected against
    the failing test's own reported output — only its clock digits, `23:13` → `22:13` —
    with no other byte in the file touched. That round left the remaining four fixtures
    untouched on purpose, since the copy of `layout_snapshot_80x24.snap` available in that
    round's working context was truncated mid-line (cut off inside the buffer's `styles`
    list), so it could not be safely edited.
  - **Round 3 (this round):** the build output confirms `header_snapshot_offline_with_downloads`
    now passes and reports the exact remaining diffs:
    - `layout_snapshot_80x24`: `"... [?] │ 01:00"` → `"... [?] │ 00:00"`
    - `layout_snapshot_120x30`: `"... [?] │ 01:00"` → `"... [?] │ 00:00"`
    - `layout_snapshot_200x50`: `"... [?] │ 01:00"` → `"... [?] │ 00:00"`
    - `now_playing_snapshot_history`: `23:15`/`23:14`/`23:13` → `22:15`/`22:14`/`22:13`
      on the three history rows.

    No `.snap` file was changed this round, though. Every one of these four fixtures, as
    supplied in this round's own working context, was cut off mid-line or mid-file before
    reaching the end of its `content` list and/or its `styles` list — not a full, verbatim
    copy of the file in the repository. Given this task's own governing rule ("a fixture
    that is short by one line fails exactly like a fixture that is wrong"), and that these
    are `styles` lists whose exact entries depend on exactly which cells the renderer
    restyles (which this round has no reliable way to infer for the truncated tail of any
    of the four files — e.g. whether a wrapped hint line like "queue is empty — press
    unbound on an album to start" adds its own extra style spans beyond the plain
    border-column pattern visible in the untruncated rows), reconstructing the missing
    portions by inference was judged more likely to reintroduce the exact corruption this
    rework exists to fix than to leave the four fixtures failing one more round.

    The four target clock corrections above are recorded here so the next round can apply
    them directly, one file at a time, once it has been given the complete, untruncated
    contents of each `.snap` file to copy from. **Next round should fix exactly one of
    these four** — `layout_snapshot_80x24.snap` is the smallest and the recommended next
    target — using a full, untruncated read of that one file.
  - **Why a process-wide `TZ` pin instead of test-level injection:** a per-test seam (making
    `local_hour_minute`'s UTC offset an explicit, test-overridable parameter) was considered,
    since it would leave `cargo run`'s displayed clock untouched. It was rejected for this
    fix set because `local_hour_minute` is a `loxia-core` free function shared by every
    caller that turns a `Timestamp` into a local hour/minute, not just these two widgets;
    giving it a test-injectable time source is a real, separate change to that function's
    signature and every caller of it, not a one-line fixture fix, and was judged out of
    scope for this rework.
