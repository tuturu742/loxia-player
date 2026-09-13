//! Event — a result from the outside world. The runtime converts every `Event` 1:1 into an
//! `Action` (`docs/04-state-and-input.md` §1); `Event::Data`/`Audio`/`System` reuse the exact
//! payload types `Action::Data`/`Audio`/`System` carry so that conversion is a plain wrap, never a
//! translation.

use serde::{Deserialize, Serialize};

use crate::action::{Action, AudioEvent, DataAction, PlayerAction, SystemEvent};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Data(DataAction),
    Audio(AudioEvent),
    System(SystemEvent),
    /// `10-11`: a media-key/OS-now-playing-centre control (`souvlaki`'s `MediaControlEvent`,
    /// mapped by `workers::mpris`) arriving on `souvlaki`'s own callback thread — the first
    /// worker to ever need to inject a `PlayerAction`, so this variant didn't exist before it.
    Player(PlayerAction),
}

impl Event {
    pub fn into_action(self) -> Action {
        match self {
            Event::Data(d) => Action::Data(d),
            Event::Audio(a) => Action::Audio(a),
            Event::System(s) => Action::System(s),
            Event::Player(p) => Action::Player(p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_action_wraps_without_translation() {
        let event = Event::System(SystemEvent::Refresh);
        assert_eq!(event.into_action(), Action::System(SystemEvent::Refresh));
    }

    /// `10-11`.
    #[test]
    fn player_event_wraps_without_translation() {
        let event = Event::Player(PlayerAction::Next);
        assert_eq!(event.into_action(), Action::Player(PlayerAction::Next));
    }
}
