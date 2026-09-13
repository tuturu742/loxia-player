//! `Scenario`: the dispatch-and-assert harness every reducer test builds on.

use crate::action::Action;
use crate::effect::Effect;
use crate::reducer;
use crate::state::AppState;

pub struct Scenario {
    state: AppState,
    effects: Vec<Effect>,
}

impl Scenario {
    pub fn new(state: AppState) -> Self {
        Scenario {
            state,
            effects: Vec::new(),
        }
    }

    /// Applies `action` via `reducer::apply`, accumulating any returned effects onto the running
    /// total so a multi-step scenario can assert on the whole sequence.
    pub fn dispatch(mut self, action: Action) -> Self {
        let mut new_effects = reducer::apply(&mut self.state, action);
        self.effects.append(&mut new_effects);
        self
    }

    pub fn dispatch_all(self, actions: impl IntoIterator<Item = Action>) -> Self {
        actions.into_iter().fold(self, Scenario::dispatch)
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    pub fn last_effect(&self) -> Option<&Effect> {
        self.effects.last()
    }

    pub fn assert_toast_contains(&self, needle: &str) {
        assert!(
            self.state.toasts.iter().any(|t| t.message.contains(needle)),
            "no toast contains {needle:?}; toasts: {:?}",
            self.state.toasts
        );
    }

    pub fn assert_no_effects(&self) {
        assert!(
            self.effects.is_empty(),
            "expected no effects, got {:?}",
            self.effects
        );
    }
}
