# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Workspace scaffolding: six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`,
  `loxia-cache`, `loxia-tui`, `loxia-player`) with the full module tree per `docs/01-architecture.md`.
- Locked dependency set per `docs/13-dependencies.md`.

### Fixed
- Made the snapshot test suite deterministic without touching the five large insta fixtures that
  render `loxia_core::local_hour_minute` (the header clock, and four Now Playing history views).
  Those fixtures were captured on a machine one hour ahead of UTC and encode that offset in their
  committed HH:MM text; running the suite under a different host timezone (including plain UTC)
  renders a different hour and fails the comparison.
  - **Option taken: pin the test timezone to the fixed UTC+1 offset the fixtures already encode**,
    not regenerate the fixtures to UTC. `.cargo/config.toml`'s `TZ` env var now reads
    `Etc/GMT-1` (tzdata's `Etc/GMT-N` zones are UTC+N, fixed, no DST table — the inverted sign is
    intentional and documented inline in that file). A *named* region such as `Europe/London` was
    rejected for the same reason a bare `UTC` pin was: a region with daylight saving renders a
    different hour depending on the calendar date embedded in a given test's timestamp, which is
    the same non-determinism this change exists to remove, just triggered by the date instead of
    the host.
  - The header widget fixture
    (`loxia_tui__widgets__header__tests__header_snapshot_offline_with_downloads.snap`), which had
    been mistakenly regenerated to `22:13` under a prior `TZ=UTC` pin, is reverted to its
    originally-committed `23:13` so it matches what `TZ=Etc/GMT-1` now renders. The other four
    affected fixtures (the 1, 9, 19, 26, and 37 KB Now Playing/layout snapshots) are untouched —
    they were never regenerated and already encode UTC+1 correctly.
  - Rejected: regenerating all five fixtures under `TZ=UTC`. They are large blocks of
    box-drawing output where a partial/short regeneration fails identically to a wrong one (an
    earlier attempt at four of them landed short by 244–3,943 bytes each), and it is strictly
    more edited surface for the same outcome.
