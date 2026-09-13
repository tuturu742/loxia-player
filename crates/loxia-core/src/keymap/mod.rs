//! KeyChord, KeyBinding, KeyMap, ActionId.
//!
//! `loxia-core` cannot depend on `ratatui`/`crossterm` (`docs/01-architecture.md` §3.2), so
//! [`KeyCode`] and [`KeyModifiers`] are crate-local mirrors, not re-exports — `loxia-tui`'s input
//! handler converts a real `crossterm::event::KeyEvent` into a [`KeyChord`] at the boundary. Only
//! the variants the default keymap and user rebinding actually need are represented.

pub mod defaults;
pub mod parse;
pub mod resolve;
pub mod validate;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use strum::{Display, EnumIter, EnumString};

use crate::config::ConfigWarning;
use crate::state::modal::ModalKind;
use parse::{parse_binding, render_binding};

/// `Char(' ')` represents Space — there is no separate `Space` variant, matching how crossterm
/// itself reports the space bar (`03-09` converts from the real `crossterm::event::KeyCode`).
/// `parse.rs`'s friendly syntax spells it `"space"` for readability, but it is this variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Insert,
}

/// A bitflag-shaped set over `CTRL`/`ALT`/`SHIFT` — three plain bools rather than the `bitflags`
/// crate, since there is no need for a fourth dependency just to name three booleans.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

/// Length 1 or 2 — length 2 supports the `g`-prefixed sequences (`g g`, `g a`, `g l`). Not
/// `Serialize`/`Deserialize`: nothing persists a `KeyBinding` directly (keybindings round-trip
/// through `parse_binding`/`render_binding`'s string form instead), and `SmallVec` needs its own
/// `serde` feature enabled for the derive, which nothing else here needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct KeyBinding(pub SmallVec<[KeyChord; 2]>);

/// Carves out the two exclusive input modes; `Normal` is a single flat table where no chord can
/// mean two things (`docs/04-state-and-input.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputContext {
    Normal,
    TextInput,
    Modal(ModalKind),
}

/// Every bindable action identity (`docs/04-state-and-input.md` §6). Distinct from `Action`
/// (`03-03`): a keymap binding names *which* action fires, `resolve.rs` turns an `ActionId` plus
/// the current UI context into a concrete, payload-carrying `Action`.
///
/// `Display`/`FromStr` render the stable snake_case form (`play_pause`, `queue_artist_only`, …)
/// used as `config.toml`'s `[keybindings]` keys — renaming a variant is a breaking config change
/// requiring a migration.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    EnumIter,
)]
#[strum(serialize_all = "snake_case")]
pub enum ActionId {
    // Navigation
    MoveDown,
    MoveUp,
    NavLeft,
    NavRight,
    HalfPageUp,
    HalfPageDown,
    GoToTop,
    GoToBottom,
    PopColumn,
    NextTab,
    PrevTab,
    JumpTab1,
    JumpTab2,
    JumpTab3,
    JumpTab4,
    JumpTab5,
    JumpTab6,
    JumpTab7,
    JumpTab8,
    JumpTab9,
    /// A tenth tab arrived with `Tab::AlbumArtists`, so the `Alt+N` run needs one more than the
    /// digits `1`..`9` provide: `Alt+0` continues it, as it does in every browser's tab bar.
    JumpTab10,
    OpenFilter,
    GoToArtist,
    GoToAlbum,
    Cancel,
    // Playback and seeking
    PlayPause,
    NextTrack,
    PrevTrack,
    Stop,
    SeekBack5,
    SeekForward5,
    SeekBack30,
    SeekForward30,
    VolumeUp,
    VolumeDown,
    ToggleMute,
    // Queue and selection
    QueueArtistOnly,
    QueueFullContext,
    InsertNext,
    InstantMix,
    ToggleShuffle,
    CycleRepeat,
    OpenSortMenu,
    RemoveEntry,
    ToggleVisualSelect,
    ToggleItem,
    SelectAll,
    // Audio and DSP
    ToggleEqualizer,
    CycleReplayGain,
    CycleQualityProfile,
    OpenDevicePicker,
    OpenSleepTimer,
    // Items, playlists and views
    ToggleFavorite,
    ToggleDownload,
    SaveQueueAsPlaylist,
    AddToPlaylist,
    DeletePlaylist,
    /// `07-03`: reorders the focused track within a `PlaylistTracks` column by one position.
    MoveTrackUp,
    MoveTrackDown,
    ToggleZenMode,
    ToggleHistory,
    ToggleLyrics,
    LyricsScrollUp,
    LyricsScrollDown,
    ToggleHelp,
    // System
    Quit,
    Refresh,
}

