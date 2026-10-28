//! Stable action identities for declarative callbacks (Phase 26–28).

use std::cell::RefCell;
use std::collections::HashMap;

/// Stable id for a press / menu action registered with the runtime.
pub type ActionId = u64;

thread_local! {
    static ACTIONS: RefCell<HashMap<ActionId, Box<dyn FnMut()>>> = RefCell::new(HashMap::new());
    /// String menu / legacy ids → ActionId (Phase 28).
    static ACTION_ALIASES: RefCell<HashMap<String, ActionId>> = RefCell::new(HashMap::new());
}

/// Register or replace a callback for `id`.
pub fn register_action(id: ActionId, f: impl FnMut() + 'static) {
    ACTIONS.with(|m| {
        m.borrow_mut().insert(id, Box::new(f));
    });
}

/// Map a string menu id to an [`ActionId`] (Phase 28 MenuBar / ContextMenu bridge).
pub fn register_action_alias(alias: impl Into<String>, id: ActionId) {
    ACTION_ALIASES.with(|m| {
        m.borrow_mut().insert(alias.into(), id);
    });
}

/// Resolve a string alias to an ActionId, if registered.
pub fn action_id_for_alias(alias: &str) -> Option<ActionId> {
    ACTION_ALIASES.with(|m| m.borrow().get(alias).copied())
}

/// Remove a registered action.
pub fn unregister_action(id: ActionId) {
    ACTIONS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

/// Invoke the callback for `id` if present.
pub fn dispatch_action(id: ActionId) {
    ACTIONS.with(|m| {
        if let Some(f) = m.borrow_mut().get_mut(&id) {
            f();
        }
    });
}

/// Dispatch by string: numeric ActionId, then alias table (Phase 28).
pub fn dispatch_action_str(id: &str) -> bool {
    if let Ok(aid) = id.parse::<ActionId>() {
        dispatch_action(aid);
        return true;
    }
    if let Some(aid) = action_id_for_alias(id) {
        dispatch_action(aid);
        return true;
    }
    false
}

/// True when an action is registered (tests / diagnostics).
pub fn has_action(id: ActionId) -> bool {
    ACTIONS.with(|m| m.borrow().contains_key(&id))
}
