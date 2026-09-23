# loxia — Branding

Closes the `TBD` left in `design_overview` §9. Asset generation is task `12-06`.

## The name and the mark

*Loxia* is the genus of the crossbills — finches whose upper and lower mandibles cross at the tip,
an adaptation for prising seeds out of conifer cones. It is the sibling of `pyrrhula` (the
bullfinches) in the Fringillidae suite.

The mark is a crossbill head in profile, reduced to three elements:

1. **The head** — a solid disc with a small nape wedge, enough to read as a bird rather than a ball.
2. **The crossed mandibles** — the upper descending to the right, the lower ascending, crossing at
   roughly the two-thirds point. This is the literal identifying feature of the genus, and it
   doubles as a `><` terminal glyph.
3. **The cursor bar** — a rounded bar beneath the head, reading as a terminal cursor or prompt
   underline.

The mandibles are the load-bearing idea: they are simultaneously the bird's defining anatomy and a
shell prompt. Everything else exists to make that legible.

## Files

Covers `assets/` (this directory and its `themes/` subdirectory) as actually tracked in git — run
`git ls-files -- assets` to reproduce this list. Every file below ships as part of this
repository and carries its `GPL-3.0-or-later` licence, per the first line of `LICENSE` and the
`license = "GPL-3.0-or-later"` field under `[workspace.package]` in `Cargo.toml` (also restated in
`THIRD_PARTY_LICENSES.md`).

| File | Use | Licence |
| :-- | :-- | :-- |
| `BRANDING.md` | This inventory and design rationale. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `logo.svg` | Full-colour tile. README, release pages, macOS and Windows app icons at ≥ 48 px. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `logo-mono.svg` | Simplified, `currentColor`. Favicons, 16 and 32 px icons, anywhere a single colour is required. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `eq_presets.toml` | Factory equalizer presets, embedded into `loxia-audio` via `include_str!` (`docs/05-audio-engine.md` §5). | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `themes/amber_crt.toml` | Built-in colour theme: monochrome amber-on-black CRT. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `themes/cyberpunk_neon.toml` | Built-in colour theme: loxia's own branding palette (see Palette below). | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `themes/darcula.toml` | Built-in colour theme: dark IDE grey/purple/orange. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `themes/default_terminal.toml` | Built-in colour theme: transparent, uses the terminal's own ANSI palette. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `themes/far_blue.toml` | Built-in colour theme: deep, cool blue. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `themes/green_crt.toml` | Built-in colour theme: monochrome green-phosphor CRT. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |
| `themes/oled_black.toml` | Built-in colour theme: pure black (OLED pixel-off) with cyan/magenta accents. | GPL-3.0-or-later (`LICENSE`; `Cargo.toml` `[workspace.package] license`) |

`icons/loxia-{16,32,48,128,256,512}.png`, `icons/loxia.ico`, `icons/loxia.icns`, and `banner.txt`
were previously listed here but do not exist in the repository — there is no `icons/` directory
and no `banner.txt` under `assets/` (or anywhere else) per `git ls-files`. They are the deliverable
of task `12-06` (`tasks/phase-12-packaging/12-06-branding-assets.md`), which has not run yet, and
have been removed from this inventory until they exist and are tracked. The design rules that will
govern them (below, and in the ASCII-banner section) are left in place as forward-looking spec, not
as claims that the files are present.

**Use `logo-mono.svg` at 16 and 32 px.** The full mark's eye, nape, and gradients collapse into an
indistinct blob below about 48 px; the simplified variant knocks the eye out as a hole and thickens
the mandibles so the crossing survives.

## Palette

Drawn from the `cyberpunk_neon` theme, so the mark and the default screenshot theme agree.

| Role | Hex | Use |
| :-- | :-- | :-- |
| Tile | `#0B0E14` | Background |
| Tile edge | `#1C2230` | Inner border |
| Head | `#E8ECF4` | The disc and nape |
| Accent (upper mandible) | 
