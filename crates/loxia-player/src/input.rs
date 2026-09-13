//! crossterm events -> Action via the keymap and HitMap.
//!
//! The bridge between `crossterm`'s types and `loxia-core`'s local ones (`docs/04-state-and-input.md`
//! §§5, 7).

use std::time::{Duration, Instant};

use loxia_core::action::{
    Action, ItemAction, NavAction, PlayerAction, QueueAction, SelectAction, SettingsAction,
    SystemEvent, ViewAction,
};
use loxia_core::keymap::resolve::{Resolution, resolve};
use loxia_core::keymap::{
    ActionId, InputContext, KeyChord, KeyCode as LoxiaKeyCode, KeyModifiers as LoxiaKeyModifiers,
};
use loxia_core::model::MediaItem;
use loxia_core::state::AppState;
use loxia_core::state::modal::{Modal, ModalKind};
use loxia_core::state::nav::Tab;
use loxia_core::state::player::SeekTarget;
use loxia_core::state::settings::SettingsSection;
use loxia_tui::hit::{HitMap, HitTarget, TransportButton};
use ratatui::crossterm::event::{
    Event as CrosstermEvent, KeyCode as CtKeyCode, KeyEventKind, KeyModifiers as CtKeyModifiers,
    MouseButton as CtMouseButton, MouseEvent as CtMouseEvent, MouseEventKind as CtMouseEventKind,
};
use ratatui::layout::Rect;

/// Just enough of the last render to scale a couple of actions the keymap itself cannot know the
/// size of. `04-02` (root layout) is the eventual source of a real value; until wired, callers
/// pass their best guess (e.g. the raw terminal height).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub rows: usize,
}

/// `10-04`: "a second click within 400 ms on the same target" — the double-click window.
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

/// Mouse gesture state carried across frames by the runtime loop — a click's own double-click
/// detection and a drag's own continued target both need to remember something from one event to
/// the next, unlike every other keyboard-driven `to_action` call, which is stateless.
#[derive(Debug, Default)]
pub struct MouseState {
    last_click: Option<(HitTarget, Instant)>,
    /// The target *and its own `Rect`* captured on `Down` — `Rect` is captured here, not
    /// re-resolved from the current pointer position on each `Drag`, because "a drag that leaves
    /// the widget's rect continues to control it until release" (this task's own spec): once the
    /// pointer is outside the rect, hit-testing the *current* position would find nothing (or the
    /// wrong thing) there at all.
    drag: Option<(Rect, HitTarget)>,
}

pub fn to_action(
    state: &AppState,
    ev: CrosstermEvent,
    viewport: Viewport,
    hits: &HitMap,
    mouse: &mut MouseState,
    now: Instant,
) -> Option<Action> {
    match ev {
        CrosstermEvent::Resize(w, h) => Some(Action::System(SystemEvent::Resize { w, h })),
        // `10-10`: forwarded (rather than discarded, as `Paste` still is) so the reducer can
        // suppress a desktop notification while the terminal already has focus.
        CrosstermEvent::FocusGained => {
            Some(Action::System(SystemEvent::TerminalFocusChanged(true)))
        }
        CrosstermEvent::FocusLost => Some(Action::System(SystemEvent::TerminalFocusChanged(false))),
        CrosstermEvent::Paste(_) => None,
        CrosstermEvent::Mouse(mev) => mouse_action(state, hits, mouse, mev, viewport, now),
        CrosstermEvent::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return None;
            }
            // Always-on, in every context, before any keymap lookup — a user must always be able
            // to leave (`docs/04-state-and-input.md` §7).
            if key.modifiers.contains(CtKeyModifiers::CONTROL)
                && matches!(key.code, CtKeyCode::Char('c'))
            {
                return Some(Action::System(SystemEvent::Quit));
            }

            let chord = to_chord(key.code, key.modifiers)?;

            // `11-02`: every raw key while a keymap capture is in progress is claimed here,
            // verbatim, ahead of `context_for`/`resolve` entirely — "the next key event is
            // captured verbatim rather than resolved" (this task's own spec). Checked the same way
            // `Ctrl+C` is checked ahead of every context below; `Ctrl+C` itself already returned
            // above, so it can never reach here as a captured chord (`docs/12-decisions.md`).
            if matches!(
                state.modal,
                Some(Modal::KeymapEditor {
                    capturing: true,
                    ..
                })
            ) {
                return Some(
                    if chord.code == LoxiaKeyCode::Esc && chord.mods == LoxiaKeyModifiers::default()
                    {
                        Action::Modal(loxia_core::action::ModalAction::CancelCapture)
                    } else {
                        Action::Modal(loxia_core::action::ModalAction::CaptureChord(chord))
                    },
                );
            }

            // The sidebar's "Quit" row is focused (`↓` past the last tab): `Enter`/`→` quit outright,
            // ahead of any tab-specific dispatch (e.g. the Settings tab, whose own sidebar handler
            // would otherwise claim `Enter`/`→` for its section list). `↑`/`↓` fall through so
            // `move_focused` -> `switch_tab` can navigate off the Quit row. No modal is open here (a
            // modal takes the whole context), so this is safe to check unconditionally.
            if state.nav.sidebar_quit_focused
                && chord.mods == LoxiaKeyModifiers::default()
                && matches!(
                    chord.code,
                    LoxiaKeyCode::Enter | LoxiaKeyCode::Right | LoxiaKeyCode::Char('l')
                )
            {
                return Some(Action::System(SystemEvent::Quit));
            }

            match context_for(state) {
                // Falls back to the normal table for anything a text field has no use for. Without
                // it, `Alt+1`..`Alt+9` typed their bare digit into the Search query (the arms below
                // match on `code` alone, so a modified `Char` still read as text) and `F1`..`F10`
                // vanished entirely — the tab jumps and Help were unreachable from Search
                // (`docs/12-decisions.md`).
                InputContext::TextInput => text_input_action(state, chord)
                    .or_else(|| normal_resolve(state, viewport, chord)),
                // `11-02`: `Enter` on `Modal::KeymapEditor` means "start capture" or "confirm/rebind
                // a finished one" depending on whether a capture has already produced something —
                // checked ahead of the generic `Enter -> Submit` arm just below, the more specific
                // arm winning as Rust match arms always do top-to-bottom.
                InputContext::Modal(ModalKind::KeymapEditor)
                    if chord.code == LoxiaKeyCode::Enter =>
                {
                    Some(keymap_editor_enter_action(state))
                }
                // `10-05`: `Enter` submitting the open modal is checked before `resolve` at all —
                // every modal kind's own `reducer::modal::submit` arm already exists and is
                // already exercised directly by reducer tests (`Modal::Help`'s is a harmless
                // no-op), but until this task nothing ever produced `ModalAction::Submit` from a
                // real keypress except the text-field special case below
                // (`modal_text_input_action`, which never reaches this branch at all). Checked
                // ahead of `resolve` the same way `Ctrl+C` is checked ahead of every context above
                // — `docs/12-decisions.md`.
                InputContext::Modal(_) if chord.code == LoxiaKeyCode::Enter => {
                    Some(Action::Modal(loxia_core::action::ModalAction::Submit))
                }
                // `10-06`: `p`/`b` (presets/bypass) have no existing `ActionId` worth reusing —
                // hardcoded here exactly like `modal_text_input_action`'s own `Enter`/`Tab` are
                // for a text-editing modal's field, rather than inventing a single-purpose
                // `ActionId` solely to route through `keymap::resolve`.
                InputContext::Modal(ModalKind::Equalizer)
                    if matches!(
                        chord.code,
                        LoxiaKeyCode::Char('p') | LoxiaKeyCode::Char('b') | LoxiaKeyCode::Char('t')
                    ) && chord.mods == LoxiaKeyModifiers::default() =>
                {
                    match chord.code {
                        LoxiaKeyCode::Char('p') => {
                            Some(Action::Modal(loxia_core::action::ModalAction::CyclePreset))
                        }
                        LoxiaKeyCode::Char('b') => {
                            Some(Action::Modal(loxia_core::action::ModalAction::ToggleBypass))
                        }
                        // `t`, not `o`: this modal's keys are hardcoded and so context-scoped,
                        // but `o` is `OpenSortMenu` everywhere else in the app and reusing it here
                        // reads as "ordering" at a glance. `t` is bound to nothing in any context.
                        LoxiaKeyCode::Char('t') => Some(Action::Modal(
                            loxia_core::action::ModalAction::ToggleEqEnabled,
                        )),
                        _ => unreachable!(),
                    }
                }
                // `10-07`: `Space`/`d` (activate/disarm) have no existing `ActionId` worth
                // reusing either — hardcoded the same way `p`/`b` are above.
                InputContext::Modal(ModalKind::SleepTimer)
                    if matches!(
                        chord.code,
                        LoxiaKeyCode::Char(' ') | LoxiaKeyCode::Char('d')
                    ) && chord.mods == LoxiaKeyModifiers::default() =>
                {
                    match chord.code {
                        LoxiaKeyCode::Char(' ') => Some(Action::Modal(
                            loxia_core::action::ModalAction::ActivateField,
                        )),
                        LoxiaKeyCode::Char('d') => Some(Action::Modal(
                            loxia_core::action::ModalAction::DisarmSleepTimer,
                        )),
                        _ => unreachable!(),
                    }
                }
                // `10-09`: `Tab`/`e` (toggle apply-to target / open Settings → Sorting) have no
                // existing `ActionId` worth reusing either — hardcoded the same way `Space`/`d`
                // are above. `Tab` specifically (not `p`/`b`-style bare letters) since this modal
                // has no text field for it to otherwise mean "commit and move on".
                InputContext::Modal(ModalKind::SortProfile)
                    if matches!(chord.code, LoxiaKeyCode::Tab | LoxiaKeyCode::Char('e'))
                        && chord.mods == LoxiaKeyModifiers::default() =>
                {
                    match chord.code {
                        LoxiaKeyCode::Tab => Some(Action::Modal(
                            loxia_core::action::ModalAction::ToggleSortTarget,
                        )),
                        LoxiaKeyCode::Char('e') => Some(Action::Modal(
                            loxia_core::action::ModalAction::OpenSettingsSorting,
                        )),
                        _ => unreachable!(),
                    }
                }
                // `11-02`: `d`/`x`/`R` (reset row / unbind row / reset all, behind a confirm) have
                // no existing `ActionId` worth reusing either — hardcoded the same way `Tab`/`e`
                // are for `SortProfile` above. Not reachable while `capturing` (that's claimed
                // entirely by the check above `context_for`), so these three letters are free to
                // mean something else here than they would as a captured chord.
                InputContext::Modal(ModalKind::KeymapEditor)
                    if matches!(
                        chord.code,
                        LoxiaKeyCode::Char('d') | LoxiaKeyCode::Char('x') | LoxiaKeyCode::Char('R')
                    ) && chord.mods == LoxiaKeyModifiers::default() =>
                {
                    match chord.code {
                        LoxiaKeyCode::Char('d') => Some(Action::Modal(
                            loxia_core::action::ModalAction::ResetRowToDefault,
                        )),
                        LoxiaKeyCode::Char('x') => {
                            Some(Action::Modal(loxia_core::action::ModalAction::UnbindRow))
                        }
                        LoxiaKeyCode::Char('R') => Some(Action::Modal(
                            loxia_core::action::ModalAction::ConfirmResetAllKeybindings,
                        )),
                        _ => unreachable!(),
                    }
                }
                ctx @ InputContext::Modal(_) => match resolve(&state.keymap, ctx, None, chord) {
                    Resolution::Action(id) => action_for(id, state, viewport),
                    Resolution::Pending | Resolution::None => None,
                },
                // `11-01`: the Settings tab's own row navigation — a flat per-section row list,
                // not a Miller column, so it needs its own dispatch rather than `resolve`'s
                // shared `Normal` table (which would route `↑`/`↓`/`←`/`→` to column
                // navigation instead). Falls back to that same table for anything it doesn't
                // itself claim (`Alt+1..9`/`F2..9`/`Ctrl+Q`/`Ctrl+R`/...), so leaving the tab, or
                // quitting, still works normally.
                InputContext::Normal if state.nav.active_tab == Tab::Settings => {
                    settings_row_action(state, chord)
                        .or_else(|| normal_resolve(state, viewport, chord))
                }
                InputContext::Normal => normal_resolve(state, viewport, chord),
            }
        }
    }
}

/// `resolve`'s own shared `Normal`-context table — factored out so `11-01`'s Settings-tab
/// dispatch can fall back to it for every chord it doesn't itself claim.
fn normal_resolve(state: &AppState, viewport: Viewport, chord: KeyChord) -> Option<Action> {
    let pending = state.pending_chord.as_ref().map(|(c, _)| c);
    match resolve(&state.keymap, InputContext::Normal, pending, chord) {
        Resolution::Action(id) => action_for(id, state, viewport),
        Resolution::Pending => Some(Action::System(SystemEvent::SetPendingChord(chord))),
        // An invalid continuation resolves to no action at all; `pending_chord` is left for
        // `Tick`'s already-built 1-second expiry (`03-08`) to clear, rather than inventing a
        // second "clear" action alongside `SetPendingChord`.
        Resolution::None => None,
    }
}

/// `11-01`: `↑`/`↓`/`k`/`j` move the row cursor, `←`/`→`/`Space` adjust the focused row's own
/// value, `Ctrl+E` reveals a focused secret field, `Tab`/`Shift+Tab` cycle sections (the closest
/// analogue to a settings dialog's own sub-tabs — this task's own spec names no dedicated
/// section-switch key, `docs/12-decisions.md`), and `Enter`'s meaning depends on the focused
/// row's own control (`StartTextEdit` for a `Text` row, `ActivateRow` for an `Action` row,
/// otherwise nothing — there is nothing to "enter" on a `Toggle`/`Select`/`Slider`/`Number` row).
fn settings_row_action(state: &AppState, chord: KeyChord) -> Option<Action> {
    // Focus parked on the main tab sidebar (reached by `Esc` out of the section list): `↑`/`↓`
    // switch tabs (handed to `normal_resolve` -> `move_focused`, which sees Settings on
    // `NavFocus::Sidebar`), `→`/`Enter` steps back into the section list. No sub-editor can be open
    // here, so this is checked ahead of everything (`docs/12-decisions.md`).
    if state.nav.sidebar_focused {
        return match chord.code {
            LoxiaKeyCode::Right | LoxiaKeyCode::Enter
                if chord.mods == LoxiaKeyModifiers::default() =>
            {
                Some(Action::Settings(SettingsAction::FocusSectionList))
            }
            _ => None,
        };
    }
    // Focus on the left-hand section (group) list: `↑`/`↓` pick a group, `Enter`/`→` step into its
    // rows, `Esc`/`←` step out to the tab sidebar. Also mutually exclusive with any open sub-editor.
    if state.settings.section_list_focused {
        if chord.mods != LoxiaKeyModifiers::default() {
            return None;
        }
        return match chord.code {
            LoxiaKeyCode::Up | LoxiaKeyCode::Char('k') => {
                Some(Action::Settings(SettingsAction::PrevSection))
            }
            LoxiaKeyCode::Down | LoxiaKeyCode::Char('j') => {
                Some(Action::Settings(SettingsAction::NextSection))
            }
            LoxiaKeyCode::Enter | LoxiaKeyCode::Right | LoxiaKeyCode::Char('l') => {
                Some(Action::Settings(SettingsAction::FocusRows))
            }
            LoxiaKeyCode::Esc | LoxiaKeyCode::Left | LoxiaKeyCode::Char('h') => {
                Some(Action::Settings(SettingsAction::LeaveToTabSidebar))
            }
            _ => None,
        };
    }
    // `11-03`: the server-profile editor's own row navigation is a completely different table
    // from the ordinary Settings row list below (`Tab` cycles fields, not sections; `Esc` closes
    // the editor, not the generic `Nav::Cancel` ladder) — checked first, unconditionally, since
    // this table below has nothing meaningful to say while it's open.
    if let Some(editor) = &state.settings.server_editor {
        return server_editor_row_action(editor, chord);
    }
    // `11-04`: the sort-profile editor's own row navigation — same "checked first,
    // unconditionally" shape as the server editor above.
    if let Some(editor) = &state.settings.sort_profile_editor {
        return sort_editor_row_action(editor, chord);
    }
    // `11-05`: the EQ-preset editor's own row navigation — same shape again.
    if state.settings.eq_preset_editor.is_some() {
        return eq_editor_row_action(chord);
    }
    // `11-07`: the About section's own two hotkeys, checked (not early-returned) so `Tab`/
    // `Shift+Tab` section-cycling below still works even while the licences pane is open — unlike
    // the three sub-editors above, this isn't a modal takeover of the whole section.
    if state.settings.section == SettingsSection::About
        && let Some(action) = about_row_action(state, chord)
    {
        return Some(action);
    }

    let is_reveal_secret = chord.mods
        == (LoxiaKeyModifiers {
            ctrl: true,
            ..Default::default()
        })
        && chord.code == LoxiaKeyCode::Char('e');
    // Anything modified (Alt+N, Shift+Tab, Ctrl+Q, Ctrl+R, ...) is not this function's own — let
    // it fall straight through to `normal_resolve`.
    if chord.mods != LoxiaKeyModifiers::default() && !is_reveal_secret {
        return None;
    }

    match chord.code {
        LoxiaKeyCode::Up | LoxiaKeyCode::Char('k') => {
            Some(Action::Settings(SettingsAction::MoveRow(-1)))
        }
        LoxiaKeyCode::Down | LoxiaKeyCode::Char('j') => {
            Some(Action::Settings(SettingsAction::MoveRow(1)))
        }
        LoxiaKeyCode::Left => Some(Action::Settings(SettingsAction::AdjustValue(-1))),
        LoxiaKeyCode::Right | LoxiaKeyCode::Char(' ') => {
            Some(Action::Settings(SettingsAction::AdjustValue(1)))
        }
        LoxiaKeyCode::Char('e') if chord.mods.ctrl => {
            Some(Action::Settings(SettingsAction::ToggleRevealSecret))
        }
        // `Tab`/`Shift+Tab` are deliberately **not** claimed here. They cycle the sidebar tabs
        // everywhere else, and entering a tab focuses its content — so claiming them for section
        // cycling meant that tabbing onto Settings trapped the user there with no way to tab back
        // out (`docs/12-decisions.md`). Sections are reached the way the left pane already
        // provides: `Esc` to the section list, `j`/`k` to pick, `Enter` to step into its rows.
        LoxiaKeyCode::Enter => Some(settings_enter_action(state)),
        // `Esc` steps out of the rows to the section (group) list — the user's chosen "go back"
        // key, since `←`/`h` is already spoken for by value adjustment on this row
        // (`docs/12-decisions.md`). Only reached when no sub-editor/licences pane claimed it first.
        LoxiaKeyCode::Esc => Some(Action::Settings(SettingsAction::FocusSectionList)),
        _ => None,
    }
}

