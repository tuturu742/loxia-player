# Contributing to loxia

This project is built from a pre-written task library, not ad-hoc feature requests.

- **What to work on:** pick the lowest-numbered unticked task in `tasks/README.md` whose
  prerequisites are already ticked. Each task file is self-contained — it states the goal, the
  exact files to touch, the specification, and the acceptance tests. You should not need to read
  `design_overview` to execute one.
- **Background and rationale:** `docs/` is the design blueprint the task library implements.
  `docs/12-decisions.md` records every place implementation diverged from the original spec, with
  the reason — read it before touching keybindings or `Cargo.toml`.

## Workflow

1. One task = one branch = one PR. Branch name: `feat/<task-id>-<slug>` (e.g.
   `feat/03-06-reducer-navigation`).
2. Never cross a crate boundary in a single task unless the task explicitly says to.
3. Tick the task's checkbox in `tasks/README.md` in the same PR that completes it.
4. If implementation forces a deviation from `docs/`, fix the doc in the same PR and add a row to
   `docs/12-decisions.md` §9 explaining what changed and why.

## Definition of Done (every task, no exceptions)

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the task's Acceptance section exists and passes
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`

Run `just check-all` before opening a PR.

## Hard rules

- `loxia-core` has zero I/O — no `tokio`, `reqwest`, `ratatui`, or filesystem access.
- No `unwrap()` / `expect()` outside `main.rs` bootstrap and tests.
- `crossterm` is never a direct dependency — use `ratatui::crossterm`.
- Never hardcode a keybinding in UI text — render through `KeyMap::hint_for(ActionId)`.
- Never log a token or a stream URL — redact in `Debug`/`Display`.
