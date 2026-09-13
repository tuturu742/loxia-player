# 00-05 · cargo-deny policy

**Phase:** 00 — Scaffolding · **Agent:** E · **Size:** S
**Prerequisites:** `00-04`
**Reference:** `docs/11-packaging.md` §7, `docs/13-dependencies.md`

## Goal
Add `deny.toml` so licence obligations and security advisories are tracked mechanically. The project
ships GPL-3.0 software that links LGPL libraries; a dependency sneaking in an incompatible licence
must fail CI, not surface at release time.

## Files
- `deny.toml`

## Specification

**`[licenses]`**
- `allow` = `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`,
  `ISC`, `Unicode-3.0`, `Zlib`, `MPL-2.0`, `CC0-1.0`, `GPL-3.0-or-later`, `LGPL-2.1`,
  `CDLA-Permissive-2.0`. The last one is not a code licence — it covers `webpki-roots`'
  Mozilla-sourced CA certificate bundle data, pulled in transitively for TLS root verification, and
  is permissive by design. Our own workspace crates use the SPDX identifier `GPL-3.0-or-later`
  (matching `Cargo.toml`'s `license.workspace = true`), not bare `GPL-3.0` — allow the identifier
  that is actually declared.
- `confidence-threshold = 0.9`.
- Any licence outside the allow list is an **error**. Adding to the list requires a note in
  `docs/11-packaging.md` §7 in the same PR.

**`[bans]`**
- `multiple-versions = "warn"` globally. cargo-deny's `multiple-versions` check is graph-wide, not
  per-crate, so it cannot natively express "deny only for these four crates" — third-party crates
  duplicate transitive deps like `rand` and `syn` routinely and harmlessly (see
  `docs/13-dependencies.md` rule 5), and a hard `"deny"` here would fail CI on things we don't
  control. The crates that structurally must not duplicate — `tokio`, `reqwest`, `ratatui`,
  `image`, because their types cross our own crate boundaries — are enforced instead by a small
  dedicated step in `.github/workflows/ci.yml`'s `lint` job that greps `cargo tree -d` for exactly
  those four names. Add that step in this task alongside `deny.toml`.
- `deny` these outright via `[[bans.deny]]`, with an explanatory comment each:
  - `openssl`, `openssl-sys`, `native-tls` — the project is rustls-only
  - `chrono`, `fs2`, `fuzzy-matcher`, `libmpv` — superseded, see `docs/12-decisions.md` §1, and
    confirmed absent from the resolved graph (`cargo tree -i <crate>` errors "did not match any
    packages" for all four)
  - **Not `time`, and not `crossterm`.** Both are genuinely superseded *for our own code* (`time`
    by `jiff`, `crossterm` by using it only through `ratatui::crossterm`), but both are also
    unavoidable transitive dependencies here — `time` via `ratatui-widgets` and via our own
    `tracing-appender`; `crossterm` via `ratatui-crossterm`. cargo-deny's ban mechanism operates on
    the resolved graph and cannot distinguish "direct" from "transitive", so banning either name
    outright fails the build. Confirm with `cargo tree -i time` and `cargo tree -i crossterm`
    before assuming a crate is absent — "superseded" in `12-decisions.md` §1 describes what *we*
    depend on, not every crate that could ever appear in the tree. Enforce "never a direct
    dependency of a loxia crate" for both instead with a permanent
    `grep -rE "^(time|crossterm) " crates/*/Cargo.toml` step in the `lint` CI job, alongside the
    tokio/reqwest/ratatui/image duplicate check.

**`[advisories]`** — `yanked = "deny"`, unmaintained crates warn. No blanket ignores; each ignored
advisory needs an inline comment with the reason and a review date.

**`[sources]`** — `unknown-registry = "deny"`, `unknown-git = "deny"`. Everything comes from
crates.io.

## Acceptance
- `cargo deny check` passes on the current dependency tree.
- The `lint` CI job is now fully green.
- Temporarily adding `chrono` to any crate makes `cargo deny check` fail with the ban reason.
  Revert before merging.

## Done when
The global DoD in `tasks/README.md` is satisfied.
