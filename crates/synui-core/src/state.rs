//! Lightweight reactive state primitive with Signal epochs (Phase 27).

use crate::runtime::{schedule_apply_keys, schedule_rebuild};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

type Subscriber<T> = Rc<RefCell<dyn FnMut(&T)>>;

/// Stable identity for a [`Signal`] (pointer of the shared value cell).
pub type SignalId = u64;

thread_local! {
    /// Stack of dependency scopes for [`crate::begin_dep_scope`].
    static DEP_STACK: RefCell<Vec<Vec<(SignalId, u64)>>> = RefCell::new(Vec::new());
    /// Latest published epoch per Signal id (for Memo validity checks).
    static LIVE_EPOCHS: RefCell<HashMap<SignalId, u64>> = RefCell::new(HashMap::new());
    /// Signals mutated since last [`take_dirty_signals`] (Phase 28 root skip).
    static DIRTY_SIGNALS: RefCell<HashSet<SignalId>> = RefCell::new(HashSet::new());
    /// Debug: force App to rebuild even when root deps miss dirty set.
    static FORCE_FULL_REBUILD: Cell<bool> = Cell::new(false);
    /// Phase 36: Signal reads awaiting attribution to the next [`View::with_key`].
    static PENDING_SIGNAL_DEPS: RefCell<Vec<(SignalId, u64)>> = RefCell::new(Vec::new());
    /// Phase 41: SignalIds registered on the host Signal graph (section / with_key).
    /// Synced from `ViewEngine` after apply so [`Signal::set`] can demote to Apply-only.
    static REGISTERED_SIGNAL_IDS: RefCell<HashSet<SignalId>> = RefCell::new(HashSet::new());
    /// Phase 42: counts of schedule demotions (registered→APPLY_KEYS vs unregistered→REBUILD).
    static SIGNAL_SCHEDULE_APPLY: Cell<u32> = Cell::new(0);
    static SIGNAL_SCHEDULE_REBUILD: Cell<u32> = Cell::new(0);
}

/// Push a dependency-collection scope (Phase 27 Memo / Phase 28 root).
pub fn begin_dep_scope() {
    DEP_STACK.with(|s| s.borrow_mut().push(Vec::new()));
}

/// Pop the current scope and return recorded `(SignalId, epoch)` pairs.
///
/// When the outermost scope ends, clears [`take_pending_signal_deps`] leftovers.
pub fn end_dep_scope() -> Vec<(SignalId, u64)> {
    DEP_STACK.with(|s| {
        let out = s.borrow_mut().pop().unwrap_or_default();
        if s.borrow().is_empty() {
            clear_pending_signal_deps();
        }
        out
    })
}

/// True when a Memo / dep scope is active.
pub fn dep_scope_active() -> bool {
    DEP_STACK.with(|s| !s.borrow().is_empty())
}

fn track_pending_read(id: SignalId, epoch: u64) {
    if !dep_scope_active() {
        return;
    }
    PENDING_SIGNAL_DEPS.with(|p| {
        let mut pending = p.borrow_mut();
        if let Some((_, e)) = pending.iter_mut().find(|(i, _)| *i == id) {
            *e = epoch;
        } else {
            pending.push((id, epoch));
        }
    });
}

