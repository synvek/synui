//! Shared frame host sync for winit and headless (Phase 33).

use synui_core::{TextMeasurer, WidgetStore};

use crate::overlay_sync::{layout_overlays, sync_overlays, sync_tooltip};
use crate::positioning::apply_sticky_and_fixed;
use crate::scroll::{apply_scroll_views, sync_virtual_mounts};
use crate::split_pane::sync_split_panes;

/// Before layout: keep overlay reparenting in sync with widget props.
pub fn sync_pre_layout(store: &mut WidgetStore) {
    sync_overlays(store);
}

/// After `LayoutEngine::compute`: split panes first (overwrite taffy flex), then
/// overlays / scroll / sticky so they see the real pane bounds.
pub fn sync_post_layout(store: &mut WidgetStore, text: &mut dyn TextMeasurer) {
    // Taffy lays SplitPane children as a flex column (~50/50). Fix primary/secondary
    // before scroll/virt so Detail is never observed at the mid-height flex slot.
    sync_split_panes(store, text);
    layout_overlays(store, text);
    apply_scroll_views(store);
    apply_sticky_and_fixed(store);
    // Tip must use post-scroll bounds; otherwise it sticks to the pre-scroll anchor.
    sync_tooltip(store);
}

/// After layout (or with prior bounds): split panes + virtual mounts before paint.
pub fn sync_pre_paint(store: &mut WidgetStore, text: &mut dyn TextMeasurer) {
    // Paint-only frames still need pane sync when ratio changed without layout.
    sync_split_panes(store, text);
    sync_virtual_mounts(store, text);
}
