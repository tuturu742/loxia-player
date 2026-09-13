# 12-06 · Branding assets

**Phase:** 12 — Packaging · **Agent:** E · **Size:** S
**Prerequisites:** none
**Reference:** `assets/BRANDING.md`, `design_overview` §9

## Goal
Produce the icon set and boot banner from the existing logo, closing the `TBD` left in
`design_overview` §9. Can be done at any time; it blocks the installers, which need icon files.

## Files
- `assets/logo.svg` (exists — authored alongside this task library)
- `assets/BRANDING.md` (exists)
- `assets/banner.txt`
- `assets/icons/loxia-{16,32,48,128,256,512}.png`
- `assets/icons/loxia.ico`
- `assets/icons/loxia.icns`

## Specification

`assets/logo.svg` and `assets/BRANDING.md` are already authored — the crossed-mandible mark
described there. This task derives everything else from them.

**Raster export.** Render the SVG at each listed size with `resvg` or `rsvg-convert`. At 16 px the
detailed mark is illegible, so use the **simplified single-colour variant** documented in
`BRANDING.md` for 16 and 32 px. An icon that is a grey smudge in a taskbar is worse than a simpler
one that reads.

**`loxia.ico`** packs 16, 32, 48, and 256 for the Windows installer and executable.
**`loxia.icns`** packs 16 through 512 for `Loxia.app`.

**`banner.txt`** — the ASCII boot banner, shown by `--version` and in the About view. Constraints:
at most 64 columns so it fits an 80-column terminal with margin; ASCII only, no box-drawing
characters, so it renders in any font; and it must include the version placeholder `{version}`,
substituted at runtime.

Keep it restrained — it prints on every `--version` invocation, and a twelve-line block for a
one-line question is an irritation.

**Colour.** The banner uses the active theme's `Accent`, not hardcoded ANSI codes, so it works on
light and dark backgrounds.

Wire the icons into: the WiX manifest (`12-02`), `Info.plist` (`12-03`), and `loxia.desktop`
(`12-04`).

## Acceptance
- All six PNG sizes exist and are non-empty.
- The 16 px and 32 px icons use the simplified variant and are legible at actual size — screenshot
  in the PR.
- `loxia.ico` and `loxia.icns` contain the documented size sets, verified with `identify` and
  `iconutil`.
- `banner.txt` is at most 64 columns and contains only ASCII.
- `--version` prints the banner with the version substituted and the theme accent applied.
- The banner renders correctly on both a light and a dark terminal background — screenshots in the
  PR.
- The icon appears in the Windows installer, the macOS app bundle, and a Linux launcher.

## Done when
The global DoD in `tasks/README.md` is satisfied.