/// Always returns `Some` — `Enter` must never fall through to `normal_resolve`'s own global
/// binding (which would otherwise resolve to something nonsensical here, like queueing a
/// selection that doesn't exist in this tab): a `Toggle`/`Select`/`Slider`/`Number` row has
/// nothing to "enter", so this claims the key with a harmless, idempotent `MoveRow(0)` instead of
/// silently letting it leak through to the wrong table.
fn settings_enter_action(state: &AppState) -> Action {
    use loxia_core::reducer::settings::{Control, rows_for_section};

    let rows = rows_for_section(
        state.settings.section,
        &state.config,
        &state.player.known_devices,
    );
    match rows.get(state.settings.cursor).map(|r| &r.control) {
        Some(Control::Text { .. }) => Action::Settings(SettingsAction::StartTextEdit),
        Some(Control::Action { .. }) => Action::Settings(SettingsAction::ActivateRow),
        _ => Action::Settings(SettingsAction::MoveRow(0)),
    }
}

/// `11-03`: the server-profile editor's own row navigation — active whenever `SettingsState::
/// server_editor` is `Some`, entirely separate from the ordinary Settings row table above (`Tab`
/// means "next field" here, not "next section"; letters like `a`/`x`/`D`/`S` are only meaningful
/// in the plain profile list, not the add/edit form). `j`/`k`/arrows and `Ctrl+E` are shared
/// between both sub-views since `SettingsAction::MoveRow`/`ToggleRevealSecret` already branch on
/// which one is open (`reducer::settings::apply`).
fn server_editor_row_action(
    editor: &loxia_core::state::settings::ServerEditorState,
    chord: KeyChord,
) -> Option<Action> {
    let m = chord.mods;
    let allowed = m == LoxiaKeyModifiers::default()
        || m == (LoxiaKeyModifiers {
            ctrl: true,
            ..Default::default()
        })
        || m == (LoxiaKeyModifiers {
            shift: true,
            ..Default::default()
        });
    if !allowed {
        return None;
    }

    if editor.editing.is_some() {
        match chord.code {
            LoxiaKeyCode::Tab if m.shift => Some(Action::Settings(SettingsAction::MoveRow(-1))),
            LoxiaKeyCode::Tab => Some(Action::Settings(SettingsAction::MoveRow(1))),
            LoxiaKeyCode::Up | LoxiaKeyCode::Char('k') if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::MoveRow(-1)))
            }
            LoxiaKeyCode::Down | LoxiaKeyCode::Char('j') if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::MoveRow(1)))
            }
            LoxiaKeyCode::Char('e') if m.ctrl => {
                Some(Action::Settings(SettingsAction::ToggleRevealSecret))
            }
            LoxiaKeyCode::Char('x') if m == LoxiaKeyModifiers::default() => Some(Action::Settings(
                SettingsAction::ServerEditorRemoveFocusedHeader,
            )),
            // `[←→]` cycles whichever row offers a choice — `http`/`https` on `Protocol`, the
            // selected address on `Endpoint` — and is a no-op everywhere else (the reducer refuses
            // the wrong field too, but checking here means these keys still fall through to
            // `normal_resolve` rather than being silently swallowed for no reason).
            //
            // `Endpoint` was missing from this check while its own row *advertised* `[←→]`, so the
            // keys did nothing on the one row that most obviously invited them
            // (`docs/12-decisions.md`).
            LoxiaKeyCode::Left if m == LoxiaKeyModifiers::default() => cycles_with_arrows(editor)
                .then_some(Action::Settings(SettingsAction::ServerEditorCycleProtocol(
                    -1,
                ))),
            LoxiaKeyCode::Right if m == LoxiaKeyModifiers::default() => cycles_with_arrows(editor)
                .then_some(Action::Settings(SettingsAction::ServerEditorCycleProtocol(
                    1,
                ))),
            LoxiaKeyCode::Enter => Some(server_editor_enter_action(editor)),
            LoxiaKeyCode::Esc if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::ServerEditorClose))
            }
            _ => None,
        }
    } else {
        match chord.code {
            LoxiaKeyCode::Up | LoxiaKeyCode::Char('k') if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::MoveRow(-1)))
            }
            LoxiaKeyCode::Down | LoxiaKeyCode::Char('j') if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::MoveRow(1)))
            }
            LoxiaKeyCode::Char('a') if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::ServerEditorAddNew))
            }
            LoxiaKeyCode::Char('x') if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::ServerEditorRemove))
            }
            LoxiaKeyCode::Char('D') if m == LoxiaKeyModifiers::default() => Some(Action::Settings(
                SettingsAction::ServerEditorToggleDeleteData,
            )),
            LoxiaKeyCode::Char('S') if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::ServerEditorSwitch))
            }
            LoxiaKeyCode::Enter => Some(server_editor_enter_action(editor)),
            LoxiaKeyCode::Esc if m == LoxiaKeyModifiers::default() => {
                Some(Action::Settings(SettingsAction::ServerEditorClose))
            }
            _ => None,
        }
    }
}

/// Whether the focused row is one `[←→]` means something on.
fn cycles_with_arrows(editor: &loxia_core::state::settings::ServerEditorState) -> bool {
    use loxia_core::reducer::settings::server_draft_field_at;
    use loxia_core::state::settings::ServerDraftField;
    let Some(draft) = &editor.editing else {
        return false;
    };
    matches!(
        server_draft_field_at(draft, draft.field),
        Some(ServerDraftField::Protocol | ServerDraftField::Endpoint)
    )
}

/// Always returns `Some` — the same "`Enter` must never fall through to the global table" lesson
/// `settings_enter_action` above already learned (`11-01`).
fn server_editor_enter_action(editor: &loxia_core::state::settings::ServerEditorState) -> Action {
    use loxia_core::reducer::settings::server_draft_field_at;
    use loxia_core::state::settings::ServerDraftField;

    let Some(draft) = &editor.editing else {
        return Action::Settings(SettingsAction::ServerEditorEdit);
    };
    match server_draft_field_at(draft, draft.field) {
        Some(
            ServerDraftField::Name
            | ServerDraftField::Host
            | ServerDraftField::Port
            | ServerDraftField::Username
            | ServerDraftField::Password
            | ServerDraftField::NewHeaderName
            | ServerDraftField::NewHeaderValue,
        ) => Action::Settings(SettingsAction::StartTextEdit),
        // `Enter` cycles it forward — the same convenience `[←→]` themselves are, not just a
        // no-op like every other non-text row below.
        Some(ServerDraftField::Protocol) => {
            Action::Settings(SettingsAction::ServerEditorCycleProtocol(1))
        }
        Some(ServerDraftField::AddHeaderAction) => {
            Action::Settings(SettingsAction::ServerEditorAddHeader)
        }
        Some(ServerDraftField::AddEndpointAction) => {
            Action::Settings(SettingsAction::ServerEditorAddEndpoint)
        }
        // `Enter` on the selector cycles it forward, the same convenience `Protocol` gets.
        Some(ServerDraftField::Endpoint) => {
            Action::Settings(SettingsAction::ServerEditorCycleProtocol(1))
        }
        Some(ServerDraftField::TestConnectionAction) => {
            Action::Settings(SettingsAction::ServerEditorTestConnection)
        }
        Some(ServerDraftField::SaveAction) => Action::Settings(SettingsAction::ServerEditorSave),
        // A header row has nothing to "enter" — harmless idempotent no-op, same as `11-01`'s own
        // `MoveRow(0)` convention.
        _ => Action::Settings(SettingsAction::MoveRow(0)),
    }
}

/// `11-04`: the sort-profile editor's own row navigation — `j`/`k`/arrows are shared with the
/// server editor above (`SettingsAction::MoveRow`, which already branches on which sub-editor is
/// open), everything else is this editor's own: `n`/`r`/`x`/`a`/`d` only mean anything while a
/// name buffer isn't being typed (that's `TextInput` context instead, checked ahead of this
/// function entirely — see `context_for`), `Ctrl+Up`/`Ctrl+Down` reorder the focused rule,
/// `←`/`→` cycle its field, `Space`/`Enter` are `sort_editor_enter_action`'s job.
fn sort_editor_row_action(
    editor: &loxia_core::state::settings::SortProfileEditorState,
    chord: KeyChord,
) -> Option<Action> {
    let m = chord.mods;
    let allowed = m == LoxiaKeyModifiers::default()
        || m == (LoxiaKeyModifiers {
            ctrl: true,
            ..Default::default()
        });
    if !allowed {
        return None;
    }

    match chord.code {
        LoxiaKeyCode::Up if m.ctrl => {
            Some(Action::Settings(SettingsAction::SortEditorReorderRule(-1)))
        }
        LoxiaKeyCode::Down if m.ctrl => {
            Some(Action::Settings(SettingsAction::SortEditorReorderRule(1)))
        }
        LoxiaKeyCode::Up | LoxiaKeyCode::Char('k') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::MoveRow(-1)))
        }
        LoxiaKeyCode::Down | LoxiaKeyCode::Char('j') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::MoveRow(1)))
        }
        LoxiaKeyCode::Left if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorCycleField(-1)))
        }
        LoxiaKeyCode::Right if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorCycleField(1)))
        }
        LoxiaKeyCode::Char('n') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorNew))
        }
        LoxiaKeyCode::Char('r') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorRename))
        }
        LoxiaKeyCode::Char('x') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorDeleteProfile))
        }
        // `R` (uppercase — arrives with shift stripped, like the server editor's own `D`/`S`).
        LoxiaKeyCode::Char('R') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorRestoreDefaults))
        }
        LoxiaKeyCode::Char('a') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorAddRule))
        }
        LoxiaKeyCode::Char('d') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorDeleteRule))
        }
        LoxiaKeyCode::Char(' ') if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorToggleDirection))
        }
        LoxiaKeyCode::Enter => Some(sort_editor_enter_action(editor)),
        LoxiaKeyCode::Esc if m == LoxiaKeyModifiers::default() => {
            Some(Action::Settings(SettingsAction::SortEditorClose))
        }
        _ => None,
    }
}

/// Always returns `Some` — the same "`Enter` must never fall through to the global table" lesson
/// every other Settings sub-editor's own `Enter` handler already learned. `Enter` on a rule row
/// toggles its direction (the same key `Space` already does, for anyone reaching for either);
/// on the profile's own header row it applies the profile to the queue instead.
fn sort_editor_enter_action(
    editor: &loxia_core::state::settings::SortProfileEditorState,
) -> Action {
    if editor.rule_cursor.is_some() {
        Action::Settings(SettingsAction::SortEditorToggleDirection)
    } else {
        Action::Settings(SettingsAction::SortEditorApply)
    }
}

/// `11-05`: the EQ-preset editor's own row navigation — `j`/`k`/arrows are shared
/// (`SettingsAction::MoveRow`, which already branches on which sub-editor is open); `s`/`r`/`x`/`e`
/// are this editor's own, and `Enter` is treated the same as `e` (the most natural "open/activate
/// this row" key, matching the other sub-editors' own "Enter must always be claimed" rule,
/// `11-01`).
fn eq_editor_row_action(chord: KeyChord) -> Option<Action> {
    let m = chord.mods;
    if m != LoxiaKeyModifiers::default() {
        return None;
    }
    match chord.code {
        LoxiaKeyCode::Up | LoxiaKeyCode::Char('k') => {
            Some(Action::Settings(SettingsAction::MoveRow(-1)))
        }
        LoxiaKeyCode::Down | LoxiaKeyCode::Char('j') => {
            Some(Action::Settings(SettingsAction::MoveRow(1)))
        }
        LoxiaKeyCode::Char('s') => Some(Action::Settings(SettingsAction::EqEditorSaveCurrent)),
        LoxiaKeyCode::Char('r') => Some(Action::Settings(SettingsAction::EqEditorRename)),
        LoxiaKeyCode::Char('x') => Some(Action::Settings(SettingsAction::EqEditorDelete)),
        LoxiaKeyCode::Char('e') | LoxiaKeyCode::Enter => {
            Some(Action::Settings(SettingsAction::EqEditorOpenEqualizer))
        }
        LoxiaKeyCode::Esc => Some(Action::Settings(SettingsAction::EqEditorClose)),
        _ => None,
    }
}

/// `11-07`: the About section's own hotkeys — `[l]` toggles the third-party-licences pane,
/// `[d]` copies diagnostics; while the pane is open, `j`/`k`/arrows scroll it and `l`/`Esc` close
/// it. Returns `None` for anything else, which lets `settings_row_action` fall through to its own
/// generic body (`Tab`/`Shift+Tab` section-cycling in particular).
fn about_row_action(state: &AppState, chord: KeyChord) -> Option<Action> {
    if chord.mods != LoxiaKeyModifiers::default() {
        return None;
    }
    if state.settings.about_licences_open {
        return match chord.code {
            LoxiaKeyCode::Up | LoxiaKeyCode::Char('k') => {
                Some(Action::Settings(SettingsAction::AboutLicencesScroll(-1)))
            }
            LoxiaKeyCode::Down | LoxiaKeyCode::Char('j') => {
                Some(Action::Settings(SettingsAction::AboutLicencesScroll(1)))
            }
            LoxiaKeyCode::Char('l') | LoxiaKeyCode::Esc => {
                Some(Action::Settings(SettingsAction::AboutToggleLicences))
            }
            _ => None,
        };
    }
    match chord.code {
        LoxiaKeyCode::Char('l') => Some(Action::Settings(SettingsAction::AboutToggleLicences)),
        LoxiaKeyCode::Char('d') => Some(Action::Settings(SettingsAction::AboutCopyDiagnostics)),
        _ => None,
    }
}