/// `10-03`: the help modal's own grouping — added to `ActionId`'s metadata exactly as that task's
/// own spec asks ("add it to `ActionId`'s metadata in `keymap/mod.rs` if it is not there"), so the
/// cheat sheet groups actions without a second, hand-maintained category table of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HelpCategory {
    Navigation,
    Playback,
    Queue,
    Selection,
    Audio,
    Items,
    Views,
    System,
}

impl ActionId {
    /// Mirrors the section comments this enum's own declaration already used (`// Navigation`,
    /// `// Playback and seeking`, ...) as real, matchable metadata — `Queue`/`Selection` split out
    /// of the combined "Queue and selection" comment, `Views` collects the toggles that were
    /// previously left implicitly under "Items, playlists and views".
    pub fn help_category(self) -> HelpCategory {
        match self {
            ActionId::MoveDown
            | ActionId::MoveUp
            | ActionId::NavLeft
            | ActionId::NavRight
            | ActionId::HalfPageUp
            | ActionId::HalfPageDown
            | ActionId::GoToTop
            | ActionId::GoToBottom
            | ActionId::PopColumn
            | ActionId::NextTab
            | ActionId::PrevTab
            | ActionId::JumpTab1
            | ActionId::JumpTab2
            | ActionId::JumpTab3
            | ActionId::JumpTab4
            | ActionId::JumpTab5
            | ActionId::JumpTab6
            | ActionId::JumpTab7
            | ActionId::JumpTab8
            | ActionId::JumpTab9
            | ActionId::JumpTab10
            | ActionId::OpenFilter
            | ActionId::GoToArtist
            | ActionId::GoToAlbum
            | ActionId::Cancel => HelpCategory::Navigation,

            ActionId::PlayPause
            | ActionId::NextTrack
            | ActionId::PrevTrack
            | ActionId::Stop
            | ActionId::SeekBack5
            | ActionId::SeekForward5
            | ActionId::SeekBack30
            | ActionId::SeekForward30
            | ActionId::VolumeUp
            | ActionId::VolumeDown
            | ActionId::ToggleMute => HelpCategory::Playback,

            ActionId::QueueArtistOnly
            | ActionId::QueueFullContext
            | ActionId::InsertNext
            | ActionId::InstantMix
            | ActionId::ToggleShuffle
            | ActionId::CycleRepeat
            | ActionId::OpenSortMenu
            | ActionId::RemoveEntry => HelpCategory::Queue,

            ActionId::ToggleVisualSelect | ActionId::ToggleItem | ActionId::SelectAll => {
                HelpCategory::Selection
            }

            ActionId::ToggleEqualizer
            | ActionId::CycleReplayGain
            | ActionId::CycleQualityProfile
            | ActionId::OpenDevicePicker
            | ActionId::OpenSleepTimer => HelpCategory::Audio,

            ActionId::ToggleFavorite
            | ActionId::ToggleDownload
            | ActionId::SaveQueueAsPlaylist
            | ActionId::AddToPlaylist
            | ActionId::DeletePlaylist
            | ActionId::MoveTrackUp
            | ActionId::MoveTrackDown => HelpCategory::Items,

            ActionId::ToggleZenMode
            | ActionId::ToggleHistory
            | ActionId::ToggleLyrics
            | ActionId::LyricsScrollUp
            | ActionId::LyricsScrollDown
            | ActionId::ToggleHelp => HelpCategory::Views,

            ActionId::Quit | ActionId::Refresh => HelpCategory::System,
        }
    }
}

/// A resolved binding table: one flat `KeyBinding -> ActionId` map (`Normal` is deliberately a
/// single table with no per-context scoping, `docs/04-state-and-input.md` §5) plus a stable
/// display hint per action for UI surfaces that must never hardcode a key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyMap {
    bindings: BTreeMap<KeyBinding, ActionId>,
    hints: BTreeMap<ActionId, String>,
    /// Every `insert` call, in order (defaults first, then overrides) — a `BTreeMap` alone only
    /// ever remembers the current winner for a key, so `validate()` needs this separate log to
    /// see what got overwritten along the way.
    insertions: Vec<(KeyBinding, ActionId)>,
}

impl KeyMap {
    /// The normative default table (`defaults.rs`), conflict-free by construction.
    pub fn defaults() -> KeyMap {
        let mut map = KeyMap::default();
        for (binding_str, action) in defaults::DEFAULT_BINDINGS {
            let binding =
                parse_binding(binding_str).expect("every default binding string is valid");
            map.insert(binding, *action);
        }
        map.recompute_hints();
        map
    }

