//! Binding-string parser and renderer. Two syntaxes accepted on read (friendly, verbose);
//! `render_binding` always emits the friendly one. `parse(render(b)) == b` for every binding —
//! the keymap editor (phase 11) depends on that round-trip being lossless.

use smallvec::SmallVec;

use super::{KeyBinding, KeyChord, KeyCode, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("empty binding string.")]
    Empty,
    #[error("unknown key name.")]
    UnknownKey(String),
    #[error("a binding may have at most 2 chords.")]
    TooManyChords(usize),
}

/// Parses either syntax. Verbose forms (`Char('?')`, `Modifiers(ALT) + Char('1')`) always
/// describe exactly one chord; only the friendly syntax supports 2-chord sequences.
pub fn parse_binding(s: &str) -> Result<KeyBinding, ParseError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    if let Some(result) = try_parse_verbose(trimmed) {
        return result.map(|chord| KeyBinding(SmallVec::from_slice(&[chord])));
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() > 2 {
        return Err(ParseError::TooManyChords(parts.len()));
    }
    let chords: SmallVec<[KeyChord; 2]> = parts
        .iter()
        .map(|p| parse_friendly_chord(p))
        .collect::<Result<_, _>>()?;
    Ok(KeyBinding(chords))
}

/// Always the friendly syntax: `ctrl+alt+shift+` (normalised order) followed by a named key or a
/// bare character, chords space-separated.
pub fn render_binding(b: &KeyBinding) -> String {
    b.0.iter().map(render_chord).collect::<Vec<_>>().join(" ")
}

fn render_chord(chord: &KeyChord) -> String {
    let mut s = String::new();
    if chord.mods.ctrl {
        s.push_str("ctrl+");
    }
    if chord.mods.alt {
        s.push_str("alt+");
    }
    if chord.mods.shift {
        s.push_str("shift+");
    }
    s.push_str(&render_code(chord.code));
    s
}

fn render_code(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::F(n) => format!("f{n}"),
    }
}

