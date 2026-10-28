//! Global frame scheduler for reactive Signals and paint/binding work (Phase 37/38).

use std::cell::Cell;

/// Typed frame work flags (Phase 37). Combine with bitwise OR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameWork(u8);

impl FrameWork {
    pub const EMPTY: Self = Self(0);
    /// Run root `view_fn` + apply (subject to RootSkip / section skip).
    pub const REBUILD_ROOT: Self = Self(1 << 0);
    /// Apply-only tick: reuse last View / refresh dirty sections (Phase 38).
    pub const APPLY_KEYS: Self = Self(1 << 1);
    /// Paint damage requested (ephemeral widget visuals).
    pub const PAINT_WIDGETS: Self = Self(1 << 2);
    /// Sync registered bindings (pull/push).
    pub const SYNC_BINDINGS: Self = Self(1 << 3);

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// True when root `view_fn` should be considered (may demote to Apply-only).
    pub const fn needs_rebuild_root(self) -> bool {
        self.intersects(Self::REBUILD_ROOT)
    }

    /// Apply-only without `REBUILD_ROOT` (explicit [`schedule_apply_keys`] or demoted).
    pub const fn needs_apply_only(self) -> bool {
        self.intersects(Self::APPLY_KEYS) && !self.intersects(Self::REBUILD_ROOT)
    }

    /// True when declarative rebuild **or** apply-only should be considered.
    pub const fn needs_rebuild(self) -> bool {
        self.intersects(Self::REBUILD_ROOT) || self.intersects(Self::APPLY_KEYS)
    }

    pub const fn needs_binding_sync(self) -> bool {
        self.intersects(Self::SYNC_BINDINGS) || self.needs_rebuild()
    }

    pub const fn needs_paint(self) -> bool {
        self.intersects(Self::PAINT_WIDGETS)
    }
}

impl std::ops::BitOr for FrameWork {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for FrameWork {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

thread_local! {
    static FRAME_WORK: Cell<u8> = const { Cell::new(0) };
}

/// Request typed work on the next frame (Phase 37).
pub fn schedule_frame_work(work: FrameWork) {
    FRAME_WORK.with(|c| c.set(c.get() | work.bits()));
}

/// Request a declarative rebuild on the next frame.
///
/// Equivalent to `schedule_frame_work(REBUILD_ROOT | SYNC_BINDINGS)`.
/// Hosts may demote to Apply-only when the Signal graph covers the dirty set
/// and a cached root View / section builders are available (Phase 38).
pub fn schedule_rebuild() {
    schedule_frame_work(FrameWork::REBUILD_ROOT | FrameWork::SYNC_BINDINGS);
}

/// Request an Apply-only frame (no root `view_fn`; Phase 38).
///
/// Equivalent to `schedule_frame_work(APPLY_KEYS | SYNC_BINDINGS)`. Prefer
/// section/`with_key` coverage so dirty regions can refresh without a full rebuild.
pub fn schedule_apply_keys() {
    schedule_frame_work(FrameWork::APPLY_KEYS | FrameWork::SYNC_BINDINGS);
}

/// Request paint-only work (ephemeral visuals; does not force `view_fn`).
///
/// Phase 39: hosts must call [`land_frame_paint`] after [`take_frame_work`] so the
/// flag lands on [`WidgetStore`] (no dual-source present path).
pub fn schedule_paint() {
    schedule_frame_work(FrameWork::PAINT_WIDGETS);
}

/// Request binding sync without necessarily rebuilding.
pub fn schedule_binding_sync() {
    schedule_frame_work(FrameWork::SYNC_BINDINGS);
}

/// Phase 39: land [`FrameWork::PAINT_WIDGETS`] onto the store so present runs.
///
/// Call after [`take_frame_work`]. Idempotent when the store is already paint-dirty
/// (e.g. [`crate::EventCtx::request_paint`] already marked a widget).
pub fn land_frame_paint(store: &mut crate::WidgetStore, work: FrameWork) {
    if work.needs_paint() && !store.needs_paint() {
        store.mark_paint_dirty();
    }
}

/// Drain pending [`FrameWork`] flags.
pub fn take_frame_work() -> FrameWork {
    FRAME_WORK.with(|c| FrameWork::from_bits(c.replace(0)))
}

/// Peek without clearing.
pub fn peek_frame_work() -> FrameWork {
    FRAME_WORK.with(|c| FrameWork::from_bits(c.get()))
}

/// Consume pending frame work; returns true if rebuild/apply was requested.
///
/// Prefer [`take_frame_work`] when paint/bind/apply-only flags matter. This is a
/// thin wrapper: `take_frame_work().needs_rebuild()` (Phase 38).
pub fn take_scheduled() -> bool {
    take_frame_work().needs_rebuild()
}

/// Peek whether a rebuild is scheduled (without clearing).
pub fn is_scheduled() -> bool {
    peek_frame_work().needs_rebuild()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_work_rebuild_via_schedule_rebuild() {
        let _ = take_frame_work();
        schedule_rebuild();
        let w = take_frame_work();
        assert!(w.needs_rebuild());
        assert!(w.needs_binding_sync());
        assert!(take_frame_work().is_empty());
    }

    #[test]
    fn take_scheduled_drains_all_flags() {
        let _ = take_frame_work();
        schedule_rebuild();
        schedule_paint();
        assert!(take_scheduled());
        assert!(take_frame_work().is_empty());
    }

    #[test]
    fn paint_only_does_not_schedule_rebuild() {
        let _ = take_frame_work();
        schedule_paint();
        assert!(!is_scheduled());
        let w = take_frame_work();
        assert!(w.needs_paint());
        assert!(!w.needs_rebuild());
    }

    #[test]
    fn land_frame_paint_marks_store() {
        use crate::store::WidgetStore;
        let _ = take_frame_work();
        schedule_paint();
        let w = take_frame_work();
        let mut store = WidgetStore::new();
        store.clear_paint_dirty();
        assert!(!store.needs_paint());
        land_frame_paint(&mut store, w);
        assert!(store.needs_paint());
    }

    #[test]
    fn schedule_apply_keys_without_rebuild_root() {
        let _ = take_frame_work();
        schedule_apply_keys();
        let w = take_frame_work();
        assert!(w.needs_apply_only());
        assert!(!w.needs_rebuild_root());
        assert!(w.needs_rebuild());
    }
}
