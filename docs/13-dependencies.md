# 13 — Locked Dependency Set

Verified against crates.io on **2026-07-21**. These versions go into the root
`[workspace.dependencies]` table; member crates reference them with `dep = { workspace = true }`.

**Do not use the versions listed in `design_overview` §8** — they are stale and four of them name
unmaintained crates. See `12-decisions.md` §1 for the replacement rationale.

## Toolchain

```
edition      = "2024"
rust-version = "1.97.1"
```
Both are driven by `ratatui` 0.30. `rust-toolchain.toml` pins the channel.

## Runtime dependencies

| Crate | Version | Features | Used by |
| :-- | :-- | :-- | :-- |
| `tokio` | `1.53` | `rt-multi-thread`, `macros`, `sync`, `time`, `fs`, `io-util`, `signal` | `loxia-player` (full runtime); `loxia-audio` (`sync` feature only — see rule 7); `loxia-emby` (`time`, `fs`, `io-util` — `with_retry`'s backoff sleep, and `08-03`'s `endpoints::download::fetch_to_file` streaming a cache/download fetch straight to disk; see rule 8) |
| `reqwest` | `0.13` | `json`, `query`, `stream`, `rustls` (**not** `default-tls`) | `loxia-emby` |
| `tokio-tungstenite` | `0.30` | `rustls-tls-webpki-roots` | `loxia-emby` |
| `rustls` | `0.23` | — | transitive, pinned for consistency |
| `futures` | `0.3` | — | `loxia-emby`, `loxia-cache` |
| `bytes` | `1.12` | — | `loxia-emby`, `loxia-cache` |
| `serde` | `1.0.229` | `derive` | `loxia-core`, `loxia-emby`, `loxia-cache`; `loxia-audio` (`09-03`: the local `PresetsFile` wrapper struct that `toml::from_str` deserializes `assets/eq_presets.toml` into) |
| `serde_json` | `1.0.151` | — | `loxia-emby`, `loxia-cache` |
| `toml` | `1.1` | — | `loxia-core`; `loxia-audio` (`09-03`: parses `assets/eq_presets.toml`, embedded via `include_str!`, into the existing `loxia_core::config::EqPreset`) |
| `ratatui` | `0.30.2` | default + `crossterm` backend | `loxia-tui` |
| `ratatui-image` | `11.0.6` | `default-features = false`, `crossterm` (its defaults pull a system `libchafa` via pkg-config, and `image`'s full format set — its Sixel/Kitty/iTerm2/half-block encoders are native Rust and need neither) | `loxia-tui` |
| `image` | `0.25` | `jpeg`, `png` (disable defaults) | `loxia-tui`, `loxia-cache`; `loxia-player` (`10-01`: `workers::network`'s `FetchImage` handler decodes off the async runtime thread via `spawn_blocking`) |
| `libmpv2` | `6.0` | — | `loxia-audio` |
| `libmpv2-sys` | `4.0` | — | `loxia-audio` (direct, not just transitive via `libmpv2` — `mpv_request_log_messages` is unwrapped by `libmpv2`, called raw against `Mpv::ctx`; see `12-decisions.md`, task `05-03`) |
| `souvlaki` | `0.8` | — | `loxia-player` |
| `notify-rust` | `4.18` | default (no `images`) | `loxia-player` — the icon is set by file path (`Notification::icon`/`image_path`), which needs no image decoding; the `images` feature is only for embedding raw pixel data and would otherwise pull `image`'s full default format set (`exr`, `avif` via `rav1e`) transitively, dragging in the unmaintained `paste` crate for no benefit — see `12-decisions.md` §9 |
| `dirs` | `6.0` | — | `loxia-core` |
| `nucleo-matcher` | `0.3` | — | `loxia-core`; `loxia-cache` (`08-05`: `offline_index.rs`'s own `search`, the same engine `04-11`'s inline filter uses) |
| `jiff` | `0.2` | `serde` | `loxia-core`, `loxia-cache` |
| `fs4` | `1.1` | `sync` | `loxia-cache` |
| `rand` | `0.10` | — | `loxia-core` (seeded, reproducible shuffle); `loxia-emby` (retry backoff jitter — not seeded, real randomness is fine there since it's not part of the pure/testable reducer) |
| `rand_chacha` | `0.10` | — | `loxia-core` (seeded, reproducible shuffle) |
| `uuid` | `1.24` | `v4`, `serde` | `loxia-emby` |
| `unicode-width` | `0.2` | — | `loxia-tui` |
| `unicode-segmentation` | `1.13` | — | `loxia-tui`, `loxia-cache` (`08-01`: `sanitize_component`'s grapheme-safe 100-character truncation — a cache/download filename must never split a multi-byte emoji mid-codepoint) |
| `strum` | `0.28` | `derive` | `loxia-core` (enum iteration for Settings); `loxia-tui` (`10-03`: `modals::help` iterates every `ActionId` to generate its rows — no `derive` feature needed there, `ActionId`'s own derive already lives in `loxia-core`) |
| `thiserror` | `2.0` | — | all libraries |
| `anyhow` | `1.0` | — | **`loxia-player` binary only** |
| `clap` | `4.6` | `derive`, `env` | `loxia-player` |
| `tracing` | `0.1` | — | `loxia-emby`, `loxia-audio`, `loxia-cache`, `loxia-player` (**not** `loxia-core` or `loxia-tui` — logging is I/O-adjacent and both of those stay pure/render-only) |
| `tracing-subscriber` | `0.3` | `env-filter`, `fmt` | `loxia-player` |
| `tracing-appender` | `0.2` | — | `loxia-player` |
| `sha2` | `0.11` | — | `loxia-cache` (content addressing), packaging checks |
| `gethostname` | `1.1` | — | `loxia-emby` (`Device=` field of `X-Emby-Authorization`, §2) — no OS-agnostic hostname lookup exists in `std`; this crate is single-purpose (no dependencies beyond `rustix` on Unix) |
| `smallvec` | `1.15` | — | `loxia-core` (`keymap::KeyBinding`, `docs/04-state-and-input.md` §7) — a binding is 1 or 2 chords; `SmallVec<[KeyChord; 2]>` avoids a heap allocation for the overwhelming majority (single-chord) case without a hand-rolled enum |

## Dev-dependencies

| Crate | Version | Purpose |
| :-- | :-- | :-- |
| `insta` | `1.48` | UI snapshot tests |
| `wiremock` | `0.6` | HTTP fixtures for `loxia-emby` |
| `proptest` | `1.11` | property tests (shuffle, keymap parsing, paths) |
| `tempfile` | `3.27` | isolated cache/config roots |
| `pretty_assertions` | `1.4` | readable diffs |
| `tokio` | `1.53` | + `test-util` for the paused-clock tests; `loxia-cache` (dev-only — `macros`, `rt-multi-thread`, `sync`, `time` — `downloads.rs`'s own concurrency/async tests, `08-04`) |
| `tiny_http` | `0.12` | `loxia-audio`'s `mpv-tests`-gated header-arrival integration test (`05-05`) — a local HTTP server asserting a custom header actually reached mpv's own request |

## Build-dependencies

| Crate | Version | Purpose |
| :-- | :-- | :-- |
| `pkg-config` | `0.3` | `loxia-audio`'s `build.rs` — adds libmpv's `-L`/rpath when it isn't on the linker's default path (e.g. Homebrew-on-Linux); a no-op on a standard system-package install. See `12-decisions.md`, task `05-03` |

## Rules

1. **`crossterm` is never a direct dependency.** Use `ratatui::crossterm`. Adding a direct
   `crossterm` dep risks two incompatible `Event` types in the same binary.
2. **TLS is rustls everywhere.** No OpenSSL, no `native-tls` — it is the single largest source of
   cross-compilation pain for the Windows and macOS release targets.
3. **`anyhow` never appears in a library crate.** Libraries return typed `thiserror` enums.
4. **Disable default features** on `image` and `reqwest`; both pull in large trees we do not need.
5. Run `cargo tree -d` after any dependency change and check specifically for `tokio`, `reqwest`,
   `ratatui`, and `image` — a duplicate major version of any of these is a build failure, not a
   warning, because their types cross our own crate boundaries (e.g. `AudioCommand` carries a
   `tokio::sync::mpsc` receiver; `loxia-tui` passes `ratatui::Frame` to the binary). **`rand` is not
   on this list** — `ratatui-image` and `proptest` each pin their own independent major version, and
   since no `rand` type crosses a loxia crate boundary, the duplication is inert. Treat a `cargo
   tree -d` hit on `rand` (or on build-only crates like `syn`/`hashbrown`) as informational.
6. `cargo deny check` gates every PR. A new dependency with a copyleft licence blocks CI until it is
   recorded in `11-packaging.md` §6.
7. **`loxia-audio` may depend on `tokio` with the `sync` feature only** (no `rt-*`, no `macros`).
   `AudioBackend::subscribe()` returns a `tokio::sync::mpsc::UnboundedReceiver<AudioEvent>` because
   that channel is handed straight to the runtime's main loop — inventing a parallel channel type
   just to avoid naming `tokio::sync` would add a translation layer for no benefit. This is a
   narrow exception to "`tokio` goes to `loxia-player` only": the crate names a channel *type*, it does not
   spawn tasks or drive an executor. Cargo's feature unification means `rt-multi-thread` is still
   compiled in for the final binary regardless, since `loxia-player` requires it — the restriction here is
   about what `loxia-audio`'s own code is allowed to *use*, not what ends up in the dependency tree.
8. **`loxia-emby` may depend on `tokio` with the `time` feature only** (no `rt-*`, no `macros`, no
   `sync`). `retry::with_retry` needs an async sleep for its backoff delay between attempts, and
   `tokio::time::sleep` is the natural choice given `reqwest`/`tokio-tungstenite` already require a
   tokio runtime to drive their futures at all — reaching for a second timer crate (`futures-timer`)
   just to avoid naming `tokio` would add a dependency for no benefit. Same shape of exception as
   rule 7: this governs what `loxia-emby`'s own code may *use*, not what the final binary links,
   since `loxia-player` already requires the full runtime.

## Crates deliberately not used

| Not used | Instead | Why |
| :-- | :-- | :-- |
| `libmpv` | `libmpv2` | Abandoned; targets a pre-0.35 mpv ABI that no current distro ships |
| `fuzzy-matcher` | `nucleo-matcher` | Last released 2020; nucleo is faster and maintained |
| `fs2` | `fs4` | Last released 2018 |
| `chrono`, `time` | `jiff` | Modern API, correct arithmetic, smaller surface |
| `symphonia`, `cpal`, `rodio` | `libmpv2` | Re-implementing gapless FLAC streaming with exclusive-mode output is a multi-month detour; mpv does it correctly today |
| `crossbeam-channel` | `tokio::sync::mpsc` | One channel abstraction in the codebase, not two |
| `lazy_static`, `once_cell` | `std::sync::LazyLock` | In std since 1.80; well below the project's MSRV |
