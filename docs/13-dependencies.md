# Dependency inventory and policy

This document is the list `CONTRIBUTING.md`'s Definition of Done refers to ("No dependency added
that is not in `docs/13-dependencies.md`"). Every direct dependency of every workspace crate —
`[dependencies]`, `[build-dependencies]`, and `[dev-dependencies]` alike — must have a row here.
Transitive-only crates (anything that shows up only in `Cargo.lock`, never in a `Cargo.toml`) are
not tracked here; those are covered by `THIRD_PARTY_LICENSES.md` and `deny.toml` instead.

This revision was produced by reconciling this file against the root `Cargo.toml`
(`[workspace.dependencies]`) and `crates/loxia-audio/Cargo.toml` + `crates/loxia-audio/build.rs`.
**Caveat, stated up front:** `crates/loxia-core/Cargo.toml`, `crates/loxia-cache/Cargo.toml`,
`crates/loxia-emby/Cargo.toml`, `crates/loxia-player/Cargo.toml`, and `crates/loxia-tui/Cargo.toml`
were not available for inspection in this reconciliation pass (their contents were not provided to
the task that produced this revision), so the claim "every direct dependency of every workspace
crate appears below" is only fully verified for the root manifest and `loxia-audio`. Every one of
those five crates almost certainly draws only from `[workspace.dependencies]` below (that is the
whole point of rule 2), but that has not been independently confirmed line-by-line here. Whoever
next touches one of those five manifests should treat this file as provisionally, not finally,
reconciled for their crate, and correct this note once they've actually diffed it.

## Rules

1. `crossterm` and `time` are never declared as a direct dependency in any crate's
   `[dependencies]`/`[dev-dependencies]`/`[build-dependencies]` — both are required only
   transitively (`crossterm` via `ratatui`'s own re-export, used as `ratatui::crossterm`; `time`
   via `ratatui-widgets` and our own `tracing-appender`). `deny.toml`'s ban list can't be scoped to
   "direct only" without also breaking the legitimate transitive use, so this is enforced instead
   by a grep step in `.github/workflows/ci.yml` ("No direct crossterm/time dependency") over every
   `crates/*/Cargo.toml`.
2. Any dependency needed by more than one crate is declared exactly once, in root `Cargo.toml`'s
   `[workspace.dependencies]`, and consumed by member crates as `{ workspace = true }`. A crate may
   declare a dependency directly instead only when it is that crate's exclusive dependency, or it
   needs a strict subset of the workspace version's features (see rule 7).
3. `tokio`, `reqwest`, `ratatui`, and `image` must each resolve to a single version across the
   whole workspace dependency graph — enforced by the `cargo tree --duplicates` step in CI, not by
   `deny.toml` (whose multiple-versions check is graph-wide and can't single out specific crate
   names). A second, incompatible constraint on any of these four is a Definition-of-Done failure.