    /// Builds on [`KeyMap::defaults`], applying `overrides` (`config.keybindings`, `ActionId`
    /// snake_case name → binding string) in iteration order, **last wins** on conflict. A user
    /// override *replaces* every binding the action previously held (defaults included), rather
    /// than adding an alias — rebinding an action to a new key is expected to give up the old one.
    /// Every parse failure or conflict produces a `ConfigWarning` rather than aborting — the app
    /// must always start (`docs/12-decisions.md` §4).
    pub fn from_config(overrides: &BTreeMap<String, String>) -> (KeyMap, Vec<ConfigWarning>) {
        let mut map = KeyMap::defaults();
        let mut warnings = Vec::new();

        for (action_name, binding_str) in overrides {
            let Ok(action) = action_name.parse::<ActionId>() else {
                // An unrecognised action key is not this function's concern — 01-02 already
                // validates the config shape; silently ignore rather than guessing.
                continue;
            };
            // `11-02`: the keymap editor's own "unbind this action entirely" (`x`) has to persist
            // as *something* other than a real binding string, since simply omitting the action's
            // key here would just leave the default in effect (`from_config` always starts from
            // `KeyMap::defaults()`) — this sentinel is the deliberate, explicit "no, really, give
            // this action nothing" instruction. Not a parse error: skips straight to the next
            // override with no warning, unlike a genuinely malformed string below.
            if binding_str.trim().eq_ignore_ascii_case("none") {
                map.remove_bindings_for(action);
                continue;
            }
            match parse_binding(binding_str) {
                Ok(binding) => {
                    if let Some(&existing_action) = map.bindings.get(&binding)
                        && existing_action != action
                    {
                        warnings.push(ConfigWarning::conflict(&binding, existing_action, action));
                    }
                    map.remove_bindings_for(action);
                    map.insert(binding, action);
                }
                Err(_) => {
                    warnings.push(ConfigWarning::invalid_binding(action, binding_str));
                }
            }
        }

        map.recompute_hints();
        (map, warnings)
    }

    fn insert(&mut self, binding: KeyBinding, action: ActionId) {
        self.insertions.push((binding.clone(), action));
        self.bindings.insert(binding, action);
    }

    pub(crate) fn remove_bindings_for(&mut self, action: ActionId) {
        let stale: Vec<KeyBinding> = self
            .bindings
            .iter()
            .filter(|&(_, &a)| a == action)
            .map(|(b, _)| b.clone())
            .collect();
        for binding in stale {
            self.bindings.remove(&binding);
        }
    }

    /// `11-02`: the keymap editor's own "commit a captured binding" step — always wins outright
    /// over whatever `binding` previously resolved to (a `BTreeMap` keyed on `KeyBinding` gives
    /// the new action that slot the instant it's inserted), so no separate "unbind the incumbent
    /// first" call is needed even for a "rebind anyway" confirmation. Mutates the live map
    /// directly and incrementally, rather than rebuilding it from `config.keybindings` via
    /// [`KeyMap::from_config`] — that rebuild replays overrides in the `BTreeMap`'s own
    /// alphabetical-by-action-name order, not the order the user actually made the edits in,
    /// which would silently let an earlier (alphabetically later) override re-win over a change
    /// just made in this session (`docs/12-decisions.md`). `config.keybindings` is still updated
    /// by the caller for what a *future restart* replays.
    pub(crate) fn rebind(&mut self, action: ActionId, binding: KeyBinding) {
        self.remove_bindings_for(action);
        self.insert(binding, action);
        self.recompute_hints();
    }

    /// `11-02`: removes every binding `action` currently holds. `hint_for` then reports it
    /// `"unbound"`, and it stays reachable only through Settings, matching this task's own "an
    /// unbound action shows `—` and remains reachable" rule.
    pub(crate) fn unbind(&mut self, action: ActionId) {
        self.remove_bindings_for(action);
        self.recompute_hints();
    }

    /// Rebuilds `hints` from scratch: for each action, the earliest-inserted binding (defaults
    /// first, then overrides in config order) that is still present in `bindings` today. Computed
    /// this way, rather than maintained incrementally on every `insert`, because a later override
    /// can both add a new binding and remove an action's earlier one — an incremental cache would
    /// need the exact same "is this still current?" check anyway.
    fn recompute_hints(&mut self) {
        self.hints.clear();
        for (binding, action) in &self.insertions {
            if self.bindings.get(binding) == Some(action) {
                self.hints
                    .entry(*action)
                    .or_insert_with(|| render_binding(binding));
            }
        }
    }

