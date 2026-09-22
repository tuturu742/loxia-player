# Testing and continuous integration

The workspace uses unit tests, property tests, fixtures, snapshots, and
feature-gated integration tests. CI checks formatting, Clippy, dependency
invariants, fixture safety, tests, and documentation-related build output.

## Standard commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

`just check-all` provides the repository's combined local check entry point.

## Test layers

| Layer | Purpose |
|---|---|
| `loxia-core` tests | Validate pure state, reducers, parsing, keymaps, queue logic, and domain rules. |
| `loxia-emby` tests | Validate request construction, DTO conversion, streams, and fixture handling. |
| `loxia-audio` tests | Validate commands, events, filters, ReplayGain, and mock behaviour. |
| `loxia-tui` snapshots | Validate rendering at multiple sizes, themes, views, widgets, and modals. |
| Feature-gated mpv tests | Exercise real libmpv behaviour when `mpv-tests` is enabled. |

Snapshots use `insta`. HTTP-facing tests use fixtures and `wiremock` where an
isolated server is required. Property tests use `proptest`.

## CI invariants

The GitHub workflow rejects direct `crossterm` and `time` dependencies in crate
manifests. It also rejects duplicate normal dependency versions of `tokio`,
`reqwest`, `ratatui`, and `image`, because their types cross workspace
boundaries.

Fixture scanning rejects non-empty access tokens, token-shaped fields and query
parameters, and RFC1918 addresses. The policy behind these checks is recorded
in [`12-decisions.md`](12-decisions.md) and
[`13-dependencies.md`](13-dependencies.md).