/// `11-01`: the Settings tab's own text-edit sub-state (`state.settings.editing.is_some()`) —
/// `Enter` commits, `Esc` cancels (restoring the previous value, not the generic `Nav::Cancel`
/// ladder `modal`/`filter` text input both reuse for the same key).
fn settings_text_input_action(chord: KeyChord) -> Option<Action> {
    match chord.code {
        LoxiaKeyCode::Enter => Some(Action::Settings(SettingsAction::CommitTextEdit)),
        LoxiaKeyCode::Esc => Some(Action::Settings(SettingsAction::CancelTextEdit)),
        LoxiaKeyCode::Backspace => Some(Action::Settings(SettingsAction::TextBackspace)),
        // Found missing in the field: without these, a typo could only ever be fixed by erasing
        // everything after it — there was no way to move the insertion point at all.
        LoxiaKeyCode::Delete => Some(Action::Settings(SettingsAction::TextDeleteForward)),
        LoxiaKeyCode::Left => Some(Action::Settings(SettingsAction::TextCursorLeft)),
        LoxiaKeyCode::Right => Some(Action::Settings(SettingsAction::TextCursorRight)),
        LoxiaKeyCode::Home => Some(Action::Settings(SettingsAction::TextCursorHome)),
        LoxiaKeyCode::End => Some(Action::Settings(SettingsAction::TextCursorEnd)),
        LoxiaKeyCode::Char(c) => Some(Action::Settings(SettingsAction::TextInput(c))),
        _ => None,
    }
}

/// `11-02`: always returns `Some`-equivalent (`Action`, never `Option`), the same lesson `11-01`'s
/// own `settings_enter_action` already learned — a bare `None` here would let `Enter` leak through
/// to `resolve`'s universal modal table (`Cancel`/`ToggleHelp`/`Quit` only), never reaching
/// `Submit` at all. `captured: None` (nothing captured yet, whether or not a previous attempt's
/// conflict is showing — that can't happen with `captured: None` since a conflict is only ever
/// set alongside a `captured` binding) starts a fresh capture; anything else (a finished capture,
/// or one awaiting a "rebind anyway" confirmation) submits it.
fn keymap_editor_enter_action(state: &AppState) -> Action {
    match &state.modal {
        Some(Modal::KeymapEditor { captured: None, .. }) => {
            Action::Modal(loxia_core::action::ModalAction::StartCapture)
        }
        _ => Action::Modal(loxia_core::action::ModalAction::Submit),
    }
}

/// `10-04`: gated entirely on `ui.enable_mouse` — when it's off, crossterm mouse capture is never
/// even enabled (`main.rs`'s own `TerminalGuard::enter`), so real mouse events shouldn't reach
/// here at all in practice; this check is what makes `mouse_ignored_when_disabled`'s own
/// synthetic-event test meaningful regardless.
fn mouse_action(
    state: &AppState,
    hits: &HitMap,
    mouse: &mut MouseState,
    ev: CtMouseEvent,
    viewport: Viewport,
    now: Instant,
) -> Option<Action> {
    if !state.config.ui.enable_mouse {
        return None;
    }
    match ev.kind {
        CtMouseEventKind::Down(CtMouseButton::Left) => {
            mouse_down(state, hits, mouse, ev.column, ev.row, viewport, now)
        }
        CtMouseEventKind::Drag(CtMouseButton::Left) => mouse_drag(mouse, ev.column, ev.row),
        CtMouseEventKind::Up(CtMouseButton::Left) => {
            // Releasing anywhere — including outside the widget the drag was controlling — ends
            // it; a drag stuck "on" past release would otherwise keep reinterpreting stray
            // `Moved` events (which never reach here at all, only `Drag` does, but the *next*
            // unrelated `Down` must not be misread as a continuation either).
            mouse.drag = None;
            None
        }
        CtMouseEventKind::ScrollUp => scroll_action(hits, ev.column, ev.row, -3),
        CtMouseEventKind::ScrollDown => scroll_action(hits, ev.column, ev.row, 3),
        _ => None,
    }
}

/// A modal open at all captures the mouse: any click that doesn't land on that modal's own
/// `ModalField`/`EqBand` closes it — "clicks inside route to `ModalField` and `EqBand` only"
/// (this task's own spec). Checked *before* the target-specific dispatch below, so e.g. a
/// `ColumnItem` left over from the frame drawn just before a modal opened can never leak a
/// navigation click through underneath it.
fn mouse_down(
    state: &AppState,
    hits: &HitMap,
    mouse: &mut MouseState,
    col: u16,
    row: u16,
    viewport: Viewport,
    now: Instant,
) -> Option<Action> {
    let (rect, target) = hits.hit_rect(col, row)?;

    let is_double = matches!(
        mouse.last_click,
        Some((last_target, at))
            if last_target == target && now.saturating_duration_since(at) <= DOUBLE_CLICK_WINDOW
    );
    mouse.last_click = Some((target, now));
    mouse.drag = Some((rect, target));

    if state.modal.is_some() && !matches!(target, HitTarget::ModalField(_) | HitTarget::EqBand(_)) {
        return Some(Action::Modal(loxia_core::action::ModalAction::Close));
    }

    click_action(state, rect, target, col, row, is_double, viewport)
}

#[allow(clippy::too_many_arguments)]
fn click_action(
    state: &AppState,
    rect: Rect,
    target: HitTarget,
    col: u16,
    row: u16,
    is_double: bool,
    viewport: Viewport,
) -> Option<Action> {
    match target {
        HitTarget::SidebarTab(tab) => Some(Action::Nav(NavAction::SetTab(tab))),
        HitTarget::ColumnItem { column, index } => Some(if is_double {
            Action::Nav(NavAction::NavRight)
        } else {
            Action::Nav(NavAction::FocusColumnAt { column, index })
        }),
        HitTarget::SeekBar => Some(Action::Player(PlayerAction::Seek(SeekTarget::Fraction(
            x_fraction(rect, col),
        )))),
        HitTarget::VolumeBar => Some(Action::Player(PlayerAction::SetVolume(volume_from_x(
            rect, col,
        )))),
        HitTarget::Transport(button) => Some(transport_action(state, button)),
        HitTarget::QuitButton => Some(Action::System(SystemEvent::Quit)),
        HitTarget::HelpButton => Some(Action::View(ViewAction::ToggleHelp)),
        HitTarget::QueueEntry(id) => Some(if is_double {
            Action::Queue(QueueAction::JumpTo(id))
        } else {
            Action::Nav(NavAction::FocusQueueEntry(id))
        }),
        HitTarget::InspectorAction(action_id) => action_for(action_id, state, viewport),
        // `10-06`: "click sets the gain from the y position and drag adjusts continuously" (this
        // task's own spec) — `Down` sets an initial gain from wherever it was clicked, exactly
        // like a `Drag` would; see `mouse_drag`'s identical handling for the continued-drag case.
        // Routes through `ModalAction::SetGainAt` (the modal's own draft/live-preview state), not
        // `PlayerAction::SetEqGain` (which would commit directly to the already-active curve,
        // bypassing `Esc`'s own "restore `gains_at_open`" undo).
        HitTarget::EqBand(band) => {
            Some(Action::Modal(loxia_core::action::ModalAction::SetGainAt {
                band,
                db: gain_from_y(rect, row),
            }))
        }
        // `10-05`: a single click sets the modal's own field cursor to this row (mirroring
        // `ColumnItem`'s `FocusColumnAt`); a double-click follows up with `Submit`, relying on the
        // first click of the pair having already moved the cursor there — the same
        // already-in-place-by-the-second-click pattern `ColumnItem`'s own double-click uses.
        HitTarget::ModalField(index) => Some(if is_double {
            Action::Modal(loxia_core::action::ModalAction::Submit)
        } else {
            Action::Modal(loxia_core::action::ModalAction::FieldSet(index))
        }),
        // A click puts the cursor on the row; a double-click plays it, the same
        // single-then-double pairing `ColumnItem` and `QueueEntry` use — the first click of the
        // pair has already moved the cursor, so `PlaySelection` acts on the right row.
        HitTarget::SectionedListItem { section, index } => Some(if is_double {
            Action::Queue(QueueAction::PlaySelection {
                full_context: false,
            })
        } else {
            Action::Nav(NavAction::FocusSectionAt { section, index })
        }),
        // `SearchQuery` has no click behaviour named anywhere in this task's own spec.
        // `NowPlayingList` is a scroll-only backdrop: a click on it always lands on the
        // `QueueEntry` row registered over it, and clicking the pane's blank tail means nothing.
        HitTarget::SearchQuery | HitTarget::NowPlayingList => None,
        // `11-01`: a click on the section list switches sections directly; a click on a row sets
        // the cursor to it (mirroring `ModalField`'s own click-sets-cursor shape) — neither this
        // task's own spec names a double-click behaviour for either, so `is_double` is unused here.
        HitTarget::SettingsSection(section) => Some(Action::Settings(
            loxia_core::action::SettingsAction::SetSection(section),
        )),
        HitTarget::SettingsRow(index) => Some(Action::Settings(
            loxia_core::action::SettingsAction::SetRow(index),
        )),
    }
}

/// Continues whatever `mouse.drag` captured on `Down`, using its own captured `Rect` — never
/// re-resolving the target from the *current* pointer position, which may now be outside every
/// registered region entirely.
fn mouse_drag(mouse: &MouseState, col: u16, row: u16) -> Option<Action> {
    let (rect, target) = mouse.drag?;
    match target {
        HitTarget::SeekBar => Some(Action::Player(PlayerAction::Seek(SeekTarget::Fraction(
            x_fraction(rect, col),
        )))),
        HitTarget::VolumeBar => Some(Action::Player(PlayerAction::SetVolume(volume_from_x(
            rect, col,
        )))),
        HitTarget::EqBand(band) => {
            Some(Action::Modal(loxia_core::action::ModalAction::SetGainAt {
                band,
                db: gain_from_y(rect, row),
            }))
        }
        _ => None,
    }
}

/// "Scroll targets the column under the pointer, not the focused one" — resolved fresh against
/// the current pointer position every time (unlike a drag, a scroll never "continues" a capture
/// across events). Only the scrollable lists have a defined meaning here: the Miller columns, and
/// the Now Playing queue/history pane, which is not a column and so was unreachable by the wheel
/// until it got a target of its own (`docs/12-decisions.md`). Every other target is silently
/// ignored rather than guessed at.
fn scroll_action(hits: &HitMap, col: u16, row: u16, delta: i32) -> Option<Action> {
    match hits.hit(col, row)? {
        &HitTarget::ColumnItem { column, .. } => {
            Some(Action::Nav(NavAction::ScrollColumn { column, delta }))
        }
        HitTarget::QueueEntry(_) | HitTarget::NowPlayingList => {
            Some(Action::Nav(NavAction::ScrollNowPlaying { delta }))
        }
        _ => None,
    }
}

fn transport_action(state: &AppState, button: TransportButton) -> Action {
    match button {
        TransportButton::Prev => Action::Player(PlayerAction::Prev),
        TransportButton::PlayPause => Action::Player(PlayerAction::PlayPause),
        TransportButton::Stop => Action::Player(PlayerAction::Stop),
        TransportButton::Next => Action::Player(PlayerAction::Next),
        // Mirrors `action_for`'s own keyboard-equivalent mapping for `ToggleShuffle`/`CycleRepeat`
        // exactly (`state.clock`, never a live clock read — `docs/12-decisions.md`, `06-03`).
        TransportButton::Shuffle => Action::Queue(QueueAction::ToggleShuffle {
            seed: state.clock.as_millisecond() as u64,
        }),
        TransportButton::Repeat => Action::Queue(QueueAction::CycleRepeat),
    }
}

/// `0.0` at `rect`'s own left edge, `1.0` at its right edge — clamped, so a drag past either side
/// still reads as fully at that end rather than panicking or extrapolating past it.
fn x_fraction(rect: Rect, col: u16) -> f32 {
    if rect.width <= 1 {
        return 0.0;
    }
    let rel = col.saturating_sub(rect.x).min(rect.width - 1);
    f32::from(rel) / f32::from(rect.width - 1)
}

fn volume_from_x(rect: Rect, col: u16) -> u8 {
    (x_fraction(rect, col) * 100.0).round() as u8
}

/// `EqBand`'s own vertical axis, matching `10-06`'s own mockup (bars growing up/down from a
/// centre 0 dB line): the top row reads `+12 dB`, the vertical centre `0 dB`, the bottom
/// `-12 dB`.
fn gain_from_y(rect: Rect, row: u16) -> f32 {
    if rect.height <= 1 {
        return 0.0;
    }
    let rel = f32::from(row.saturating_sub(rect.y).min(rect.height - 1));
    let frac = rel / f32::from(rect.height - 1); // 0.0 at the top, 1.0 at the bottom
    (1.0 - 2.0 * frac) * 12.0
}

/// A focused text field (inline column filter, or a modal's own text field) takes priority over
/// treating the context as a bare `Modal(kind)` — otherwise a printable character typed into,
/// say, the `SavePlaylist` name field would be swallowed by `resolve`'s minimal per-modal table
/// (`03-05`) instead of reaching `FieldInput`.
fn context_for(state: &AppState) -> InputContext {
    if let Some(modal) = &state.modal {
        if modal_focus_is_text(modal) {
            return InputContext::TextInput;
        }
        return InputContext::Modal(modal.kind());
    }
    if state.active_column().is_some_and(|c| c.filter_editing) {
        return InputContext::TextInput;
    }
    // `07-01`: the Search tab's query line is focused on tab entry (no explicit "start editing"
    // keystroke like the inline column filter's `/` needs) — reuses the same `TextInput` context
    // and, via `filter_text_input_action` below, the same `NavAction::FilterInput`/
    // `FilterBackspace`/`CommitFilter` the reducer already branches on for `Tab::Search`.
    if state.nav.active_tab == Tab::Search && state.search.query_focused {
        return InputContext::TextInput;
    }
    // `11-01`/`11-03`/`11-04`/`11-05`: the Settings tab's own text-edit sub-state, whether the
    // flat `SettingsState.editing` (an ordinary `Control::Text` row) or one of the three
    // sub-editors' own nested buffers — see `settings_text_edit_active`, also used by
    // `text_input_action` below so the two can never again disagree about when this context
    // applies (`docs/12-decisions.md`: they silently did, for a full release).
    if state.nav.active_tab == Tab::Settings && settings_text_edit_active(state) {
        return InputContext::TextInput;
    }
    InputContext::Normal
}

/// Whether *some* Settings text buffer is actively being typed into right now — the flat
/// `SettingsState.editing` (`11-01`), or the server-profile editor's own draft field
/// (`ServerEditorState.editing.text_buf`, `11-03`), or the sort-profile/EQ-preset editors' own
/// name buffers (`11-04`/`11-05`). `context_for` and `text_input_action` must agree on exactly
/// this, or a keystroke enters `InputContext::TextInput` (correctly) only to be handed to the
/// *wrong* handler (`filter_text_input_action`, a silent no-op in the Settings tab) once there —
/// exactly the bug found in the field: `text_input_action` used to check only the first of these
/// four, so typing into any add/edit *form* (not the plain row-level text fields `11-01` alone
/// covers) did nothing, and `Esc` (misrouted to the equally-inert `NavAction::Cancel`) couldn't
/// even back out of it — only `Ctrl+C`, checked unconditionally above `context_for` entirely,
/// still worked, which is exactly what read as "the app hangs" (`docs/12-decisions.md`).
fn settings_text_edit_active(state: &AppState) -> bool {
    state.settings.editing.is_some()
        || state
            .settings
            .server_editor
            .as_ref()
            .and_then(|e| e.editing.as_ref())
            .is_some_and(|d| d.text_buf.is_some())
        || state
            .settings
            .sort_profile_editor
            .as_ref()
            .is_some_and(|e| e.name_buf.is_some())
        || state
            .settings
            .eq_preset_editor
            .as_ref()
            .is_some_and(|e| e.name_buf.is_some())
}

fn modal_focus_is_text(modal: &Modal) -> bool {
    match modal {
        // `10-08`: field `0` is the target dropdown (`CycleSaveTarget`, not text) — fields `1`/`2`
        // are the `name`/`overview` text fields, `3` the `autosort` checkbox.
        Modal::SavePlaylist { field, .. } => *field == 1 || *field == 2,
        _ => false,
    }
}