/// Drain Signal reads not yet attributed via `View::with_key` (Phase 36).
pub fn take_pending_signal_deps() -> Vec<(SignalId, u64)> {
    PENDING_SIGNAL_DEPS.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

/// Peek pending Signal deps without draining (Phase 37 section merge mode).
pub fn peek_pending_signal_deps() -> Vec<(SignalId, u64)> {
    PENDING_SIGNAL_DEPS.with(|p| p.borrow().clone())
}

/// Drop unattributed pending Signal deps (end of build / tests).
pub fn clear_pending_signal_deps() {
    PENDING_SIGNAL_DEPS.with(|p| p.borrow_mut().clear());
}

fn track_signal_read(id: SignalId, epoch: u64) {
    LIVE_EPOCHS.with(|m| {
        m.borrow_mut().insert(id, epoch);
    });
    track_pending_read(id, epoch);
    DEP_STACK.with(|s| {
        if let Some(frame) = s.borrow_mut().last_mut() {
            if let Some((_, e)) = frame.iter_mut().find(|(i, _)| *i == id) {
                *e = epoch;
            } else {
                frame.push((id, epoch));
            }
        }
    });
}

fn publish_epoch(id: SignalId, epoch: u64) {
    LIVE_EPOCHS.with(|m| {
        m.borrow_mut().insert(id, epoch);
    });
}

fn mark_signal_dirty(id: SignalId) {
    DIRTY_SIGNALS.with(|d| {
        d.borrow_mut().insert(id);
    });
}

/// Drain Signal ids mutated since the last call (Phase 28).
pub fn take_dirty_signals() -> HashSet<SignalId> {
    DIRTY_SIGNALS.with(|d| std::mem::take(&mut *d.borrow_mut()))
}

/// Peek dirty Signal ids without draining (Phase 37 binding skip).
pub fn peek_dirty_signals() -> HashSet<SignalId> {
    DIRTY_SIGNALS.with(|d| d.borrow().clone())
}

/// Peek without clearing (tests).
pub fn dirty_signal_count() -> usize {
    DIRTY_SIGNALS.with(|d| d.borrow().len())
}

/// True when any id in `dirty` appears in `deps`.
pub fn dirty_intersects_deps(dirty: &HashSet<SignalId>, deps: &[(SignalId, u64)]) -> bool {
    deps.iter().any(|(id, _)| dirty.contains(id))
}

/// Debug: force next App rebuilds to ignore root-skip (Phase 28).
pub fn force_full_rebuild(on: bool) {
    FORCE_FULL_REBUILD.set(on);
}

pub fn force_full_rebuild_enabled() -> bool {
    FORCE_FULL_REBUILD.get()
}

/// Current published epoch for `id`, if the Signal has been read/written this process.
pub fn live_signal_epoch(id: SignalId) -> Option<u64> {
    LIVE_EPOCHS.with(|m| m.borrow().get(&id).copied())
}

/// True when every recorded dep still matches the live epoch table.
pub fn deps_unchanged(deps: &[(SignalId, u64)]) -> bool {
    deps.iter()
        .all(|(id, e)| live_signal_epoch(*id) == Some(*e))
}

/// Reactive value that notifies subscribers and schedules UI rebuild on change.
pub struct Signal<T> {
    value: Rc<RefCell<T>>,
    subscribers: Rc<RefCell<Vec<Subscriber<T>>>>,
    dirty: Rc<RefCell<bool>>,
    epoch: Rc<Cell<u64>>,
}

impl<T: Clone + PartialEq> Signal<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: Rc::new(RefCell::new(value)),
            subscribers: Rc::new(RefCell::new(Vec::new())),
            dirty: Rc::new(RefCell::new(false)),
            epoch: Rc::new(Cell::new(1)),
        }
    }

    /// Stable id for dependency tracking (shared across clones).
    pub fn id(&self) -> SignalId {
        Rc::as_ptr(&self.value) as SignalId
    }

    /// Monotonic version bumped on each successful mutation.
    pub fn epoch(&self) -> u64 {
        self.epoch.get()
    }

    pub fn get(&self) -> T {
        track_signal_read(self.id(), self.epoch.get());
        self.value.borrow().clone()
    }

    pub fn set(&self, value: T) {
        let changed = *self.value.borrow() != value;
        if changed {
            *self.value.borrow_mut() = value.clone();
            *self.dirty.borrow_mut() = true;
            self.epoch.set(self.epoch.get().wrapping_add(1));
            publish_epoch(self.id(), self.epoch.get());
            mark_signal_dirty(self.id());
            schedule_after_signal_change(self.id());
            for sub in self.subscribers.borrow_mut().iter_mut() {
                (sub.borrow_mut())(&value);
            }
        }
    }

    /// Update value without scheduling frame work.
    ///
    /// Used by focused text binding pull: the widget already painted the edit;
    /// writing the Signal must not trigger Rebuild/Apply or typing queues multi-second lag.
    /// Callers that need UI refresh after focus loss should use [`Self::set`].
    pub fn set_noschedule(&self, value: T) {
        let changed = *self.value.borrow() != value;
        if changed {
            *self.value.borrow_mut() = value.clone();
            self.epoch.set(self.epoch.get().wrapping_add(1));
            publish_epoch(self.id(), self.epoch.get());
            for sub in self.subscribers.borrow_mut().iter_mut() {
                (sub.borrow_mut())(&value);
            }
        }
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut v = self.value.borrow_mut();
        f(&mut v);
        let snapshot = v.clone();
        drop(v);
        *self.dirty.borrow_mut() = true;
        self.epoch.set(self.epoch.get().wrapping_add(1));
        publish_epoch(self.id(), self.epoch.get());
        mark_signal_dirty(self.id());
        schedule_after_signal_change(self.id());
        for sub in self.subscribers.borrow_mut().iter_mut() {
            (sub.borrow_mut())(&snapshot);
        }
    }

    pub fn subscribe(&self, f: impl FnMut(&T) + 'static) {
        self.subscribers.borrow_mut().push(Rc::new(RefCell::new(f)));
    }

    /// Returns and clears the dirty flag (for diagnostics / tests).
    pub fn take_dirty(&self) -> bool {
        let d = *self.dirty.borrow();
        *self.dirty.borrow_mut() = false;
        d
    }

    pub fn is_dirty(&self) -> bool {
        *self.dirty.borrow()
    }

    pub fn mark_dirty(&self) {
        *self.dirty.borrow_mut() = true;
        self.epoch.set(self.epoch.get().wrapping_add(1));
        publish_epoch(self.id(), self.epoch.get());
        mark_signal_dirty(self.id());
        schedule_after_signal_change(self.id());
    }
}

