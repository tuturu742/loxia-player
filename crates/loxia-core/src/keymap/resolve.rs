//! (context, chord) -> Action resolution, with pending-prefix support.

use smallvec::SmallVec;

use super::{ActionId, InputContext, KeyBinding, KeyChord, KeyCode, KeyMap};
use crate::state::modal::ModalKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Action(ActionId),
    Pending,
    None,
}

/// The keys `TextInput` still routes through the keymap — everything else (in particular any
/// bare printable character) goes to the text buffer instead (`docs/04-state-and-input.md` §5).
fn text_input_allows(chord: &KeyChord) -> bool {
    matches!(
        chord.code,
        KeyCode::Esc
            | KeyCode::Enter
            | KeyCode::Tab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
    )
}

/// Available inside every modal regardless of `kind`, on top of that modal's own table
/// (`docs/04-state-and-input.md` §5). No per-modal table is wired here yet — each modal's own
/// task builds that; see `docs/12-decisions.md`.
fn modal_universal(chord: &KeyChord) -> Option<ActionId> {
    let m = chord.mods;
    match chord.code {
        KeyCode::Esc if !m.ctrl && !m.alt && !m.shift => Some(ActionId::Cancel),
        KeyCode::Char('?') if !m.ctrl && !m.alt => Some(ActionId::ToggleHelp),
        KeyCode::Char('c') if m.ctrl && !m.alt && !m.shift => Some(ActionId::Quit),
        _ => None,
    }
}

/// `10-03`: the help modal's own table — `j`/`k`/`Ctrl+U`/`Ctrl+D`, reusing
/// `MoveDown`/`MoveUp`/`HalfPageDown`/`HalfPageUp` rather than inventing dedicated scroll
/// `ActionId`s, since `input.rs`'s own `action_for` already re-routes exactly these four to
/// `ModalAction::Scroll` specifically while `Modal::Help` is open (`docs/12-decisions.md`).
/// Hardcoded to the *default* bindings for these four actions, deliberately, the same way
/// `modal_universal`'s own `Esc`/`?`/`Ctrl+C` are — a user's own remap of `j`/`k` elsewhere is not
/// consulted here, matching precedent rather than adding a new inconsistency.
fn help_scroll(chord: &KeyChord) -> Option<ActionId> {
    let m = chord.mods;
    match chord.code {
        KeyCode::Char('j') if !m.ctrl && !m.alt && !m.shift => Some(ActionId::MoveDown),
        KeyCode::Char('k') if !m.ctrl && !m.alt && !m.shift => Some(ActionId::MoveUp),
        KeyCode::Char('d') if m.ctrl && !m.alt && !m.shift => Some(ActionId::HalfPageDown),
        KeyCode::Char('u') if m.ctrl && !m.alt && !m.shift => Some(ActionId::HalfPageUp),
        _ => None,
    }
}

