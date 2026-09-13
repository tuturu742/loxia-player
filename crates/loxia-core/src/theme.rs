//! Theme, semantic roles, palette parsing (`docs/07-ui-spec.md` §12). Parsing only — no ratatui
//! types here; `loxia-tui::style` bridges a resolved [`ThemeColor`] to `ratatui::style::Color`.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The seven built-in themes. `default_terminal` is first and is the fallback used whenever a
/// configured theme name doesn't match one of these (`config::validate`, task `01-02`).
pub const BUILTIN_THEME_NAMES: &[&str] = &[
    "default_terminal",
    "far_blue",
    "darcula",
    "cyberpunk_neon",
    "amber_crt",
    "green_crt",
    "oled_black",
];

/// Semantic colour roles — every widget resolves through one of these, never a raw colour
/// (`docs/07-ui-spec.md` §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Bg,
    Fg,
    Dim,
    Accent,
    AccentAlt,
    SelectionBg,
    SelectionFg,
    Border,
    BorderFocus,
    HeaderBg,
    Success,
    Warning,
    Error,
    ProgressFilled,
    ProgressEmpty,
}

impl Role {
    /// Every variant, in the order `docs/07-ui-spec.md` §12 lists them — used to walk a parsed
    /// theme role by role.
    const ALL: [Role; 15] = [
        Role::Bg,
        Role::Fg,
        Role::Dim,
        Role::Accent,
        Role::AccentAlt,
        Role::SelectionBg,
        Role::SelectionFg,
        Role::Border,
        Role::BorderFocus,
        Role::HeaderBg,
        Role::Success,
        Role::Warning,
        Role::Error,
        Role::ProgressFilled,
        Role::ProgressEmpty,
    ];

    /// The TOML key this role is written under — snake_case, matching `docs/07-ui-spec.md` §12
    /// exactly.
    fn key(self) -> &'static str {
        match self {
            Role::Bg => "bg",
            Role::Fg => "fg",
            Role::Dim => "dim",
            Role::Accent => "accent",
            Role::AccentAlt => "accent_alt",
            Role::SelectionBg => "selection_bg",
            Role::SelectionFg => "selection_fg",
            Role::Border => "border",
            Role::BorderFocus => "border_focus",
            Role::HeaderBg => "header_bg",
            Role::Success => "success",
            Role::Warning => "warning",
            Role::Error => "error",
            Role::ProgressFilled => "progress_filled",
            Role::ProgressEmpty => "progress_empty",
        }
    }
}

/// A theme colour, exactly as one of the three forms a theme file may spell it: the literal
/// `"reset"` (the terminal's own default), a `"#rrggbb"` hex string, or a bare 256-colour index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColor {
    Reset,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

/// Fallback glyphs for widgets whose default Unicode box/block characters (progress fill,
/// section-header rules, playback-status markers) may not render on every terminal — widgets
/// read `Theme::glyphs()` rather than embedding either set as a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
    pub progress_filled: char,
    pub progress_empty: char,
    pub rule: char,
    pub playing_marker: char,
    pub paused_marker: char,
}

impl Glyphs {
    const UNICODE: Glyphs = Glyphs {
        progress_filled: '█',
        progress_empty: '░',
        rule: '─',
        playing_marker: '▸',
        paused_marker: '‖',
    };
    const ASCII: Glyphs = Glyphs {
        progress_filled: '#',
        progress_empty: '-',
        rule: '-',
        playing_marker: '>',
        paused_marker: '=',
    };
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ThemeError(String);

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub ascii_only: bool,
    pub roles: BTreeMap<Role, ThemeColor>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::builtin(BUILTIN_THEME_NAMES[0]).expect("default_terminal is a valid builtin theme")
    }
}

/// The raw TOML shape. `roles`' values are heterogeneous (a string for `"reset"`/hex, or a bare
/// integer for a 256-colour index), so they're deserialized generically here and converted
/// role-by-role in [`Theme::parse`], which is also where an invalid or missing role is caught.
#[derive(Deserialize)]
struct RawTheme {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    ascii_only: bool,
    #[serde(default)]
    roles: BTreeMap<String, RawColor>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawColor {
    Text(String),
    Indexed(u8),
}

impl Theme {
    /// One of the seven embedded palettes (`include_str!`, so the binary never reads these from
    /// disk), or `None` for any other name.
    pub fn builtin(name: &str) -> Option<Theme> {
        let toml = match name {
            "default_terminal" => include_str!("../../../assets/themes/default_terminal.toml"),
            "far_blue" => include_str!("../../../assets/themes/far_blue.toml"),
            "darcula" => include_str!("../../../assets/themes/darcula.toml"),
            "cyberpunk_neon" => include_str!("../../../assets/themes/cyberpunk_neon.toml"),
            "amber_crt" => include_str!("../../../assets/themes/amber_crt.toml"),
            "green_crt" => include_str!("../../../assets/themes/green_crt.toml"),
            "oled_black" => include_str!("../../../assets/themes/oled_black.toml"),
            _ => return None,
        };
        Theme::parse(toml).ok()
    }

    pub fn builtin_names() -> &'static [&'static str] {
        BUILTIN_THEME_NAMES
    }

