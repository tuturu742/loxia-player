# Locked Dependency Set

This is the canonical list of loxia's runtime and development dependencies: every crate named in
`[workspace.dependencies]` in the root `Cargo.toml`, plus the handful of per-crate additions each
member crate makes directly (§5). Every entry below has been checked against its published
crates.io listing for licence and maintenance status as part of `12-05-licence-compliance-checks`;
`design_overview` §8 left this list as `TBD` in the original spec, and this document — together
with the workspace `Cargo.toml`/`Cargo.lock` — is what closed that gap.

**This document is descriptive, not authoritative.** `Cargo.toml` and `Cargo.lock` are the ground
truth for exact versions and feature sets. Where this file and the manifest disagree, the manifest
wins and this file is wrong and should be fixed — see `docs/12-decisions.md` for any place a past
implementation deviation was recorded instead of silently "fixing" the doc.

## Rules

1. `crossterm` and `time` are never a *direct* dependency of any crate in this workspace. Both are
   pulled in transitively (`crossterm` via `ratatui`'s own `crossterm` backend, `time` via
   `ratatui`'s widgets and this workspace's own `tracing-appender`), so `cargo-deny`'s ban list
   can't forbid them outright without breaking the build — CI enforces "never direct" instead by
   grepping every crate's `Cargo.toml` for a bare `crossterm`/`time` line
   (`.github/workflows/ci.yml`, the "No direct crossterm/time dependency" step). Use
   `ratatui::crossterm` and `jiff` respectively.
2. TLS is rustls-only, never `native-tls`/OpenSSL. `reqwest` disables its default features and
   opts back into `rustls` explicitly (`Cargo.toml:16`); `tokio-tungstenite` uses
   `rustls-tls-webpki-roots` (`Cargo.toml:17`); `rustls` itself is a direct workspace dependency
   (`Cargo.toml:18`) so its version is pinned rather than left to whatever the transitive graph
   would otherwise resolve.
3. `anyhow` is never used inside a library crate (`loxia-core`, `loxia-emby`, `loxia-audio`,
   `loxia-cache`, `loxia-tui`) — every library-crate error type is a concrete `thiserror` enum
   (each crate's own `error.rs`), so a caller can match on it. `anyhow` (`Cargo.toml:51`) is
   reserved for `loxia-player`'s own `main.rs`/bootstrap error handling and for test bodies, where
   an opaque, context-annotated error is the right shape.
4. Every dependency that supports it disables default features and opts back in explicitly, so an
   unused default feature (and its transitive dependencies) never silently enters the build.
   `reqwest` (`Cargo.toml:16`), `ratatui-image` (`Cargo.toml:29`) and `image` (`Cargo.toml:30`) all
   set `default-features = false`. Where a crate's defaults are already minimal and match what is
   needed, this is a no-op rather than an omission.
5. `tokio`, `reqwest`, `ratatui`, and `image` must resolve to a single version each across the
   whole workspace, because their types cross crate boundaries in this codebase (an
   `AudioEvent` channel, an HTTP response body, a rendered `Frame`, a decoded album-art buffer).
   `cargo-deny`'s graph-wide multiple-versions check can't single these four out by name (see
   `deny.toml`), so CI checks them directly instead (`.github/workflows/ci.yml`, the "No duplicate
   tokio/reqwest/ratatui/image versions" step, via `cargo tree --duplicates`).
6. Copyleft (LGPL/GPL) licences are not banned outright — loxia itself is
   `GPL-3.0-or-later` (`Cargo.toml:9`) and links dynamically against LGPL-2.1+ `libmpv`
   (`THIRD_PARTY_LICENSES.md`, "## libmpv"). Every copyleft licence actually present in the
   dependency graph is recorded in `docs/11-packaging.md` §6, with the compliance obligation it
   carries (dynamic linking, source-availability, etc.) — this file only names the dependency,
   that file is where the licence *obligation* is tracked.
7. `loxia-audio`'s direct `tokio` dependency (`crates/loxia-audio/Cargo.toml:35`,
   `default-features = false, features = ["sync"]`) is one of several documented per-crate
   exceptions to "depend on the workspace-pinned crate as-is" — see §5 below for the full list and
   why each one exists. This entry alone does *not* make it "the one" exception; §4/§5 below used
   to contradict each other on that point and have been reconciled.
8. `loxia-emby`'s direct `tokio` dependency (`crates/loxia-emby/Cargo.toml`, `time`/`fs`/`io-util`
   features only, no `rt-multi-thread`/`macros`/`signal`) is the other crate-level exception in
   the same family as rule 7: a library crate that needs a specific `tokio` *type* or utility
   (here, retry/backoff timers and streamed-download file I/O) without pulling in a second async
   runtime. If `crates/loxia-emby/Cargo.toml`'s feature list for `tokio` is ever changed, this rule
   and §5 below must be updated to match in the same PR.

