# 12-01 · cargo-dist setup

**Phase:** 12 — Packaging · **Agent:** E · **Size:** M
**Prerequisites:** `00-04`
**Reference:** `docs/11-packaging.md` §§2–3

## Goal
Configure `cargo-dist` and the tag-triggered release workflow that produces artifacts for all three
platforms.

## Files
- `dist-workspace.toml`
- `.github/workflows/release.yml`

## Specification

`cargo dist init` against the `loxia-player` binary crate, then edit `dist-workspace.toml`:

```toml
[dist]
targets = [
  "x86_64-pc-windows-msvc",
  "x86_64-apple-darwin", "aarch64-apple-darwin",
  "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
]
installers = ["shell", "powershell"]
ci = ["github"]
```

Only `crates/loxia` is published; the five library crates are `publish = false`.

**Trigger** is a `v*` tag. The workflow: build the matrix, run the licence checks (task `12-05`),
create a draft GitHub Release, attach artifacts, and leave it **draft** for manual verification.
Publishing automatically means a broken build reaches users before anyone looks at it.

**Linux glibc.** Build in a container with an older glibc (`ubuntu:22.04` or a `cross` image) so the
binaries run on more distributions than the runner's own glibc allows.

**Version consistency check** as the first workflow step: the tag, `Cargo.toml`'s version, and the
newest `CHANGELOG.md` heading must agree, or the release fails immediately rather than shipping
mislabelled artifacts.

**Artifact naming:** `loxia-<version>-<target>.<ext>`. The Windows MSI (task `12-02`) and the macOS
DMG (task `12-03`) are added as post-build steps here, so wire the extension points now.

Every artifact includes `LICENSE`, `README.md`, and `THIRD_PARTY_LICENSES.md`; the Windows ones
additionally include `COPYING.LGPL` (task `12-05`).

## Acceptance
- `dist-workspace.toml` is committed and `cargo dist plan` succeeds locally.
- Library crates are marked `publish = false`.
- A dry run on a test tag (`v0.0.1-test`) produces artifacts for all five targets.
- The draft release is created and **not** published.
- The version-consistency check fails when the tag and `Cargo.toml` disagree — demonstrated once
  and recorded in the PR.
- Each artifact contains `LICENSE`, `README.md`, and `THIRD_PARTY_LICENSES.md`.
- The Linux binary runs on a glibc older than the runner's — verified in a container.

## Done when
The global DoD in `tasks/README.md` is satisfied.
