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

| File | Use |
| :-- | :-- |
| `logo.svg` | Full-colour tile. README, release pages, macOS and Windows app icons at ≥ 48 px. |
| `logo-mono.svg` | Simplified, `currentColor`. Favicons, 16 and 32 px icons, anywhere a single colour is required. |
| `icons/loxia-{16,32,48,128,256,512}.png` | Generated from the SVGs by task `12-06`. |
| `icons/loxia.ico`, `icons/loxia.icns` | Platform icon bundles. |
| `banner.txt` | ASCII boot banner. |

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
| Accent (upper mandible) | `#FF5C9A` → `#FF2D6F` | Primary brand colour |
| Secondary (lower mandible) | `#5CE7F5` → `#22C8DC` | |
| Cursor bar | accent → secondary | Horizontal gradient |

On a light background, `logo-mono.svg` in `#0B0E14`. On dark, `#E8ECF4` or the accent.

## Geometry and clear space

The mark is drawn on a 512 × 512 grid with a corner radius of 112 (0.219 × the side), so scaling the
tile keeps the corners proportional.

**Clear space** on every side is 0.125 × the mark's width — 64 units at 512. Nothing else sits
inside it.

**Minimum sizes:** 16 px for `logo-mono.svg`; 48 px for `logo.svg`. Below 16 px, use a plain accent
square rather than an illegible mark.

Do not: recolour the mandibles to the same hue (the crossing stops reading), rotate the mark, add a
drop shadow, or place the full-colour tile on a coloured background other than its own.

## ASCII banner (`banner.txt`)

Constraints, enforced by task `12-06`:
- **At most 64 columns**, so it fits an 80-column terminal with margin.
- **ASCII only** — no box-drawing characters, which need a font that many terminals lack.
- Contains the literal `{version}`, substituted at runtime.
- Coloured with the active theme's `Accent`, never hardcoded ANSI escapes, so it works on light and
  dark backgrounds.

Keep it short. It prints on every `--version`, and a large block for a one-line question is an
irritation rather than a flourish.

```
  _           _
 | | _____  _(_) __ _     loxia {version}
 | |/ _ \ \/ / |/ _` |    a terminal music client for Emby
 | | (_) >  <| | (_| |
 |_|\___/_/\_\_|\__,_|    GPL-3.0-or-later
```

That block is 58 columns wide and ASCII-only. The `><` in the third row is deliberate — it echoes
the crossed mandibles.

## Reconciliation notes (this revision)

The claims below were checked against the files that actually ship in `assets/` (`ls assets/themes`,
`assets/eq_presets.toml`, `assets/logo.svg`, `assets/logo-mono.svg`) and, where a fix was needed,
corrected in this file.

| Claim | Claimed at | Fact | Fact at | Fixed? |
| :-- | :-- | :-- | :-- | :-- |
| Accent (upper mandible) gradient reads `#FF2D6F` → `#FF5C9A` | `assets/BRANDING.md` (Palette table, "Accent (upper mandible)" row, prior revision) | `upper` gradient's stops run offset 0 `#FF5C9A` → offset 1 `#FF2D6F` | `assets/logo.svg:8-11` | Yes — row now reads `#FF5C9A` → `#FF2D6F` |
| Secondary (lower mandible) gradient reads `#22C8DC` → `#5CE7F5` | `assets/BRANDING.md` (Palette table, "Secondary (lower mandible)" row, prior revision) | `lower` gradient's stops run offset 0 `#5CE7F5` → offset 1 `#22C8DC` | `assets/logo.svg:12-15` | Yes — row now reads `#5CE7F5` → `#22C8DC` |
| Cursor bar reads "accent → secondary" | `assets/BRANDING.md` (Palette table, "Cursor bar" row) | `bar` gradient's stops run offset 0 `#FF2D6F` → offset 1 `#22C8DC`, i.e. accent's darker stop to secondary's darker stop | `assets/logo.svg:16-19` | No change needed — already correct |
| Tile / Tile edge / Head hexes (`#0B0E14`, `#1C2230`, `#E8ECF4`) | `assets/BRANDING.md` (Palette table) | Tile `rect fill="#0B0E14"`, inner border `stroke="#1C2230"`, head/nape `fill="#E8ECF4"` | `assets/logo.svg:22-27` | No change needed — already correct |
| `logo.svg` / `logo-mono.svg` file names and roles (full-colour tile vs. `currentColor` simplified mark) | `assets/BRANDING.md` (Files table) | Both files exist on disk with matching structure (`logo.svg` uses gradients + multiple fills; `logo-mono.svg` uses a single `fill="currentColor"` group) | `assets/logo.svg:1`, `assets/logo-mono.svg:1,9` | No change needed — already correct |
| Minimum sizes 16 px (`logo-mono.svg`) / 48 px (`logo.svg`) | `assets/BRANDING.md` (Geometry and clear space) | Consistent with the Files table's own "16 and 32 px" / "≥ 48 px" guidance; no theme, logo, or preset list contradicts it | `assets/BRANDING.md` (Files table) | No change needed |
| No theme name list or EQ preset list is documented in this file | `assets/BRANDING.md` (entire file) | Seven theme files (`amber_crt`, `cyberpunk_neon`, `darcula`, `default_terminal`, `far_blue`, `green_crt`, `oled_black`) and eight preset keys (`flat`, `darkwave_ebm`, `bass_boost`, `vocal`, `acoustic`, `night_listening_warm`, `loudness`, `classical`) exist, but `BRANDING.md` makes no competing claim about either list | `assets/themes/*.toml`; `assets/eq_presets.toml:12-42` | N/A — nothing to reconcile here |
