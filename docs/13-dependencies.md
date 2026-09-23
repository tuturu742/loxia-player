# Dependencies

This is the single source of truth for what loxia may depend on. `CONTRIBUTING.md`'s Definition
of Done says "no dependency added that is not in `docs/13-dependencies.md`" — additions and
version bumps land here in the same PR that changes `Cargo.toml`/`Cargo.lock`, not after.

Every entry below is pinned exactly as it appears in `Cargo.toml` (workspace) or a crate's own
`Cargo.toml` (per-crate exception). If a line here and the manifest ever disagree, the manifest is
correct and this file is wrong — file a doc-fix PR, do not "fix" the manifest to match stale docs.

## 1. Toolchain

| What | Version | Pinned in |
| :-- | :-- | :-- |
| Rust | `1.97.1` | `Cargo.toml` (`workspace.package.rust-version`), `rust-toolchain.toml`, `clippy.toml` (`msrv`) |
| Edition | `2024` | `Cargo.toml` (`workspace.package.edition`) |

CI (`.github/workflows/ci.yml`) installs `dtolnay/rust-toolchain@stable`, but rustup transparently
overrides that with the channel pinned in `rust-toolchain.toml` on the first `cargo`/`rustc`
invocation in the checkout — the effective compiler in CI is always `1.97.1`.

There is no Node, Python, Go, or JVM runtime anywhere in the dependency graph. `scripts/*.py` are
maintainer-only utilities invoked ad hoc with whatever `python3` is on the developer's `PATH`; they
are not part of the build, the test suite, or a shipped artifact, so no Python version is pinned.

## 2. Native libraries

| What | How it's linked | Declared in |
| :-- | :-- | :-- |
| `libmpv` | Dynamically, at runtime, via `libmpv2`/`libmpv2-sys` | `crates/loxia-audio/Cargo.toml` (`libmpv2 = "6.0"`, `libmpv2-sys = "4.0"`) |
| `pkg-config` (build-time probe only, not linked into the binary) | Locates `libmpv`'s `-L` search path/rpath when it isn't on the linker's default path | `crates/loxia-audio/Cargo.toml` (`[build-dependencies] pkg-config = "0.3"`) |

`libmpv` is the only external, non-Rust runtime requirement. Its licence (LGPL-2.1-or-later) is
recorded in `THIRD_PARTY_LICENSES.md`.

## 3. Datastores and external services

**None.** loxia has no database, no cache server, and no message broker of any kind. Searching
every manifest and lockfile in the repository (`Cargo.toml`, `crates/*/Cargo.toml`, `Cargo.lock`)
for a storage client (`redis`, `sqlite`, `mongo`, `elasticsearch`/`opensearch`, or any vector
store such as `qdrant`/`pinecone`/`weaviate`/`milvus`/`chroma`/`lancedb`/`pgvector`) returns zero
hits — there is no such dependency anywhere in this codebase.

The two things that might look like "a datastore" from a distance are not one:

- **The Emby media server** is the single source of truth for the library; loxia talks to it over
  HTTP/WebSocket via `crates/loxia-emby` (`reqwest`, `tokio-tungstenite`). It is a user-supplied
  external service, not a dependency of this project.
- **The on-disk cache** (`crates/loxia-cache`, see `docs/06-cache-and-offline.md`) is a plain
  filesystem layout under the OS cache/config directories (`dirs = "6.0"`) — a manifest file and
  cached media blobs, not a database engine, and not something this project links against or
  ships.

## 4. Workspace dependencies

Pinned in `Cargo.toml` under `[workspace.dependencies]`; every crate that uses one of these
depends on it via `{ workspace = true }`, never a re-pinned version of its own (`crates/loxia-audio`'s
`tokio` line is the one documented exception — see rule 7 below).

### Async runtime & networking

| Crate | Version |
| :-- | :-- |
| `tokio` | `1.53` (features: `rt-multi-thread`, `macros`, `sync`, `time`, `fs`, `io-util`, `signal`) |
| `reqwest` | `0.13` (default-features off; `json`, `query`, `stream`, `rustls`) |
| `tokio-tungstenite` | `0.30` (`rustls-tls-webpki-roots`) |
| `rustls` | `0.23` |
| `futures` | `0.3` |
| `bytes` | `1.12` |

### Serialization & config

| Crate | Version |
| :-- | :-- |
| `serde` | `1.0.229` (`derive`) |
| `serde_json` | `1.0.151` |
| `toml` | `1.1` |

### TUI & terminal graphics

| Crate | Version |
| :-- | :-- |
| `ratatui` | `0.30.2` |
| `ratatui-image` | `11.0.6` (default-features off; `crossterm`) |
| `image` | `0.25` (default-features off; `jpeg`, `png`) |