/// `10-05`/`10-07`/`10-08`/`10-09`/`11-02`: shared by `Modal::DevicePicker`, `Modal::SleepTimer`,
/// `Modal::SavePlaylist`, `Modal::SortProfile`, and `Modal::KeymapEditor` — `j`/`k`/arrow
/// `Down`/`Up` move a flat list cursor, reusing `MoveDown`/`MoveUp` rather than dedicated
/// `ActionId`s (`input.rs`'s own `action_for` re-routes them to `ModalAction::FieldNext`/
/// `FieldPrev` for the first, second, fourth, and fifth, `CycleSaveTarget` for `SavePlaylist` only
/// while its own target dropdown has focus), the same reuse-not-reinvent shape `help_scroll`
/// already established above. Not folded into a single generic "every modal" table: a later
/// modal's own up/down keys can mean something entirely different (`Modal::Equalizer`'s arrow-key
/// gain nudging, `10-06`, moves a value, not a field cursor) — `docs/12-decisions.md`.
fn list_nav(chord: &KeyChord) -> Option<ActionId> {
    let m = chord.mods;
    if m.ctrl || m.alt || m.shift {
        return None;
    }
    match chord.code {
        KeyCode::Char('j') | KeyCode::Down => Some(ActionId::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(ActionId::MoveUp),
        _ => None,
    }
}

/// `10-06`: the equalizer's own table — `←`/`→` select a band (reusing `NavLeft`/`NavRight`,
/// re-routed by `input.rs`'s `action_for` to `ModalAction::FieldPrev`/`FieldNext`, the same generic
/// per-modal cursor `Modal::Equalizer.band` already uses); `↑`/`↓` adjust the selected band's gain
/// (reusing `MoveUp`/`MoveDown`, re-routed to `ModalAction::AdjustGain`). Arrow keys only — no
/// `j`/`k`/`h`/`l` aliases, since this task's own wireframe names only `[←/→]`/`[↑/↓]`. `p`/`b`
/// (presets/bypass) are hardcoded directly in `input.rs` instead, like `Enter`/`Tab` are for a
/// text-editing modal's own field — neither has an existing `ActionId` to reuse, and inventing one
/// solely to route through this table would be pure indirection.
fn equalizer_nav(chord: &KeyChord) -> Option<ActionId> {
    let m = chord.mods;
    if m.ctrl || m.alt || m.shift {
        return None;
    }
    match chord.code {
        KeyCode::Left => Some(ActionId::NavLeft),
        KeyCode::Right => Some(ActionId::NavRight),
        KeyCode::Up => Some(ActionId::MoveUp),
        KeyCode::Down => Some(ActionId::MoveDown),
        _ => None,
    }
}

pub fn resolve(
    map: &KeyMap,
    ctx: InputContext,
    pending: Option<&KeyChord>,
    chord: KeyChord,
) -> Resolution {
    match ctx {
        InputContext::TextInput => {
            if !text_input_allows(&chord) {
                return Resolution::None;
            }
        }
        InputContext::Modal(kind) => {
            if kind == ModalKind::Help
                && let Some(action) = help_scroll(&chord)
            {
                return Resolution::Action(action);
            }
            if matches!(
                kind,
                ModalKind::DevicePicker
                    | ModalKind::SleepTimer
                    | ModalKind::SavePlaylist
                    | ModalKind::SortProfile
                    | ModalKind::KeymapEditor
            ) && let Some(action) = list_nav(&chord)
            {
                return Resolution::Action(action);
            }
            if kind == ModalKind::Equalizer
                && let Some(action) = equalizer_nav(&chord)
            {
                return Resolution::Action(action);
            }
            return match modal_universal(&chord) {
                Some(action) => Resolution::Action(action),
                None => Resolution::None,
            };
        }
        InputContext::Normal => {}
    }

    let binding = match pending {
        Some(prefix) => KeyBinding(SmallVec::from_slice(&[*prefix, chord])),
        None => KeyBinding(SmallVec::from_slice(&[chord])),
    };

    if let Some(action) = map.resolve_normal(&binding) {
        return Resolution::Action(action);
    }

    if pending.is_none() && map.has_binding_with_prefix(&chord) {
        return Resolution::Pending;
    }

    Resolution::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::KeyModifiers;

    fn chord(code: KeyCode) -> KeyChord {
        KeyChord {
            code,
            mods: KeyModifiers::default(),
        }
    }

    fn ctrl_chord(c: char) -> KeyChord {
        KeyChord {
            code: KeyCode::Char(c),
            mods: KeyModifiers {
                ctrl: true,
                ..KeyModifiers::default()
            },
        }
    }

    #[test]
    fn text_input_context_swallows_printable_keys() {
        let map = KeyMap::defaults();
        let printable = chord(KeyCode::Char('j'));
        assert_eq!(
            resolve(&map, InputContext::TextInput, None, printable),
            Resolution::None
        );

        let esc = chord(KeyCode::Esc);
        assert_eq!(
            resolve(&map, InputContext::TextInput, None, esc),
            Resolution::Action(ActionId::Cancel)
        );
    }

    #[test]
    fn modal_context_uses_modal_table() {
        use crate::state::modal::ModalKind;
        // `ModalKind::Confirm` (the one modal kind left with no extra table of its own — `Help`
        // has one for scroll keys, `10-03`; `DevicePicker`/`SleepTimer`/`SavePlaylist`/
        // `SortProfile`/`KeymapEditor` share one for cursor keys, `10-05`/`10-07`/`10-08`/
        // `10-09`/`11-02`; `Equalizer` has one for band/gain keys, `10-06`): only the three
        // universal keys resolve, everything else is swallowed.
        let map = KeyMap::defaults();
        let ctx = InputContext::Modal(ModalKind::Confirm);

        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Esc)),
            Resolution::Action(ActionId::Cancel)
        );
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Char('?'))),
            Resolution::Action(ActionId::ToggleHelp)
        );
        assert_eq!(
            resolve(&map, ctx, None, ctrl_chord('c')),
            Resolution::Action(ActionId::Quit)
        );
        // Not one of the three universal keys, and no per-modal table exists for this kind:
        // swallowed.
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Char('j'))),
            Resolution::None
        );
    }

    /// `10-05`: `ModalKind::DevicePicker` is the second modal kind with its own extra table —
    /// `j`/`k`/arrow `Down`/`Up` move the device cursor, reusing `MoveDown`/`MoveUp` rather than
    /// dedicated `ActionId`s (`input.rs`'s own `action_for` re-routes them to
    /// `ModalAction::FieldNext`/`FieldPrev` specifically while this modal is open).
    #[test]
    fn device_picker_moves_cursor_with_jk_and_arrows() {
        use crate::state::modal::ModalKind;
        let map = KeyMap::defaults();
        let ctx = InputContext::Modal(ModalKind::DevicePicker);

        for down in [chord(KeyCode::Char('j')), chord(KeyCode::Down)] {
            assert_eq!(
                resolve(&map, ctx, None, down),
                Resolution::Action(ActionId::MoveDown)
            );
        }
        for up in [chord(KeyCode::Char('k')), chord(KeyCode::Up)] {
            assert_eq!(
                resolve(&map, ctx, None, up),
                Resolution::Action(ActionId::MoveUp)
            );
        }
        // The three universal modal keys still resolve too, unaffected by this modal's own table.
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Esc)),
            Resolution::Action(ActionId::Cancel)
        );
    }

    /// `10-07`: `ModalKind::SleepTimer` shares `list_nav` with `DevicePicker` — same `j`/`k`/arrow
    /// row-cursor behaviour, re-routed by `input.rs`'s `action_for` to the same `FieldNext`/
    /// `FieldPrev` mechanism.
    #[test]
    fn sleep_timer_moves_cursor_with_jk_and_arrows() {
        use crate::state::modal::ModalKind;
        let map = KeyMap::defaults();
        let ctx = InputContext::Modal(ModalKind::SleepTimer);

        for down in [chord(KeyCode::Char('j')), chord(KeyCode::Down)] {
            assert_eq!(
                resolve(&map, ctx, None, down),
                Resolution::Action(ActionId::MoveDown)
            );
        }
        for up in [chord(KeyCode::Char('k')), chord(KeyCode::Up)] {
            assert_eq!(
                resolve(&map, ctx, None, up),
                Resolution::Action(ActionId::MoveUp)
            );
        }
    }

    /// `10-08`: `ModalKind::SavePlaylist` shares `list_nav` too — same `j`/`k`/arrow resolution;
    /// `input.rs`'s own `action_for` is what makes it mean "cycle the target dropdown" only while
    /// `field == 0`, and a no-op otherwise.
    #[test]
    fn save_playlist_moves_cursor_with_jk_and_arrows() {
        use crate::state::modal::ModalKind;
        let map = KeyMap::defaults();
        let ctx = InputContext::Modal(ModalKind::SavePlaylist);

        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Down)),
            Resolution::Action(ActionId::MoveDown)
        );
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Up)),
            Resolution::Action(ActionId::MoveUp)
        );
    }

    /// `10-09`: `ModalKind::SortProfile` shares `list_nav` too — `j`/`k`/arrows move the profile
    /// list cursor, re-routed by `input.rs`'s `action_for` to the same `FieldNext`/`FieldPrev`
    /// mechanism `DevicePicker`/`SleepTimer` already use.
    #[test]
    fn sort_profile_moves_cursor_with_jk_and_arrows() {
        use crate::state::modal::ModalKind;
        let map = KeyMap::defaults();
        let ctx = InputContext::Modal(ModalKind::SortProfile);

        for down in [chord(KeyCode::Char('j')), chord(KeyCode::Down)] {
            assert_eq!(
                resolve(&map, ctx, None, down),
                Resolution::Action(ActionId::MoveDown)
            );
        }
        for up in [chord(KeyCode::Char('k')), chord(KeyCode::Up)] {
            assert_eq!(
                resolve(&map, ctx, None, up),
                Resolution::Action(ActionId::MoveUp)
            );
        }
    }

    /// `11-02`: `ModalKind::KeymapEditor` shares `list_nav` too — `j`/`k`/arrows move the action
    /// row cursor, re-routed by `input.rs`'s `action_for` to the same `FieldNext`/`FieldPrev`
    /// mechanism. Only reachable while `capturing` is false — `input.rs`'s own dedicated
    /// capture-mode check claims every key ahead of this function entirely while it's true.
    #[test]
    fn keymap_editor_moves_cursor_with_jk_and_arrows() {
        use crate::state::modal::ModalKind;
        let map = KeyMap::defaults();
        let ctx = InputContext::Modal(ModalKind::KeymapEditor);

        for down in [chord(KeyCode::Char('j')), chord(KeyCode::Down)] {
            assert_eq!(
                resolve(&map, ctx, None, down),
                Resolution::Action(ActionId::MoveDown)
            );
        }
        for up in [chord(KeyCode::Char('k')), chord(KeyCode::Up)] {
            assert_eq!(
                resolve(&map, ctx, None, up),
                Resolution::Action(ActionId::MoveUp)
            );
        }
    }

    /// `10-06`: `ModalKind::Equalizer` selects a band with the arrows (not `j`/`k` — this task's
    /// own wireframe only ever names `[←/→]`/`[↑/↓]`) and adjusts gain with `↑`/`↓`, both reusing
    /// existing `ActionId`s exactly like `DevicePicker`'s own table above (`input.rs`'s `action_for`
    /// re-routes them to `ModalAction::FieldPrev`/`FieldNext`/`AdjustGain` specifically while this
    /// modal is open).
    #[test]
    fn equalizer_arrows_select_band_and_adjust_gain() {
        use crate::state::modal::ModalKind;
        let map = KeyMap::defaults();
        let ctx = InputContext::Modal(ModalKind::Equalizer);

        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Left)),
            Resolution::Action(ActionId::NavLeft)
        );
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Right)),
            Resolution::Action(ActionId::NavRight)
        );
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Up)),
            Resolution::Action(ActionId::MoveUp)
        );
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Down)),
            Resolution::Action(ActionId::MoveDown)
        );
        // `j`/`k` are *not* aliased here, unlike `Help`/`DevicePicker` — this modal's own spec
        // names only the arrow keys.
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Char('j'))),
            Resolution::None
        );
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Esc)),
            Resolution::Action(ActionId::Cancel)
        );
    }

    /// `10-03`: `ModalKind::Help` is the one modal kind with its own extra table —
    /// `j`/`k`/`Ctrl+U`/`Ctrl+D` scroll it, reusing `MoveDown`/`MoveUp`/`HalfPageDown`/
    /// `HalfPageUp` rather than a dedicated scroll `ActionId` (`input.rs`'s own `action_for` is
    /// what turns these into `ModalAction::Scroll` specifically while `Modal::Help` is open).
    #[test]
    fn help_modal_scrolls_with_jk_and_ctrl_ud() {
        use crate::state::modal::ModalKind;
        let map = KeyMap::defaults();
        let ctx = InputContext::Modal(ModalKind::Help);

        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Char('j'))),
            Resolution::Action(ActionId::MoveDown)
        );
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Char('k'))),
            Resolution::Action(ActionId::MoveUp)
        );
        assert_eq!(
            resolve(&map, ctx, None, ctrl_chord('d')),
            Resolution::Action(ActionId::HalfPageDown)
        );
        assert_eq!(
            resolve(&map, ctx, None, ctrl_chord('u')),
            Resolution::Action(ActionId::HalfPageUp)
        );
        // The three universal modal keys still resolve too, unaffected by Help's own table.
        assert_eq!(
            resolve(&map, ctx, None, chord(KeyCode::Esc)),
            Resolution::Action(ActionId::Cancel)
        );
    }

    #[test]
    fn g_prefix_returns_pending_then_resolves() {
        let map = KeyMap::defaults();
        let g = chord(KeyCode::Char('g'));
        assert_eq!(
            resolve(&map, InputContext::Normal, None, g),
            Resolution::Pending
        );
        let second = chord(KeyCode::Char('a'));
        assert_eq!(
            resolve(&map, InputContext::Normal, Some(&g), second),
            Resolution::Action(ActionId::GoToArtist)
        );
    }

    #[test]
    fn invalid_sequence_after_prefix_returns_none() {
        let map = KeyMap::defaults();
        let g = chord(KeyCode::Char('g'));
        let bogus = chord(KeyCode::Char('z'));
        assert_eq!(
            resolve(&map, InputContext::Normal, Some(&g), bogus),
            Resolution::None
        );
    }
}