    /// Every distinct binding ever inserted, paired with every action it was attempted for, in
    /// insertion order. Used only by `validate()`.
    pub(crate) fn insertion_groups(&self) -> Vec<(&KeyBinding, Vec<ActionId>)> {
        let mut groups: Vec<(&KeyBinding, Vec<ActionId>)> = Vec::new();
        for (binding, action) in &self.insertions {
            match groups.iter_mut().find(|(b, _)| *b == binding) {
                Some((_, actions)) => actions.push(*action),
                None => groups.push((binding, vec![*action])),
            }
        }
        groups
    }

    /// The stable display hint for `action` (e.g. `"Ctrl+P"`), or `"unbound"` when it has no
    /// binding. When an action has several bindings, this is the first one inserted (table
    /// order), so hints stay stable across calls.
    pub fn hint_for(&self, action: ActionId) -> String {
        self.hints
            .get(&action)
            .cloned()
            .unwrap_or_else(|| "unbound".to_string())
    }

    pub fn binding_for(&self, action: ActionId) -> Option<&KeyBinding> {
        self.bindings
            .iter()
            .find(|&(_, &a)| a == action)
            .map(|(b, _)| b)
    }

    pub fn validate(&self) -> Vec<validate::KeyConflict> {
        validate::validate(self)
    }

    pub(crate) fn resolve_normal(&self, binding: &KeyBinding) -> Option<ActionId> {
        self.bindings.get(binding).copied()
    }

    pub(crate) fn has_binding_with_prefix(&self, prefix: &KeyChord) -> bool {
        self.bindings
            .keys()
            .any(|b| b.0.len() == 2 && &b.0[0] == prefix)
    }
}

impl ConfigWarning {
    fn invalid_binding(action: ActionId, value: &str) -> ConfigWarning {
        ConfigWarning::new(
            &format!("keybindings.{action}"),
            format!("keybinding for {action} is invalid (\"{value}\"); using the default"),
            crate::config::Severity::Warning,
        )
    }

    fn conflict(binding: &KeyBinding, loser: ActionId, winner: ActionId) -> ConfigWarning {
        ConfigWarning::new(
            &format!("keybindings.{winner}"),
            format!(
                "key {} is bound to both {loser} and {winner}; using {winner}",
                render_binding(binding)
            ),
            crate::config::Severity::Warning,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn default_keymap_has_no_conflicts() {
        let map = KeyMap::defaults();
        assert_eq!(
            map.validate(),
            Vec::new(),
            "the normative default table must be conflict-free by construction"
        );
    }

    #[test]
    fn default_keymap_covers_every_action_id() {
        let map = KeyMap::defaults();
        let unbound: Vec<ActionId> = ActionId::iter()
            .filter(|a| map.binding_for(*a).is_none())
            .collect();
        assert!(
            unbound.is_empty(),
            "unbound actions in the default table: {unbound:?}"
        );
    }

    #[test]
    fn defaults_match_documented_table() {
        let map = KeyMap::defaults();
        let mut rows: Vec<String> = map
            .bindings
            .iter()
            .map(|(binding, action)| format!("{} -> {action}", render_binding(binding)))
            .collect();
        rows.sort();
        insta::assert_snapshot!(rows.join("\n"));
    }

    /// F1 is the help key in effectively every program a user has ever run. It used to be an alias
    /// for `alt+1` (jump to tab 1) alongside `f2`…`f9`, and reaching for help with it was reported
    /// as a bug (`docs/12-decisions.md`). Tab 1 keeps `alt+1`; the rest keep both aliases.
    #[test]
    fn f1_opens_help_while_the_other_function_keys_still_jump_tabs() {
        let map = KeyMap::defaults();
        let bound = |s: &str| {
            let binding = parse::parse_binding(s).expect("a valid chord");
            map.bindings.get(&binding).copied()
        };

        assert_eq!(bound("f1"), Some(ActionId::ToggleHelp));
        assert_eq!(bound("alt+1"), Some(ActionId::JumpTab1));
        assert_eq!(bound("f2"), Some(ActionId::JumpTab2));
        assert_eq!(bound("f9"), Some(ActionId::JumpTab9));
    }

    /// `shift+enter` is only distinguishable from a bare `Enter` when the terminal's keyboard
    /// enhancement protocol is on, which loxia does not enable — so the chord never arrives, and
    /// advertising it left the "add to queue" action looking broken while `A`, which does work,
    /// went unmentioned (`docs/12-decisions.md`). The advertised key must be the deliverable one.
    #[test]
    fn the_advertised_queue_keys_are_ones_a_terminal_can_actually_send() {
        let map = KeyMap::defaults();

        assert_eq!(map.hint_for(ActionId::QueueArtistOnly), "enter");
        assert_eq!(map.hint_for(ActionId::QueueFullContext), "A");
        assert_ne!(
            map.hint_for(ActionId::QueueArtistOnly),
            map.hint_for(ActionId::QueueFullContext),
            "the two must be offered under visibly different keys"
        );

        // Still bound for terminals that do report it — demoted, not dropped.
        let shift_enter = parse::parse_binding("shift+enter").expect("a valid chord");
        assert_eq!(
            map.bindings.get(&shift_enter),
            Some(&ActionId::QueueFullContext)
        );
    }

    #[test]
    fn user_override_replaces_default() {
        let mut overrides = BTreeMap::new();
        overrides.insert("play_pause".to_string(), "ctrl+alt+p".to_string());
        let (map, warnings) = KeyMap::from_config(&overrides);
        assert!(warnings.is_empty());
        assert_eq!(map.hint_for(ActionId::PlayPause), "ctrl+alt+p");
    }

    #[test]
    fn unparseable_keybinding_is_dropped_with_warning() {
        let mut overrides = BTreeMap::new();
        overrides.insert("play_pause".to_string(), "not a real binding".to_string());
        let (map, warnings) = KeyMap::from_config(&overrides);
        // Keeps the default binding.
        assert_eq!(map.hint_for(ActionId::PlayPause), "space");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("play_pause"));
        assert!(warnings[0].message.contains("not a real binding"));
    }

    #[test]
    fn conflicting_overrides_last_wins_with_warning() {
        // "n" defaults to NextTrack; rebind ToggleShuffle onto it too.
        let mut overrides = BTreeMap::new();
        overrides.insert("toggle_shuffle".to_string(), "n".to_string());
        let (map, warnings) = KeyMap::from_config(&overrides);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            map.binding_for(ActionId::ToggleShuffle)
                .map(render_binding)
                .as_deref(),
            Some("n")
        );
    }

