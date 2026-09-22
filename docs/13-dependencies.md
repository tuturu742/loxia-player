# Dependencies

Workspace dependency versions are centralised in the root `Cargo.toml`. Crates use
`workspace = true` when they consume a shared dependency.

## Rules

1. `crossterm` and `time` are never direct dependencies. Use ratatui's re-export for terminal
   integration and `jiff` for time handling.
2. `tokio`, `reqwest`, `ratatui`, and `image` each resolve to one normal dependency version across
   the workspace because their types cross crate boundaries.
3. `loxia-core` has no I/O dependencies such as Tokio, reqwest, ratatui, or filesystem access.
4. New dependencies are documented here and added to the workspace dependency table when shared.
5. Dependency changes update `Cargo.lock` and, when relevant, `THIRD_PARTY_LICENSES.md`.
6. `libmpv2-sys` is named directly by `loxia-audio` for the mpv logging FFI that `libmpv2` does not
   expose.
7. `loxia-audio` names Tokio with only the `sync` feature for its public MPSC channel types.

## Shared versions

The root workspace currently pins, among others:

| Dependency | Version |
|---|---|
| `tokio` | `1.53` |
| `reqwest` | `0.13` |
| `ratatui` | `0.30.2` |
| `ratatui-image` | `11.0.6` |
| `image` | `0.25` |
| `libmpv2` | `6.0` |
| `serde` | `1.0.229` |
| `toml` | `1.1` |
| `jiff` | `0.2` |
| `clap` | `4.6` |

CI enforces the direct-dependency and duplicate-version rules described in
[`10-testing-and-ci.md`](10-testing-and-ci.md).