/// Phase 41: schedule Apply-only when `id` is on the host Signal graph; else Rebuild.
fn schedule_after_signal_change(id: SignalId) {
    if is_signal_registered(id) {
        SIGNAL_SCHEDULE_APPLY.set(SIGNAL_SCHEDULE_APPLY.get().wrapping_add(1));
        schedule_apply_keys();
    } else {
        SIGNAL_SCHEDULE_REBUILD.set(SIGNAL_SCHEDULE_REBUILD.get().wrapping_add(1));
        schedule_rebuild();
    }
}

/// Phase 42: `(apply_demotions, rebuild_demotions)` since last take.
///
/// Counts [`Signal::set`] / `update` / `mark_dirty` schedule choices:
/// registered → APPLY_KEYS; unregistered (incl. pre-sync first frame) → REBUILD_ROOT.
pub fn take_signal_schedule_stats() -> (u32, u32) {
    let a = SIGNAL_SCHEDULE_APPLY.replace(0);
    let r = SIGNAL_SCHEDULE_REBUILD.replace(0);
    (a, r)
}

/// Peek without clearing (diagnostics).
pub fn signal_schedule_stats() -> (u32, u32) {
    (SIGNAL_SCHEDULE_APPLY.get(), SIGNAL_SCHEDULE_REBUILD.get())
}

/// True when `id` was synced from the last host graph rebuild (section / `with_key`).
pub fn is_signal_registered(id: SignalId) -> bool {
    REGISTERED_SIGNAL_IDS.with(|s| s.borrow().contains(&id))
}

/// Replace the registered SignalId set (call after `SignalDepGraph::rebuild_from_keys`).
pub fn sync_registered_signals(ids: impl IntoIterator<Item = SignalId>) {
    REGISTERED_SIGNAL_IDS.with(|s| {
        let mut set = s.borrow_mut();
        set.clear();
        set.extend(ids);
    });
}

/// Clear registered SignalIds (structure tear-down / tests).
pub fn clear_registered_signals() {
    REGISTERED_SIGNAL_IDS.with(|s| s.borrow_mut().clear());
}