    /// A role absent from the file is not an error — it simply falls back to
    /// [`ThemeColor::Reset`] (via [`Theme::color`]), so a partial or hand-edited theme still
    /// renders rather than failing to load entirely. `loxia-core` cannot itself log a warning
    /// about it (no `tracing` dependency here — logging is I/O-adjacent, `13-dependencies.md`
    /// rule); a caller that can (`loxia`'s bootstrap) uses [`Theme::missing_roles`] to do so.
    pub fn parse(toml: &str) -> Result<Theme, ThemeError> {
        let raw: RawTheme = toml::from_str(toml).map_err(|e| ThemeError(e.to_string()))?;

        let mut roles = BTreeMap::new();
        for role in Role::ALL {
            if let Some(raw_color) = raw.roles.get(role.key()) {
                roles.insert(role, parse_color(role, raw_color)?);
            }
        }

        Ok(Theme {
            name: raw.name.unwrap_or_default(),
            ascii_only: raw.ascii_only,
            roles,
        })
    }

    /// Falls back to [`ThemeColor::Reset`] for a role the theme file didn't define.
    pub fn color(&self, r: Role) -> ThemeColor {
        self.roles.get(&r).copied().unwrap_or(ThemeColor::Reset)
    }

    /// Roles absent from the parsed file, and therefore resolving to `Reset` via [`Theme::color`]
    /// — for a caller that can log (see [`Theme::parse`]'s doc) to warn about a partial theme.
    pub fn missing_roles(&self) -> Vec<Role> {
        Role::ALL
            .into_iter()
            .filter(|r| !self.roles.contains_key(r))
            .collect()
    }

    pub fn glyphs(&self) -> Glyphs {
        if self.ascii_only {
            Glyphs::ASCII
        } else {
            Glyphs::UNICODE
        }
    }
}

fn parse_color(role: Role, raw: &RawColor) -> Result<ThemeColor, ThemeError> {
    match raw {
        RawColor::Indexed(n) => Ok(ThemeColor::Indexed(*n)),
        RawColor::Text(s) => {
            if s.eq_ignore_ascii_case("reset") {
                return Ok(ThemeColor::Reset);
            }
            let hex = s.strip_prefix('#').ok_or_else(|| {
                ThemeError(format!(
                    "role {}: {s:?} is neither \"reset\" nor a \"#rrggbb\" hex string",
                    role.key()
                ))
            })?;
            if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ThemeError(format!(
                    "role {}: {s:?} must be exactly 6 hex digits",
                    role.key()
                )));
            }
            let byte = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16);
            let (r, g, b) = (byte(0..2), byte(2..4), byte(4..6));
            match (r, g, b) {
                (Ok(r), Ok(g), Ok(b)) => Ok(ThemeColor::Rgb(r, g, b)),
                _ => Err(ThemeError(format!(
                    "role {}: {s:?} is not valid hex",
                    role.key()
                ))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_seven_builtins_parse() {
        for name in BUILTIN_THEME_NAMES {
            let theme = Theme::builtin(name).unwrap_or_else(|| panic!("{name} failed to parse"));
            assert_eq!(theme.name, *name);
        }
        assert_eq!(BUILTIN_THEME_NAMES.len(), 7);
    }

    #[test]
    fn default_terminal_bg_is_reset() {
        let theme = Theme::builtin("default_terminal").unwrap();
        assert_eq!(theme.color(Role::Bg), ThemeColor::Reset);
    }

    #[test]
    fn oled_black_bg_is_pure_black() {
        let theme = Theme::builtin("oled_black").unwrap();
        assert_eq!(theme.color(Role::Bg), ThemeColor::Rgb(0, 0, 0));
    }

    #[test]
    fn missing_role_falls_back_to_reset_with_warning() {
        let theme = Theme::parse("name = \"partial\"\n[roles]\nfg = \"#ffffff\"\n").unwrap();
        assert_eq!(theme.color(Role::Fg), ThemeColor::Rgb(0xff, 0xff, 0xff));
        // Every other role was never set — all must fall back to Reset, not panic or default to
        // something arbitrary.
        assert_eq!(theme.color(Role::Bg), ThemeColor::Reset);
        assert_eq!(theme.color(Role::Accent), ThemeColor::Reset);
        // The "warning" a caller with logging access (unlike `loxia-core` itself) would emit:
        // every role but `Fg` shows up as missing.
        assert!(theme.missing_roles().contains(&Role::Bg));
        assert!(!theme.missing_roles().contains(&Role::Fg));
        assert_eq!(theme.missing_roles().len(), Role::ALL.len() - 1);
    }

    #[test]
    fn invalid_hex_is_a_parse_error() {
        let bad = "name = \"bad\"\n[roles]\nfg = \"#zzzzzz\"\n";
        assert!(Theme::parse(bad).is_err());

        let too_short = "name = \"bad\"\n[roles]\nfg = \"#fff\"\n";
        assert!(Theme::parse(too_short).is_err());

        let no_hash = "name = \"bad\"\n[roles]\nfg = \"ffffff\"\n";
        assert!(Theme::parse(no_hash).is_err());
    }

    #[test]
    fn indexed_colour_parses() {
        let toml = "name = \"idx\"\n[roles]\naccent = 12\n";
        let theme = Theme::parse(toml).unwrap();
        assert_eq!(theme.color(Role::Accent), ThemeColor::Indexed(12));
    }

    #[test]
    fn ascii_glyphs_have_no_multibyte_chars() {
        let glyphs = Glyphs::ASCII;
        for c in [
            glyphs.progress_filled,
            glyphs.progress_empty,
            glyphs.rule,
            glyphs.playing_marker,
            glyphs.paused_marker,
        ] {
            assert_eq!(c.len_utf8(), 1, "{c:?} is not a single ASCII byte");
        }
    }
}
