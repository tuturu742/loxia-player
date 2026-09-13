# 12-03 · macOS packaging

**Phase:** 12 — Packaging · **Agent:** E · **Size:** M
**Prerequisites:** `12-01`
**Reference:** `docs/11-packaging.md` §5

## Goal
A `.dmg`, a `.pkg`, and a Homebrew tap formula. macOS uses a system mpv from Homebrew rather than a
bundled one.

## Files
- `packaging/macos/Info.plist`
- `packaging/macos/build-dmg.sh`
- `packaging/macos/loxia.rb`
- `.github/workflows/release.yml` (extend)

## Specification

**`.dmg`** contains `Loxia.app` — a wrapper whose `Info.plist` launches Terminal running the
bundled binary — an `/Applications` symlink, and the plain `loxia-player` binary for CLI users who do not
want the app wrapper.

**`.pkg`** installs `/usr/local/bin/loxia` (Intel) or `/opt/homebrew/bin/loxia` (Apple Silicon),
with licence files into `/usr/local/share/doc/loxia/`. A **preinstall script** checks for `mpv` and
warns with the `brew install mpv` instruction when it is absent — it warns rather than blocking,
since a user may install mpv afterwards.

**Universal binary.** Produce one with `lipo` from the two arch builds when cargo-dist does not do
it directly; a single universal artifact avoids users downloading the wrong one.

**Homebrew tap** (`loxia.rb`): `depends_on "mpv"`, downloads the release tarball, verifies its
SHA-256, installs the binary and the man page. The release workflow bumps the formula's version and
hash in the tap repository automatically.

**Signing and notarisation.** When a Developer ID is available in CI secrets, sign and notarise both
the DMG and the PKG, and staple the ticket. When it is not, the workflow **skips signing and the
README documents the workaround prominently**:
```
xattr -d com.apple.quarantine /Applications/Loxia.app
```
Shipping an installer that fails Gatekeeper with an unexplained "damaged" dialog is worse than
shipping an unsigned one with clear instructions — that dialog tells the user the download is
corrupt, which is untrue and unfixable from their side.

**Path handling.** macOS has no `XDG_STATE_HOME`, so state falls back to the data directory
(task `01-03`). Verify the resolved paths on a real machine.

## Acceptance
- The DMG mounts and `Loxia.app` launches a working terminal session.
- The PKG installs the binary onto `PATH` on both architectures.
- The preinstall warning appears on a machine with no mpv, and installation still completes.
- `brew tap <user>/tap && brew install loxia` installs and pulls mpv as a dependency.
- The universal binary reports both architectures under `lipo -info`.
- Licence files are present in both artifacts.
- Config, cache, and state resolve to the documented macOS paths — verified with `loxia-player --doctor`.
- When unsigned, the README workaround is present and works on a clean machine.
- Manual QA rows 1–4 and 13 from `docs/10-testing-and-ci.md` §8, pasted into the PR.

## Done when
The global DoD in `tasks/README.md` is satisfied.
