# 00-01 · Workspace skeleton

**Phase:** 00 — Scaffolding · **Agent:** A · **Size:** S
**Prerequisites:** none
**Reference:** `docs/01-architecture.md` §2–3

## Goal
Create the Cargo workspace with six member crates and every module file stubbed. At the end,
`cargo build --workspace` succeeds and the module tree matches the architecture doc exactly, so
later tasks only add bodies — never new files in unexpected places.

## Files
- `Cargo.toml` (workspace root)
- `rust-toolchain.toml`
- `crates/loxia-core/{Cargo.toml,src/lib.rs}` + every module file from `docs/01-architecture.md` §3.1
- `crates/loxia-emby/{Cargo.toml,src/lib.rs}` + §3.2 modules
- `crates/loxia-audio/{Cargo.toml,src/lib.rs}` + §3.3 modules
- `crates/loxia-cache/{Cargo.toml,src/lib.rs}` + §3.4 modules
- `crates/loxia-tui/{Cargo.toml,src/lib.rs}` + §3.5 modules
- `crates/loxia-player/{Cargo.toml,src/main.rs}` + §3.6 modules

## Specification

Root `Cargo.toml`:
```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version      = "0.1.0"
edition      = "2024"
rust-version = "1.97.1"
license      = "GPL-3.0-or-later"
repository   = "<repo url>"
authors      = ["loxia contributors"]
```
Leave `[workspace.dependencies]` empty — task `00-02` fills it.

`rust-toolchain.toml`:
```toml
[toolchain]
channel    = "1.97.1"
components = ["rustfmt", "clippy"]
```

Each member `Cargo.toml` inherits with `version.workspace = true`, `edition.workspace = true`,
`rust-version.workspace = true`, `license.workspace = true`.

Every module file is created with a `//!` doc comment naming its responsibility (copy the one-line
description from the architecture doc) and nothing else. `lib.rs` declares each module with
`pub mod`; private helpers use `mod`. Where a module has submodules, create the directory plus
`mod.rs`.

Dependency edges to declare now, even though the crates are empty:
- `loxia-emby`, `loxia-audio`, `loxia-cache`, `loxia-tui` → `loxia-core` only
- `loxia-player` → all five

Do **not** add any external dependency in this task.

## Acceptance
- `cargo build --workspace` succeeds.
- `cargo tree -p loxia-tui` shows `loxia-core` and **no other workspace crate**. Same for
  `loxia-emby`, `loxia-audio`, `loxia-cache`.
- `ls` of each crate's `src/` matches the trees in `docs/01-architecture.md` §3 exactly — no missing
  files, no extra ones.

## Done when
The global DoD in `tasks/README.md` is satisfied.
