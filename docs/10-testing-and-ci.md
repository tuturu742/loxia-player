# Testing and CI

The workspace uses unit tests, integration tests, snapshots, fixtures, and targeted real-mpv tests
to protect its behaviour.

## Local checks

Run the same baseline checks used by CI:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

`just check-all` provides the repository's combined local check entry point.

## Test boundaries

`loxia-core` tests exercise pure state and reducer behaviour. `loxia-audio` uses `MockEngine` for
deterministic tests; tests requiring a real libmpv instance are gated by the `mpv-tests` feature.
`loxia-emby` uses fixtures and endpoint-level tests. `loxia-tui` uses snapshots for rendering,
widgets, views, and modals.

Fixtures contain representative server data but no credentials. Snapshot changes require review
because they describe user-visible output.

## CI

The GitHub workflow checks formatting and Clippy, prevents direct `crossterm` and `time`
dependencies, detects duplicate versions of boundary-crossing dependencies, and scans Emby fixtures
for credentials and private addresses.

Dependency policy is documented in `13-dependencies.md`; release and manual checks are covered by
`11-packaging.md` and `14-manual-test-plan.md`.