impl<T: Clone> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            value: Rc::clone(&self.value),
            subscribers: Rc::clone(&self.subscribers),
            dirty: Rc::clone(&self.dirty),
            epoch: Rc::clone(&self.epoch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::take_scheduled;

    #[test]
    fn signal_notifies_on_set() {
        let _ = take_scheduled();
        let sig = Signal::new(0i32);
        let seen = Rc::new(RefCell::new(0));
        let s = Rc::clone(&seen);
        sig.subscribe(move |v| *s.borrow_mut() = *v);
        sig.set(42);
        assert_eq!(*seen.borrow(), 42);
        assert!(take_scheduled());
    }

    #[test]
    fn signal_epoch_bumps_and_tracks_in_scope() {
        let sig = Signal::new(1i32);
        let e0 = sig.epoch();
        begin_dep_scope();
        let _ = sig.get();
        let deps = end_dep_scope();
        assert_eq!(deps, vec![(sig.id(), e0)]);
        sig.set(2);
        assert!(sig.epoch() > e0);
        assert!(take_dirty_signals().contains(&sig.id()));
    }

    #[test]
    fn dirty_set_independent_of_dep_scope() {
        let _ = take_dirty_signals();
        let a = Signal::new(0i32);
        let b = Signal::new(0i32);
        begin_dep_scope();
        let _ = a.get();
        let deps = end_dep_scope();
        b.set(1);
        let dirty = take_dirty_signals();
        assert!(dirty.contains(&b.id()));
        assert!(!dirty_intersects_deps(&dirty, &deps));
    }

    #[test]
    fn unregistered_set_schedules_rebuild_root() {
        use crate::runtime::{peek_frame_work, take_frame_work};
        clear_registered_signals();
        let _ = take_frame_work();
        let sig = Signal::new(0i32);
        sig.set(1);
        let w = peek_frame_work();
        assert!(w.needs_rebuild_root());
        assert!(!w.needs_apply_only());
        let _ = take_frame_work();
    }

    #[test]
    fn registered_set_schedules_apply_keys() {
        use crate::runtime::{peek_frame_work, take_frame_work};
        clear_registered_signals();
        let _ = take_frame_work();
        let _ = take_signal_schedule_stats();
        let sig = Signal::new(0i32);
        sync_registered_signals([sig.id()]);
        assert!(is_signal_registered(sig.id()));
        sig.set(1);
        let w = peek_frame_work();
        assert!(
            w.needs_apply_only(),
            "Phase 41: graph-covered Signal::set must schedule APPLY_KEYS"
        );
        assert!(!w.needs_rebuild_root());
        let (apply, rebuild) = take_signal_schedule_stats();
        assert_eq!(apply, 1);
        assert_eq!(rebuild, 0);
        let _ = take_frame_work();
        clear_registered_signals();
    }

    #[test]
    fn set_noschedule_updates_without_frame_work() {
        use crate::runtime::{peek_frame_work, take_frame_work};
        clear_registered_signals();
        let _ = take_frame_work();
        let _ = take_signal_schedule_stats();
        let sig = Signal::new(String::new());
        sig.set_noschedule("hello".into());
        assert_eq!(sig.get(), "hello");
        let w = peek_frame_work();
        assert!(
            !w.needs_rebuild() && !w.needs_apply_only(),
            "focused binding pull must not schedule Rebuild/Apply"
        );
        let (apply, rebuild) = take_signal_schedule_stats();
        assert_eq!(apply, 0);
        assert_eq!(rebuild, 0);
        // Contrasting: set() still schedules.
        sig.set("world".into());
        assert!(peek_frame_work().needs_rebuild_root());
        let _ = take_frame_work();
        clear_registered_signals();
    }

    #[test]
    fn first_set_before_registered_sync_schedules_rebuild() {
        use crate::runtime::{peek_frame_work, take_frame_work};
        clear_registered_signals();
        let _ = take_frame_work();
        let _ = take_signal_schedule_stats();
        let sig = Signal::new(0i32);
        assert!(!is_signal_registered(sig.id()));
        sig.set(1);
        let w = peek_frame_work();
        assert!(w.needs_rebuild_root());
        assert!(!w.needs_apply_only());
        let (apply, rebuild) = take_signal_schedule_stats();
        assert_eq!(apply, 0);
        assert_eq!(rebuild, 1);
        sync_registered_signals([sig.id()]);
        let _ = take_frame_work();
        sig.set(2);
        assert!(peek_frame_work().needs_apply_only());
        let (apply, rebuild) = take_signal_schedule_stats();
        assert_eq!(apply, 1);
        assert_eq!(rebuild, 0);
        let _ = take_frame_work();
        clear_registered_signals();
    }
}
