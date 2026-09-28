# Contributing to loxia

Issues and pull requests are welcome. This document is the short version of what the code expects; the
user-facing documentation lives in [`docs/user/`](docs/user/README.md).

## Reporting a bug

Include `loxia-player --doctor` output — it covers the config, keybinding conflicts, libmpv, output
devices, terminal graphics, storage and server reachability, with tokens and header values already
redacted. Add your terminal emulator and version (a great many rendering issues are terminal-specific),
the relevant log lines, and a screenshot for anything about layout.

[Troubleshooting](docs/user/troubleshooting.md#reporting-a-bug) has the full checklist.

## Getting set up

```sh
git clone https://github.com/tuturu742/loxia-player.git
cd loxia-player
cargo build
```

You need libmpv and its development files — see [Installation](docs/user/installation.md#installing-mpv).
The Rust toolchain is pinned in `rust-toolchain.toml`; `rustup` applies it automatically.

Two extra tools for the full check suite:

```sh
cargo install cargo-deny cargo-insta just
```

## Before opening a PR

```sh
just check-all
```

That runs, and all four must be clean:

| | |
| :-- | :-- |
| `cargo fmt --all -- --check` | Formatting |
| `cargo clippy --workspace --all-targets -- -D warnings` | No warnings, anywhere, including tests |
| `cargo test --workspace --lib --bins --tests --examples` | The whole suite, examples included |
| `cargo deny check` | Licences, advisories, duplicate versions |

Other recipes: `just fmt`, `just lint`, `just test`, `just run`, `just snap` (review insta snapshots).

CI runs the same things on Linux, macOS and Windows, plus a few greps described in
`.github/workflows/ci.yml`.

## The rules that matter

These are load-bearing, not style preferences. Each is enforced by CI, a test, or both.

1. **`loxia-core` has no I/O.** No `tokio`, no `reqwest`, no `ratatui`, no filesystem. It must compile
   and test on a machine with no network and no audio device. This is the single most important rule in
   the codebase — it is what makes the app testable.
2. **The reducer is pure.** `Action` in, mutated `AppState` plus a `Vec<Effect>` out. Never a clock
   read, never a random number, never an HTTP call. Time comes in as `state.clock` (the last `Tick`'s
   timestamp) and randomness as an explicit seed on the action.
3. **No `unwrap()` or `expect()`** outside `main.rs` bootstrap and tests. Every fallible path returns a
   typed error: `thiserror` per crate, `anyhow` only in the binary.
4. **`crossterm` is never a direct dependency.** Use `ratatui::crossterm`. Same for `time` — use `jiff`.
   CI greps for both.
5. **`#[cfg(target_os = …)]` is confined** to `loxia-audio::device` and `loxia-core::paths`. A grep test
   in `loxia-audio` enforces it. If you need per-OS *data* rather than per-OS *code*, match on
   `std::env::consts::OS` instead.
6. **Never hardcode a keybinding in UI text.** Every key hint renders through `KeyMap::hint_for(ActionId)`,
   so remapping updates the help modal, the inspector legend and every inline hint at once.
7. **Never log a token or a stream URL.** Redact in `Debug` and `Display`. A stream URL carries an
   `api_key`; `RedactedUrl` and `RedactedSecret` exist for exactly this.
8. **Add dependencies sparingly**, and to `[workspace.dependencies]` so every crate shares one
   version. `tokio`, `reqwest`, `ratatui` and `image` must stay single-versioned across the whole
   graph; CI checks that, and `cargo deny` gates licences and advisories.

## Documenting a change

- **User-visible behaviour** → update the relevant file in [`docs/user/`](docs/user/README.md). A new
  config key belongs in `configuration.md` with its default; a new binding belongs in `keybindings.md`.
- **A non-obvious decision** → say why in a comment at the point it is made. The codebase leans on
  comments that explain reasoning rather than restating the code.
- **Anything a user would notice** → a `CHANGELOG.md` entry under `[Unreleased]`.
- **Public items get doc comments.** The crate's `lib.rs` module list stays current.

Comments should say *why*, not *what*. The existing code is fairly heavily commented in places where a
decision is not obvious from the code — match that, and skip it where the code speaks for itself.

## Testing

Test at the layer that makes the behaviour provable without ceremony:

- **Reducer logic** — plain unit tests over `AppState`. `loxia_core::test_support::fixtures` has
  realistic artists, albums, tracks and queues; use them rather than hand-building state.
- **Rendering** — `insta` snapshots at 80×24, 120×30 and 200×50. `just snap` reviews changes.
- **The Emby client** — `wiremock` against recorded fixtures. Fixtures must carry no token and no
  private IP address; CI greps for both.
- **The audio engine** — `MockEngine` for anything that does not need a real device.
- **Invariants** — `proptest`, where one exists worth stating.

A test whose name states the behaviour is worth more than a test that merely exercises a line.

## Pull requests

- One logical change per PR. Branch naming is up to you.
- Say what changed and why in the description. If it fixes an issue, link it.
- `just check-all` clean, and a changelog entry if a user would notice.

## Licence

By contributing you agree your work is licensed under **GPL-3.0-or-later**, the same as the project.