## Datastores

loxia has exactly one persistence mechanism: plain files on disk, written by `loxia-cache`
(manifest/session/offline-index files under the user's cache directory — `docs/06-cache-and-offline.md`)
and by `loxia-core::config` (a single `config.toml`). There is no embedded or external database —
no SQLite, no Redis, no Postgres, no vector store of any kind — anywhere in the dependency graph.
A repository-wide search for a datastore client or service turned up zero hits; see the PR
description for the exact command and output. Nothing here should be added to this section unless
a manifest actually declares it.

## Runtime dependencies

| Crate | Features | Used by | Purpose |
| :-- | :-- | :-- | :-- |
| `tokio` | `rt-multi-thread`, `macros`, `sync`, `time`, `fs`, `io-util`, `signal` (full set: `loxia-player` only) | `loxia-player` (owns the runtime; all `workers/*`); `loxia-audio` and `loxia-emby` take a reduced feature set directly (§5, rules 7–8) | Async runtime for networking, the audio-event channel, and file I/O |
| `reqwest` | `json`, `query`, `stream`, `rustls` | `loxia-emby` (`client.rs`, `endpoints/*`) | HTTP client for the Emby API |
| `tokio-tungstenite` | `rustls-tls-webpki-roots` | `loxia-emby` (`ws.rs`) | Emby WebSocket remote control |
| `rustls` | — | transitive TLS backend for `reqwest`/`tokio-tungstenite` (rule 2) | TLS |
| `futures` | — | `loxia-emby`, `loxia-player` (`workers/*`) | Async combinators/streams |
| `bytes` | — | `loxia-emby` (streamed response bodies), `loxia-cache` (`downloads.rs`) | Shared byte-buffer type |
| `serde` | `derive` | `loxia-core` (`config/schema.rs`), `loxia-emby` (`dto/*`), `loxia-cache` (`manifest.rs`, `session.rs`, `offline_index.rs`) | (De)serialization |
| `serde_json` | — | `loxia-emby` (`dto/*`), `loxia-cache` | JSON (Emby API responses, cache manifests) |
| `toml` | — | `loxia-core` (`config/mod.rs`) | Config file format |
| `ratatui` | — | `loxia-tui` (all `views/*`, `widgets/*`), `loxia-player` (`terminal.rs`) | TUI rendering |
| `ratatui-image` | `crossterm` (default features off, `Cargo.toml:29`) | `loxia-tui` (`widgets/album_art.rs`) | Terminal album-art rendering |
| `image` | `jpeg`, `png` (default features off, `Cargo.toml:30`) | `loxia-tui` (`widgets/album_art.rs`) | Album-art decoding |
| `libmpv2` | — | `loxia-audio` (`mpv/*`) | Audio playback backend |
| `souvlaki` | — | `loxia-player` (`workers/mpris.rs`) | MPRIS / OS media-key integration |
| `notify-rust` | — | `loxia-player` (`workers/notify.rs`) | Desktop notifications |
| `dirs` | — | `loxia-core` (`paths.rs`) | Per-OS config/cache directory resolution |
| `nucleo-matcher` | — | `loxia-tui` (`views/search.rs`, inline filter) | Fuzzy matching |
| `jiff` | `serde` | `loxia-core`, `loxia-cache` (`session.rs`) | Timestamps/durations |
| `fs4` | `sync` | `loxia-cache` (manifest/session file locking) | Cross-process file locks |
| `rand` / `rand_chacha` | — | `loxia-core` (`queue/shuffle.rs`) | Shuffle |
| `uuid` | `v4`, `serde` | `loxia-core` (`model/ids.rs`), `loxia-emby` (device id) | Identifiers |
| `unicode-width` / `unicode-segmentation` | — | `loxia-tui` (`text.rs`) | Terminal-column-correct text layout |
| `strum` | `derive` | `loxia-core` (`config/schema.rs`, keymap) | Enum ↔ string mapping |
| `thiserror` | — | every library crate's own `error.rs` | Error types |
| `anyhow` | — | `loxia-player` only (rule 3) | Bootstrap/CLI error context |
| `clap` | `derive`, `env` | `loxia-player` (`main.rs`) | CLI argument parsing |
| `tracing` / `tracing-subscriber` / `tracing-appender` | `env-filter`, `fmt` | `loxia-player` (logging setup), instrumented throughout | Structured logging |
| `sha2` | — | `loxia-cache` (`layout.rs`) | Cache-key hashing |
| `gethostname` | — | `loxia-emby` (`auth.rs`, device info) | Device identification for Emby auth |
| `smallvec` | — | `loxia-core` (`queue.rs`) | Small-vector optimisation |

## Per-crate exceptions

Everything in the table above is declared once, in `[workspace.dependencies]`, and referenced by
member crates as `{ workspace = true }`. A small number of crate-level `Cargo.toml`s declare a
dependency *directly* instead, each for a stated reason. This is the complete list — §4's rule 7
note above is not "the one" exception, it is one entry in this list:

- `crates/loxia-audio/Cargo.toml:22` — `libmpv2-sys = "4.0"`. Needed only to call
  `mpv_request_log_messages` directly against `Mpv::ctx`, which `libmpv2` itself does not wrap
  (`docs/12-decisions.md`). Already in the graph transitively as `libmpv2`'s own sys crate; this
  names the exact same version directly rather than adding a second one.
- `crates/loxia-audio/Cargo.toml:29` — `toml = { workspace = true }` and
  `crates/loxia-audio/Cargo.toml:30` — `serde = { workspace = true }`, both used directly (not
  transitively via `loxia-core`) to parse `assets/eq_presets.toml` through a local `PresetsFile`
  wrapper struct that needs its own `#[derive(Deserialize)]` (`09-03`).
- `crates/loxia-audio/Cargo.toml:35` — `tokio` with only the `sync` feature (rule 7): the audio
  backend's `subscribe()` returns a `tokio::sync::mpsc` receiver, so this crate names the channel
  *type* only, not the runtime `loxia-player` owns.
- `crates/loxia-audio/Cargo.toml:43` — `pkg-config = "0.3"` (build-dependency). `libmpv2-sys`'s own
  `build.rs` emits a bare `-lmpv` with no search path; this crate's `build.rs` probes `pkg-config`
  for the right `-L`/rpath when libmpv isn't on the linker's default path (e.g. Homebrew-on-Linux).
  A no-op where it isn't needed.
- `crates/loxia-audio/Cargo.toml:50` — `tiny_http = "0.12"` (dev-dependency). Used only by the
  `mpv-tests`-gated header-arrival integration test, asserting a custom HTTP header actually
  reached mpv's request.
- `crates/loxia-emby/Cargo.toml` — `tokio` with only `time`/`fs`/`io-util` features (rule 8): retry
  backoff timers and streamed-download file I/O, without owning a runtime.

## Dev dependencies

| Crate | Features | Used by | Purpose |
| :-- | :-- | :-- | :-- |
| `insta` | — | `loxia-audio` (`eq.rs` snapshots), `loxia-core` (keymap snapshot), `loxia-tui` (view/widget/modal snapshots), `loxia-emby` (query/stream/endpoint snapshots) | Snapshot testing |
| `wiremock` | — | `loxia-emby` (HTTP client tests) | Mock HTTP server |
| `proptest` | — | `loxia-audio` (`eq.rs`), `loxia-core` | Property-based testing |
| `tempfile` | — | `loxia-cache` (manifest/LRU/download tests) | Temporary directories/files in tests |
| `pretty_assertions` | — | `loxia-audio`, workspace-wide | Readable test-assertion diffs |
| `tokio` | `test-util` | `loxia-cache` (dev-only, async test helpers) | Deterministic async test timing |
| `tiny_http` | — | `loxia-audio` (dev-dependency; see per-crate exceptions above, `mpv-tests`) | Local test HTTP server |

## Crates deliberately not used

Design rationale, not a dependency list — recorded here so the same "why not just use X" question
doesn't get re-litigated per task:

- **`crossterm` / `time` as direct dependencies** — never added directly; see rule 1. `ratatui`
  re-exports the former, `jiff` replaces the latter everywhere loxia's own code needs a
  date/duration type.
- **A database crate (`rusqlite`, `sqlx`, `sled`, …)** — not used, and not missing. loxia persists
  only flat `config.toml` and cache-manifest/session files on disk (`docs/06-cache-and-offline.md`);
  there is no query surface that would justify an embedded or external database, and none is
  declared anywhere in the workspace (see "Datastores" above).
- **`log`** — `tracing` is used everywhere instead, for structured, span-aware logging; adding
  `log` alongside it would mean two logging facades to keep straight.
- **`config`/`figment`-style layered config crates** — the config schema is small enough that a
  single `toml`-backed struct with hand-written migration (`config/migrate.rs`) is simpler than a
  general layered-config framework, and keeps `loxia-core` free of extra dependencies per
  `CONTRIBUTING.md`'s "`loxia-core` has zero I/O" rule.
- **`native-tls`/OpenSSL bindings** — rustls-only by policy (rule 2); never added even as an
  optional feature, since a build-time TLS-backend choice is exactly the kind of thing that quietly
  reintroduces a system OpenSSL dependency on some platforms and not others.
- **A second HTTP client (`ureq`, `isahc`, …)** — `reqwest` covers every HTTP need (`loxia-emby`);
  a second client would violate rule 5 in spirit even if it named a different crate.
