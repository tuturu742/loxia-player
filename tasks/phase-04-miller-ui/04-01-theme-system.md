# 04-01 · Theme system

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** M
**Prerequisites:** `03-01`
**Reference:** `docs/07-ui-spec.md` §12

## Goal
Semantic theming with the seven built-in themes embedded in the binary. Every later widget resolves
colours through roles, so no widget ever names a colour.

## Files
- `crates/loxia-core/src/theme.rs`
- `crates/loxia-tui/src/style.rs`
- `assets/themes/{default_terminal,far_blue,darcula,cyberpunk_neon,amber_crt,green_crt,oled_black}.toml`

## Specification

**`loxia-core::theme`** — parsing only, no ratatui types:
```
pub struct Theme { pub name: String, pub ascii_only: bool, pub roles: BTreeMap<Role, ThemeColor> }
pub enum Role { Bg, Fg, Dim, Accent, AccentAlt, SelectionBg, SelectionFg, Border, BorderFocus,
                HeaderBg, Success, Warning, Error, ProgressFilled, ProgressEmpty }
pub enum ThemeColor { Reset, Rgb(u8,u8,u8), Indexed(u8) }

impl Theme {
    pub fn builtin(name: &str) -> Option<Theme>;   // from include_str!
    pub fn builtin_names() -> &'static [&'static str];
    pub fn parse(toml: &str) -> Result<Theme, ThemeError>;
    pub fn color(&self, r: Role) -> ThemeColor;    // falls back to Reset for a missing role
}
```

`task 01-02` already added `pub const BUILTIN_THEME_NAMES: &[&str] = &[..]` (the 7 names below) to
this file so config validation's "unknown theme falls back to `default_terminal`" rule has a real
list to check against. `builtin_names()` here returns that same constant — don't redefine the list.

**`loxia-tui::style`** — the ratatui bridge:
```
pub fn style(theme: &Theme, role: Role) -> Style;
pub fn fg(theme: &Theme, role: Role) -> Style;
pub fn selection(theme: &Theme, focused: bool) -> Style;
pub fn border(theme: &Theme, focused: bool) -> Style;
```

Theme files use hex strings (`"#1b1f2b"`), the literal `"reset"`, or an integer for a 256-colour
index. A missing role falls back to `Reset` with a warning at parse time, so a partial theme still
renders.

**`default_terminal` must leave `Bg` as `Reset`.** Painting a background there breaks terminal
transparency, which is the entire point of that theme. `oled_black` pins `Bg` to `#000000`.

Author the seven palettes to the descriptions in `design_overview` §4.3. Every theme must keep
`Fg`-on-`Bg` and `SelectionFg`-on-`SelectionBg` legible; the snapshot tests below are the check.

**Glyph sets.** `ascii_only` selects the fallback glyph table used by the progress bar, section
headers, and status markers. Define it here as a `Glyphs` struct with both variants so widgets read
`theme.glyphs()` rather than embedding literals.

## Acceptance
- `all_seven_builtins_parse` — a loop over `builtin_names()`.
- `default_terminal_bg_is_reset`
- `oled_black_bg_is_pure_black`
- `missing_role_falls_back_to_reset_with_warning`
- `invalid_hex_is_a_parse_error`
- `indexed_colour_parses`
- `ascii_glyphs_have_no_multibyte_chars` — every glyph in the ASCII set is one byte.
- `theme_snapshot_per_theme` — an `insta` snapshot rendering a small sample widget (a bordered list
  with a selected row and a progress bar) once per theme, reviewed for legibility.

## Done when
The global DoD in `tasks/README.md` is satisfied.
