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
`git ls-files -- assets` to reproduce this list.

| File | Use |
| :-- | :-- |
| `BRANDING.md` | This inventory and design rationale. |
| `logo.svg` | Full-colour tile. README, release pages, macOS and Windows app icons at ≥ 48 px. |
| `logo-mono.svg` | Simplified, `currentColor`. Favicons, 16 and 32 px icons, anywhere a single colour is required. |
| `eq_presets.toml` | Factory equalizer presets, embedded into `loxia-audio` via `include_str!` (`docs/05-audio-engine.md` §5). |
| `themes/amber_crt.toml` | Built-in colour theme: monochrome amber-on-black CRT. |
| `themes/cyberpunk_neon.toml` | Built-in colour theme: loxia's own branding palette (see Palette below). |
| `themes/darcula.toml` | Built-in colour theme: dark IDE grey/purple/orange. |
| `themes/default_terminal.toml` | Built-in colour theme: transparent, uses the terminal's own ANSI palette. |
| `themes/far_blue.toml` | Built-in colour theme: deep, cool blue. |
| `themes/green_crt.toml` | Built-in colour theme: monochrome green-phosphor CRT. |
| `themes/oled_black.toml` | Built-in colour theme: pure black (OLED pixel-off) with cyan/magenta accents. |

`icons/loxia-{16,32,48,128,256,512}.png`, `icons/loxia.ico`, `icons/loxia.icns`, and `banner.txt`
were previously listed here but do not exist in the repository — there is no `icons/` directory
and no `banner.txt` under `assets/` (or anywhere else) per `git ls-files`. They are the deliverable
of task `12-06` (`tasks/phase-12-packaging/12-06-branding-assets.md`), which has not run yet, and
have been removed from this inventory until they exist and are tracked. The design rules that will
govern them (below, and in the ASCII-banner section) are left in place as forward-looking spec, not
as claims that the files are present.

## Palette

Derived from `logo.svg`'s own gradient stops and fills.

| Element | Colour(s) | Role |
| :-- | :-- | :-- |
| Tile | `#0B0E14` | Background tile fill |
| Tile edge | `#1C2230` | Tile border stroke |
| Head | `#E8ECF4` | Off-white head silhouette |
| Accent (upper mandible) | `#FF2D6F` → `#FF5C9A` | Primary brand colour |
| Accent (lower mandible) | `#22C8DC` → `#5CE7F5` | Secondary brand colour |
| Cursor bar | `#FF2D6F` → `#22C8DC` | Gradient bar spanning both accent colours |

The mark is designed against the dark tile background (`#0B0E14`) and currently has no separate
light-background variant — anywhere it must sit on a light surface, place the full tile (including
its background), never the mark alone.

## Geometry and clear space

- Canvas: 512×512, tile corner radius 112px (roughly 22% of the canvas width) at any export size.
- Clear space: keep at least one head-radius (104px at the 512px canvas) of empty space on every
  side of the mark when placing it next to other marks or text.
- Minimum size: do not render `logo.svg` below 16px; below that, use `logo-mono.svg` instead — the
  gradient fills and the eye highlight stop reading as anything but noise at that size.

## ASCII banner (banner.txt)

A plain-text banner for terminal splash/startup output. This is the deliverable of task `12-06`
(`tasks/phase-12-packaging/12-06-branding-assets.md`), which has not run yet — there is no
`banner.txt` in the repository (see `## Files` above). Until it exists, keep any startup text to
the bare name `loxia`; do not hand-roll an ASCII crossbill ahead of that task.

Example of the intended shape (illustrative only — not a shipped asset):

```text
 _           _
| | _____  _(_) __ _
| |/ _ \ \/ / |/ _` |
| | (_) >  <| | (_| |
|_|\___/_/\_\_|\__,_|
```

The `><` in the third line is the crossed-mandibles glyph from the mark, carried into plain text.
