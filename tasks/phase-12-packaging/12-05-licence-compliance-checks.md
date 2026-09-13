# 12-05 · Licence compliance checks

**Phase:** 12 — Packaging · **Agent:** E · **Size:** M
**Prerequisites:** `12-02`, `12-03`, `12-04`, `11-07`
**Reference:** `docs/11-packaging.md` §7, `design_overview` §10

## Goal
Make the LGPL and GPL obligations **mechanically verified** in CI, so a release cannot ship without
them. This task blocks any release containing mpv binaries.

## Files
- `.github/workflows/release.yml` (extend)
- `scripts/check-licences.sh`
- `COPYING.LGPL`
- `THIRD_PARTY_LICENSES.md` (verify — authored in `11-07`)

## Specification

`scripts/check-licences.sh` runs after the build and **fails the release** on any violation. It
implements the checklist in `docs/11-packaging.md` §7:

| # | Check | How |
| :-- | :-- | :-- |
| 1 | `mpv-1.dll` is a **standalone file** in every Windows artifact | extract the MSI and ZIP; assert the file exists |
| 2 | `mpv-1.dll` is **not statically linked** into `loxia.exe` | assert `loxia.exe` imports from `mpv-1.dll` (`dumpbin /dependents` or an equivalent PE parse) |
| 3 | `COPYING.LGPL` present and non-empty in every artifact containing mpv binaries | extract and check size |
| 4 | `LICENSE` (GPL-3.0) present in **every** artifact | extract and check |
| 5 | `THIRD_PARTY_LICENSES.md` present, and contains the mpv copyright line, the source URL `https://github.com/mpv-player/mpv`, and the no-warranty disclaimer | grep for all three |
| 6 | The release manifest records the **exact bundled mpv version and source URL** | read `packaging/windows/mpv.lock`, write into the release notes |
| 7 | `cargo deny check licenses` passes | run it |

Check 2 is the substantive LGPL requirement: §6 obliges us to let a user relink against a modified
library, which static linking would prevent. An import-table assertion proves dynamic linking rather
than assuming it.

**`COPYING.LGPL`** is the complete, unmodified LGPL-2.1 text, committed to the repository.

**Release notes generation.** The workflow appends a `Third-party components` section naming the
bundled mpv version, its source URL, and its licence — so the source-availability pointer is on the
release page itself, not only inside the archive.

**Failure behaviour.** Any failed check aborts the release before the draft is created. There is no
override flag; a compliance check that can be skipped under time pressure is not a compliance check.

## Acceptance
- `scripts/check-licences.sh` passes on a real build of all artifacts.
- Deliberately removing `COPYING.LGPL` from the WiX manifest fails the release. Restore afterwards.
- Deliberately corrupting the mpv notice text in `THIRD_PARTY_LICENSES.md` fails check 5. Restore.
- Check 2 correctly reports dynamic linking on the real build, and correctly **fails** against a
  synthetic statically-linked binary.
- The generated release notes name the mpv version and source URL.
- `cargo deny check licenses` is green.
- Settings → About displays the same notice text that ships in the artifacts (task `11-07`).

## Done when
The global DoD in `tasks/README.md` is satisfied, and every box in `docs/11-packaging.md` §7 is
ticked.
