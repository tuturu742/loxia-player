//! The default binding table — normative, transcribed exactly from `docs/04-state-and-input.md`
//! §6, aliases included. If a binding here looks wrong, change the doc in the same change rather
//! than diverging from it.

use super::ActionId;

pub(crate) const DEFAULT_BINDINGS: &[(&str, ActionId)] = &[
    // Navigation
    ("j", ActionId::MoveDown),
    ("down", ActionId::MoveDown),
    ("k", ActionId::MoveUp),
    ("up", ActionId::MoveUp),
    ("h", ActionId::NavLeft),
    ("left", ActionId::NavLeft),
    ("l", ActionId::NavRight),
    ("right", ActionId::NavRight),
    ("ctrl+u", ActionId::HalfPageUp),
    ("pageup", ActionId::HalfPageUp),
    ("ctrl+d", ActionId::HalfPageDown),
    ("pagedown", ActionId::HalfPageDown),
    ("g g", ActionId::GoToTop),
    ("home", ActionId::GoToTop),
    ("G", ActionId::GoToBottom),
    ("end", ActionId::GoToBottom),
    ("backspace", ActionId::PopColumn),
    ("tab", ActionId::NextTab),
    ("shift+tab", ActionId::PrevTab),
    // `f1` is *not* an alias for `alt+1` the way `f2`…`f9` are for `alt+2`…`alt+9`: F1 is the help
    // key in effectively every program a user has ever run, and reaching for it expecting help
    // while it silently jumped to the first tab was reported as a bug (`docs/12-decisions.md`).
    // Tab 1 keeps `alt+1`; only its F-row alias is given up.
    ("alt+1", ActionId::JumpTab1),
    ("alt+2", ActionId::JumpTab2),
    ("f2", ActionId::JumpTab2),
    ("alt+3", ActionId::JumpTab3),
    ("f3", ActionId::JumpTab3),
    ("alt+4", ActionId::JumpTab4),
    ("f4", ActionId::JumpTab4),
    ("alt+5", ActionId::JumpTab5),
    ("f5", ActionId::JumpTab5),
    ("alt+6", ActionId::JumpTab6),
    ("f6", ActionId::JumpTab6),
    ("alt+7", ActionId::JumpTab7),
    ("f7", ActionId::JumpTab7),
    ("alt+8", ActionId::JumpTab8),
    ("f8", ActionId::JumpTab8),
    ("alt+9", ActionId::JumpTab9),
    ("f9", ActionId::JumpTab9),
    ("alt+0", ActionId::JumpTab10),
    ("f10", ActionId::JumpTab10),
    ("/", ActionId::OpenFilter),
    ("g a", ActionId::GoToArtist),
    ("g l", ActionId::GoToAlbum),
    ("esc", ActionId::Cancel),
    // Playback and seeking
    ("space", ActionId::PlayPause),
    ("n", ActionId::NextTrack),
    ("p", ActionId::PrevTrack),
    ("S", ActionId::Stop),
    ("[", ActionId::SeekBack5),
    ("]", ActionId::SeekForward5),
    ("{", ActionId::SeekBack30),
    ("}", ActionId::SeekForward30),
    ("+", ActionId::VolumeUp),
    ("=", ActionId::VolumeUp),
    ("-", ActionId::VolumeDown),
    ("_", ActionId::VolumeDown),
    ("M", ActionId::ToggleMute),
    // Queue and selection
    ("enter", ActionId::QueueArtistOnly),
    ("a", ActionId::QueueArtistOnly),
    // `A` is listed **first** because `hint_for` advertises an action's first binding, and this is
    // the one that actually reaches us. `shift+enter` needs the terminal's keyboard-enhancement
    // protocol to be distinguishable from a bare `Enter`, which loxia does not enable — so without
    // it the chord is simply undeliverable, and the legend was advertising the one key that could
    // never work while hiding the one that does (`docs/12-decisions.md`). Kept as an alias for the
    // terminals that do report it.
    ("A", ActionId::QueueFullContext),
    ("shift+enter", ActionId::QueueFullContext),
    ("i", ActionId::InsertNext),
    ("m", ActionId::InstantMix),
    ("s", ActionId::ToggleShuffle),
    ("R", ActionId::CycleRepeat),
    ("o", ActionId::OpenSortMenu),
    ("x", ActionId::RemoveEntry),
    ("v", ActionId::ToggleVisualSelect),
    (".", ActionId::ToggleItem),
    ("V", ActionId::SelectAll),
    // Audio and DSP
    ("e", ActionId::ToggleEqualizer),
    ("r", ActionId::CycleReplayGain),
    ("q", ActionId::CycleQualityProfile),
    ("O", ActionId::OpenDevicePicker),
    ("T", ActionId::OpenSleepTimer),
    // Items, playlists and views
    ("f", ActionId::ToggleFavorite),
    ("d", ActionId::ToggleDownload),
    ("P", ActionId::SaveQueueAsPlaylist),
    ("ctrl+p", ActionId::AddToPlaylist),
    ("X", ActionId::DeletePlaylist),
    ("ctrl+up", ActionId::MoveTrackUp),
    ("ctrl+down", ActionId::MoveTrackDown),
    ("z", ActionId::ToggleZenMode),
    ("H", ActionId::ToggleHistory),
    ("L", ActionId::ToggleLyrics),
    // The shifted pair of the `j`/`k` that move a list: unsynced lyrics are the other scrollable
    // thing on screen, and both letters were free in every context (`docs/12-decisions.md`).
    ("K", ActionId::LyricsScrollUp),
    ("J", ActionId::LyricsScrollDown),
    ("?", ActionId::ToggleHelp),
    ("f1", ActionId::ToggleHelp),
    // System
    ("ctrl+q", ActionId::Quit),
    ("ctrl+c", ActionId::Quit),
    ("ctrl+r", ActionId::Refresh),
];
