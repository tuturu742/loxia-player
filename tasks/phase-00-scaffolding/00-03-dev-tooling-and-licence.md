# 00-03 · Dev tooling and licence

**Phase:** 00 — Scaffolding · **Agent:** E · **Size:** S
**Prerequisites:** `00-01`
**Reference:** `docs/11-packaging.md` §1, `docs/README.md`

## Goal
Add the repo-level developer ergonomics and the licence, so every later task has one command to run
all checks and the licensing question is settled before any code ships.

## Files
- `LICENSE` — full GPL-3.0 text
- `justfile`
- `.editorconfig`
- `.gitignore`
- `rustfmt.toml`
- `clippy.toml`
- `CONTRIBUTING.md`
- `CHANGELOG.md`

## Specification

**`LICENSE`** — the complete, unmodified GPL-3.0 text. `Cargo.toml` already declares
`license = "GPL-3.0-or-later"` (task `00-01`).

**`justfile`** targets:
| Target | Command |
| :-- | :-- |
| `fmt` | `cargo fmt --all` |
| `lint` | `cargo clippy --workspace --all-targets -- -D warnings` |
| `test` | `cargo test --workspace` |
| `run` | `cargo run -p loxia-player --` |
| `check-all` | `cargo fmt --all -- --check && just lint && just test && cargo deny check` |
| `snap` | `cargo insta review` |

**`rustfmt.toml`:** `edition = "2024"`, `max_width = 100`. Nothing else — do not enable nightly-only
options, they will fail on the pinned stable toolchain.

**`clippy.toml`:** `msrv = "1.97.1"`.

**`.gitignore`:** `/target`, `*.pending-snap`, `.DS_Store`, `/dist`, `**/*.rs.bk`, and — importantly
— `loxia-player.log` and any `config.toml` at the repo root, so a developer's real server credentials can
never be committed.

**`CONTRIBUTING.md`:** short. Points at `docs/README.md` for conventions and `tasks/README.md` for
what to work on. States the branch naming rule (`feat/<task-id>-<slug>`) and the global DoD.

**`CHANGELOG.md`:** Keep-a-Changelog skeleton with an `## [Unreleased]` section.

## Acceptance
- `just check-all` runs all four steps (it may fail on `cargo deny` until `00-05` lands; the target
  must exist and invoke correctly).
- `LICENSE` contains the string `GNU GENERAL PUBLIC LICENSE` and `Version 3`.
- `git check-ignore -v config.toml` reports a match.

## Done when
The global DoD in `tasks/README.md` is satisfied.
