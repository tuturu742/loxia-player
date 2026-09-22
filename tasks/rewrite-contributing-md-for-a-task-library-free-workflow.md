# Rewrite CONTRIBUTING.md for a task-library-free workflow

- title: Rewrite CONTRIBUTING.md for a task-library-free workflow
- description: Repo: loxia (Rust Cargo workspace, TUI Emby music client). Depends on the removal of `tasks/`; assume that directory is gone.

Goal: `CONTRIBUTING.md` currently opens with 'This project is built from a pre-written task library' and tells contributors to pick the lowest-numbered unticked task in `tasks/README.md`, use branch names like `feat/03-06-reducer-navigation`, and tick checkboxes. None of that exists any more. Rewrite the file so it describes how someone actually contributes now.

File to modify: `CONTRIBUTING.md` only.

Required content:
1. `## Getting set up` — the system libmpv dependency, `cargo build`, `just check-all`.
2. `## What to work on` — open an issue or pick one up; `docs/ROADMAP.md` lists planned work; `docs/` is the reference for how the system is designed; `docs/12-decisions.md` records every deviation from the design and must be read before touching keybindings or `Cargo.toml`.
3. `## Workflow` — one change = one branch = one PR; branch naming `feat/<slug>` / `fix/<slug>` (no task-id component); do not cross a crate boundary in one change unless it is inherently cross-cutting and the PR description says so; if implementation forces a deviation from `docs/`, fix the doc in the same PR and add a row to `docs/12-decisions.md` §9 explaining what changed and why; if a change completes a roadmap entry, remove that entry from `docs/ROADMAP.md` in the same PR.
4. `## Definition of Done` — keep the existing checklist minus the 'every named test in the task's Acceptance section' line and the task-ticking line. Retain: `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` green; public items documented and the crate's `lib.rs` module list updated; no dependency added that is not in `docs/13-dependencies.md`; run `just check-all` before opening a PR. Add: new behaviour has a test.
5. `## Hard rules` — keep verbatim: `loxia-core` has zero I/O (no `tokio`, `reqwest`, `ratatui`, filesystem); no `unwrap()`/`expect()` outside `main.rs` bootstrap and tests; `crossterm` is never a direct dependency, use `ratatui::crossterm`; never hardcode a keybinding in UI text, render through `KeyMap::hint_for(ActionId)`; never log a token or a stream URL, redact in `Debug`/`Display`.

Done when: `CONTRIBUTING.md` contains no reference to `tasks/`, task ids, task files, or ticking checkboxes; every relative link in it resolves to an existing file; the hard rules and the trimmed Definition of Done are intact.

## Brief

<knowledge id="k1" class="lore" source="Repo overview" entry="Repo: loxia">
loxia is a Rust workspace implementing a terminal (TUI) music client for Emby servers, built with ratatui and libmpv2, licensed GPL-3.0-or-later. The Cargo workspace defines six crates: `loxia-core` (zero-I/O domain layer with state, reducer, effects, keymap, config, queue and models), `loxia-emby` (async Emby REST/WebSocket client with endpoints, DTOs, retry, and stream/transcode URL building), `loxia-audio` (mpv-backed playback with EQ, gapless, ReplayGain and device handling), `loxia-cache` (downloads, LRU, manifests, offline index, scrobble/session persistence), plus `loxia-tui` and `loxia-player` binaries referenced in the changelog. Development is task-driven from a pre-written library (tasks/README.md) implementing the design blueprint in `docs/`, with strict rules like zero I/O in core, no direct crossterm/time deps, and single-version tokio/reqwest/ratatui/image enforced by CI. The repo ships assets including SVG logos, an ASCII banner, factory EQ presets, and multiple color themes (e.g. cyberpunk_neon, amber_crt, oled_black). Its branding doc explicitly ties it to a Fringillidae-themed suite, naming `pyrrhula` (bullfinch) as its sibling project, with loxia being the crossbill genus. Testing relies on insta snapshots, wiremock, proptest, and JSON fixtures for Emby responses.
</knowledge>

<knowledge id="k2" class="lore" source="Repo overview" entry="Repo overview">
## Overview

This project is a **Fringillidae-themed suite** of terminal music clients for Emby servers, written in Rust. The genus-based naming (`loxia` = crossbill) signals a family of related applications, with only one repository — **loxia** — described here; its branding documentation names `pyrrhula` (bullfinch) as its sibling project.

### Collective goal

Provide a fast, offline-capable, keyboard-driven TUI music experience for Emby media servers, cleanly separating domain logic, network I/O, playback, caching, an

## Status

Scaffolded by the delegated coding agent. TODO: implement.
