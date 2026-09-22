# Contributing to loxia

## Getting set up

loxia links against system `libmpv`. Install mpv (or the development package that provides
`libmpv`) for your platform before building.

```sh
cargo build
just check-all
```

## What to work on

Open an issue for the change you want to make, or pick up an existing issue. The planned work is
listed in [`docs/ROADMAP.md`](docs/ROADMAP.md), and `docs/` is the reference for how the system is
designed.

[`docs/12-decisions.md`](docs/12-decisions.md) records every deviation from the design. Read it
before touching keybindings or `Cargo.toml`.

## Workflow

1. One change = one branch = one PR. Name branches `feat/<slug>` or `fix/<slug>`.
2. Do not cross a crate boundary in one change unless the change is inherently cross-cutting and
   the PR description says so.
3. If implementation forces a deviation from `docs/`, fix the documentation in the same PR and add
   a row to [`docs/12-decisions.md`](docs/12-decisions.md) §9 explaining what changed and why.
4. If a change completes a roadmap entry, remove that entry from
   [`docs/ROADMAP.md`](docs/ROADMAP.md) in the same PR.

## Definition of Done

- `cargo fmt --all -- --check` clean
- `cargo clippy --workspace --all-targets -- -D warnings` clean
- `cargo test --workspace` green
- New behaviour has a test
- Public items documented; the crate's `lib.rs` module list updated
- No dependency added that is not in [`docs/13-dependencies.md`](docs/13-dependencies.md)
- Run `just check-all` before opening a PR

## Hard rules

- `loxia-core` has zero I/O (no `tokio`, `reqwest`, `ratatui`, filesystem).
- no `unwrap()`/`expect()` outside `main.rs` bootstrap and tests.
- `crossterm` is never a direct dependency, use `ratatui::crossterm`.
- never hardcode a keybinding in UI text, render through `KeyMap::hint_for(ActionId)`.
- never log a token or a stream URL, redact in `Debug`/`Display`.