/// `TextInput` covers three unrelated dispatch tables — a modal's own text field, the shared
/// `NavAction::FilterInput`/etc. table used by both the inline column filter (`04-11`) and the
/// Search tab's query line (`07-01`, same actions, branched on `active_tab` inside the reducer),
/// and (`11-01`) the Settings tab's own text-edit sub-state — `context_for` conflates all of
/// these into one `InputContext` variant (deliberately: `keymap::resolve`'s `Normal` table must
/// never see any of them), but each dispatches differently, so this re-checks which one is
/// actually focused before choosing a table.
fn text_input_action(state: &AppState, chord: KeyChord) -> Option<Action> {
    if state.nav.active_tab == Tab::Settings && settings_text_edit_active(state) {
        settings_text_input_action(chord)
    } else if state.modal.as_ref().is_some_and(modal_focus_is_text) {
        modal_text_input_action(chord)
    } else {
        // On the Search tab, `↓`/`↑` step between the query line and the results even though the
        // query line is technically a text field — `move_search_cursor` treats query-line and
        // results as one flat column, so a lone artist result is reachable with the arrows a user
        // naturally reaches for, not only `Tab` (`docs/12-decisions.md`). The inline column filter
        // deliberately does *not* claim the arrows (its column keeps its own cursor for when
        // filtering ends), so this is scoped to the Search tab only.
        if state.nav.active_tab == Tab::Search && chord.mods == LoxiaKeyModifiers::default() {
            match chord.code {
                LoxiaKeyCode::Down => return Some(Action::Nav(NavAction::MoveDown { n: 1 })),
                LoxiaKeyCode::Up => return Some(Action::Nav(NavAction::MoveUp { n: 1 })),
                _ => {}
            }
        }
        filter_text_input_action(chord)
    }
}

/// A modal's own text field (`docs/04-state-and-input.md` §5: "Only `Esc`, `Enter`, `Tab`,
/// `Shift+Tab`, and the arrows are bindable") — handled directly rather than through
/// `keymap::resolve`'s shared `Normal` binding table, which would otherwise route `Tab` to
/// `NextTab` (sidebar switching) instead of `FieldNext`. `Esc` and the arrows fall through to
/// `resolve` since `Esc`'s universal `Cancel` binding is already correct here, and no
/// cursor-within-text model exists for the arrows to usefully diverge from `resolve`'s answer.
fn modal_text_input_action(chord: KeyChord) -> Option<Action> {
    match chord.code {
        LoxiaKeyCode::Enter => Some(Action::Modal(loxia_core::action::ModalAction::Submit)),
        LoxiaKeyCode::Tab if chord.mods.shift => {
            Some(Action::Modal(loxia_core::action::ModalAction::FieldPrev))
        }
        LoxiaKeyCode::Tab => Some(Action::Modal(loxia_core::action::ModalAction::FieldNext)),
        LoxiaKeyCode::Backspace => Some(Action::Modal(
            loxia_core::action::ModalAction::FieldBackspace,
        )),
        LoxiaKeyCode::Char(c) => Some(Action::Modal(loxia_core::action::ModalAction::FieldInput(
            c,
        ))),
        LoxiaKeyCode::Esc => Some(Action::Nav(NavAction::Cancel)),
        _ => None,
    }
}

/// The inline column filter (`04-11`, `docs/07-ui-spec.md` §5): `Enter` commits the filter text
/// and leaves text-input mode without clearing it; `Esc` clears it entirely (`Cancel`'s ladder).
fn filter_text_input_action(chord: KeyChord) -> Option<Action> {
    // `Ctrl`/`Alt` make it a command, not a character: `Alt+1` is "jump to tab 1", and inserting a
    // literal `1` into the query is never what was meant. `Shift` is excluded from this test — it
    // is how capital letters are typed.
    if chord.mods.ctrl || chord.mods.alt {
        return None;
    }
    match chord.code {
        LoxiaKeyCode::Enter => Some(Action::Nav(NavAction::CommitFilter)),
        LoxiaKeyCode::Backspace => Some(Action::Nav(NavAction::FilterBackspace)),
        LoxiaKeyCode::Char(c) => Some(Action::Nav(NavAction::FilterInput(c))),
        LoxiaKeyCode::Esc => Some(Action::Nav(NavAction::Cancel)),
        // A real bug found in the field: without this, `Tab` on the Search tab (whose query line
        // is focused on every tab entry, `docs/07-ui-spec.md` §9) fell all the way through to
        // nothing — `to_action`'s own `InputContext::TextInput` arm has no `normal_resolve`
        // fallback the way the Settings tab's own dispatch does, so an unhandled key here is
        // simply swallowed, not passed on. A single-line query has no use for a literal tab
        // character, so `Tab`/`Shift+Tab` switch tabs here exactly as they do everywhere else,
        // the same carve-out `modal_text_input_action` already makes for its own text field
        // (`docs/12-decisions.md`).
        LoxiaKeyCode::Tab if chord.mods.shift => Some(Action::Nav(NavAction::PrevTab)),
        LoxiaKeyCode::Tab => Some(Action::Nav(NavAction::NextTab)),
        _ => None,
    }
}

fn to_chord(code: CtKeyCode, mods: CtKeyModifiers) -> Option<KeyChord> {
    let code = match code {
        CtKeyCode::Char(c) => LoxiaKeyCode::Char(c),
        CtKeyCode::Enter => LoxiaKeyCode::Enter,
        CtKeyCode::Esc => LoxiaKeyCode::Esc,
        CtKeyCode::Tab => LoxiaKeyCode::Tab,
        // Terminals send Shift+Tab as its own escape sequence, which crossterm reports as
        // `BackTab` with **no** SHIFT modifier — never `Tab` with one. Without this arm it fell to
        // the `_ => return None` below and was dropped before reaching any binding, so `Shift+Tab`
        // did nothing anywhere in the app while every `Tab if mods.shift` arm sat unreachable
        // (`docs/12-decisions.md`).
        CtKeyCode::BackTab => {
            return Some(KeyChord {
                code: LoxiaKeyCode::Tab,
                mods: LoxiaKeyModifiers {
                    shift: true,
                    ctrl: mods.contains(CtKeyModifiers::CONTROL),
                    alt: mods.contains(CtKeyModifiers::ALT),
                },
            });
        }
        CtKeyCode::Backspace => LoxiaKeyCode::Backspace,
        CtKeyCode::Delete => LoxiaKeyCode::Delete,
        CtKeyCode::Left => LoxiaKeyCode::Left,
        CtKeyCode::Right => LoxiaKeyCode::Right,
        CtKeyCode::Up => LoxiaKeyCode::Up,
        CtKeyCode::Down => LoxiaKeyCode::Down,
        CtKeyCode::Home => LoxiaKeyCode::Home,
        CtKeyCode::End => LoxiaKeyCode::End,
        CtKeyCode::PageUp => LoxiaKeyCode::PageUp,
        CtKeyCode::PageDown => LoxiaKeyCode::PageDown,
        CtKeyCode::Insert => LoxiaKeyCode::Insert,
        CtKeyCode::F(n) => LoxiaKeyCode::F(n),
        _ => return None,
    };

    let mut shift = mods.contains(CtKeyModifiers::SHIFT);
    let ctrl = mods.contains(CtKeyModifiers::CONTROL);
    let alt = mods.contains(CtKeyModifiers::ALT);
    // `A` and `shift+a` must produce the same chord — matches `keymap::parse`'s own normalisation
    // (`03-04`).
    if let LoxiaKeyCode::Char(c) = code
        && shift
        && c.is_ascii_uppercase()
    {
        shift = false;
    }

    Some(KeyChord {
        code,
        mods: LoxiaKeyModifiers { ctrl, alt, shift },
    })
}

/// Exhaustive over `ActionId` — every default keybinding resolves to a concrete `Action`.
///
/// `10-03`: `MoveDown`/`MoveUp`/`HalfPageDown`/`HalfPageUp` are re-routed to
/// `ModalAction::Scroll` whenever `Modal::Help` is open — `keymap::resolve`'s own `help_scroll`
/// table is the only way these four `ActionId`s can even be resolved while a modal is open at all
/// (every other modal context only ever resolves `Cancel`/`ToggleHelp`/`Quit`), so this check is
/// never reached for any other modal kind.
fn action_for(id: ActionId, state: &AppState, viewport: Viewport) -> Option<Action> {
    if matches!(state.modal, Some(Modal::Help { .. })) {
        let delta = match id {
            ActionId::MoveDown => Some(1),
            ActionId::MoveUp => Some(-1),
            ActionId::HalfPageDown => Some(viewport.rows as i32 / 2),
            ActionId::HalfPageUp => Some(-(viewport.rows as i32 / 2)),
            _ => None,
        };
        if let Some(delta) = delta {
            return Some(Action::Modal(loxia_core::action::ModalAction::Scroll(
                delta,
            )));
        }
    }

    // `10-05`/`10-07`/`10-09`/`11-02`: `keymap::resolve`'s own `list_nav` table is the only way
    // `MoveDown`/`MoveUp` can resolve at all while `Modal::DevicePicker`/`Modal::SleepTimer`/
    // `Modal::SortProfile`/`Modal::KeymapEditor` is open — re-routed here to `ModalAction::
    // FieldNext`/`FieldPrev`, the generic list-cursor mechanism (`reducer::modal::field_next`/
    // `field_prev`, pre-built for every modal kind since `03-07`) rather than the
    // column-navigation `Action::Nav` these two `ActionId`s produce everywhere else.
    if matches!(
        state.modal,
        Some(
            Modal::DevicePicker { .. }
                | Modal::SleepTimer { .. }
                | Modal::SortProfile { .. }
                | Modal::KeymapEditor { .. }
        )
    ) {
        match id {
            ActionId::MoveDown => {
                return Some(Action::Modal(loxia_core::action::ModalAction::FieldNext));
            }
            ActionId::MoveUp => {
                return Some(Action::Modal(loxia_core::action::ModalAction::FieldPrev));
            }
            _ => {}
        }
    }

    // `10-06`: `keymap::resolve`'s own `equalizer_nav` table is the only way `NavLeft`/`NavRight`/
    // `MoveUp`/`MoveDown` resolve at all while `Modal::Equalizer` is open — `←`/`→` reroute to the
    // same generic `FieldPrev`/`FieldNext` band-cursor mechanism `DevicePicker` uses above, and
    // `↑`/`↓` reroute to `AdjustGain`, the modal's own live-preview gain nudge (`±0.5dB` is
    // `10-06`'s own spec value, not derived from anywhere else).
    if matches!(state.modal, Some(Modal::Equalizer { .. })) {
        match id {
            ActionId::NavLeft => {
                return Some(Action::Modal(loxia_core::action::ModalAction::FieldPrev));
            }
            ActionId::NavRight => {
                return Some(Action::Modal(loxia_core::action::ModalAction::FieldNext));
            }
            ActionId::MoveUp => {
                return Some(Action::Modal(loxia_core::action::ModalAction::AdjustGain(
                    0.5,
                )));
            }
            ActionId::MoveDown => {
                return Some(Action::Modal(loxia_core::action::ModalAction::AdjustGain(
                    -0.5,
                )));
            }
            _ => {}
        }
    }

    // `10-08`: `keymap::resolve`'s own `list_nav` table is the only way `MoveDown`/`MoveUp`
    // resolve at all while `Modal::SavePlaylist` is open — re-routed to `CycleSaveTarget` only
    // while the target dropdown itself has focus (`field == 0`); on any other field (the two text
    // fields, the sort checkbox), the reducer's own `cycle_save_target` already no-ops, but
    // resolving to `None` here instead keeps a stray `MoveUp`/`MoveDown` from touching `state.dirty`
    // for literally no reason.
    if let Some(Modal::SavePlaylist { field, .. }) = &state.modal {
        return match (*field, id) {
            (0, ActionId::MoveDown) => Some(Action::Modal(
                loxia_core::action::ModalAction::CycleSaveTarget(1),
            )),
            (0, ActionId::MoveUp) => Some(Action::Modal(
                loxia_core::action::ModalAction::CycleSaveTarget(-1),
            )),
            (_, ActionId::MoveDown | ActionId::MoveUp) => None,
            _ => action_for_normal(id, state, viewport),
        };
    }

    action_for_normal(id, state, viewport)
}

/// The exhaustive `ActionId` -> `Action` table every modal-specific reroute above eventually falls
/// through to.
fn action_for_normal(id: ActionId, state: &AppState, viewport: Viewport) -> Option<Action> {
    match id {
        ActionId::MoveDown => Some(Action::Nav(NavAction::MoveDown { n: 1 })),
        ActionId::MoveUp => Some(Action::Nav(NavAction::MoveUp { n: 1 })),
        ActionId::NavLeft => Some(Action::Nav(NavAction::NavLeft)),
        ActionId::NavRight => Some(Action::Nav(NavAction::NavRight)),
        ActionId::HalfPageUp => Some(Action::Nav(NavAction::HalfPageUp {
            n: viewport.rows / 2,
        })),
        ActionId::HalfPageDown => Some(Action::Nav(NavAction::HalfPageDown {
            n: viewport.rows / 2,
        })),
        ActionId::GoToTop => Some(Action::Nav(NavAction::GoToTop)),
        ActionId::GoToBottom => Some(Action::Nav(NavAction::GoToBottom)),
        ActionId::PopColumn => Some(Action::Nav(NavAction::PopColumn)),
        ActionId::NextTab => Some(Action::Nav(NavAction::NextTab)),
        ActionId::PrevTab => Some(Action::Nav(NavAction::PrevTab)),
        ActionId::JumpTab1 => Some(Action::Nav(NavAction::SetTab(Tab::NowPlaying))),
        ActionId::JumpTab2 => Some(Action::Nav(NavAction::SetTab(Tab::Favourites))),
        ActionId::JumpTab3 => Some(Action::Nav(NavAction::SetTab(Tab::Search))),
        ActionId::JumpTab4 => Some(Action::Nav(NavAction::SetTab(Tab::Playlists))),
        ActionId::JumpTab5 => Some(Action::Nav(NavAction::SetTab(Tab::Artists))),
        ActionId::JumpTab6 => Some(Action::Nav(NavAction::SetTab(Tab::AlbumArtists))),
        ActionId::JumpTab7 => Some(Action::Nav(NavAction::SetTab(Tab::Albums))),
        ActionId::JumpTab8 => Some(Action::Nav(NavAction::SetTab(Tab::Genres))),
        ActionId::JumpTab9 => Some(Action::Nav(NavAction::SetTab(Tab::Folders))),
        ActionId::JumpTab10 => Some(Action::Nav(NavAction::SetTab(Tab::Settings))),
        ActionId::OpenFilter => Some(Action::Nav(NavAction::OpenFilter)),
        ActionId::GoToArtist => Some(Action::Nav(NavAction::GoToArtist)),
        ActionId::GoToAlbum => Some(Action::Nav(NavAction::GoToAlbum)),
        ActionId::Cancel => Some(Action::Nav(NavAction::Cancel)),

        ActionId::PlayPause => Some(Action::Player(PlayerAction::PlayPause)),
        ActionId::NextTrack => Some(Action::Player(PlayerAction::Next)),
        ActionId::PrevTrack => Some(Action::Player(PlayerAction::Prev)),
        ActionId::Stop => Some(Action::Player(PlayerAction::Stop)),
        ActionId::SeekBack5 => Some(Action::Player(PlayerAction::Seek(SeekTarget::Relative(
            -5_000,
        )))),
        ActionId::SeekForward5 => Some(Action::Player(PlayerAction::Seek(SeekTarget::Relative(
            5_000,
        )))),
        ActionId::SeekBack30 => Some(Action::Player(PlayerAction::Seek(SeekTarget::Relative(
            -30_000,
        )))),
        ActionId::SeekForward30 => Some(Action::Player(PlayerAction::Seek(SeekTarget::Relative(
            30_000,
        )))),
        ActionId::VolumeUp => Some(Action::Player(PlayerAction::VolumeDelta(5))),
        ActionId::VolumeDown => Some(Action::Player(PlayerAction::VolumeDelta(-5))),
        ActionId::ToggleMute => Some(Action::Player(PlayerAction::ToggleMute)),

        ActionId::QueueArtistOnly => enter_action(state),
        ActionId::QueueFullContext => Some(Action::Queue(QueueAction::QueueSelection {
            full_context: true,
        })),
        ActionId::InsertNext => Some(Action::Queue(QueueAction::InsertNext)),
        ActionId::InstantMix => Some(Action::Queue(QueueAction::InstantMix)),
        // `state.clock` (the last `Tick`'s timestamp), never a raw clock read — the reducer
        // itself must stay deterministic (`docs/12-decisions.md`, `06-03`).
        ActionId::ToggleShuffle => Some(Action::Queue(QueueAction::ToggleShuffle {
            seed: state.clock.as_millisecond() as u64,
        })),
        ActionId::CycleRepeat => Some(Action::Queue(QueueAction::CycleRepeat)),
        ActionId::OpenSortMenu => Some(Action::Modal(loxia_core::action::ModalAction::Open(
            ModalKind::SortProfile,
        ))),
        ActionId::RemoveEntry => remove_entry_action(state),
        ActionId::ToggleVisualSelect => Some(Action::Select(SelectAction::ToggleVisualMode)),
        ActionId::ToggleItem => Some(Action::Select(SelectAction::ToggleItem)),
        ActionId::SelectAll => Some(Action::Select(SelectAction::SelectAll)),

        ActionId::ToggleEqualizer => Some(Action::Modal(loxia_core::action::ModalAction::Open(
            ModalKind::Equalizer,
        ))),
        ActionId::CycleReplayGain => Some(Action::Player(PlayerAction::CycleReplayGain)),
        ActionId::CycleQualityProfile => Some(Action::Player(PlayerAction::CycleQuality)),
        ActionId::OpenDevicePicker => Some(Action::Modal(loxia_core::action::ModalAction::Open(
            ModalKind::DevicePicker,
        ))),
        ActionId::OpenSleepTimer => Some(Action::Modal(loxia_core::action::ModalAction::Open(
            ModalKind::SleepTimer,
        ))),

        ActionId::ToggleFavorite => Some(Action::Item(ItemAction::ToggleFavorite)),
        ActionId::ToggleDownload => Some(Action::Item(ItemAction::ToggleDownload)),
        // `10-08`: `P` always means the whole queue, `Ctrl+P` the current selection (or the
        // focused item, `reducer::modal::save_playlist_tracks`'s own fallback) — closing the gap
        // this comment used to flag: the generic `Open(ModalKind)` this used to go through has no
        // way to also carry that distinction, so both keys silently behaved identically before.
        ActionId::SaveQueueAsPlaylist => Some(Action::Modal(
            loxia_core::action::ModalAction::OpenSavePlaylist(
                loxia_core::state::modal::SaveSource::Queue,
            ),
        )),
        ActionId::AddToPlaylist => Some(Action::Modal(
            loxia_core::action::ModalAction::OpenSavePlaylist(
                loxia_core::state::modal::SaveSource::Selection,
            ),
        )),
        ActionId::DeletePlaylist => delete_playlist_action(state),
        ActionId::MoveTrackUp => move_track_action(state, -1),
        ActionId::MoveTrackDown => move_track_action(state, 1),
        ActionId::ToggleZenMode => Some(Action::View(ViewAction::ToggleZen)),
        ActionId::ToggleHistory => Some(Action::View(ViewAction::ToggleHistory)),
        ActionId::ToggleLyrics => Some(Action::View(ViewAction::ToggleLyrics)),
        ActionId::LyricsScrollUp => Some(Action::View(ViewAction::ScrollLyrics(-1))),
        ActionId::LyricsScrollDown => Some(Action::View(ViewAction::ScrollLyrics(1))),
        ActionId::ToggleHelp => Some(Action::View(ViewAction::ToggleHelp)),

        ActionId::Quit => Some(Action::System(SystemEvent::Quit)),
        ActionId::Refresh => Some(Action::System(SystemEvent::Refresh)),
    }
}

