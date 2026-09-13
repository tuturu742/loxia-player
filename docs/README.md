# loxia — Implementation Blueprint

`design_overview` states *what* the product is. These documents state *how it gets built*.
`../tasks/` states *what to do next*, in order.

**Every statement in `docs/` is decided.** There are no TBDs, no open questions, and no lists of
alternatives. Where the original spec was ambiguous, contradictory, or impossible against the Emby
API, the resolution is recorded in `12-decisions.md` with its rationale.

## Read order

| Doc | Purpose | Read it when |
| :-- | :-- | :-- |
| `01-architecture.md` | Workspace layout, crate boundaries, module tree, threading & data-flow model | Before writing any code |
| `02-data-model.md` | Every domain type, its fields, and which crate owns it | Implementing any crate |
| `03-emby-api.md` | Emby endpoint contract: routes, params, auth, error mapping | Working on `loxia-emby` |
| `04-state-and-input.md` | `AppState` tree, Action/Effect/Event taxonomy, reducer rules, **the default keymap** | Working on `loxia-core` or input |
| `05-audio-engine.md` | libmpv2 integration, device enumeration, EQ, ReplayGain, gapless | Working on `loxia-audio` |
| `06-cache-and-offline.md` | LRU cache, permanent downloads, scrobble buffer, offline mode | Working on `loxia-cache` |
| `07-ui-spec.md` | Per-view widget trees, layout constraints, focus model, theming | Working on `loxia-tui` |
| `08-roadmap.md` | Phases 00–12, entry/exit criteria, parallelization plan | Planning a work session |
| `09-traceability.md` | Which task implements which spec requirement | Checking coverage |
| `10-testing-and-ci.md` | Test strategy per layer, fixtures, CI matrix | Writing tests |
| `11-packaging.md` | cargo-dist config, per-OS installers, GPL/LGPL compliance | Phase 12 |
| `12-decisions.md` | Every decision made against the original spec, with rationale | **Before implementing keybindings or touching `Cargo.toml`** |
| `13-dependencies.md` | The locked dependency set with exact versions | Adding any dependency |
| `14-manual-test-plan.md` | Every user-facing feature as a manual checklist, plus a regression sweep of defects found in live use | Before a release, or after a batch of changes |

## Toolchain

- **Rust edition 2024, MSRV 1.97.1** — driven by `ratatui` 0.30.
- Pinned in `rust-toolchain.toml`. Do not raise the MSRV without updating that file and CI.

## Working conventions (binding on all agents)

1. **One task file = one branch = one PR.** Task IDs are `<phase>-<nn>` (e.g. `04-03`).
   Branch name: `feat/04-03-miller-column-widget`.
2. **Never cross a crate boundary in a single task** unless the task says so. If you need a new type
   in a lower crate, it is already a separate, earlier task — find it.
3. **No `unwrap()` / `expect()` outside `main.rs` bootstrap and tests.** Every fallible path returns
   a typed error (`thiserror` per crate; `anyhow` only in the binary).
4. **The reducer is pure.** `loxia-core` has zero I/O, zero `tokio`, zero `ratatui`. It must compile
   and test on a machine with no network and no audio device. This is the single most important
   architectural rule — it is what makes the app testable.
5. **Feature flags gate every OS-specific path.** `#[cfg(target_os = ...)]` is confined to
   `loxia-audio::device` and `loxia-core::paths`. Nowhere else.
6. **Never hardcode a keybinding in UI text.** All key hints render through
   `KeyMap::hint_for(ActionId)` so remapping updates them everywhere.
7. **Add no dependency** that is not in `13-dependencies.md`. If one is genuinely needed, add it to
   that file in the same PR with a one-line justification.

## Definition of Done (every task)

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the task's Acceptance section exists and passes
- [ ] Public items have doc comments; the crate's `lib.rs` module list is updated
- [ ] The task file's checkbox is ticked in `../tasks/README.md`

## Agent roles

Work parallelizes along crate lines.

| Agent | Owns | Phases |
| :-- | :-- | :-- |
| **A — Core** | `loxia-core` (state, actions, queue, sort, config, keymap) | 00, 01, 03, 06, 11 |
| **B — Network** | `loxia-emby`, `loxia-cache` | 02, 08 |
| **C — Audio** | `loxia-audio` | 05, 09 |
| **D — UI** | `loxia-tui` | 04, 07, 10 |
| **E — Release** | CI, packaging, docs, licensing | 12 (starts at 00, runs in background) |

Phases 00→01→02 are the critical path and must run sequentially. After phase 02, agents A, C, and D
run concurrently.
