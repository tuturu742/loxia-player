# 12-04 · Linux packaging

**Phase:** 12 — Packaging · **Agent:** E · **Size:** M
**Prerequisites:** `12-01`
**Reference:** `docs/11-packaging.md` §6

## Goal
A portable tarball, an AUR package, Homebrew-on-Linux support, and desktop integration.

## Files
- `packaging/aur/PKGBUILD`
- `packaging/aur/README.md`
- `packaging/linux/loxia.desktop`
- `packaging/linux/loxia.1`

## Specification

**Tarball.** Dynamically linked against system libmpv, containing `loxia-player`, `LICENSE`,
`THIRD_PARTY_LICENSES.md`, `README.md`, `loxia.1`, and `loxia.desktop`. Built against an older glibc
(task `12-01`); the README states the minimum version and the libmpv requirement.

**AUR `loxia-bin`.** A PKGBUILD fetching the release tarball by version, with
`depends=('mpv')` and `optdepends=('libnotify: desktop notifications')`. It installs the binary to
`/usr/bin`, the man page to `/usr/share/man/man1`, the desktop file, and the licence to
`/usr/share/licenses/loxia-bin/`.

`packaging/aur/README.md` documents the publishing procedure — SSH key setup, the `.SRCINFO`
regeneration step, and whether the release workflow pushes automatically or a maintainer does it by
hand. Undocumented AUR publishing is how packages go stale.

**Homebrew on Linux** uses the same formula as macOS (task `12-03`); confirm it builds there.

**Man page** (`loxia.1`, in roff): synopsis, all CLI flags from task `01-06`, the config file
location, the environment variables (`LOXIA_LOG`, `LOXIA_ASCII`), and a `SEE ALSO` pointing at the
repository. Generated from the clap definition where practical so it cannot drift from the flags.

**Desktop file** with `Terminal=true`, the icon derived from `assets/logo.svg`, and
`Categories=AudioVideo;Audio;Player;`. MPRIS works without it, but a launcher entry is expected on
desktop systems.

**libmpv version check.** The README states the minimum (mpv ≥ 0.35, matching libmpv2's requirement
from `docs/12-decisions.md` §1) and `loxia-player --doctor` reports the loaded version, so a user on an old
distribution gets a clear diagnosis rather than a confusing symbol error.

## Acceptance
- The tarball extracts and runs on Debian stable, Ubuntu LTS, and Arch.
- The binary runs on a distribution with an older glibc than the build runner's.
- `makepkg -si` from the PKGBUILD installs a working package and pulls mpv.
- `namcap` reports no errors on the built package.
- The AUR publishing procedure is documented and has been followed once end to end.
- `man loxia` renders and lists every CLI flag.
- The desktop entry appears in a launcher and starts the app.
- `loxia-player --doctor` reports the libmpv version and path.
- Homebrew-on-Linux installs successfully.
- Manual QA rows 1–4 and 13 from `docs/10-testing-and-ci.md` §8, pasted into the PR.

## Done when
The global DoD in `tasks/README.md` is satisfied.
