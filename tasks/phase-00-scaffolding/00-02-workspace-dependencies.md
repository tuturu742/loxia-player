# 00-02 · Workspace dependencies

**Phase:** 00 — Scaffolding · **Agent:** A · **Size:** S
**Prerequisites:** `00-01`
**Reference:** `docs/13-dependencies.md`

## Goal
Populate `[workspace.dependencies]` with the exact locked set and wire each member crate to the
subset it needs. Afterwards no task ever picks a version — it writes `dep = { workspace = true }`.

## Files
- `Cargo.toml` (root)
- every `crates/*/Cargo.toml`

## Specification

Copy the tables in `docs/13-dependencies.md` verbatim into `[workspace.dependencies]`, including the
feature lists. That file is the sole authority; do not consult `design_overview` §8, which is stale.

Points that are easy to get wrong:

- **`crossterm` is not listed and must not be added.** Use `ratatui::crossterm`.
- `reqwest` — `default-features = false`, features `["json", "stream", "rustls-tls"]`.
- `image` — `default-features = false`, features `["jpeg", "png"]`.
- `anyhow` goes to `crates/loxia` **only**. A library crate depending on `anyhow` is a review
  failure.
- `tokio` goes to `crates/loxia` only, with one exception: `loxia-audio` depends on it with the
  `sync` feature alone (no runtime), because `AudioBackend::subscribe()` returns a
  `tokio::sync::mpsc::UnboundedReceiver` — see `docs/13-dependencies.md` rule 7. `loxia-emby` and
  `loxia-cache` take `futures` and are generic over the runtime otherwise.
- `loxia-core` may depend only on: `serde`, `serde_json`, `toml`, `thiserror`, `jiff`,
  `rand`, `rand_chacha`, `nucleo-matcher`, `dirs`, `strum`. **No `tracing`** — logging is
  I/O-adjacent, and the pure reducer returns `ConfigWarning`s/toasts for the binary to log rather
  than logging itself. **Nothing else, ever.**

Add to each crate a `test-support` feature (default off) that will later gate the fixture builders;
`loxia-core` declares it now, the others declare
`test-support = ["loxia-core/test-support"]`.

## Acceptance
- `cargo build --workspace` succeeds.
- `cargo tree -d` reports **no duplicate major versions** of `tokio`, `reqwest`, `ratatui`, `image`,
  or `serde` — see `docs/13-dependencies.md` rule 5 for why `rand` is deliberately not on this list
  (transitive deps of `ratatui-image` and `proptest` each pin their own major version, harmlessly).
- `cargo tree -p loxia-core` contains no `tokio`, no `reqwest`, no `ratatui`.
- `grep -r "crossterm" crates/*/Cargo.toml` returns nothing.

## Done when
The global DoD in `tasks/README.md` is satisfied.