/// `DeletePlaylist` needs to know *which* playlist — the one currently selected — which the
/// keymap cannot supply. `None` (silently no-op) when the cursor isn't on a playlist row.
fn delete_playlist_action(state: &AppState) -> Option<Action> {
    match state.selected_item()? {
        MediaItem::Playlist(p) => Some(Action::Item(ItemAction::DeletePlaylist(
            loxia_core::model::PlaylistId::from(p.id.as_str()),
        ))),
        _ => None,
    }
}

/// `x` is context-dependent (`07-03`): on a `PlaylistTracks` column it removes the focused (or
/// every selected) track from that playlist instead of removing a queue entry — the two contexts
/// share one key because they're never both meaningful at once (`docs/04-state-and-input.md`).
fn remove_entry_action(state: &AppState) -> Option<Action> {
    if let Some(action) = remove_from_playlist_action(state) {
        return Some(action);
    }
    Some(Action::Queue(QueueAction::RemoveEntry))
}

fn remove_from_playlist_action(state: &AppState) -> Option<Action> {
    let column = state.active_column()?;
    let loxia_core::state::nav::ColumnKind::PlaylistTracks { of_playlist } = &column.kind else {
        return None;
    };
    let playlist = loxia_core::model::PlaylistId::from(of_playlist.as_str());

    let entries: Vec<loxia_core::model::PlaylistEntryId> =
        if column.selection.visual_mode && !column.selection.selected.is_empty() {
            column
                .items
                .iter()
                .filter_map(|item| match item {
                    MediaItem::Track(t) if column.selection.selected.contains(&t.id) => {
                        t.playlist_entry_id.clone()
                    }
                    _ => None,
                })
                .collect()
        } else {
            match column.items.get(column.cursor) {
                Some(MediaItem::Track(t)) => t.playlist_entry_id.iter().cloned().collect(),
                _ => Vec::new(),
            }
        };
    if entries.is_empty() {
        return None;
    }
    Some(Action::Item(ItemAction::RemoveFromPlaylist {
        playlist,
        entries,
    }))
}

/// `Ctrl+Up`/`Ctrl+Down` — `delta` is `-1`/`+1`. On the Now Playing tab (`07-06`) this reorders the
/// queue instead of a `PlaylistTracks` row (`07-03`); a no-op outside both contexts, at the
/// top/bottom already, or without a real `PlaylistEntryId` on the focused row.
fn move_track_action(state: &AppState, delta: isize) -> Option<Action> {
    if state.nav.active_tab == Tab::NowPlaying {
        return move_queue_entry_action(state, delta);
    }
    let column = state.active_column()?;
    let loxia_core::state::nav::ColumnKind::PlaylistTracks { of_playlist } = &column.kind else {
        return None;
    };
    let MediaItem::Track(track) = column.items.get(column.cursor)? else {
        return None;
    };
    let entry = track.playlist_entry_id.clone()?;
    let new_index = (column.cursor as isize + delta).clamp(0, column.items.len() as isize - 1);
    if new_index as usize == column.cursor {
        return None;
    }
    Some(Action::Item(ItemAction::MoveInPlaylist {
        playlist: loxia_core::model::PlaylistId::from(of_playlist.as_str()),
        entry,
        new_index: new_index as usize,
    }))
}

/// `07-06`: reorders the focused queue row by one position — a no-op on the History sub-view
/// (read-only) or at the top/bottom already.
fn move_queue_entry_action(state: &AppState, delta: isize) -> Option<Action> {
    if state.now_playing_subview != loxia_core::state::NowPlayingSub::Queue {
        return None;
    }
    let len = state.queue.play_order.len();
    if len == 0 {
        return None;
    }
    let from = state.now_playing_cursor;
    let to = (from as isize + delta).clamp(0, len as isize - 1) as usize;
    if to == from {
        return None;
    }
    Some(Action::Queue(QueueAction::MoveEntry { from, to }))
}

/// `Enter` (and `a`, the same binding) queues the current selection everywhere except the Now
/// Playing tab (`07-06`), where it jumps to the focused queue row or re-queues the focused history
/// entry instead — neither is "queue the selection", so this is a genuine context split, not a
/// fallback chain like `remove_entry_action`'s.
fn enter_action(state: &AppState) -> Option<Action> {
    if state.nav.active_tab == Tab::NowPlaying {
        return now_playing_enter_action(state);
    }
    // `Enter` **replaces** the queue and plays now; `Shift+Enter`/`A` (`QueueFullContext`) append
    // instead. This is the `ActionId::QueueArtistOnly` binding (`enter` *and* `a`) — the legacy name
    // predates the replace/append split; both keys now mean "play this" (`docs/12-decisions.md`).
    Some(Action::Queue(QueueAction::PlaySelection {
        full_context: false,
    }))
}

