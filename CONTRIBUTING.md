# Contributing to loxia

`docs/` is the reference set for the implemented system. Start with
[`docs/README.md`](docs/README.md), and use [`docs/ROADMAP.md`](docs/ROADMAP.md) to find work that
is not yet implemented. [`docs/12-decisions.md`](docs/12-decisions.md) records significant
implementation choices and documented behaviour changes.

## Workflow

1. Create a focused branch and pull request for one coherent change.
2. Read the relevant reference documentation before changing code.
3. Keep crate boundaries explicit; explain an intentional boundary change in the pull request.
4. Update the relevant reference document in the same pull request when implementation changes
   documented behaviour, and add a row to `docs/12-decisions.md` §9.
5. Run `just check-all` before opening a pull request.

## Definition of Done

- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] `cargo test --workspace` is green
- [ ] Relevant public items and crate module documentation are current
- [ ] Relevant reference documentation is current
- [ ] No dependency is added outside the rules in `docs/13-dependencies.md`

## Hard rules

- `loxia-core` has zero I/O: it does not depend on `tokio`, `reqwest`, `ratatui`, or filesystem
  access.
- Do not use `unwrap()` or `expect()` outside bootstrap code in `main.rs` and tests.
- `crossterm` is never a direct dependency; use `ratatui::crossterm`.
- Do not hardcode a keybinding in UI text; render it through `KeyMap::hint_for(ActionId)`.
- Do not log a token or stream URL; redact it in `Debug` and `Display`.
