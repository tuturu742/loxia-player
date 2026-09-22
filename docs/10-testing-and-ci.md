# Testing and CI

The workspace uses unit tests, integration tests, snapshots, property tests, and manual checks.
The normal local verification command is:

```text
just check-all
```

## Required automated checks

Contributors run the following checks before opening a pull request:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

CI runs formatting and Clippy checks, enforces selected dependency constraints, scans Emby fixtures
for credentials and private addresses, and runs the workspace test suite.

## Test support

`loxia-core::test_support` provides fixtures and scenarios for deterministic state tests.
`loxia-audio::MockEngine` supports playback tests without audio hardware. Emby tests use local JSON
fixtures and request snapshots. TUI tests use Insta snapshots for stable rendering checks.

Real libmpv integration tests are gated behind the `mpv-tests` feature because they require an
installed libmpv library. They complement, rather than replace, the ordinary workspace test suite.

## Snapshot workflow

A `.snap.new` file is a rejected candidate produced by a failing Insta assertion. It is not source
material and is ignored by Git. A snapshot change is accepted only after reviewing the behavioural
change and updating the corresponding `.snap` file intentionally.

## Documentation checks

Public crate and module documentation must remain current. `cargo doc --workspace --no-deps` checks
intra-documentation links. Documentation that describes changed behaviour is updated in the same
change and recorded in [`12-decisions.md`](12-decisions.md) §9.