fn parse_named_key(s: &str) -> Option<KeyCode> {
    let lower = s.to_ascii_lowercase();
    Some(match lower.as_str() {
        "space" => KeyCode::Char(' '),
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "insert" | "ins" => KeyCode::Insert,
        _ if lower.len() >= 2
            && lower.starts_with('f')
            && lower[1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            let n: u8 = lower[1..].parse().ok()?;
            if (1..=12).contains(&n) {
                KeyCode::F(n)
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

/// Strips `ctrl+`/`alt+`/`shift+` prefixes (case-insensitively, any order — normalised to
/// `ctrl+alt+shift+` on render regardless of input order) and parses the remainder as a named key
/// or a single bare, case-sensitive character.
fn parse_friendly_chord(s: &str) -> Result<KeyChord, ParseError> {
    let mut mods = KeyModifiers::default();
    let mut rest = s;
    loop {
        // ASCII-only prefixes: lowercasing never changes byte length or offsets, so slicing the
        // original (case-preserved) `rest` at the same byte index recovered from the lowercased
        // copy is always valid and needs no allocation.
        let lower_rest = rest.to_ascii_lowercase();
        if lower_rest.starts_with("ctrl+") {
            mods.ctrl = true;
            rest = &rest[5..];
        } else if lower_rest.starts_with("alt+") {
            mods.alt = true;
            rest = &rest[4..];
        } else if lower_rest.starts_with("shift+") {
            mods.shift = true;
            rest = &rest[6..];
        } else {
            break;
        }
    }

    let code = parse_named_key(rest).or_else(|| {
        let mut chars = rest.chars();
        let c = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        Some(KeyCode::Char(c))
    });
    let code = code.ok_or_else(|| ParseError::UnknownKey(rest.to_string()))?;

    // Canonical form: a `Char` chord never carries a `shift` flag — the character itself already
    // encodes it. `shift+a` and `A` must parse to the identical chord, or the round-trip breaks.
    if mods.shift
        && let KeyCode::Char(c) = code
    {
        let normalized = if c.is_ascii_lowercase() {
            c.to_ascii_uppercase()
        } else {
            c
        };
        return Ok(KeyChord {
            code: KeyCode::Char(normalized),
            mods: KeyModifiers {
                shift: false,
                ..mods
            },
        });
    }
    Ok(KeyChord { code, mods })
}

fn parse_verbose_char(s: &str) -> Result<KeyCode, ParseError> {
    let inner = s
        .strip_prefix("Char(")
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(|| ParseError::UnknownKey(s.to_string()))?;
    let inner = inner.trim().trim_matches('\'');
    let mut chars = inner.chars();
    let c = chars
        .next()
        .ok_or_else(|| ParseError::UnknownKey(s.to_string()))?;
    if chars.next().is_some() {
        return Err(ParseError::UnknownKey(s.to_string()));
    }
    Ok(KeyCode::Char(c))
}

fn parse_verbose_modifiers(s: &str) -> Result<KeyChord, ParseError> {
    let rest = s
        .strip_prefix("Modifiers(")
        .ok_or_else(|| ParseError::UnknownKey(s.to_string()))?;
    let (mods_part, rest) = rest
        .split_once(')')
        .ok_or_else(|| ParseError::UnknownKey(s.to_string()))?;

    let mut mods = KeyModifiers::default();
    for part in mods_part.split('+') {
        match part.trim().to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => mods.ctrl = true,
            "ALT" => mods.alt = true,
            "SHIFT" => mods.shift = true,
            other => return Err(ParseError::UnknownKey(other.to_string())),
        }
    }

    let rest = rest.trim().trim_start_matches('+').trim();
    let code = if rest.starts_with("Char(") {
        parse_verbose_char(rest)?
    } else {
        parse_named_key(rest).ok_or_else(|| ParseError::UnknownKey(rest.to_string()))?
    };
    Ok(KeyChord { code, mods })
}

/// `None` means "not verbose syntax, try the friendly parser instead" — every friendly named key
/// already parses case-insensitively, so only `Char('...')` and `Modifiers(...)` need a distinct
/// path here.
fn try_parse_verbose(s: &str) -> Option<Result<KeyChord, ParseError>> {
    if s.starts_with("Modifiers(") {
        Some(parse_verbose_modifiers(s))
    } else if s.starts_with("Char(") {
        Some(parse_verbose_char(s).map(|code| KeyChord {
            code,
            mods: KeyModifiers::default(),
        }))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn arb_keycode() -> impl Strategy<Value = KeyCode> {
        prop_oneof![
            (b'a'..=b'z').prop_map(|b| KeyCode::Char(b as char)),
            (b'A'..=b'Z').prop_map(|b| KeyCode::Char(b as char)),
            Just(KeyCode::Char('?')),
            Just(KeyCode::Char('.')),
            Just(KeyCode::Char('[')),
            Just(KeyCode::Enter),
            Just(KeyCode::Esc),
            Just(KeyCode::Tab),
            Just(KeyCode::Backspace),
            Just(KeyCode::Delete),
            Just(KeyCode::Left),
            Just(KeyCode::Right),
            Just(KeyCode::Up),
            Just(KeyCode::Down),
            Just(KeyCode::Home),
            Just(KeyCode::End),
            Just(KeyCode::PageUp),
            Just(KeyCode::PageDown),
            Just(KeyCode::Insert),
            (1u8..=12).prop_map(KeyCode::F),
        ]
    }

    fn arb_chord() -> impl Strategy<Value = KeyChord> {
        (arb_keycode(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
            |(code, ctrl, alt, shift)| {
                // Canonical form: `shift` is only ever meaningful (kept) on a non-`Char` code.
                let shift = if matches!(code, KeyCode::Char(_)) {
                    false
                } else {
                    shift
                };
                KeyChord {
                    code,
                    mods: KeyModifiers { ctrl, alt, shift },
                }
            },
        )
    }

    fn arb_binding() -> impl Strategy<Value = KeyBinding> {
        prop_oneof![
            arb_chord().prop_map(|c| KeyBinding(SmallVec::from_slice(&[c]))),
            (arb_chord(), arb_chord()).prop_map(|(a, b)| KeyBinding(SmallVec::from_slice(&[a, b]))),
        ]
    }

    proptest! {
        #[test]
        fn parse_render_roundtrip(b in arb_binding()) {
            let rendered = render_binding(&b);
            let parsed = parse_binding(&rendered).unwrap();
            prop_assert_eq!(parsed, b);
        }
    }

    #[test]
    fn friendly_and_verbose_parse_identically() {
        let cases = [
            ("j", "Char('j')"),
            ("?", "Char('?')"),
            ("space", "Space"),
            ("enter", "Enter"),
            ("alt+1", "Modifiers(ALT) + Char('1')"),
            ("ctrl+p", "Modifiers(CONTROL) + Char('p')"),
        ];
        for (friendly, verbose) in cases {
            assert_eq!(
                parse_binding(friendly).unwrap(),
                parse_binding(verbose).unwrap(),
                "{friendly:?} vs {verbose:?}"
            );
        }
    }

    #[test]
    fn shift_char_normalises() {
        let a = parse_binding("shift+a").unwrap();
        let b = parse_binding("A").unwrap();
        assert_eq!(a, b);
        assert_eq!(render_binding(&a), "A");
        assert_eq!(render_binding(&b), "A");
    }

    #[test]
    fn modifier_order_is_normalised() {
        let b = parse_binding("alt+ctrl+p").unwrap();
        assert_eq!(render_binding(&b), "ctrl+alt+p");
    }

    #[test]
    fn two_chord_sequence_parses() {
        let b = parse_binding("g a").unwrap();
        assert_eq!(b.0.len(), 2);
        assert_eq!(render_binding(&b), "g a");
    }

    #[test]
    fn three_chord_sequence_is_an_error() {
        assert!(matches!(
            parse_binding("g g g"),
            Err(ParseError::TooManyChords(3))
        ));
    }

    #[test]
    fn unknown_key_name_is_an_error() {
        assert!(matches!(
            parse_binding("foobar"),
            Err(ParseError::UnknownKey(_))
        ));
    }

    #[test]
    fn empty_string_is_an_error() {
        assert!(matches!(parse_binding(""), Err(ParseError::Empty)));
        assert!(matches!(parse_binding("   "), Err(ParseError::Empty)));
    }

    #[test]
    fn case_insensitive_named_keys() {
        let space = KeyBinding(SmallVec::from_slice(&[KeyChord {
            code: KeyCode::Char(' '),
            mods: KeyModifiers::default(),
        }]));
        assert_eq!(parse_binding("Space").unwrap(), space);
        assert_eq!(parse_binding("space").unwrap(), space);
        assert_eq!(parse_binding("SPACE").unwrap(), space);
    }

    #[test]
    fn bare_char_is_case_sensitive() {
        let p = parse_binding("p").unwrap();
        let shift_p = parse_binding("P").unwrap();
        assert_ne!(p, shift_p);
    }
}
