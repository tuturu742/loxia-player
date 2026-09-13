//! Keybinding conflict detection.

use super::{ActionId, InputContext, KeyBinding, KeyMap};

/// A binding attempted for more than one distinct action, in insertion order (defaults first,
/// then config overrides). `context` is always `Normal` today — the default table and its
/// overrides only ever populate the one flat table (`docs/04-state-and-input.md` §5); the field
/// exists for a future per-modal binding table to reuse the same conflict-reporting shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyConflict {
    pub context: InputContext,
    pub binding: KeyBinding,
    pub actions: Vec<ActionId>,
}

/// Every binding ever inserted into `map` (default or override) that was attempted for more than
/// one distinct action. Empty for `KeyMap::defaults()` by construction — this is the check that
/// proves it.
pub fn validate(map: &KeyMap) -> Vec<KeyConflict> {
    let mut conflicts = Vec::new();
    for (binding, actions) in map.insertion_groups() {
        let mut distinct: Vec<ActionId> = Vec::new();
        for action in actions {
            if !distinct.contains(&action) {
                distinct.push(action);
            }
        }
        if distinct.len() > 1 {
            conflicts.push(KeyConflict {
                context: InputContext::Normal,
                binding: binding.clone(),
                actions: distinct,
            });
        }
    }
    conflicts
}
