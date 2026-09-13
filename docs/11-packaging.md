# 11 — Packaging, Distribution & Licence Compliance

## 1. Licence

**loxia is licensed GPL-3.0-or-later.**

`design_overview` §10 specifies mpv's LGPL obligations but never states loxia's own licence. mpv is
frequently built with GPL-configured components, so a permissive licence would require auditing the
bundled DLL's build flags before every release. GPL-3.0 is unconditionally compatible with either
mpv build configuration and matches the norms of the mpv ecosystem.

`LICENSE` at the repo root carries the full GPL-3.0 text. `Cargo.toml` sets
`license = "GPL-3.0-or-later"`.

## 2. Tooling

`cargo-dist` drives the release matrix from `dist-workspace.toml`, triggered by pushing a `v*` tag.
It builds per-target artifacts, creates the GitHub Release, and emits installer scripts. Steps
cargo-dist cannot do — WiX customisation for the mpv bundle, AUR publishing — run as post-build
steps in the same workflow.

## 3. Target matrix

| Target | Artifacts | mpv strategy |
| :-- | :-- | :-- |
| `x86_64-pc-windows-msvc` | `loxia-player-setup.msi`, `loxia-player-x86_64-windows.zip` | **Bundled** `mpv-1.dll` beside the exe |
| `aarch64-apple-darwin`, `x86_64-apple-darwin` | `loxia-player-vX.Y.Z.dmg`, `loxia-player.pkg`, `.tar.gz` | **System** mpv via a Homebrew dependency |
| `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | `.tar.gz`, AUR `loxia-player-bin` | **System** libmpv from the distribution |

## 4. Windows

- **MSI (WiX):** installs `loxia-player.exe`, `mpv-1.dll`, `LICENSE`, `COPYING.LGPL`, and
  `THIRD_PARTY_LICENSES.md` into `%ProgramFiles%\Loxia\`; appends that directory to the system
  `PATH`; registers an uninstaller; creates a Start Menu entry that launches Windows Terminal.
- **`mpv-1.dll` sourcing:** a pinned build from the official `shinchiro/mpv-winbuild` releases. The
  release tag and SHA-256 are recorded in `packaging/windows/mpv.lock`, and CI verifies the hash
  before packaging. A mismatch fails the release.
- **Loading order:** the app must find the DLL next to the exe first, so the portable ZIP works
  without touching `PATH`.
- **Portable ZIP:** `loxia-player.exe`, `mpv-1.dll`, both licence files, `README.txt`. No install required.
- **Package managers:** a Scoop bucket manifest at `packaging/scoop/loxia-player.json` and a WinGet
  manifest submitted to `microsoft/winget-pkgs`, both referencing the release assets by hash.

## 5. macOS

- **`.dmg`:** contains `Loxia.app` — a wrapper whose `Info.plist` launches Terminal running the
  bundled binary — plus an `/Applications` symlink and the plain binary for CLI users.
- **`.pkg`:** installs `/usr/local/bin/loxia-player` and the licence files into
  `/usr/local/share/doc/loxia-player/`. A preinstall check warns when `mpv` is absent, with the
  `brew install mpv` instruction.
- **Homebrew tap:** `homebrew-tap/Formula/loxia-player.rb`, `depends_on "mpv"`, downloading the release
  tarball with a SHA-256 check. Bumped automatically by the release workflow.
- **Signing:** sign and notarise both artifacts when a Developer ID is available. When it is not,
  the README documents the `xattr -d com.apple.quarantine` workaround prominently. Shipping an
  installer that fails Gatekeeper with no explanation is not acceptable.

## 6. Linux

- **Tarball:** dynamically linked against system libmpv, including licence files and a `loxia.1`
  man page. Build in an older glibc container to maximise reach and document the minimum.
- **AUR `loxia-player-bin`:** a PKGBUILD fetching the release tarball, `depends=('mpv')`,
  `optdepends` for `libnotify`. Publishing method documented in `packaging/aur/README.md`.
- **Homebrew on Linux:** the same formula as macOS.
- **Desktop integration:** a `.desktop` file and an icon derived from `assets/logo.svg`. MPRIS works
  without it.

## 7. Licence compliance checklist — blocking for any release containing mpv binaries

Derived from `design_overview` §10. Every item is verified by CI, not by memory.

1. [ ] `mpv-1.dll` ships as a **standalone dynamic library**, never statically linked into
   `loxia-player.exe`. *CI check:* the MSI and ZIP each contain a separate `mpv-1.dll`.
2. [ ] The library is **replaceable**: it sits in the install directory and is loaded by name, so a
   user can drop in a newer mpv build. Documented in the README.
3. [ ] `COPYING.LGPL` (LGPL-2.1) is present in **every artifact containing mpv binaries**.
   *CI check:* unpack each Windows artifact and assert the file exists and is non-empty.
4. [ ] `LICENSE` (GPL-3.0) is present in every artifact.
5. [ ] `THIRD_PARTY_LICENSES.md` contains the exact notice from `design_overview` §10.2 — the
   copyright line, the source URL, and the no-warranty disclaimer.
6. [ ] That notice is **reachable in the app** at Settings → About. The file is embedded with
   `include_str!` so it can never drift from the shipped copy.
7. [ ] The release manifest records the **exact mpv version and upstream source URL** used for the
   bundled DLL, so a user can rebuild the identical library.
8. [ ] `cargo deny check` passes. A new dependency with a copyleft licence blocks CI until it is
   reviewed and recorded here.

macOS and Linux link against a system mpv the user installed themselves, so obligations there are
limited to attribution — items 4 through 7, which we satisfy regardless.

## 8. Versioning and release process

- SemVer. Pre-1.0, minor bumps may break config; every breaking config change increments
  `schema_version` and ships a migration in `config/migrate.rs`.
- `CHANGELOG.md` in Keep-a-Changelog format. CI requires either a changelog entry or an explicit
  `no-changelog` label on the PR.
- Release: bump version → update changelog → tag `vX.Y.Z` → CI builds the matrix → confirm the
  licence-artifact checks passed → run the §8 QA checklist in `10-testing-and-ci.md` → publish →
  bump the Homebrew, Scoop, and AUR manifests.

## 9. `loxia-player --doctor`

A diagnostics subcommand that prints and checks: version; config path and validity; **keybinding
conflicts**; mpv version and load path; detected audio devices; terminal graphics protocol;
cache and download paths with sizes; and server reachability.

It is the first thing to request in a bug report, and it makes the release QA checklist far faster
to execute.
