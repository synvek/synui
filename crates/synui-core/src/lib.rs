//! Core types for the SynUI framework: widgets, events, focus, and geometry.

mod action;
mod block_doc;
mod event;
mod focus;
mod geometry;
mod overlay;
mod render;
mod rich_text;
mod runtime;
mod state;
mod store;
mod style;
mod widget;

#[cfg(feature = "alloc_counter")]
pub mod alloc_counter;

pub use action::{
    action_id_for_alias, dispatch_action, dispatch_action_str, has_action, register_action,
    register_action_alias, unregister_action, ActionId,
};
pub use block_doc::{Block, BlockDoc, InlineBlock};
pub use event::*;
pub use focus::*;
pub use geometry::*;
pub use overlay::*;
pub use render::{
    IconPainter, ImageFit, ImagePainter, LottieJsonSource, PaintResources, TextAlign, TextPainter,
    TextStyleRun,
};
pub use rich_text::{RichDoc, TextSpan};
pub use runtime::{
    is_scheduled, land_frame_paint, peek_frame_work, schedule_apply_keys, schedule_binding_sync,
    schedule_frame_work, schedule_paint, schedule_rebuild, take_frame_work, take_scheduled,
    FrameWork,
};
pub use state::{
    begin_dep_scope, clear_pending_signal_deps, clear_registered_signals, dep_scope_active,
    deps_unchanged, dirty_intersects_deps, dirty_signal_count, end_dep_scope, force_full_rebuild,
    force_full_rebuild_enabled, is_signal_registered, live_signal_epoch, peek_dirty_signals,
    peek_pending_signal_deps, signal_schedule_stats, sync_registered_signals, take_dirty_signals,
    take_pending_signal_deps, take_signal_schedule_stats, Signal, SignalId,
};
pub use store::{
    alloc_widget_id, damaged_frame_budget, force_full_paint, force_full_paint_enabled,
    set_damaged_frame_budget, LayoutStats, OverlayPlacement, OverlayReparent, PaintMode,
    PaintStats, WidgetStore, DAMAGED_FRAME_BUDGET,
};
pub use style::*;
pub use widget::*;
