# Testing and CI

The workspace uses unit tests, integration tests, property tests, HTTP fixtures, and insta snapshots.

## Local checks

Run the workspace checks with:

```text
just check-all
```

The CI workflow runs formatting, Clippy with warnings denied, dependency-policy checks, fixture
secret scanning, and the workspace test suite. Contributors can also run:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

## Test boundaries

`loxia-core` tests exercise pure state, reducers, keymaps, queues, configuration, and models.
`loxia-emby` tests use fixtures and request mocking. `loxia-audio` normally uses `MockEngine`;
tests that require a real libmpv installation are gated by the `mpv-tests` feature. `loxia-tui`
uses snapshot tests for layouts, widgets, views, and modals.

## Documentation checks

Public Rust API documentation builds with `cargo doc --workspace --no-deps`. Documentation links
inside this directory point only to surviving reference documents. Dependency changes follow
[`13-dependencies.md`](13-dependencies.md), and behaviour-changing documentation corrections are
recorded under §9 of [`12-decisions.md`](12-decisions.md).
