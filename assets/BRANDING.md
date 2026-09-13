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
| Accent (upper mandible) | `#FF2D6F` → `#FF5C9A` | Primary brand colour |
| Secondary (lower mandible) | `#22C8DC` → `#5CE7F5` | |
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