    #[test]
    fn conflict_warning_names_both_actions() {
        let mut overrides = BTreeMap::new();
        overrides.insert("toggle_shuffle".to_string(), "n".to_string());
        let (_, warnings) = KeyMap::from_config(&overrides);
        assert!(warnings[0].message.contains("next_track"));
        assert!(warnings[0].message.contains("toggle_shuffle"));
    }

    #[test]
    fn hint_for_unbound_action_returns_unbound() {
        let map = KeyMap::default();
        assert_eq!(map.hint_for(ActionId::PlayPause), "unbound");
    }

    #[test]
    fn hint_for_is_stable_across_calls() {
        let map = KeyMap::defaults();
        assert_eq!(
            map.hint_for(ActionId::MoveDown),
            map.hint_for(ActionId::MoveDown)
        );
        assert_eq!(map.hint_for(ActionId::MoveDown), "j");
    }

    /// `11-02`: the keymap editor's own "unbind entirely" persists as the literal string
    /// `"none"` — loading it back must actually remove the action's default binding, not just
    /// fail to parse it (which would silently keep the default in effect instead).
    #[test]
    fn none_sentinel_unbinds_the_action_on_load() {
        let mut overrides = BTreeMap::new();
        overrides.insert("play_pause".to_string(), "none".to_string());
        let (map, warnings) = KeyMap::from_config(&overrides);
        assert!(warnings.is_empty());
        assert_eq!(map.hint_for(ActionId::PlayPause), "unbound");
    }

    #[test]
    fn none_sentinel_is_case_insensitive() {
        let mut overrides = BTreeMap::new();
        overrides.insert("play_pause".to_string(), "None".to_string());
        let (map, _) = KeyMap::from_config(&overrides);
        assert_eq!(map.hint_for(ActionId::PlayPause), "unbound");
    }

    #[test]
    fn rebind_overwrites_the_incumbents_slot_only() {
        let mut map = KeyMap::defaults();
        // `j` and `down` both default to `MoveDown` — stealing just `j` must leave `down` intact.
        let j = parse_binding("j").unwrap();
        map.rebind(ActionId::ToggleShuffle, j.clone());
        assert_eq!(map.resolve_normal(&j), Some(ActionId::ToggleShuffle));
        assert_eq!(map.hint_for(ActionId::MoveDown), "down");
    }

    #[test]
    fn unbind_clears_every_binding_for_the_action() {
        let mut map = KeyMap::defaults();
        map.unbind(ActionId::MoveDown);
        assert_eq!(map.hint_for(ActionId::MoveDown), "unbound");
        assert!(map.binding_for(ActionId::MoveDown).is_none());
    }
}
