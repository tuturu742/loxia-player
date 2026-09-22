# Testing and continuous integration

The workspace uses unit tests, integration tests, property tests, fixture tests, and insta snapshot
tests. `cargo test --workspace` runs the ordinary workspace suite without requiring a real mpv
installation.

## Local checks

Run the following before submitting a change:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

`just check-all` runs the repository's combined local check command.

## CI checks

The GitHub Actions workflow runs formatting, Clippy with warnings denied, and dependency-policy
checks. It rejects direct `crossterm` and `time` dependencies and rejects duplicate normal
dependency versions of `tokio`, `reqwest`, `ratatui`, and `image`.

CI also scans Emby fixtures for non-empty token fields, token query parameters, and RFC1918
addresses. Fixture identifiers and media metadata are allowed when they are not credentials.

## Test boundaries

- `loxia-core` tests exercise pure state, reducer, keymap, and queue behaviour.
- `loxia-emby` tests use fixtures and HTTP doubles.
- `loxia-audio` normally uses `MockEngine`; real-mpv tests require the `mpv-tests` feature.
- `loxia-tui` uses snapshots for rendering.
- Public Rust APIs keep documentation current so `cargo doc` detects broken intra-doc links.

See [`13-dependencies.md`](13-dependencies.md) for dependency policy.