fn now_playing_enter_action(state: &AppState) -> Option<Action> {
    match state.now_playing_subview {
        loxia_core::state::NowPlayingSub::Queue => {
            let &entry_index = state.queue.play_order.get(state.now_playing_cursor)?;
            let entry = state.queue.entries.get(entry_index)?;
            Some(Action::Queue(QueueAction::JumpTo(entry.entry_id)))
        }
        loxia_core::state::NowPlayingSub::History => {
            let sorted = state.history_sorted();
            let entry = sorted.get(state.now_playing_cursor)?;
            Some(Action::Queue(QueueAction::RequeueTrack(Box::new(
                entry.track.clone(),
            ))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::keymap::KeyMap;
    use ratatui::crossterm::event::{
        KeyEvent, KeyEventKind as CtKind, KeyEventState, MouseEvent, MouseEventKind,
    };

    fn press(code: CtKeyCode, mods: CtKeyModifiers) -> CrosstermEvent {
        CrosstermEvent::Key(KeyEvent {
            code,
            modifiers: mods,
            kind: CtKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn viewport() -> Viewport {
        Viewport { rows: 40 }
    }

    /// Every pre-existing (keyboard-only) test predates `to_action`'s own `10-04` signature
    /// change — none of them care about mouse state, so this supplies fresh, empty defaults for
    /// the three new parameters rather than repeating them at every call site.
    fn ta(state: &AppState, ev: CrosstermEvent, viewport: Viewport) -> Option<Action> {
        to_action(
            state,
            ev,
            viewport,
            &HitMap::default(),
            &mut MouseState::default(),
            Instant::now(),
        )
    }

    fn state_with_keymap() -> AppState {
        AppState {
            keymap: KeyMap::defaults(),
            ..AppState::default()
        }
    }

    /// Terminals send Shift+Tab as its own sequence, which crossterm reports as `BackTab` with no
    /// SHIFT modifier. `to_chord` had no arm for it, so the chord was dropped before reaching any
    /// binding and Shift+Tab did nothing anywhere (`docs/12-decisions.md`).
    #[test]
    fn shift_tab_arrives_as_backtab_and_cycles_tabs_backwards() {
        let state = state_with_keymap();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::BackTab, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::PrevTab))
        );
        // Some terminals additionally set SHIFT alongside BackTab; both must resolve the same.
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::BackTab, CtKeyModifiers::SHIFT),
                viewport()
            ),
            Some(Action::Nav(NavAction::PrevTab))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Tab, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::NextTab))
        );
    }

    #[test]
    fn key_release_events_are_ignored() {
        let state = state_with_keymap();
        let ev = CrosstermEvent::Key(KeyEvent {
            code: CtKeyCode::Char('j'),
            modifiers: CtKeyModifiers::NONE,
            kind: CtKind::Release,
            state: KeyEventState::NONE,
        });
        assert_eq!(ta(&state, ev, viewport()), None);
    }

    /// `10-10`: forwarded, not discarded like `Paste` still is — the reducer needs these to
    /// suppress a desktop notification while the terminal already has focus.
    #[test]
    fn focus_events_become_terminal_focus_changed() {
        let state = state_with_keymap();
        assert_eq!(
            ta(&state, CrosstermEvent::FocusGained, viewport()),
            Some(Action::System(SystemEvent::TerminalFocusChanged(true)))
        );
        assert_eq!(
            ta(&state, CrosstermEvent::FocusLost, viewport()),
            Some(Action::System(SystemEvent::TerminalFocusChanged(false)))
        );
    }

    #[test]
    fn paste_events_are_ignored() {
        let state = state_with_keymap();
        assert_eq!(
            ta(&state, CrosstermEvent::Paste(String::new()), viewport()),
            None
        );
    }

    #[test]
    fn uppercase_char_normalises_shift() {
        let state = state_with_keymap();
        // crossterm reports a physical Shift+A as `Char('A')` plus the `SHIFT` modifier — already
        // the shifted character, not `Char('a')` — so normalisation must drop the redundant
        // `SHIFT` bit rather than leave two chords that both spell "capital A" resolving
        // differently.
        let plain_upper = ta(
            &state,
            press(CtKeyCode::Char('A'), CtKeyModifiers::NONE),
            viewport(),
        );
        let shifted = ta(
            &state,
            press(CtKeyCode::Char('A'), CtKeyModifiers::SHIFT),
            viewport(),
        );
        assert_eq!(plain_upper, shifted);
        assert!(plain_upper.is_some());
    }

    #[test]
    fn ctrl_c_quits_from_every_context() {
        let mut modal_state = state_with_keymap();
        modal_state.modal = Some(Modal::Help {
            context: InputContext::Normal,
            scroll: 0,
        });
        let mut text_input_state = state_with_keymap();
        text_input_state.modal = Some(Modal::SavePlaylist {
            target: loxia_core::state::modal::PlaylistTarget::New,
            target_cursor: 0,
            name: String::new(),
            overview: String::new(),
            autosort: false,
            field: 1,
            source: loxia_core::state::modal::SaveSource::Queue,
            error: None,
        });

        for state in [state_with_keymap(), modal_state, text_input_state] {
            let action = ta(
                &state,
                press(CtKeyCode::Char('c'), CtKeyModifiers::CONTROL),
                viewport(),
            );
            assert_eq!(action, Some(Action::System(SystemEvent::Quit)));
        }
    }

    // --- 10-03: help modal scroll ---------------------------------------------------------------

    fn help_state() -> AppState {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::Help {
            context: InputContext::Normal,
            scroll: 0,
        });
        state
    }

    #[test]
    fn jk_scroll_help_instead_of_moving_columns() {
        let state = help_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('j'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Scroll(1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('k'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Scroll(-1)))
        );
    }

    #[test]
    fn ctrl_u_d_scroll_help_by_half_a_page() {
        let state = help_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('d'), CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Scroll(20)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('u'), CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Scroll(-20)))
        );
    }

    #[test]
    fn jk_still_moves_columns_outside_help() {
        let state = state_with_keymap();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('j'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::MoveDown { n: 1 }))
        );
    }

    // --- 10-05: device picker modal keyboard input --------------------------------------------

    fn device_picker_state() -> AppState {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: loxia_core::state::nav::LoadState::Loading,
        });
        state
    }

    #[test]
    fn jk_and_arrows_move_the_device_cursor_not_columns() {
        let state = device_picker_state();
        for down in [CtKeyCode::Char('j'), CtKeyCode::Down] {
            assert_eq!(
                ta(&state, press(down, CtKeyModifiers::NONE), viewport()),
                Some(Action::Modal(loxia_core::action::ModalAction::FieldNext))
            );
        }
        for up in [CtKeyCode::Char('k'), CtKeyCode::Up] {
            assert_eq!(
                ta(&state, press(up, CtKeyModifiers::NONE), viewport()),
                Some(Action::Modal(loxia_core::action::ModalAction::FieldPrev))
            );
        }
    }

    #[test]
    fn enter_submits_the_open_modal() {
        let state = device_picker_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Submit))
        );
    }

    #[test]
    fn esc_cancels_the_device_picker() {
        let state = device_picker_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::Cancel))
        );
    }

    // --- 10-06: equalizer modal keyboard input --------------------------------------------------

    fn equalizer_state() -> AppState {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::Equalizer {
            band: 0,
            draft_gains: [0.0; 10],
            gains_at_open: [0.0; 10],
            preset_idx: None,
            bypassed: false,
            enabled: true,
            enabled_at_open: true,
        });
        state
    }

    #[test]
    fn arrows_select_band_and_adjust_gain() {
        let state = equalizer_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Right, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::FieldNext))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Left, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::FieldPrev))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Up, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::AdjustGain(
                0.5
            )))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::AdjustGain(
                -0.5
            )))
        );
    }

    #[test]
    fn p_cycles_preset_and_b_toggles_bypass() {
        let state = equalizer_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('p'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::CyclePreset))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('b'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::ToggleBypass))
        );
    }

    /// The equalizer modal's own letters are hardcoded here rather than routed through `KeyMap`,
    /// so `keymap::validate` cannot see them and no conflict check covers them. Overlapping a
    /// Normal-context binding is not itself a bug — they are context-scoped, and `p` has always
    /// doubled as "previous track" outside this modal — but a letter whose *global* meaning a user
    /// would expect to still apply is. `o` was picked for off first and had to be changed: it is
    /// `OpenSortMenu` everywhere else, so it read as "ordering" beside a list-shaped editor
    /// (`docs/12-decisions.md`). This pins the set so a future change is a deliberate one.
    #[test]
    fn the_equalizer_modals_hardcoded_letters_are_p_b_and_t() {
        let state = equalizer_state();
        let letters = ['p', 'b', 't'];
        for c in letters {
            assert!(
                ta(
                    &state,
                    press(CtKeyCode::Char(c), CtKeyModifiers::NONE),
                    viewport()
                )
                .is_some(),
                "{c} must be handled by the equalizer modal"
            );
        }
        assert!(
            !letters.contains(&'o'),
            "`o` opens the sort menu everywhere else — do not reuse it here"
        );
    }

    #[test]
    fn t_toggles_the_equalizer_off_and_on() {
        let state = equalizer_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('t'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::ToggleEqEnabled
            ))
        );
    }

    #[test]
    fn enter_submits_and_esc_cancels_the_equalizer() {
        let state = equalizer_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Submit))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::Cancel))
        );
    }

    // --- 10-07: sleep timer modal keyboard input ------------------------------------------------

    fn sleep_timer_state() -> AppState {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::SleepTimer {
            trigger: loxia_core::state::player::SleepTrigger::Duration(
                std::time::Duration::from_secs(900),
            ),
            fade_out: true,
            quit_after: false,
            field_cursor: 0,
        });
        state
    }

    #[test]
    fn jk_and_arrows_move_the_sleep_timer_row_cursor() {
        let state = sleep_timer_state();
        for down in [CtKeyCode::Char('j'), CtKeyCode::Down] {
            assert_eq!(
                ta(&state, press(down, CtKeyModifiers::NONE), viewport()),
                Some(Action::Modal(loxia_core::action::ModalAction::FieldNext))
            );
        }
        for up in [CtKeyCode::Char('k'), CtKeyCode::Up] {
            assert_eq!(
                ta(&state, press(up, CtKeyModifiers::NONE), viewport()),
                Some(Action::Modal(loxia_core::action::ModalAction::FieldPrev))
            );
        }
    }

    #[test]
    fn space_activates_and_d_disarms() {
        let state = sleep_timer_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char(' '), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::ActivateField
            ))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('d'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::DisarmSleepTimer
            ))
        );
    }

    #[test]
    fn enter_submits_and_esc_cancels_the_sleep_timer() {
        let state = sleep_timer_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Submit))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::Cancel))
        );
    }

    // --- 10-08: save playlist modal keyboard input ----------------------------------------------

    #[test]
    fn p_opens_with_queue_ctrl_p_opens_with_selection() {
        let state = state_with_keymap();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('P'), CtKeyModifiers::SHIFT),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::OpenSavePlaylist(
                    loxia_core::state::modal::SaveSource::Queue
                )
            ))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('p'), CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::OpenSavePlaylist(
                    loxia_core::state::modal::SaveSource::Selection
                )
            ))
        );
    }

    fn save_playlist_state(field: usize) -> AppState {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::SavePlaylist {
            target: loxia_core::state::modal::PlaylistTarget::New,
            target_cursor: 0,
            name: String::new(),
            overview: String::new(),
            autosort: false,
            field,
            source: loxia_core::state::modal::SaveSource::Queue,
            error: None,
        });
        state
    }

    #[test]
    fn jk_and_arrows_cycle_the_target_dropdown_only_when_focused() {
        let state = save_playlist_state(0);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::CycleSaveTarget(1)
            ))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Up, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::CycleSaveTarget(-1)
            ))
        );

        // On any other field, Up/Down resolve to nothing at all.
        let state = save_playlist_state(3);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            None
        );
    }

    // --- 10-09: sort profile modal keyboard input -----------------------------------------------

    fn sort_profile_state() -> AppState {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::SortProfile {
            profiles: Vec::new(),
            cursor: 0,
            editing: None,
            target: loxia_core::state::modal::SortApplyTarget::Queue,
        });
        state
    }

    #[test]
    fn jk_and_arrows_move_the_sort_profile_cursor() {
        let state = sort_profile_state();
        for down in [CtKeyCode::Char('j'), CtKeyCode::Down] {
            assert_eq!(
                ta(&state, press(down, CtKeyModifiers::NONE), viewport()),
                Some(Action::Modal(loxia_core::action::ModalAction::FieldNext))
            );
        }
        for up in [CtKeyCode::Char('k'), CtKeyCode::Up] {
            assert_eq!(
                ta(&state, press(up, CtKeyModifiers::NONE), viewport()),
                Some(Action::Modal(loxia_core::action::ModalAction::FieldPrev))
            );
        }
    }

    #[test]
    fn tab_toggles_target_and_e_opens_settings_sorting() {
        let state = sort_profile_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Tab, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::ToggleSortTarget
            ))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('e'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::OpenSettingsSorting
            ))
        );
    }

    #[test]
    fn enter_submits_and_esc_cancels_the_sort_profile() {
        let state = sort_profile_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Submit))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::Cancel))
        );
    }

    #[test]
    fn g_prefix_then_a_resolves_to_go_to_artist() {
        let mut state = state_with_keymap();
        let g = ta(
            &state,
            press(CtKeyCode::Char('g'), CtKeyModifiers::NONE),
            viewport(),
        );
        assert!(matches!(
            g,
            Some(Action::System(SystemEvent::SetPendingChord(_)))
        ));
        if let Some(Action::System(SystemEvent::SetPendingChord(chord))) = g {
            state.pending_chord = Some((chord, loxia_core::Timestamp::now()));
        }

        let a = ta(
            &state,
            press(CtKeyCode::Char('a'), CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(a, Some(Action::Nav(NavAction::GoToArtist)));
    }

    #[test]
    fn g_prefix_then_invalid_clears_pending() {
        let mut state = state_with_keymap();
        state.pending_chord = Some((
            KeyChord {
                code: LoxiaKeyCode::Char('g'),
                mods: LoxiaKeyModifiers::default(),
            },
            loxia_core::Timestamp::now(),
        ));

        let action = ta(
            &state,
            press(CtKeyCode::Char('z'), CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(action, None);
    }

    #[test]
    fn half_page_uses_viewport_rows() {
        let state = state_with_keymap();
        let action = ta(
            &state,
            press(CtKeyCode::Char('d'), CtKeyModifiers::CONTROL),
            Viewport { rows: 40 },
        );
        assert_eq!(action, Some(Action::Nav(NavAction::HalfPageDown { n: 20 })));
    }

    #[test]
    fn text_input_routes_printable_to_field() {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::SavePlaylist {
            target: loxia_core::state::modal::PlaylistTarget::New,
            target_cursor: 0,
            name: String::new(),
            overview: String::new(),
            autosort: false,
            field: 1,
            source: loxia_core::state::modal::SaveSource::Queue,
            error: None,
        });

        let action = ta(
            &state,
            press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(
            action,
            Some(Action::Modal(loxia_core::action::ModalAction::FieldInput(
                'x'
            )))
        );
    }

    #[test]
    fn text_input_still_resolves_esc_and_enter() {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::SavePlaylist {
            target: loxia_core::state::modal::PlaylistTarget::New,
            target_cursor: 0,
            name: "a".to_string(),
            overview: String::new(),
            autosort: false,
            field: 1,
            source: loxia_core::state::modal::SaveSource::Queue,
            error: None,
        });

        let esc = ta(
            &state,
            press(CtKeyCode::Esc, CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(esc, Some(Action::Nav(NavAction::Cancel)));

        let enter = ta(
            &state,
            press(CtKeyCode::Enter, CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(
            enter,
            Some(Action::Modal(loxia_core::action::ModalAction::Submit))
        );
    }

    #[test]
    fn resize_maps_to_system_action() {
        let state = state_with_keymap();
        let action = ta(&state, CrosstermEvent::Resize(120, 40), viewport());
        assert_eq!(
            action,
            Some(Action::System(SystemEvent::Resize { w: 120, h: 40 }))
        );
    }

    #[test]
    fn mouse_ignored_when_disabled() {
        // A real hit target actually sits at (5, 5) — proving `enable_mouse = false` is what
        // suppresses the click, not merely that nothing was there to hit anyway.
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 10, 10),
            HitTarget::SidebarTab(Tab::Favourites),
        );
        let ev = CrosstermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(ratatui::crossterm::event::MouseButton::Left),
            column: 5,
            row: 5,
            modifiers: CtKeyModifiers::NONE,
        });

        let mut disabled = state_with_keymap();
        disabled.config.ui.enable_mouse = false;
        assert_eq!(
            to_action(
                &disabled,
                ev.clone(),
                viewport(),
                &hits,
                &mut MouseState::default(),
                Instant::now()
            ),
            None
        );

        let mut enabled = state_with_keymap();
        enabled.config.ui.enable_mouse = true;
        assert_eq!(
            to_action(
                &enabled,
                ev,
                viewport(),
                &hits,
                &mut MouseState::default(),
                Instant::now()
            ),
            Some(Action::Nav(NavAction::SetTab(Tab::Favourites)))
        );
    }

    fn state_with_filter_editing() -> AppState {
        use loxia_core::state::nav::{Column, ColumnKind, NavFocus, Tab};

        let mut state = state_with_keymap();
        let mut column = Column::new(ColumnKind::Artists, "Artists");
        column.filter = Some(String::new());
        column.filter_editing = true;
        state.nav.active_tab = Tab::Artists;
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![column]);
        state.nav.focus = NavFocus::Column(0);
        state
    }

    /// This is the bug `04-11` found and fixed: `context_for` correctly reported `TextInput` for
    /// an active inline filter even before this task, but `text_input_action` unconditionally
    /// mapped every key to a `ModalAction` — meaning a printable key typed into the filter with no
    /// modal open silently went nowhere at all (`Action::Modal(FieldInput(c))` reaching a reducer
    /// with `state.modal == None` is a no-op). Typing into the filter must dispatch a `Nav` action.
    #[test]
    fn filter_editing_routes_printable_to_nav_filter_input() {
        let state = state_with_filter_editing();
        let action = ta(
            &state,
            press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(action, Some(Action::Nav(NavAction::FilterInput('x'))));
    }

    #[test]
    fn filter_editing_backspace_maps_to_filter_backspace() {
        let state = state_with_filter_editing();
        let action = ta(
            &state,
            press(CtKeyCode::Backspace, CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(action, Some(Action::Nav(NavAction::FilterBackspace)));
    }

    #[test]
    fn filter_editing_enter_commits_filter() {
        let state = state_with_filter_editing();
        let action = ta(
            &state,
            press(CtKeyCode::Enter, CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(action, Some(Action::Nav(NavAction::CommitFilter)));
    }

    #[test]
    fn filter_editing_esc_cancels() {
        let state = state_with_filter_editing();
        let action = ta(
            &state,
            press(CtKeyCode::Esc, CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(action, Some(Action::Nav(NavAction::Cancel)));
    }

    /// A committed filter (`filter.is_some()` but `filter_editing == false`, i.e. after `Enter`)
    /// must not keep swallowing keystrokes as `TextInput` — ordinary navigation resumes over the
    /// still-narrowed list.
    #[test]
    fn committed_filter_does_not_stay_in_text_input_mode() {
        let mut state = state_with_filter_editing();
        state
            .nav
            .per_tab_stacks
            .get_mut(&loxia_core::state::nav::Tab::Artists)
            .unwrap()[0]
            .filter_editing = false;

        let action = ta(
            &state,
            press(CtKeyCode::Char('j'), CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(action, Some(Action::Nav(NavAction::MoveDown { n: 1 })));
    }

    /// From the Search query line, a *modified* chord is a command, not text. `Alt+1`..`Alt+9`
    /// typed their bare digit into the query (the text arms match on `code` alone, so a modified
    /// `Char` still read as text) and `F1`..`F10` were swallowed whole — the `TextInput` arm had no
    /// fallback to the normal table (`docs/12-decisions.md`).
    #[test]
    fn commands_reach_the_normal_table_from_the_search_query() {
        let mut state = state_with_keymap();
        state.nav.active_tab = Tab::Search;
        assert!(
            state.search.query_focused,
            "focused on tab entry by default"
        );

        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('1'), CtKeyModifiers::ALT),
                viewport()
            ),
            Some(Action::Nav(NavAction::SetTab(Tab::NowPlaying))),
            "Alt+1 must jump tabs, not type a 1"
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::F(1), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::View(loxia_core::action::ViewAction::ToggleHelp)),
            "F1 must open help rather than vanish"
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::F(5), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::SetTab(Tab::Artists))),
            "F5 must jump tabs"
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('q'), CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::System(SystemEvent::Quit)),
            "Ctrl+Q must still quit from a text field"
        );
    }

    /// ...while ordinary typing, including capitals, still goes into the query.
    #[test]
    fn plain_typing_still_reaches_the_search_query() {
        let mut state = state_with_keymap();
        state.nav.active_tab = Tab::Search;
        for (code, mods, expected) in [
            (CtKeyCode::Char('j'), CtKeyModifiers::NONE, 'j'),
            (CtKeyCode::Char('A'), CtKeyModifiers::SHIFT, 'A'),
            (CtKeyCode::Char('1'), CtKeyModifiers::NONE, '1'),
        ] {
            assert_eq!(
                ta(&state, press(code, mods), viewport()),
                Some(Action::Nav(NavAction::FilterInput(expected))),
                "{code:?} should be typed into the query"
            );
        }
    }

    /// `07-01`: the Search tab's query line has no `filter_editing` flag to key off (it isn't a
    /// Miller column at all) — typing must still reach `NavAction::FilterInput`, not the `Normal`
    /// table (which would resolve `'j'` to `MoveDown` instead of appending to the query).
    #[test]
    fn search_query_focused_routes_typing_to_filter_input() {
        let mut state = state_with_keymap();
        state.nav.active_tab = Tab::Search;
        assert!(
            state.search.query_focused,
            "focused on tab entry by default"
        );

        let action = ta(
            &state,
            press(CtKeyCode::Char('j'), CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(action, Some(Action::Nav(NavAction::FilterInput('j'))));
    }

    /// `↓`/`↑` on the query line step into and out of the results even though the query line is a
    /// text field — the reducer treats query and results as one flat list, so the arrows a user
    /// reaches for actually reach the results (`docs/12-decisions.md`).
    #[test]
    fn search_query_focused_routes_down_and_up_to_move() {
        let mut state = state_with_keymap();
        state.nav.active_tab = Tab::Search;
        assert!(state.search.query_focused);

        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::MoveDown { n: 1 }))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Up, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::MoveUp { n: 1 }))
        );
    }

    #[test]
    fn search_query_unfocused_uses_normal_table() {
        let mut state = state_with_keymap();
        state.nav.active_tab = Tab::Search;
        state.search.query_focused = false;

        let action = ta(
            &state,
            press(CtKeyCode::Char('j'), CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(action, Some(Action::Nav(NavAction::MoveDown { n: 1 })));
    }

    /// Settings' three-level focus: `Esc` in the rows steps out to the section list, `↑`/`↓` there
    /// pick a group, `Esc` again leaves for the tab sidebar, and from the sidebar `↑`/`↓` fall
    /// through to tab-switching (`docs/12-decisions.md`).
    #[test]
    fn settings_esc_steps_out_to_the_section_list_then_arrows_pick_groups() {
        let mut state = state_with_keymap();
        state.nav.active_tab = Tab::Settings;

        // Rows: Esc -> section list.
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::FocusSectionList))
        );

        // In the section list: arrows move between groups, Enter/→ enters rows, Esc leaves.
        state.settings.section_list_focused = true;
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::NextSection))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::FocusRows))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::LeaveToTabSidebar))
        );
    }

    #[test]
    fn settings_on_the_tab_sidebar_lets_arrows_switch_tabs() {
        let mut state = state_with_keymap();
        state.nav.active_tab = Tab::Settings;
        state.nav.sidebar_focused = true;

        // `↓` isn't claimed by the Settings dispatch here — it falls through to the normal table so
        // `move_focused` can switch tabs.
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::MoveDown { n: 1 }))
        );
        // `→` steps back into the section list.
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Right, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::FocusSectionList))
        );
    }

    /// A real bug found in the field: with the query line focused on every tab entry
    /// (`docs/07-ui-spec.md` §9), `Tab` used to be silently swallowed — `to_action`'s own
    /// `InputContext::TextInput` arm has no fallback to `normal_resolve` the way the Settings tab's
    /// own dispatch does, so an unhandled key here never reaches tab-switching at all. The user's
    /// own report: "it doesn't switch from search, it is stuck to it."
    #[test]
    fn tab_switches_tabs_even_while_the_search_query_is_focused() {
        let mut state = state_with_keymap();
        state.nav.active_tab = Tab::Search;
        assert!(
            state.search.query_focused,
            "focused on tab entry by default"
        );

        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Tab, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::NextTab))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Tab, CtKeyModifiers::SHIFT),
                viewport()
            ),
            Some(Action::Nav(NavAction::PrevTab))
        );
    }

    // --- 07-06: now playing tab -------------------------------------------------------------

    fn now_playing_state() -> AppState {
        let mut state = loxia_core::test_support::fixtures::fixture_playing_queue();
        state.keymap = KeyMap::defaults();
        state.nav.active_tab = Tab::NowPlaying;
        state
    }

    /// `Enter` on the Now Playing tab's Queue sub-view jumps to the focused row, not "queue the
    /// selection" (`07-06`) — the cursor auto-follows `queue.position` until moved, so this also
    /// covers the common case of pressing `Enter` on the currently-playing row itself.
    #[test]
    fn enter_on_now_playing_queue_jumps_to_cursor() {
        let mut state = now_playing_state();
        state.now_playing_cursor = 1;
        let expected_id = state.queue.entries[state.queue.play_order[1]].entry_id;

        let action = ta(
            &state,
            press(CtKeyCode::Enter, CtKeyModifiers::NONE),
            viewport(),
        );
        assert_eq!(
            action,
            Some(Action::Queue(QueueAction::JumpTo(expected_id)))
        );
    }

    /// `Enter` (`a`'s own binding) on the Now Playing tab's History sub-view re-queues the focused
    /// entry (`07-06`) instead of jumping — History is read-only otherwise.
    #[test]
    fn enter_on_now_playing_history_requeues() {
        let mut state = now_playing_state();
        state.now_playing_subview = loxia_core::state::NowPlayingSub::History;
        state
            .history
            .push_back(loxia_core::state::queue::HistoryEntry {
                track: state.queue.entries[0].track.clone(),
                played_at: loxia_core::test_support::fixtures::fixed_epoch(),
                completed: true,
            });
        state.now_playing_cursor = 0;

        let action = ta(
            &state,
            press(CtKeyCode::Enter, CtKeyModifiers::NONE),
            viewport(),
        );
        assert!(matches!(
            action,
            Some(Action::Queue(QueueAction::RequeueTrack(_)))
        ));
    }

    /// `Ctrl+Down` on the Now Playing tab's Queue sub-view reorders the queue (`07-06`) instead of
    /// a `PlaylistTracks` row (`07-03`) — the two contexts never overlap.
    #[test]
    fn ctrl_down_on_now_playing_queue_reorders() {
        let mut state = now_playing_state();
        state.now_playing_cursor = 0;

        let action = ta(
            &state,
            press(CtKeyCode::Down, CtKeyModifiers::CONTROL),
            viewport(),
        );
        assert_eq!(
            action,
            Some(Action::Queue(QueueAction::MoveEntry { from: 0, to: 1 }))
        );
    }

    // --- 10-04: mouse support -------------------------------------------------------------------

    fn mouse_ev(kind: MouseEventKind, col: u16, row: u16) -> CrosstermEvent {
        CrosstermEvent::Mouse(MouseEvent {
            kind,
            column: col,
            row,
            modifiers: CtKeyModifiers::NONE,
        })
    }

    fn mouse_enabled_miller_state() -> AppState {
        let mut state = loxia_core::test_support::fixtures::fixture_miller_3col();
        state.config.ui.enable_mouse = true;
        state.keymap = KeyMap::defaults();
        state
    }

    #[test]
    fn click_focuses_column_and_sets_cursor() {
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 10, 1),
            HitTarget::ColumnItem {
                column: 0,
                index: 1,
            },
        );
        let mut mouse = MouseState::default();

        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );

        assert_eq!(
            action,
            Some(Action::Nav(NavAction::FocusColumnAt {
                column: 0,
                index: 1
            }))
        );
    }

    #[test]
    fn double_click_drills_in() {
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 10, 1),
            HitTarget::ColumnItem {
                column: 0,
                index: 1,
            },
        );
        let mut mouse = MouseState::default();
        let now = Instant::now();
        let down = || mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0);

        let first = to_action(&state, down(), viewport(), &hits, &mut mouse, now);
        assert_eq!(
            first,
            Some(Action::Nav(NavAction::FocusColumnAt {
                column: 0,
                index: 1
            }))
        );

        let second = to_action(
            &state,
            down(),
            viewport(),
            &hits,
            &mut mouse,
            now + Duration::from_millis(100),
        );
        assert_eq!(second, Some(Action::Nav(NavAction::NavRight)));
    }

    #[test]
    fn two_clicks_on_different_targets_is_not_double() {
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 10, 1),
            HitTarget::ColumnItem {
                column: 0,
                index: 1,
            },
        );
        hits.push(
            Rect::new(0, 1, 10, 1),
            HitTarget::ColumnItem {
                column: 0,
                index: 2,
            },
        );
        let mut mouse = MouseState::default();
        let now = Instant::now();

        to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut mouse,
            now,
        );
        let second = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 1),
            viewport(),
            &hits,
            &mut mouse,
            now + Duration::from_millis(50),
        );

        assert_eq!(
            second,
            Some(Action::Nav(NavAction::FocusColumnAt {
                column: 0,
                index: 2
            })),
            "a different target must never register as a double-click, regardless of timing"
        );
    }

    #[test]
    fn double_click_window_is_400ms() {
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 10, 1),
            HitTarget::ColumnItem {
                column: 0,
                index: 1,
            },
        );
        let now = Instant::now();

        let mut just_inside = MouseState::default();
        to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut just_inside,
            now,
        );
        let inside = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut just_inside,
            now + Duration::from_millis(400),
        );
        assert_eq!(inside, Some(Action::Nav(NavAction::NavRight)));

        let mut just_outside = MouseState::default();
        to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut just_outside,
            now,
        );
        let outside = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut just_outside,
            now + Duration::from_millis(401),
        );
        assert_eq!(
            outside,
            Some(Action::Nav(NavAction::FocusColumnAt {
                column: 0,
                index: 1
            })),
            "401ms must miss the double-click window"
        );
    }

    #[test]
    fn seek_bar_click_seeks_to_fraction() {
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(Rect::new(10, 5, 21, 1), HitTarget::SeekBar); // cols 10..=30, so col 20 is the midpoint
        let mut mouse = MouseState::default();

        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 20, 5),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );

        match action {
            Some(Action::Player(PlayerAction::Seek(SeekTarget::Fraction(f)))) => {
                assert!((f - 0.5).abs() < 0.01, "expected ~0.5, got {f}");
            }
            other => panic!("expected a Seek(Fraction), got {other:?}"),
        }
    }

    #[test]
    fn drag_continues_outside_widget_rect() {
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(Rect::new(10, 5, 11, 1), HitTarget::SeekBar); // cols 10..=20
        let mut mouse = MouseState::default();

        // Mouse-down inside the bar captures it.
        to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 15, 5),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );

        // Dragging far to the right, well outside the bar's own rect, must still control it —
        // clamped to the far end, not silently dropped.
        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Drag(CtMouseButton::Left), 200, 5),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        assert_eq!(
            action,
            Some(Action::Player(PlayerAction::Seek(SeekTarget::Fraction(
                1.0
            ))))
        );
    }

    #[test]
    fn drag_released_outside_clears_capture() {
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(Rect::new(10, 5, 11, 1), HitTarget::SeekBar);
        let mut mouse = MouseState::default();

        to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 15, 5),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        to_action(
            &state,
            mouse_ev(MouseEventKind::Up(CtMouseButton::Left), 200, 5),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );

        // A further drag with nothing captured produces no action at all.
        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Drag(CtMouseButton::Left), 200, 5),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        assert_eq!(
            action, None,
            "the drag must not still be stuck on after release"
        );
    }

    #[test]
    fn scroll_targets_column_under_pointer_not_focused() {
        // `fixture_miller_3col` focuses column 2; scrolling over column 0's own area must still
        // scroll column 0, never the focused one.
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 10, 1),
            HitTarget::ColumnItem {
                column: 0,
                index: 1,
            },
        );
        let mut mouse = MouseState::default();

        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::ScrollDown, 5, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );

        assert_eq!(
            action,
            Some(Action::Nav(NavAction::ScrollColumn {
                column: 0,
                delta: 3
            }))
        );
    }

    /// The Now Playing pane is not a Miller column, so `ScrollColumn` could never reach it and the
    /// wheel simply did nothing there while working everywhere else (`docs/12-decisions.md`).
    /// Both the entry rows and the pane backdrop behind them must scroll it.
    #[test]
    fn scroll_over_the_now_playing_pane_moves_its_cursor() {
        let state = mouse_enabled_miller_state();
        let mut mouse = MouseState::default();

        for (target, label) in [
            (
                HitTarget::QueueEntry(loxia_core::model::QueueEntryId(7)),
                "an entry row",
            ),
            (HitTarget::NowPlayingList, "the pane backdrop"),
        ] {
            let mut hits = HitMap::default();
            hits.push(Rect::new(0, 0, 10, 5), target);

            for (kind, delta) in [
                (MouseEventKind::ScrollDown, 3),
                (MouseEventKind::ScrollUp, -3),
            ] {
                assert_eq!(
                    to_action(
                        &state,
                        mouse_ev(kind, 5, 2),
                        viewport(),
                        &hits,
                        &mut mouse,
                        Instant::now(),
                    ),
                    Some(Action::Nav(NavAction::ScrollNowPlaying { delta })),
                    "{kind:?} over {label} should scroll the queue"
                );
            }
        }
    }

    /// The backdrop is registered under the whole pane; a *click* on it must stay inert, so it
    /// never shadows the entry rows drawn over it.
    #[test]
    fn clicking_the_now_playing_backdrop_does_nothing() {
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 10, 5), HitTarget::NowPlayingList);
        let mut mouse = MouseState::default();

        assert_eq!(
            to_action(
                &state,
                mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 2),
                viewport(),
                &hits,
                &mut mouse,
                Instant::now(),
            ),
            None
        );
    }

    #[test]
    fn enter_and_right_on_the_quit_row_quit() {
        let mut state = state_with_keymap();
        state.nav.sidebar_quit_focused = true;

        for code in [CtKeyCode::Enter, CtKeyCode::Right, CtKeyCode::Char('l')] {
            assert_eq!(
                ta(&state, press(code, CtKeyModifiers::NONE), viewport()),
                Some(Action::System(SystemEvent::Quit)),
                "{code:?} on the focused Quit row should quit"
            );
        }
    }

    #[test]
    fn transport_buttons_dispatch_actions() {
        let state = mouse_enabled_miller_state();
        for (button, expected) in [
            (TransportButton::Prev, Action::Player(PlayerAction::Prev)),
            (
                TransportButton::PlayPause,
                Action::Player(PlayerAction::PlayPause),
            ),
            (TransportButton::Stop, Action::Player(PlayerAction::Stop)),
            (TransportButton::Next, Action::Player(PlayerAction::Next)),
            (
                TransportButton::Repeat,
                Action::Queue(QueueAction::CycleRepeat),
            ),
        ] {
            let mut hits = HitMap::default();
            hits.push(Rect::new(0, 0, 5, 1), HitTarget::Transport(button));
            let mut mouse = MouseState::default();
            let action = to_action(
                &state,
                mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 2, 0),
                viewport(),
                &hits,
                &mut mouse,
                Instant::now(),
            );
            assert_eq!(action, Some(expected), "button {button:?}");
        }

        // `Shuffle` carries a seed derived from `state.clock` rather than a fixed payload — only
        // its own action *kind* is checked here.
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 5, 1),
            HitTarget::Transport(TransportButton::Shuffle),
        );
        let mut mouse = MouseState::default();
        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 2, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        assert!(matches!(
            action,
            Some(Action::Queue(QueueAction::ToggleShuffle { .. }))
        ));
    }

    #[test]
    fn eq_band_click_and_drag_set_gain_from_y() {
        // `10-06`: "click sets the gain from the y position and drag adjusts continuously" — both
        // `Down` and `Drag` produce `ModalAction::SetGainAt` (the modal's own draft/live-preview
        // state), never `PlayerAction::SetEqGain` (which would commit directly to the already-
        // active curve, bypassing `Esc`'s own undo).
        let state = mouse_enabled_miller_state();
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 3, 11), HitTarget::EqBand(2)); // rows 0..=10
        let mut mouse = MouseState::default();

        // Mouse-down at the very top row: +12 dB.
        let down = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 1, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        match down {
            Some(Action::Modal(loxia_core::action::ModalAction::SetGainAt { band, db })) => {
                assert_eq!(band, 2);
                assert!(
                    (db - 12.0).abs() < 0.01,
                    "expected ~+12dB at the top, got {db}"
                );
            }
            other => panic!("expected SetGainAt, got {other:?}"),
        }

        let drag_top = to_action(
            &state,
            mouse_ev(MouseEventKind::Drag(CtMouseButton::Left), 1, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        match drag_top {
            Some(Action::Modal(loxia_core::action::ModalAction::SetGainAt { band, db })) => {
                assert_eq!(band, 2);
                assert!(
                    (db - 12.0).abs() < 0.01,
                    "expected ~+12dB at the top, got {db}"
                );
            }
            other => panic!("expected SetGainAt, got {other:?}"),
        }

        // Bottom row: -12 dB.
        let drag_bottom = to_action(
            &state,
            mouse_ev(MouseEventKind::Drag(CtMouseButton::Left), 1, 10),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        match drag_bottom {
            Some(Action::Modal(loxia_core::action::ModalAction::SetGainAt { db, .. })) => {
                assert!(
                    (db + 12.0).abs() < 0.01,
                    "expected ~-12dB at the bottom, got {db}"
                );
            }
            other => panic!("expected SetGainAt, got {other:?}"),
        }
    }

    #[test]
    fn click_outside_modal_closes_it() {
        let mut state = mouse_enabled_miller_state();
        state.modal = Some(Modal::Help {
            context: InputContext::Normal,
            scroll: 0,
        });
        let mut hits = HitMap::default();
        // Whatever was underneath before the modal opened — a click here must close the modal
        // rather than dispatch the column-focus action it would otherwise produce.
        hits.push(
            Rect::new(0, 0, 10, 1),
            HitTarget::ColumnItem {
                column: 0,
                index: 1,
            },
        );
        let mut mouse = MouseState::default();

        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );

        assert_eq!(
            action,
            Some(Action::Modal(loxia_core::action::ModalAction::Close))
        );
    }

    #[test]
    fn click_on_modal_field_sets_cursor() {
        let state = device_picker_state();
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 10, 1), HitTarget::ModalField(2));
        let mut mouse = MouseState::default();

        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );

        assert_eq!(
            action,
            Some(Action::Modal(loxia_core::action::ModalAction::FieldSet(2)))
        );
    }

    #[test]
    fn double_click_on_modal_field_submits() {
        let state = device_picker_state();
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 10, 1), HitTarget::ModalField(2));
        let mut mouse = MouseState::default();
        let now = Instant::now();
        let down = || mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0);

        let first = to_action(&state, down(), viewport(), &hits, &mut mouse, now);
        assert_eq!(
            first,
            Some(Action::Modal(loxia_core::action::ModalAction::FieldSet(2)))
        );

        let second = to_action(
            &state,
            down(),
            viewport(),
            &hits,
            &mut mouse,
            now + Duration::from_millis(100),
        );
        assert_eq!(
            second,
            Some(Action::Modal(loxia_core::action::ModalAction::Submit))
        );
    }

    #[test]
    fn click_inside_modal_field_routes_through() {
        let mut state = mouse_enabled_miller_state();
        state.modal = Some(Modal::Help {
            context: InputContext::Normal,
            scroll: 0,
        });
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 10, 1), HitTarget::EqBand(0));
        let mut mouse = MouseState::default();

        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );

        // A click on `EqBand` routes to `SetGainAt` (per
        // `eq_band_click_and_drag_set_gain_from_y`) rather than closing the modal, unlike
        // `click_outside_modal_closes_it` — the point of this test.
        assert_eq!(
            action,
            Some(Action::Modal(loxia_core::action::ModalAction::SetGainAt {
                band: 0,
                db: 0.0
            }))
        );
    }

    // --- 11-01: Settings tab keyboard wiring --------------------------------------------------

    fn settings_state() -> AppState {
        let mut state = state_with_keymap();
        state.nav.active_tab = loxia_core::state::nav::Tab::Settings;
        state
    }

    #[test]
    fn jk_and_arrows_move_the_settings_row_cursor() {
        let state = settings_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('j'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Up, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(-1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('k'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(-1)))
        );
    }

    #[test]
    fn left_right_and_space_adjust_the_focused_value() {
        let state = settings_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Left, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::AdjustValue(-1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Right, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::AdjustValue(1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char(' '), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::AdjustValue(1)))
        );
    }

    #[test]
    fn ctrl_e_reveals_the_focused_secret() {
        let state = settings_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('e'), CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ToggleRevealSecret))
        );
    }

    /// Settings used to claim `Tab`/`Shift+Tab` for section cycling. Since entering a tab focuses
    /// its content, tabbing onto Settings then trapped the user there with no way to tab back out
    /// (`docs/12-decisions.md`). Tab means "next tab" everywhere, without exception.
    #[test]
    fn tab_cycles_tabs_even_on_the_settings_tab() {
        let state = settings_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Tab, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::NextTab))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::BackTab, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Nav(NavAction::PrevTab))
        );
    }

    #[test]
    fn enter_on_a_text_row_starts_editing() {
        let mut state = settings_state();
        // "download directory" — the Audio section's own former Text rows (output driver/device)
        // are dropdowns now, so this uses a row that is genuinely still free text. Located by
        // label rather than by index, so adding a Cache row above it doesn't silently retarget
        // this test at a different control.
        let section = loxia_core::state::settings::SettingsSection::Cache;
        state.settings.section = section;
        state.settings.cursor =
            loxia_core::reducer::settings::rows_for_section(section, &state.config, &[])
                .iter()
                .position(|r| r.label == "download directory")
                .expect("the download directory row exists");
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::StartTextEdit))
        );
    }

    #[test]
    fn enter_on_a_toggle_row_does_nothing() {
        let mut state = settings_state();
        state.settings.section = loxia_core::state::settings::SettingsSection::Cache;
        state.settings.cursor = 0; // "enabled", a Toggle control
        // Claimed as a harmless no-op (`MoveRow(0)`), not left to fall through to
        // `normal_resolve`'s own global `Enter` binding (`QueueSelection`), which would make no
        // sense in this tab.
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(0)))
        );
    }

    #[test]
    fn enter_on_an_action_row_activates_it() {
        let mut state = settings_state();
        state.settings.section = loxia_core::state::settings::SettingsSection::Servers;
        // "servers" is the trailing `Action` row in that section.
        let rows = loxia_core::reducer::settings::rows_for_section(
            state.settings.section,
            &state.config,
            &state.player.known_devices,
        );
        state.settings.cursor = rows.len() - 1;
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ActivateRow))
        );
    }

    #[test]
    fn unclaimed_chords_fall_through_to_normal_resolve() {
        let state = settings_state();
        // `Ctrl+Q` (Quit) isn't claimed by `settings_row_action` at all — it must still work.
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('q'), CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::System(SystemEvent::Quit))
        );
    }

    #[test]
    fn text_edit_mode_routes_typing_and_enter_esc_correctly() {
        let mut state = settings_state();
        state.settings.section = loxia_core::state::settings::SettingsSection::Audio;
        state.settings.cursor = 0;
        state.settings.editing = Some(loxia_core::state::settings::TextEdit::new("auto"));

        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::TextInput('x')))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Backspace, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::TextBackspace))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::CommitTextEdit))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::CancelTextEdit))
        );
    }

    #[test]
    fn click_on_settings_section_switches_section() {
        let state = settings_state();
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 16, 1),
            HitTarget::SettingsSection(loxia_core::state::settings::SettingsSection::Audio),
        );
        let mut mouse = MouseState::default();
        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        assert_eq!(
            action,
            Some(Action::Settings(SettingsAction::SetSection(
                loxia_core::state::settings::SettingsSection::Audio
            )))
        );
    }

    #[test]
    fn click_on_settings_row_sets_the_cursor() {
        let state = settings_state();
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 16, 1), HitTarget::SettingsRow(3));
        let mut mouse = MouseState::default();
        let action = to_action(
            &state,
            mouse_ev(MouseEventKind::Down(CtMouseButton::Left), 5, 0),
            viewport(),
            &hits,
            &mut mouse,
            Instant::now(),
        );
        assert_eq!(action, Some(Action::Settings(SettingsAction::SetRow(3))));
    }

    fn keymap_editor_state(capturing: bool) -> AppState {
        let mut state = state_with_keymap();
        state.modal = Some(Modal::KeymapEditor {
            action_cursor: 0,
            capturing,
            conflict: None,
            captured: None,
            capture_deadline: None,
        });
        state
    }

    /// `11-02`: every raw key while `capturing` is claimed verbatim, ahead of `resolve` entirely
    /// — `j` would otherwise resolve to `MoveDown` (`Action::Nav`), never `CaptureChord`.
    #[test]
    fn capturing_claims_every_key_verbatim() {
        let state = keymap_editor_state(true);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('j'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::CaptureChord(KeyChord {
                    code: LoxiaKeyCode::Char('j'),
                    mods: LoxiaKeyModifiers::default(),
                })
            ))
        );
    }

    #[test]
    fn esc_while_capturing_cancels_not_closes() {
        let state = keymap_editor_state(true);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::CancelCapture
            ))
        );
    }

    #[test]
    fn enter_starts_capture_when_nothing_captured_yet() {
        let state = keymap_editor_state(false);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::StartCapture))
        );
    }

    #[test]
    fn enter_submits_once_something_is_captured() {
        let mut state = keymap_editor_state(false);
        if let Some(Modal::KeymapEditor { captured, .. }) = &mut state.modal {
            *captured = Some(loxia_core::keymap::parse::parse_binding("z").unwrap());
        }
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::Submit))
        );
    }

    #[test]
    fn d_x_and_shift_r_map_to_the_per_row_and_reset_all_actions() {
        let state = keymap_editor_state(false);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('d'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::ResetRowToDefault
            ))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::UnbindRow))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('R'), CtKeyModifiers::SHIFT),
                viewport()
            ),
            Some(Action::Modal(
                loxia_core::action::ModalAction::ConfirmResetAllKeybindings
            ))
        );
    }

    #[test]
    fn jk_and_arrows_move_the_keymap_editor_cursor() {
        let state = keymap_editor_state(false);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::FieldNext))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Up, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Modal(loxia_core::action::ModalAction::FieldPrev))
        );
    }

    /// Even while capturing, `Ctrl+C` still quits — checked unconditionally before anything else
    /// in `to_action`, including the capture-mode intercept itself, so a user is never trapped.
    #[test]
    fn ctrl_c_still_quits_while_capturing() {
        let state = keymap_editor_state(true);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('c'), CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::System(SystemEvent::Quit))
        );
    }

    fn server_editor_state(editing: bool) -> AppState {
        use loxia_core::state::settings::{ServerDraft, ServerEditorState};
        let mut state = settings_state();
        state.settings.server_editor = Some(ServerEditorState {
            editing: if editing {
                Some(ServerDraft::default())
            } else {
                None
            },
            ..ServerEditorState::default()
        });
        state
    }

    #[test]
    fn server_editor_list_keys_map_to_actions() {
        let state = server_editor_state(false);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('a'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorAddNew))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorRemove))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('D'), CtKeyModifiers::SHIFT),
                viewport()
            ),
            Some(Action::Settings(
                SettingsAction::ServerEditorToggleDeleteData
            ))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('S'), CtKeyModifiers::SHIFT),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorSwitch))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorEdit))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorClose))
        );
    }

    /// A real bug found in the field: `context_for` correctly entered `InputContext::TextInput`
    /// while a server-draft field's own `text_buf` was active, but `text_input_action` only ever
    /// checked the flat `SettingsState.editing` to decide *how* to handle that context — every
    /// keystroke fell through to `filter_text_input_action` (a silent no-op with no active Miller
    /// column to filter), and `Esc` there maps to the equally inert `NavAction::Cancel`, so the
    /// add/edit form looked completely frozen: typing did nothing, `Enter` did nothing, and there
    /// was no way to back out short of `Ctrl+C`, which quits the whole app. This exercises the
    /// real path (`to_action`, not the reducer directly, which every other server-editor test in
    /// this crate already covered and which never caught this) for all three affected sub-editors.
    #[test]
    fn typing_into_server_draft_field_reaches_the_reducer() {
        use loxia_core::state::settings::{ServerDraft, ServerEditorState};
        let mut state = settings_state();
        state.settings.server_editor = Some(ServerEditorState {
            editing: Some(ServerDraft {
                text_buf: Some(loxia_core::state::settings::TextEdit::default()),
                ..ServerDraft::default()
            }),
            ..ServerEditorState::default()
        });

        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::TextInput('x'))),
            "a character typed into the field must reach the reducer's own TextInput handling"
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::CommitTextEdit)),
            "Enter must commit the field, not silently no-op as NavAction::CommitFilter"
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::CancelTextEdit)),
            "Esc must cancel the field edit, not silently no-op as NavAction::Cancel"
        );
    }

    #[test]
    fn typing_into_sort_profile_name_buffer_reaches_the_reducer() {
        use loxia_core::state::settings::SortProfileEditorState;
        let mut state = settings_state();
        state.settings.sort_profile_editor = Some(SortProfileEditorState {
            name_buf: Some(loxia_core::state::settings::TextEdit::default()),
            ..SortProfileEditorState::default()
        });

        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::TextInput('x')))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::CommitTextEdit))
        );
    }

    #[test]
    fn typing_into_eq_preset_name_buffer_reaches_the_reducer() {
        use loxia_core::state::settings::EqPresetEditorState;
        let mut state = settings_state();
        state.settings.eq_preset_editor = Some(EqPresetEditorState {
            name_buf: Some(loxia_core::state::settings::TextEdit::default()),
            ..EqPresetEditorState::default()
        });

        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::TextInput('x')))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::CommitTextEdit))
        );
    }

    #[test]
    fn server_editor_form_tab_cycles_fields_not_sections() {
        let state = server_editor_state(true);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Tab, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Tab, CtKeyModifiers::SHIFT),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(-1)))
        );
    }

    /// The `address:` row advertises `[←→]` in its own hint, and the arrows did nothing there:
    /// only `Protocol` was in the check (`docs/12-decisions.md`).
    #[test]
    fn arrows_cycle_the_address_selector_too() {
        use loxia_core::state::settings::{ServerDraft, ServerEditorState};
        let mut state = settings_state();
        let mut draft = ServerDraft::default();
        draft.field = loxia_core::reducer::settings::server_draft_fields(&draft)
            .iter()
            .position(|f| *f == loxia_core::state::settings::ServerDraftField::Endpoint)
            .unwrap();
        state.settings.server_editor = Some(ServerEditorState {
            editing: Some(draft),
            ..ServerEditorState::default()
        });

        for (code, delta) in [(CtKeyCode::Left, -1), (CtKeyCode::Right, 1)] {
            assert_eq!(
                ta(&state, press(code, CtKeyModifiers::NONE), viewport()),
                Some(Action::Settings(SettingsAction::ServerEditorCycleProtocol(
                    delta
                ))),
                "{code:?} on the address row"
            );
        }
    }

    #[test]
    fn protocol_field_left_right_and_enter_cycle_it() {
        use loxia_core::state::settings::{ServerDraft, ServerEditorState};
        let mut state = settings_state();
        let mut draft = ServerDraft::default();
        // Located rather than counted: the form has grown a row before this one, and hardcoding an
        // index would have silently re-pointed the test at a different field.
        draft.field = loxia_core::reducer::settings::server_draft_fields(&draft)
            .iter()
            .position(|f| *f == loxia_core::state::settings::ServerDraftField::Protocol)
            .unwrap();
        state.settings.server_editor = Some(ServerEditorState {
            editing: Some(draft),
            ..ServerEditorState::default()
        });

        for code in [CtKeyCode::Left, CtKeyCode::Right, CtKeyCode::Enter] {
            let expected_delta = match code {
                CtKeyCode::Left => -1,
                _ => 1,
            };
            assert_eq!(
                ta(&state, press(code, CtKeyModifiers::NONE), viewport()),
                Some(Action::Settings(SettingsAction::ServerEditorCycleProtocol(
                    expected_delta
                ))),
                "{code:?}"
            );
        }
    }

    #[test]
    fn protocol_cycle_keys_are_a_no_op_on_a_different_field() {
        // Field 0 is Name — Left/Right must not fire `ServerEditorCycleProtocol` there (the
        // reducer would refuse it anyway, but this keeps them free to fall through to whatever,
        // if anything, `normal_resolve` would otherwise say).
        let state = server_editor_state(true);
        assert_ne!(
            ta(
                &state,
                press(CtKeyCode::Left, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorCycleProtocol(
                -1
            )))
        );
        assert_ne!(
            ta(
                &state,
                press(CtKeyCode::Right, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorCycleProtocol(
                1
            )))
        );
    }

    #[test]
    fn server_editor_form_esc_closes_the_form_not_the_editor() {
        let state = server_editor_state(true);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorClose))
        );
    }

    #[test]
    fn server_editor_form_letters_are_not_the_list_shortcuts() {
        // `a` is the list sub-view's own "add new" shortcut — unclaimed while the form itself is
        // open, so (like any other chord `settings_row_action` doesn't recognise) it falls
        // through to the global keymap table instead of doing nothing.
        let state = server_editor_state(true);
        assert_ne!(
            ta(
                &state,
                press(CtKeyCode::Char('a'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::ServerEditorAddNew))
        );
    }

    fn sort_editor_state(rule_cursor: Option<usize>) -> AppState {
        use loxia_core::state::settings::SortProfileEditorState;
        let mut state = settings_state();
        state.settings.sort_profile_editor = Some(SortProfileEditorState {
            rule_cursor,
            ..SortProfileEditorState::default()
        });
        state
    }

    #[test]
    fn sort_editor_list_keys_map_to_actions() {
        let state = sort_editor_state(None);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('n'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorNew))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('r'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorRename))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorDeleteProfile))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('R'), CtKeyModifiers::SHIFT),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorRestoreDefaults))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('a'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorAddRule))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('d'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorDeleteRule))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Up, CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorReorderRule(-1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::CONTROL),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorReorderRule(1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorClose))
        );
    }

    #[test]
    fn enter_on_a_profile_header_applies_it() {
        let state = sort_editor_state(None);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorApply))
        );
    }

    #[test]
    fn enter_or_space_on_a_rule_row_toggles_direction() {
        let state = sort_editor_state(Some(0));
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorToggleDirection))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char(' '), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorToggleDirection))
        );
    }

    #[test]
    fn left_right_cycle_the_field_on_a_rule_row() {
        let state = sort_editor_state(Some(0));
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Left, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorCycleField(-1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Right, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::SortEditorCycleField(1)))
        );
    }

    #[test]
    fn jk_and_arrows_move_the_sort_editor_cursor() {
        let state = sort_editor_state(None);
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Up, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(-1)))
        );
    }

    fn eq_editor_state() -> AppState {
        use loxia_core::state::settings::EqPresetEditorState;
        let mut state = settings_state();
        state.settings.eq_preset_editor = Some(EqPresetEditorState::default());
        state
    }

    #[test]
    fn eq_editor_keys_map_to_actions() {
        let state = eq_editor_state();
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('s'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::EqEditorSaveCurrent))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('r'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::EqEditorRename))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('x'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::EqEditorDelete))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Char('e'), CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::EqEditorOpenEqualizer))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Enter, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::EqEditorOpenEqualizer))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Esc, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::EqEditorClose))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Down, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(1)))
        );
        assert_eq!(
            ta(
                &state,
                press(CtKeyCode::Up, CtKeyModifiers::NONE),
                viewport()
            ),
            Some(Action::Settings(SettingsAction::MoveRow(-1)))
        );
    }
}