### Audio backend

| Crate | Version |
| :-- | :-- |
| `libmpv2` | `6.0` |

### Cross-platform OS integrations

| Crate | Version |
| :-- | :-- |
| `souvlaki` | `0.8` |
| `notify-rust` | `4.18` |
| `dirs` | `6.0` |

### Utilities

| Crate | Version |
| :-- | :-- |
| `nucleo-matcher` | `0.3` |
| `jiff` | `0.2` (`serde`) |
| `fs4` | `1.1` (`sync`) |
| `rand` | `0.10` |
| `rand_chacha` | `0.10` |
| `uuid` | `1.24` (`v4`, `serde`) |
| `unicode-width` | `0.2` |
| `unicode-segmentation` | `1.13` |
| `strum` | `0.28` (`derive`) |
| `thiserror` | `2.0` |
| `anyhow` | `1.0` |
| `clap` | `4.6` (`derive`, `env`) |
| `tracing` | `0.1` |
| `tracing-subscriber` | `0.3` (`env-filter`, `fmt`) |
| `tracing-appender` | `0.2` |
| `sha2` | `0.11` |
| `gethostname` | `1.1` |
| `smallvec` | `1.15` |

### Dev dependencies

| Crate | Version |
| :-- | :-- |
| `insta` | `1.48` |
| `wiremock` | `0.6` |
| `proptest` | `1.11` |
| `tempfile` | `3.27` |
| `pretty_assertions` | `1.4` |

## 5. Per-crate exceptions to the workspace set

These are declared outside `[workspace.dependencies]`, in a single crate's own `Cargo.toml`, and
are exceptions on purpose:

- **`crates/loxia-audio/Cargo.toml`**: `libmpv2-sys = "4.0"` — named directly only to route mpv's
  internal log messages into `tracing` via a raw FFI call libmpv2 itself doesn't wrap; it is
  already in the graph transitively as `libmpv2`'s own sys crate, so this pins the identical
  version rather than adding a new one.
- **`crates/loxia-audio/Cargo.toml`**: `pkg-config = "0.3"` (build-dependency) — probes for
  `libmpv`'s link path at build time; a no-op when the library is already on the default search
  path.
- **`crates/loxia-audio/Cargo.toml`**: `tiny_http = "0.12"` (dev-dependency) — the `mpv-tests`
  suite's header-arrival integration test only.
- **`crates/loxia-audio/Cargo.toml`**: `toml`, `serde` (direct, not `{ workspace = true }` for a
  wrapper type) and `insta` (dev) — parsing `assets/eq_presets.toml` and snapshotting the EQ
  filter-string/command output.

## 6. Rules

1. **`crossterm` and `time` are never a direct dependency** of any crate in this workspace. Both
   are required transitively (`crossterm` via `ratatui::crossterm`, `time` via `ratatui-widgets`
   and `tracing-appender`), so `cargo-deny` cannot ban them outright (see `deny.toml`); CI enforces
   this instead by grepping every `crates/*/Cargo.toml` for a bare `crossterm`/`time` dependency
   line (`.github/workflows/ci.yml`, "No direct crossterm/time dependency").
2. **`tokio`, `reqwest`, `ratatui`, and `image` must never appear at more than one version** in the
   resolved dependency graph — their types cross crate boundaries in this workspace, so a
   duplicate would silently split incompatible types across crates. Enforced by CI via
   `cargo tree --workspace -e normal --duplicates`.
3. `loxia-core` has zero I/O dependencies: no `tokio`, `reqwest`, `ratatui`, or filesystem access
   (`CONTRIBUTING.md`, `docs/01-architecture.md`).
4. `loxia-audio` and `loxia-emby` may not depend on each other.
5. No dependency is added to any crate that is not listed in this file first, in the same PR.
6. Every dependency's licence must be compatible with `GPL-3.0-or-later` and appear in
   `THIRD_PARTY_LICENSES.md`, regenerated via `cargo deny list --format human --layout crate`.
7. **Exception:** `crates/loxia-audio/Cargo.toml` pins its own `tokio = { version = "1.53",
   default-features = false, features = ["sync"] }` rather than using
   `{ workspace = true }`. `AudioBackend::subscribe()` only needs the `tokio::sync::mpsc` channel
   *type*, not the runtime — the version must still track the workspace's `1.53`, but the feature
   set is deliberately narrower than `loxia-player`'s full runtime.
8. No datastore, cache server, or message broker dependency (Redis, SQLite, Mongo, Elasticsearch,
   any vector database) may be added. loxia is a local client for a user-supplied Emby server;
   see §3.