4. `loxia-core` has zero I/O (`CONTRIBUTING.md`'s Hard rules): it may not add `tokio`, `reqwest`,
   `ratatui`, `libmpv2`, or any other dependency that performs filesystem, network, or terminal
   I/O, or spawns a thread.
5. A native/system library linked by a crate's `build.rs` is named here even though it never
   appears in `Cargo.lock` (Cargo has no visibility into it at all), and its licence is recorded in
   `THIRD_PARTY_LICENSES.md`. Currently: `libmpv`, linked by `loxia-audio` — see §4 below.
6. A `[build-dependencies]` or `[dev-dependencies]` entry is tracked the same as a
   `[dependencies]` entry for the purposes of this document and of the Definition of Done — a
   dev-only or build-only addition still needs a row here before it lands.
7. **Exception to rule 2:** `loxia-audio`'s direct `tokio = { version = "1.53", default-features =
   false, features = ["sync"] }`. `AudioBackend::subscribe()` returns a `tokio::sync::mpsc` channel
   type, so the crate must name `tokio` itself, but it pulls in only the `sync` feature (not the
   full multi-threaded runtime `loxia-player`, the binary, uses) so that `loxia-audio` itself never
   becomes an async-runtime-owning crate. Sanctioned by `docs/12-decisions.md` §9.

## 1. Workspace-level dependencies (`[workspace.dependencies]`, root `Cargo.toml`)

State: **(a)** for every row below — each is both declared in the manifest and listed here.

### Async runtime & networking

| Dependency | Version / features | Notes |
| :-- | :-- | :-- |
| `tokio` | `1.53`, features `rt-multi-thread, macros, sync, time, fs, io-util, signal` | Full runtime; used by `loxia-player` and other I/O crates. `loxia-audio` uses a narrower direct pin — see rule 7. |
| `reqwest` | `0.13`, `default-features = false`, features `json, query, stream, rustls` | HTTP client, rustls (not native-tls) backend — no OpenSSL system dependency. |
| `tokio-tungstenite` | `0.30`, features `rustls-tls-webpki-roots` | WebSocket client for the Emby real-time API. |
| `rustls` | `0.23` | TLS backend shared by `reqwest`/`tokio-tungstenite`. |
| `futures` | `0.3` | Stream/future combinators. |
| `bytes` | `1.12` | Shared buffer type across the HTTP/audio boundary. |

### Serialization & config

| Dependency | Version / features | Notes |
| :-- | :-- | :-- |
| `serde` | `1.0.229`, feature `derive` | |
| `serde_json` | `1.0.151` | Emby DTOs. |
| `toml` | `1.1` | Config file and `assets/eq_presets.toml`. |

### TUI & terminal graphics

| Dependency | Version / features | Notes |
| :-- | :-- | :-- |
| `ratatui` | `0.30.2` | Re-exports `crossterm` as `ratatui::crossterm` — see rule 1. |
| `ratatui-image` | `11.0.6`, `default-features = false`, feature `crossterm` | The `crossterm` *feature* here selects `ratatui-image`'s own crossterm-backed terminal query code; it is not a direct `crossterm` dependency declaration (rule 1 still holds). |
| `image` | `0.25`, `default-features = false`, features `jpeg, png` | Album art decoding. |

### Audio backend

| Dependency | Version / features | Notes |
| :-- | :-- | :-- |
| `libmpv2` | `6.0` | Safe libmpv bindings; see §4 for the underlying system library. |

### Cross-platform OS integrations

| Dependency | Version / features | Notes |
| :-- | :-- | :-- |
| `souvlaki` | `0.8` | MPRIS / OS media-key integration. |
| `notify-rust` | `4.18` | Desktop notifications. |
| `dirs` | `6.0` | Per-OS config/cache directory resolution. |

### Utilities

| Dependency | Version / features | Notes |
| :-- | :-- | :-- |
| `nucleo-matcher` | `0.3` | Fuzzy filter/search. |
| `jiff` | `0.2`, feature `serde` | Date/time; the sanctioned replacement for `time` (rule 1). |
| `fs4` | `1.1`, feature `sync` | Cross-platform advisory file locking (cache lockfile). |
| `rand` | `0.10` | Shuffle. |
| `rand_chacha` | `0.10` | Deterministic seeded RNG for shuffle tests. |
| `uuid` | `1.24`, features `v4, serde` | |
| `unicode-width` | `0.2` | TUI text layout. |
| `unicode-segmentation` | `1.13` | TUI text layout. |
| `strum` | `0.28`, feature `derive` | Enum ↔ string conversions. |
| `thiserror` | `2.0` | Error types across every crate. |
| `anyhow` | `1.0` | `loxia-player`'s bootstrap-only error type. |
| `clap` | `4.6`, features `derive, env` | CLI parsing. |
| `tracing` | `0.1` | |
| `tracing-subscriber` | `0.3`, features `env-filter, fmt` | |
| `tracing-appender` | `0.2` | Pulls in `time` transitively — see rule 1. |
| `sha2` | `0.11` | Cache key hashing. |
| `gethostname` | `1.1` | Emby device identification. |
| `smallvec` | `1.15` | |

### Dev dependencies (workspace-level)

| Dependency | Version | Notes |
| :-- | :-- | :-- |
| `insta` | `1.48` | Snapshot testing. |
| `wiremock` | `0.6` | HTTP mocking for `loxia-emby`. |
| `proptest` | `1.11` | Property tests (queue shuffle, EQ). |
| `tempfile` | `3.27` | Cache/session I/O tests. |
| `pretty_assertions` | `1.4` | |

## 2. Crate-specific direct dependencies

### `loxia-audio` (`crates/loxia-audio/Cargo.toml`)

| Dependency | Where | Version | State | Notes |
| :-- | :-- | :-- | :-- | :-- |
| `loxia-core` | `[dependencies]` | path | (a) | Internal workspace crate, not a third-party dependency; listed for completeness. |
| `libmpv2` | `[dependencies]` | workspace | (a) | See §1. |
| `libmpv2-sys` | `[dependencies]` | `4.0` | **(b)** | Not in `[workspace.dependencies]`. Names, directly, a crate already present transitively as `libmpv2`'s own sys dependency, so that `mpv_request_log_messages` can be called via raw FFI against `Mpv::ctx` (`libmpv2` itself has no safe wrapper for it — its own doc comment on `create_client` calls the call "unimplemented"). The crate's own `Cargo.toml` comment points at `docs/12-decisions.md` for the rationale; treated here as sanctioned by that existing entry. |
| `thiserror` | `[dependencies]` | workspace | (a) | |
| `tracing` | `[dependencies]` | workspace | (a) | |
| `toml` | `[dependencies]` | workspace | (a) | Parses the embedded `assets/eq_presets.toml`. |
| `serde` | `[dependencies]` | workspace | (a) | `PresetsFile`'s own `#[derive(Deserialize)]`. |
| `tokio` | `[dependencies]` | `1.53`, `sync` only | **(b)** | Sanctioned exception, rule 7 above / `docs/12-decisions.md` §9. |
| `pkg-config` | `[build-dependencies]` | `0.3` | **(c)** | Not in `[workspace.dependencies]`, and its in-manifest comment gives a solid rationale (adding `-L`/rpath for libmpv on installs where it isn't on the linker's default path, e.g. Homebrew-on-Linux) but does **not** cite an existing `docs/12-decisions.md` entry the way the `libmpv2-sys` and `tokio` rows above do. **Flagged for the maintainer:** please add a `docs/12-decisions.md` §9 row for this build-dependency (it is clearly a deliberate, reasoned addition, not an oversight) so this can move from (c) to (b); recorded here in the meantime as an undocumented-but-apparently-intentional addition, per this task's own instructions. |
| `proptest` | `[dev-dependencies]` | workspace | (a) | |
| `pretty_assertions` | `[dev-dependencies]` | workspace | (a) | |
| `insta` | `[dev-dependencies]` | workspace | (a) | |
| `tiny_http` | `[dev-dependencies]` | `0.12` | **(c)** | Not in `[workspace.dependencies]` and not sanctioned by any `docs/12-decisions.md` entry found. Used only by the `mpv-tests`-gated header-arrival integration test (a local server asserting a custom header reached mpv's own HTTP request), so it is dev-only and never ships in a release binary, but it is still a new direct dependency per rule 6. **Flagged for the maintainer to ratify** with a `docs/12-decisions.md` row, or to replace with an existing workspace HTTP-mocking dependency (`wiremock`) if that can serve the same test. |

## 3. libmpv — system dependency (not in `Cargo.lock`)

`crates/loxia-audio` links dynamically against native **libmpv**, not just against the `libmpv2`/
`libmpv2-sys` Rust crates:

- `crates/loxia-audio/build.rs` probes for it via `pkg-config::Config::new().probe("mpv")` and, if
  found, emits `cargo:rustc-link-search=native=...` and a matching `-Wl,-rpath,...` link arg.
- `libmpv2-sys`'s own build script (a transitive dependency, not vendored here) emits the actual
  `cargo:rustc-link-lib=mpv`.
- Neither of those facts, nor the library itself, is representable in `Cargo.lock` — `libmpv` is
  never fetched, built, or version-pinned by Cargo. It is expected to be present on the host system
  (see `crates/loxia-audio/src/error.rs::library_not_found_hint` for the per-OS install
  instructions shown to a user when it is missing).

**Status, stated explicitly:** `libmpv` is a required system dependency of `loxia-audio` (and
therefore of `loxia-player`), licensed **LGPL-2.1-or-later**, dynamically linked. It is documented
in `THIRD_PARTY_LICENSES.md` under `## libmpv`, which already carries the correct licence notice —
that section is confirmed accurate against `build.rs`/`Cargo.toml` in this pass and needed no
change.

## 4. Reverse check — entries with no manifest backing

Every entry above traces to an actual line in either the root `Cargo.toml` or
`crates/loxia-audio/Cargo.toml`, both inspected directly for this revision. No previous version of
this file was available to diff against in this pass (its prior content was not provided to the
task that produced this revision), so a true before/after "was documented but no longer used"
comparison could not be performed. If a prior revision of this file listed something not present
above (for example, a dependency version or feature set that no longer matches
`[workspace.dependencies]`), it has been superseded by this rewrite; anyone who still has the prior
text should diff it against §1–§2 above and fold in anything genuinely still true that this pass
missed.

## 5. `cargo tree -i crossterm` check (task step 6)

Requested command: `cargo tree --workspace -i crossterm -e normal --depth 1`.

This could not be executed against a live `cargo` in the environment this reconciliation was
performed in. As a substitute, every manifest actually available in this pass was checked by hand
for a direct `crossterm` (or `time`) line:

- Root `Cargo.toml` (`[workspace.dependencies]`): no `crossterm` entry. `ratatui-image` carries a
  `crossterm` *feature flag*, not a `crossterm` dependency declaration; `ratatui` itself re-exports
  `crossterm` as `ratatui::crossterm` per `CONTRIBUTING.md`'s Hard rules.
- `crates/loxia-audio/Cargo.toml`: no `crossterm` entry (and none would be legitimate there — this
  crate has no UI layer).
- `crates/loxia-core/Cargo.toml`, `crates/loxia-cache/Cargo.toml`, `crates/loxia-emby/Cargo.toml`,
  `crates/loxia-player/Cargo.toml`, `crates/loxia-tui/Cargo.toml`: **not available for inspection
  in this pass.** `loxia-tui` in particular is the crate most likely to need this check, since it is
  the one that actually calls into `ratatui::crossterm` for terminal I/O.
- `.github/workflows/ci.yml` already runs an equivalent check on every push/PR ("No direct
  crossterm/time dependency"): it greps every `crates/*/Cargo.toml` for a line starting `crossterm`
  or `time` and fails the build if one is found.

**Result recorded here: no direct `crossterm` dependency found in the manifests actually inspected
in this pass (root and `loxia-audio`), and the repository's own CI gate independently re-checks all
five member crates on every commit.** This is not the same as a clean `cargo tree -i crossterm`
result, since three-fifths of the member crates were not directly inspected here. Per this task's
own instructions: if a future run of `cargo tree --workspace -i crossterm -e normal --depth 1` (or
the CI grep step) does turn up a direct dependency, that is a code defect to fix in a separate PR —
record the offending `Cargo.toml:line` there, not in this document.

## 6. Open items for the maintainer (undocumented additions to ratify)

The following are genuine, present-in-the-manifest dependencies that this pass could not trace to
an existing `docs/12-decisions.md` entry. They are listed above with state **(c)** and repeated
here for visibility:

- `crates/loxia-audio/Cargo.toml` `[build-dependencies]` — `pkg-config = "0.3"`.
- `crates/loxia-audio/Cargo.toml` `[dev-dependencies]` — `tiny_http = "0.12"`.

Both have a clear, reasoned, in-manifest justification (see §2 above) and neither ships in a
release binary's dependency set in a way that changes runtime behaviour, but per the Definition of
Done neither should have landed without a corresponding `docs/12-decisions.md` §9 row. Ratify by
either adding that row (promoting them to state (b)) or removing them if a workspace dependency can
serve the same purpose.

===FILE: docs/12-decisions.md===
DELETE_NOT_APPLICABLE
