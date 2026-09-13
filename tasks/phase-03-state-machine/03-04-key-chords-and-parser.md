# 03-04 · Key chords and binding parser

**Phase:** 03 — State machine · **Agent:** A · **Size:** M
**Prerequisites:** `03-03`
**Reference:** `docs/04-state-and-input.md` §7

## Goal
Represent keybindings and parse them from config strings, in both the friendly and verbose syntaxes,
with an exact round-trip. The keymap editor in phase 11 depends on that round-trip being lossless.

## Files
- `crates/loxia-core/src/keymap/mod.rs`, `parse.rs`

## Specification

```
pub struct KeyChord { pub code: KeyCode, pub mods: KeyModifiers }
pub struct KeyBinding(pub SmallVec<[KeyChord; 2]>);   // length 1 or 2
```
`loxia-core` cannot depend on `crossterm`, so define **local** `KeyCode` and `KeyModifiers` types
here (`Char(char)`, `Enter`, `Esc`, `Tab`, `Backspace`, `Delete`, `Left`, `Right`, `Up`, `Down`,
`Home`, `End`, `PageUp`, `PageDown`, `F(u8)`, `Insert`). The binary converts from crossterm's types
in task `03-09`. `KeyModifiers` is a bitflag over `CTRL`, `ALT`, `SHIFT`.

**Parsing** (`parse.rs`):
```
pub fn parse_binding(s: &str) -> Result<KeyBinding, ParseError>;
pub fn render_binding(b: &KeyBinding) -> String;   // always the friendly syntax
```

Friendly syntax, which is what `render_binding` emits:
- Named keys, case-insensitive: `space enter esc tab backspace delete left right up down home end
  pageup pagedown insert f1`…`f12`
- Bare characters, **case-sensitive**: `a`, `A`, `?`, `[`, `.`
- Modifiers: `ctrl+`, `alt+`, `shift+`, normalised to that order
- Two-chord sequences separated by a single space: `g a`

Verbose syntax, accepted for compatibility with `design_overview` §7:
- `Char('?')`, `Space`, `Enter`
- `Modifiers(ALT) + Char('1')`, `Modifiers(CONTROL) + Char('p')`

Normalisation rules:
- `shift+a` and `A` parse to the **same** chord. Pick one canonical form — `A` — and always render
  that, or the round-trip test fails.
- A modifier applied to a named key keeps the modifier: `shift+enter` stays `shift+enter`.
- Unknown key names, empty strings, and sequences longer than 2 chords are `ParseError`, so
  `01-02` can drop the entry with a warning.

## Acceptance
- `parse_render_roundtrip` (proptest over generated `KeyBinding`s) — `parse(render(b)) == b`.
- `friendly_and_verbose_parse_identically` — table test over every binding in
  `docs/04-state-and-input.md` §6 expressed both ways.
- `shift_char_normalises` — `shift+a` and `A` produce equal chords, and both render as `A`.
- `modifier_order_is_normalised` — `alt+ctrl+p` renders as `ctrl+alt+p`.
- `two_chord_sequence_parses` — `g a`.
- `three_chord_sequence_is_an_error`
- `unknown_key_name_is_an_error`
- `empty_string_is_an_error`
- `case_insensitive_named_keys` — `Space`, `space`, `SPACE` all parse.
- `bare_char_is_case_sensitive` — `p` and `P` are different bindings.

## Done when
The global DoD in `tasks/README.md` is satisfied.
