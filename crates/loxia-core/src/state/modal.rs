//! Modal enum and per-modal local state. `AppState.modal` is `Option<Modal>` — only one modal at
//! a time, by construction; opening a second replaces it.

use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::config::{Direction, SortField, SortProfile};
use crate::keymap::{ActionId, InputContext, KeyBinding};
use crate::model::{AudioDevice, PlaylistId};

use super::nav::LoadState;
use super::player::SleepTrigger;

/// Bare discriminant of [`Modal`], with no local state — what [`crate::keymap::InputContext`]
/// keys on, and what conflict-detection tables group by. Needs `Serialize`/`Deserialize` because
/// `Action::Modal::Open(ModalKind)` does (`Action` is fully serialisable for replay tests);
/// `Modal` itself, below, is never serialised, so it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModalKind {
    Help,
    Equalizer,
    DevicePicker,
    SleepTimer,
    SavePlaylist,
    SortProfile,
    KeymapEditor,
    Confirm,
}

/// Which playlist a `SavePlaylist` modal writes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistTarget {
    New,
    Existing(PlaylistId),
}

/// What tracks a `SavePlaylist` modal is saving — the whole queue (`P`) or the active column
/// selection (`Ctrl+P`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveSource {
    Queue,
    Selection,
}

/// A draft copy of one sort rule being edited in the `SortProfile` modal, kept separate from the
/// committed `SortRule` so cancelling the edit discards changes cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleEditor {
    pub field: SortField,
    pub direction: Direction,
}

/// `10-09`: what a `SortProfile` modal's own `Enter` applies the selected profile to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortApplyTarget {
    Queue,
    Column,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    Help {
        context: InputContext,
        scroll: u16,
    },
    Equalizer {
        band: usize,
        draft_gains: [f32; 10],
        gains_at_open: [f32; 10],
        preset_idx: Option<usize>,
        bypassed: bool,
        /// Drafted like the gains are: `o` flips it live, `Enter` commits it to
        /// `config.equalizer.enabled`, `Esc` reverts to `enabled_at_open`.
        enabled: bool,
        enabled_at_open: bool,
    },
    DevicePicker {
        devices: Vec<AudioDevice>,
        cursor: usize,
        load: LoadState,
    },
    SleepTimer {
        trigger: SleepTrigger,
        fade_out: bool,
        quit_after: bool,
        field_cursor: usize,
    },
    SavePlaylist {
        target: PlaylistTarget,
        /// `10-08`: which row of the target dropdown is highlighted — `0` is `Create New
        /// Playlist…`, `1..=N` index into the loaded existing-playlists list (`reducer::modal::
        /// existing_playlists`). Kept alongside `target` itself (the semantic "what will actually
        /// be saved to") rather than derived from it on every read, since resolving an `Existing`
        /// target back to its position in that list would otherwise need a linear search anyway.
        target_cursor: usize,
        name: String,
        overview: String,
        autosort: bool,
        field: usize,
        source: SaveSource,
        /// `10-08`: "`Enter` with an empty name shows an inline `name is required`... rather than
        /// closing" — set by `reducer::modal::submit`'s own validation, cleared by any further
        /// edit (`field_input`/`field_backspace`/`cycle_save_target`/`activate_field`).
        error: Option<String>,
    },
    SortProfile {
        profiles: Vec<SortProfile>,
        cursor: usize,
        editing: Option<RuleEditor>,
        /// `10-09`: "Apply to: Active Queue / Current Column" — defaults to the queue when
        /// something is playing, otherwise the column (`reducer::modal::build_sort_profile_modal`).
        target: SortApplyTarget,
    },
    KeymapEditor {
        action_cursor: usize,
        capturing: bool,
        conflict: Option<ActionId>,
        /// The chord(s) the user has captured so far while `capturing`, awaiting either a second
        /// chord (within `capture_deadline`) to extend it into a 2-chord sequence, or `Submit`'s
        /// conflict check once capture ends. `11-02` also reuses this same field, once `capturing`
        /// has gone back to `false`, as "the finished capture awaiting confirmation" — a captured
        /// binding is never discarded except by an explicit cancel or a completed commit.
        captured: Option<KeyBinding>,
        /// `11-02`: armed the instant the first chord of a capture is recorded, cleared the
        /// instant a second one arrives or the window lapses — `reducer::modal::
        /// expire_capture_window` (called from `tick`) finalizes the capture as a 1-chord binding
        /// once this passes, mirroring `AppState::pending_chord`'s own prefix-timeout shape
        /// (`docs/04-state-and-input.md` §7's "2-second window" for the editor specifically, as
        /// opposed to `pending_chord`'s 1-second window for *resolving* an existing sequence).
        capture_deadline: Option<crate::Timestamp>,
    },
    Confirm {
        prompt: String,
        on_confirm: Box<Action>,
    },
}

impl Modal {
    pub fn kind(&self) -> ModalKind {
        match self {
            Modal::Help { .. } => ModalKind::Help,
            Modal::Equalizer { .. } => ModalKind::Equalizer,
            Modal::DevicePicker { .. } => ModalKind::DevicePicker,
            Modal::SleepTimer { .. } => ModalKind::SleepTimer,
            Modal::SavePlaylist { .. } => ModalKind::SavePlaylist,
            Modal::SortProfile { .. } => ModalKind::SortProfile,
            Modal::KeymapEditor { .. } => ModalKind::KeymapEditor,
            Modal::Confirm { .. } => ModalKind::Confirm,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_matches_variant() {
        let modal = Modal::Confirm {
            prompt: "Delete?".to_string(),
            on_confirm: Box::new(Action::Modal(crate::action::ModalAction::Close)),
        };
        assert_eq!(modal.kind(), ModalKind::Confirm);
    }
}
