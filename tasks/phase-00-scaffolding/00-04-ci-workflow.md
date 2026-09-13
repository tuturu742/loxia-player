# 00-04 · CI workflow

**Phase:** 00 — Scaffolding · **Agent:** E · **Size:** S
**Prerequisites:** `00-03`
**Reference:** `docs/10-testing-and-ci.md` §7

## Goal
Add `ci.yml` so every PR is checked on all three platforms from the first commit. Cross-platform
bugs — path handling above all — are cheap now and expensive in phase 08.

## Files
- `.github/workflows/ci.yml`

## Specification

Triggers: `push` to `master`, and `pull_request`. Concurrency group per ref with
`cancel-in-progress: true`.

Jobs:

| Job | Runner | Steps |
| :-- | :-- | :-- |
| `lint` | `ubuntu-latest` | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo deny check`; the fixture-secret grep below |
| `test` | matrix: `ubuntu-latest`, `macos-latest`, `windows-latest` | `cargo test --workspace` |
| `build` | same matrix | `cargo build --release --workspace` |

All jobs:
- `dtolnay/rust-toolchain@stable` pinned to the version in `rust-toolchain.toml`
- `Swatinem/rust-cache@v2`

**Fixture-secret grep** (in `lint`): fail the build if `crates/loxia-emby/tests/fixtures/` contains
a non-empty `AccessToken`/`access_token`/`api_key`/`X-Emby-Token`/`X-MediaBrowser-Token` field, an
`api_key=`/`token=` query parameter, or an RFC1918 address. **Do not** match on "any long hex
string" — real Emby responses are legitimately full of 32-character hex `Id`s, `Etag`s,
`ImageTags`, and `PresentationUniqueKey`s, which are indistinguishable from a token by shape alone;
that heuristic fails on every real fixture (found while capturing fixtures for task `02-01`, see
`docs/12-decisions.md`). Write this as a small shell step with a clear failure message naming the
offending file. This runs from day one so a fixture captured in phase 02 can never leak the
project's real token.

**`cargo deny check` will fail until task `00-05` adds `deny.toml`** — that is expected and is fixed
by the next task. Do not add a `continue-on-error`.

Do **not** run `mpv-tests` or any feature requiring audio hardware; CI has none.

## Acceptance
- CI is green on the PR for this task, except for `cargo deny check`, which is fixed by `00-05`.
- The `test` job runs on all three platforms.
- Introducing a temporary file containing a fake 40-character hex token under
  `crates/loxia-emby/tests/fixtures/` makes the `lint` job fail. Remove it before merging.

## Done when
The global DoD in `tasks/README.md` is satisfied.
