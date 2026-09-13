# 12-02 · Windows packaging

**Phase:** 12 — Packaging · **Agent:** E · **Size:** L
**Prerequisites:** `12-01`
**Reference:** `docs/11-packaging.md` §4

## Goal
An MSI installer and a portable ZIP, both bundling `mpv-1.dll` so Windows users need not install
mpv themselves. This is the only platform where we ship an mpv binary, so it carries the LGPL
obligations.

## Files
- `packaging/windows/loxia.wxs`
- `packaging/windows/mpv.lock`
- `packaging/windows/fetch-mpv.ps1`
- `.github/workflows/release.yml` (extend)

## Specification

**mpv sourcing.** `fetch-mpv.ps1` downloads a **pinned** build from the official
`shinchiro/mpv-winbuild` releases. `mpv.lock` records the release tag, the asset name, and the
SHA-256. The script verifies the hash and **fails the build on a mismatch** — an unverified DLL
pulled at release time is a supply-chain hole.

`mpv.lock` also records the **upstream mpv version and source URL**, which task `12-05` copies into
the release manifest to satisfy the LGPL source-availability requirement.

**MSI (WiX).** Installs into `%ProgramFiles%\Loxia\`:
`loxia.exe`, `mpv-1.dll`, `LICENSE`, `COPYING.LGPL`, `THIRD_PARTY_LICENSES.md`, `README.md`.
Appends the directory to the **system** `PATH`, registers an uninstaller, and creates a Start Menu
entry launching `wt.exe -- loxia` with a fallback to `cmd /k loxia` when Windows Terminal is absent.

**`mpv-1.dll` must remain a standalone file.** It is never statically linked, never embedded in the
exe, and sits in a directory where an administrator can replace it — LGPL §6 requires the user be
able to relink against a modified library. Task `12-05` asserts this mechanically.

**DLL loading order.** The app must find `mpv-1.dll` **next to the exe first**, so the portable ZIP
works without touching `PATH`. Call `SetDllDirectoryW` with the exe's directory at startup, before
libmpv2 loads, or use delay-loading.

**Portable ZIP:** `loxia.exe`, `mpv-1.dll`, both licence files, `README.txt`. No installation, no
registry writes, no PATH changes.

**Package managers:** `packaging/scoop/loxia.json` pointing at the ZIP with its hash, and a WinGet
manifest for submission to `microsoft/winget-pkgs`.

**Uninstall** removes the install directory and the PATH entry, and leaves user data in `%APPDATA%`
and `%LOCALAPPDATA%` untouched — documented in the README.

## Acceptance
- `fetch-mpv.ps1` verifies the SHA-256 and fails on a deliberately corrupted download.
- The MSI installs on a **clean Windows VM with no mpv present** and loxia plays audio.
- The MSI contains `mpv-1.dll` as a **separate file** — verified by extracting the MSI.
- The portable ZIP runs from a directory not on `PATH`.
- Replacing `mpv-1.dll` with a different build and relaunching still works.
- `LICENSE`, `COPYING.LGPL`, and `THIRD_PARTY_LICENSES.md` are present in both artifacts.
- The Start Menu entry launches a usable terminal.
- Uninstall removes the program and the PATH entry but not user data.
- Path-sanitiser tests (task `08-01`) pass on this build.
- Manual QA rows 1–4 and 13 from `docs/10-testing-and-ci.md` §8, pasted into the PR.

## Done when
The global DoD in `tasks/README.md` is satisfied.
