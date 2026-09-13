# loxia — architecture reference

These documents describe how loxia is built and why. They are for contributors.

**Looking for how to use or install it?** That is [`user/`](user/README.md), or the
[README](../README.md).

## Read order

| Doc | Purpose | Read it when |
| :-- | :-- | :-- |
| [`01-architecture.md`](01-architecture.md) | Workspace layout, crate boundaries, module tree, threading and data-flow model | Before writing any code |
| [`02-data-model.md`](02-data-model.md) | Every domain type, its fields, and which crate owns it | Working in any crate |
| [`03-emby-api.md`](03-emby-api.md) | The Emby endpoint contract: routes, params, auth, error mapping | Working on `loxia-emby` |
| [`04-state-and-input.md`](04-state-and-input.md) | The `AppState` tree, the Action/Effect/Event taxonomy, reducer rules, the default keymap | Working on `loxia-core` or input |
| [`05-audio-engine.md`](05-audio-engine.md) | libmpv2 integration, device enumeration, EQ, ReplayGain, gapless | Working on `loxia-audio` |
| [`06-cache-and-offline.md`](06-cache-and-offline.md) | The LRU cache, permanent downloads, the scrobble buffer, offline mode | Working on `loxia-cache` |
| [`07-ui-spec.md`](07-ui-spec.md) | Per-view widget trees, layout constraints, the focus model, theming | Working on `loxia-tui` |
| [`10-testing-and-ci.md`](10-testing-and-ci.md) | Test strategy per layer, fixtures, the CI matrix | Writing tests |
| [`11-packaging.md`](11-packaging.md) | Distribution targets and the GPL/LGPL compliance checklist | Working on release tooling |
| [`12-decisions.md`](12-decisions.md) | **The decision record.** Every decision taken during development, with its rationale | Before changing keybindings, `Cargo.toml`, or anything that looks arbitrary |
| [`13-dependencies.md`](13-dependencies.md) | The locked dependency set, with versions and justifications | Adding any dependency |
| [`14-manual-test-plan.md`](14-manual-test-plan.md) | Every user-facing feature as a manual checklist, plus a regression sweep of defects found in live use | Before a release, or after a batch of changes |

`12-decisions.md` is the one to know about. Several hundred comments in the source point at it, because
it is where the reason for a non-obvious choice actually lives. Filenames are load-bearing for the same
reason — the code refers to them by path.

The numbering has two gaps (`08` and `09`), left by a build-phase roadmap and a task-traceability
matrix that were removed once the work they tracked was finished. Forward-looking plans are in
[`ROADMAP.md`](../ROADMAP.md).

## The crates

```
loxia-core    ─ state, actions, the reducer, config, keymap, queue, sort. Zero I/O.
loxia-emby    ─ the Emby HTTP client, DTOs, stream URLs, the WebSocket
loxia-audio   ─ the libmpv2 backend, device enumeration, EQ, ReplayGain
loxia-cache   ─ the rolling LRU cache, downloads, the offline index, the scrobble buffer
loxia-tui     ─ ratatui widgets and views. Renders from &AppState; owns no state.
loxia-player  ─ the binary: CLI, bootstrap, workers, the event loop, --doctor
```

Dependencies run strictly upward: `loxia-core` depends on nothing of ours, and only `loxia-player`
depends on everything. `loxia-tui` cannot reach `loxia-audio`, which is why pure data both need (device
labelling, EQ band frequencies) lives in `loxia-core`.

## Toolchain

Rust edition 2024, **MSRV 1.97.1**, driven by `ratatui` 0.30 and pinned in `rust-toolchain.toml`. Do not
raise it without updating that file.

## The rules that matter

Load-bearing, not style preferences. Each is enforced by CI, a test, or both. Source comments cite them
by number, so the numbering is stable.

1. **`loxia-core` has no I/O.** No `tokio`, no `reqwest`, no `ratatui`, no filesystem. It must compile
   and test on a machine with no network and no audio device. This is what makes the app testable, and
   it is the most important rule here.
2. **The reducer is pure.** `Action` in, mutated `AppState` plus a `Vec<Effect>` out. No clock read, no
   random number, no I/O. Time arrives as `state.clock` (the last `Tick`'s timestamp) and randomness as
   an explicit seed on the action.
3. **No `unwrap()` / `expect()`** outside `main.rs` bootstrap and tests. Every fallible path returns a
   typed error — `thiserror` per crate, `anyhow` only in the binary.
4. **`crossterm` is never a direct dependency** — use `ratatui::crossterm`. Nor is `time` — use `jiff`.
   Both are required transitively, so `cargo deny` cannot ban them; CI greps the manifests instead.
5. **`#[cfg(target_os = …)]` is confined** to `loxia-audio::device` and `loxia-core::paths`, and nowhere
   else. A grep test over `loxia-audio`'s own source enforces it. For per-OS *data* rather than per-OS
   *code*, match on `std::env::consts::OS`.
6. **Never hardcode a keybinding in UI text.** Every hint renders through `KeyMap::hint_for(ActionId)`,
   so a remap updates the help modal, the inspector legend and every inline hint at once.
7. **Never log a token or a stream URL.** Redact in `Debug`/`Display`. `RedactedUrl` and
   `RedactedSecret` exist for this; a stream URL carries an `api_key` in its query string.
8. **Add no dependency** that is not in [`13-dependencies.md`](13-dependencies.md). If one is genuinely
   needed, add it there in the same PR with a justification. `tokio`, `reqwest`, `ratatui` and `image`
   must stay single-versioned across the graph, which CI also checks.

## Contributing

[`CONTRIBUTING.md`](../CONTRIBUTING.md) has the workflow, the check suite and the testing conventions.
