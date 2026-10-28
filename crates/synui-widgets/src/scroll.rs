//! Scrollable viewport container and shared vertical scrollbar helpers.

use std::collections::{HashMap, HashSet, VecDeque};

use synui_core::alloc_widget_id;
use synui_core::{
    Constraints, Dimension, Display, Event, EventCtx, EventResult, FlexDirection, JustifyContent,
    LayoutStyle, MeasureCtx, PaintCtx, Point, PointerButton, Rect, Size, TextMeasurer, Widget,
    WidgetId, WidgetStore,
};
use synui_render::paint_simple_rect;
use vello::peniko::Color;
use vello::Scene;

use crate::basic::Label;
use crate::virtual_mount::{
    collect_all_widget_ids, collect_widget_ids, reconcile_mount_window,
    restore_host_focus_if_needed, scroll_at_edge, MountPoolPolicy, VSCROLL_POOL_CAP,
};
use crate::virtual_viewport::{layout_row_subtree, VirtualViewport};

const SCROLLBAR_WIDTH: f64 = 8.0;
const SCROLLBAR_MIN_THUMB: f64 = 24.0;
const SCROLLBAR_PAD: f64 = 2.0;

/// Right gutter reserved so the track stays hittable above children (ScrollView).
pub fn v_scrollbar_gutter(bounds: Rect, content_height: f64) -> f64 {
    if content_height > bounds.height() + 0.5 {
        SCROLLBAR_WIDTH + SCROLLBAR_PAD * 2.0
    } else {
        0.0
    }
}

/// Geometry for a vertical scrollbar (None when content fits).
#[derive(Clone, Copy, Debug)]
pub struct VScrollbarGeom {
    pub track: Rect,
    pub thumb: Rect,
    pub max_scroll: f64,
    pub thumb_travel: f64,
}

/// Compute track/thumb for the right-edge vertical scrollbar.
pub fn v_scrollbar_geom(
    bounds: Rect,
    scroll_y: f64,
    content_height: f64,
) -> Option<VScrollbarGeom> {
    let view_h = bounds.height();
    if content_height <= view_h + 0.5 || view_h <= 1.0 {
        return None;
    }
    let track = Rect::new(
        bounds.x1 - SCROLLBAR_WIDTH - SCROLLBAR_PAD,
        bounds.y0 + SCROLLBAR_PAD,
        bounds.x1 - SCROLLBAR_PAD,
        bounds.y1 - SCROLLBAR_PAD,
    );
    if track.height() < 4.0 {
        return None;
    }
    let ratio = (view_h / content_height).clamp(0.05, 1.0);
    let thumb_h = (track.height() * ratio)
        .max(SCROLLBAR_MIN_THUMB)
        .min(track.height());
    let max_scroll = (content_height - view_h).max(0.0);
    let t = if max_scroll > 0.0 {
        (scroll_y / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_travel = (track.height() - thumb_h).max(0.0);
    let thumb_y = track.y0 + t * thumb_travel;
    let thumb = Rect::new(track.x0, thumb_y, track.x1, thumb_y + thumb_h);
    Some(VScrollbarGeom {
        track,
        thumb,
        max_scroll,
        thumb_travel,
    })
}

/// Paint a vertical scrollbar on the right edge of `bounds` when content overflows.
pub fn paint_v_scrollbar(
    scene: &mut Scene,
    bounds: Rect,
    scroll_y: f64,
    content_height: f64,
    track: Color,
    thumb: Color,
) {
    let Some(g) = v_scrollbar_geom(bounds, scroll_y, content_height) else {
        return;
    };
    paint_simple_rect(scene, g.track, track, 4.0);
    paint_simple_rect(scene, g.thumb, thumb, 4.0);
}

/// Interactive vertical scrollbar state (thumb drag + track click).
#[derive(Debug, Default, Clone)]
pub struct VScrollbarState {
    dragging: bool,
    /// Pointer Y relative to thumb top when drag started.
    drag_grab_y: f64,
}

impl VScrollbarState {
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Map a thumb-top Y to scroll offset.
    fn scroll_from_thumb_top(geom: &VScrollbarGeom, thumb_top: f64) -> f64 {
        if geom.thumb_travel <= 0.0 || geom.max_scroll <= 0.0 {
            return 0.0;
        }
        let t = ((thumb_top - geom.track.y0) / geom.thumb_travel).clamp(0.0, 1.0);
        t * geom.max_scroll
    }

    /// Handle primary pointer down. Returns `Some(new_scroll)` if consumed.
    pub fn pointer_down(
        &mut self,
        pos: Point,
        bounds: Rect,
        scroll_y: f64,
        content_height: f64,
    ) -> Option<f64> {
        let geom = v_scrollbar_geom(bounds, scroll_y, content_height)?;
        if !geom.track.contains(pos) {
            return None;
        }
        if geom.thumb.contains(pos) {
            self.dragging = true;
            self.drag_grab_y = pos.y - geom.thumb.y0;
            Some(scroll_y)
        } else {
            // Track click: jump so thumb centers on the click.
            self.dragging = true;
            self.drag_grab_y = geom.thumb.height() * 0.5;
            let thumb_top = pos.y - self.drag_grab_y;
            Some(Self::scroll_from_thumb_top(&geom, thumb_top))
        }
    }

    /// Handle pointer move while dragging. Returns `Some(new_scroll)` if dragging.
    pub fn pointer_move(&mut self, pos: Point, bounds: Rect, content_height: f64) -> Option<f64> {
        if !self.dragging {
            return None;
        }
        let geom = v_scrollbar_geom(bounds, 0.0, content_height)?;
        let thumb_top = pos.y - self.drag_grab_y;
        Some(Self::scroll_from_thumb_top(&geom, thumb_top))
    }

    /// Handle pointer up. Returns true if a drag ended.
    pub fn pointer_up(&mut self) -> bool {
        if self.dragging {
            self.dragging = false;
            true
        } else {
            false
        }
    }

    /// True if `pos` is over the scrollbar track (when visible).
    pub fn hit(bounds: Rect, scroll_y: f64, content_height: f64, pos: Point) -> bool {
        v_scrollbar_geom(bounds, scroll_y, content_height)
            .map(|g| g.track.contains(pos))
            .unwrap_or(false)
    }
}

const TRANSPARENT_DEFAULT_EXTENT: f64 = 32.0;

/// Clipped viewport that scrolls its children vertically.
///
/// Opt-in [`ScrollView::with_virtual`] mounts only visible slots (Phase 20 MVP).
/// [`ScrollView::enable_transparent_virt`] virtualizes a flat column of declared
/// children without a slot builder (Phase 23).
/// Nested virt: when at a scroll edge, wheel events chain to ancestors (Phase 24).
pub struct ScrollView {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    /// Vertical scroll offset in px (content moves up as this increases).
    pub offset_y: f64,
    /// Content height in layout space.
    pub content_height: f64,
    layout_style: LayoutStyle,
    /// Base Y positions after Taffy layout (before scroll).
    pub base_y: Vec<f64>,
    scrollbar: VScrollbarState,
    /// Virtual slot mode (false = ordinary full-mount scroll).
    virtual_mode: bool,
    /// Flat-column transparent virt: `logical_children` are slots (Phase 23).
    transparent_virt: bool,
    /// When true (default), wheel at edge does not consume — parent may scroll.
    pub scroll_chaining: bool,
    /// All declared children in order when [`Self::transparent_virt`] is on.
    logical_children: Vec<WidgetId>,
    pub slot_count: usize,
    pub slot_extent: f64,
    vp: VirtualViewport,
    mounted: Vec<(usize, WidgetId)>,
    pool: HashMap<usize, WidgetId>,
    pool_order: VecDeque<usize>,
    slot_builder: Option<Box<dyn Fn(usize) -> Box<dyn synui_core::ErasedWidget>>>,
    /// Explicit per-slot extent (Phase 21).
    extent_fn: Option<Box<dyn Fn(usize) -> f64>>,
    /// Measure mounted slots to fill height cache (Phase 21).
    pub measure_heights: bool,
    /// Optional viewport fill; `None` uses theme surface.
    pub background: Option<Color>,
}

impl ScrollView {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            offset_y: 0.0,
            content_height: 0.0,
            layout_style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: synui_core::AlignItems::Stretch,
                width: Dimension::Fill,
                height: Dimension::Fill,
                // Allow the scroll viewport to shrink inside a flex parent instead of
                // growing to content height (classic flex + overflow min-size trap).
                min_height: Dimension::Length(0.0),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                gap: 4.0,
                padding: [0.0; 4],
                scroll_container: true,
                ..LayoutStyle::default()
            },
            base_y: Vec::new(),
            scrollbar: VScrollbarState::default(),
            virtual_mode: false,
            transparent_virt: false,
            scroll_chaining: true,
            logical_children: Vec::new(),
            slot_count: 0,
            slot_extent: TRANSPARENT_DEFAULT_EXTENT,
            vp: VirtualViewport::new(0, TRANSPARENT_DEFAULT_EXTENT),
            mounted: Vec::new(),
            pool: HashMap::new(),
            pool_order: VecDeque::new(),
            slot_builder: None,
            extent_fn: None,
            measure_heights: false,
            background: None,
        }
    }

    pub fn with_children(mut self, children: Vec<WidgetId>) -> Self {
        if self.transparent_virt {
            self.set_logical_children(children);
        } else {
            self.child_ids = children;
        }
        self
    }

    /// Viewport background fill. When unset, uses theme surface.
    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }

    /// Replace direct children (non-transparent full-mount scroll).
    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.transparent_virt = false;
        if self.slot_builder.is_none() && self.slot_count == 0 {
            self.virtual_mode = false;
        }
        self.logical_children.clear();
        self.mounted.clear();
        self.child_ids = children;
    }

    /// Treat direct children as a flat virtualized column (Phase 23).
    ///
    /// Declared widgets stay alive in the store; only the visible window is
    /// parented under this ScrollView each frame. Not for nested flex/grid trees.
    pub fn enable_transparent_virt(mut self, on: bool) -> Self {
        self.set_transparent_virt(on);
        self
    }

    pub fn set_transparent_virt(&mut self, on: bool) {
        self.transparent_virt = on;
        if on {
            self.virtual_mode = true;
            self.slot_builder = None;
            self.measure_heights = true;
            crate::virtual_mount::register_virt_host(
                self.id,
                crate::virtual_mount::VirtHostKind::Scroll,
            );
            if !self.child_ids.is_empty() && self.logical_children.is_empty() {
                let kids = std::mem::take(&mut self.child_ids);
                self.set_logical_children(kids);
            } else {
                self.resync_transparent_geometry();
            }
        } else if self.slot_builder.is_none() && self.slot_count == 0 {
            self.virtual_mode = false;
            if !self.logical_children.is_empty() {
                self.child_ids = std::mem::take(&mut self.logical_children);
            }
        }
    }

    pub fn is_transparent_virt(&self) -> bool {
        self.transparent_virt
    }

    pub fn set_logical_children(&mut self, children: Vec<WidgetId>) {
        self.logical_children = children;
        self.resync_transparent_geometry();
        // Defer parenting to sync — avoid laying out the full column.
        self.child_ids.clear();
        self.mounted.clear();
    }

    pub fn logical_children(&self) -> &[WidgetId] {
        &self.logical_children
    }

    fn resync_transparent_geometry(&mut self) {
        if !self.transparent_virt {
            return;
        }
        let n = self.logical_children.len();
        self.slot_count = n;
        self.slot_extent = TRANSPARENT_DEFAULT_EXTENT;
        self.vp.item_count = n;
        self.vp.item_extent = self.slot_extent;
        self.measure_heights = true;
        self.vp.ensure_measured_slots();
        self.content_height = self.vp.content_height();
    }

    /// Opt-in virtual scroll: fixed slot height, builder-mounted windows.
    pub fn with_virtual(mut self, slot_count: usize, slot_extent: f64) -> Self {
        self.set_virtual(slot_count, slot_extent);
        self
    }

    pub fn set_virtual(&mut self, slot_count: usize, slot_extent: f64) {
        self.virtual_mode = true;
        self.slot_count = slot_count;
        self.slot_extent = slot_extent.max(1.0);
        self.vp = VirtualViewport::new(slot_count, self.slot_extent);
        self.content_height = self.vp.content_height();
        crate::virtual_mount::register_virt_host(
            self.id,
            crate::virtual_mount::VirtHostKind::Scroll,
        );
    }

    pub fn is_virtual(&self) -> bool {
        self.virtual_mode
    }

    pub fn set_slot_builder(
        &mut self,
        f: impl Fn(usize) -> Box<dyn synui_core::ErasedWidget> + 'static,
    ) {
        self.slot_builder = Some(Box::new(f));
        self.virtual_mode = true;
        crate::virtual_mount::register_virt_host(
            self.id,
            crate::virtual_mount::VirtHostKind::Scroll,
        );
    }

    pub fn set_slot_extent_fn(&mut self, f: impl Fn(usize) -> f64 + 'static) {
        self.extent_fn = Some(Box::new(f));
        self.measure_heights = false;
        self.virtual_mode = true;
        self.rebuild_height_cache();
    }

    pub fn set_slot_heights(&mut self, heights: Vec<f64>) {
        self.slot_count = heights.len();
        self.extent_fn = None;
        self.measure_heights = false;
        self.virtual_mode = true;
        self.vp.set_heights(heights);
        self.content_height = self.vp.content_height();
    }

    pub fn enable_measure_heights(&mut self, on: bool) {
        self.measure_heights = on;
        if on {
            self.extent_fn = None;
            self.vp.ensure_measured_slots();
        }
        self.virtual_mode = true;
    }

    pub fn rebuild_height_cache(&mut self) {
        self.vp.item_count = self.slot_count;
        self.vp.item_extent = self.slot_extent;
        if let Some(f) = self.extent_fn.as_ref() {
            let mut heights = Vec::with_capacity(self.slot_count);
            for i in 0..self.slot_count {
                heights.push(f(i).max(1.0));
            }
            self.vp.set_heights(heights);
        } else if self.measure_heights {
            self.vp.ensure_measured_slots();
        } else {
            self.vp.clear_variable_heights();
            self.vp.item_extent = self.slot_extent;
        }
        self.content_height = self.vp.content_height();
    }

    pub fn mounted_count(&self) -> usize {
        self.mounted.len()
    }

    pub fn max_offset(&self) -> f64 {
        (self.content_height - self.bounds.height()).max(0.0)
    }

    pub fn clamp_offset(&mut self) {
        let max = self.max_offset();
        self.offset_y = self.offset_y.clamp(0.0, max);
        crate::virtual_mount::mark_virt_host_dirty(self.id);
    }
}

impl Default for ScrollView {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ScrollView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn layout_style(&self) -> &LayoutStyle {
        &self.layout_style
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn clip_bounds(&self) -> Option<Rect> {
        Some(self.bounds)
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(
            constraints.max_width.min(800.0) as f64,
            constraints.max_height.min(600.0) as f64,
        )
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let bg = self.background.unwrap_or(ctx.theme.surface);
        paint_simple_rect(scene, self.bounds, bg, 0.0);
    }

    fn paint_overlay(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let mut track = ctx.theme.outline;
        track.components[3] = 0.25;
        let mut thumb = ctx.theme.on_surface;
        thumb.components[3] = if self.scrollbar.is_dragging() {
            0.65
        } else {
            0.45
        };
        paint_v_scrollbar(
            scene,
            self.bounds,
            self.offset_y,
            self.content_height,
            track,
            thumb,
        );
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                ..
            } => {
                if let Some(y) = self.scrollbar.pointer_down(
                    *position,
                    self.bounds,
                    self.offset_y,
                    self.content_height,
                ) {
                    self.offset_y = y;
                    self.clamp_offset();
                    ctx.grab_pointer();
                    ctx.stop_propagation();
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::PointerMove { position, .. } => {
                if let Some(y) =
                    self.scrollbar
                        .pointer_move(*position, self.bounds, self.content_height)
                {
                    self.offset_y = y;
                    self.clamp_offset();
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::PointerUp { .. } => {
                if self.scrollbar.pointer_up() {
                    ctx.release_pointer();
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::Scroll {
                position, delta_y, ..
            } => {
                if !self.bounds.contains(*position) {
                    return EventResult::IGNORED;
                }
                let max = self.max_offset();
                if self.scroll_chaining && scroll_at_edge(self.offset_y, max, *delta_y as f64) {
                    // At edge — let ancestors consume (nested virt chaining).
                    return EventResult::IGNORED;
                }
                self.offset_y -= *delta_y as f64;
                self.clamp_offset();
                ctx.stop_propagation();
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn hit_test(&self, point: Point) -> bool {
        self.bounds.contains(point)
    }

    fn hit_test_prefer_self(&self, point: Point) -> bool {
        VScrollbarState::hit(self.bounds, self.offset_y, self.content_height, point)
            || self.scrollbar.is_dragging()
    }
}

/// After layout (or scroll), sync ScrollView child translations from `base_y` / offset.
pub fn apply_scroll_views(store: &mut WidgetStore) {
    let ids: Vec<WidgetId> = store
        .root()
        .into_iter()
        .flat_map(|r| collect_ids(store, r))
        .collect();
    for id in ids {
        if !store
            .get(id)
            .map(|w| w.as_any().is::<ScrollView>())
            .unwrap_or(false)
        {
            continue;
        }
        let is_virtual = store
            .get(id)
            .and_then(|w| w.as_any().downcast_ref::<ScrollView>())
            .map(|sv| sv.is_virtual())
            .unwrap_or(false);
        if is_virtual {
            if let Some(w) = store.get_mut(id) {
                if let Some(sv) = w.as_any_mut().downcast_mut::<ScrollView>() {
                    sv.rebuild_height_cache();
                    sv.vp.viewport_height = sv.bounds.height();
                    sv.vp.scroll_y = sv.offset_y;
                    sv.content_height = sv.vp.content_height();
                    sv.clamp_offset();
                    sv.vp.scroll_y = sv.offset_y;
                }
            }
            continue;
        }
        let (children, offset, bounds, had_base) = {
            let w = store.get(id).unwrap();
            let sv = w.as_any().downcast_ref::<ScrollView>().unwrap();
            (
                sv.children().to_vec(),
                sv.offset_y,
                sv.bounds(),
                sv.base_y.len() == sv.children().len() && !sv.children().is_empty(),
            )
        };

        let base_y: Vec<f64> = if had_base {
            store
                .get(id)
                .unwrap()
                .as_any()
                .downcast_ref::<ScrollView>()
                .unwrap()
                .base_y
                .clone()
        } else {
            children
                .iter()
                .map(|&cid| {
                    store
                        .get(cid)
                        .map(|c| c.bounds().y0 + offset)
                        .unwrap_or(bounds.y0)
                })
                .collect()
        };

        let layout_base: Vec<f64> = if !had_base {
            children
                .iter()
                .map(|&cid| store.get(cid).map(|c| c.bounds().y0).unwrap_or(bounds.y0))
                .collect()
        } else {
            base_y.clone()
        };

        let mut max_bottom = bounds.y0;
        for (i, &cid) in children.iter().enumerate() {
            if let Some(w) = store.get(cid) {
                let h = w.bounds().height();
                max_bottom = max_bottom.max(layout_base[i] + h);
            }
        }
        let content_height = (max_bottom - bounds.y0).max(0.0);
        let max_off = (content_height - bounds.height()).max(0.0);
        let offset = offset.clamp(0.0, max_off);
        let gutter = v_scrollbar_gutter(bounds, content_height);
        let content_right = bounds.x1 - gutter;

        for (i, &cid) in children.iter().enumerate() {
            let (old_y, h) = store
                .get(cid)
                .map(|w| {
                    let b = w.bounds();
                    (b.y0, b.height())
                })
                .unwrap_or((layout_base[i], 0.0));
            let y0 = layout_base[i] - offset;
            let dy = y0 - old_y;
            if let Some(w) = store.get_mut(cid) {
                let b = w.bounds();
                w.set_bounds(Rect::new(b.x0, y0, content_right.max(b.x0), y0 + h));
            }
            // Bounds are absolute: shift the whole content subtree, not only the
            // direct child (which is often a StyledBox wrapper from style_css).
            if dy.abs() > 0.01 {
                let kids: Vec<WidgetId> = store.children_of(cid).to_vec();
                for kid in kids {
                    shift_subtree_y(store, kid, dy);
                }
            }
            // Taffy laid out the full width; clamp so content doesn't sit under the gutter.
            clamp_subtree_right(store, cid, content_right);
        }
        if let Some(w) = store.get_mut(id) {
            if let Some(sv) = w.as_any_mut().downcast_mut::<ScrollView>() {
                sv.base_y = layout_base;
                sv.content_height = content_height;
                sv.offset_y = offset;
            }
        }
    }
}

/// Translate a widget and all descendants by `dy` (absolute layout coords).
fn shift_subtree_y(store: &mut WidgetStore, id: WidgetId, dy: f64) {
    let kids: Vec<WidgetId> = store.children_of(id).to_vec();
    if let Some(w) = store.get_mut(id) {
        let b = w.bounds();
        w.set_bounds(Rect::new(b.x0, b.y0 + dy, b.x1, b.y1 + dy));
    }
    for kid in kids {
        shift_subtree_y(store, kid, dy);
    }
}

/// Shrink any descendant whose right edge extends past `max_x1`.
fn clamp_subtree_right(store: &mut WidgetStore, id: WidgetId, max_x1: f64) {
    let kids: Vec<WidgetId> = store.children_of(id).to_vec();
    if let Some(w) = store.get_mut(id) {
        let b = w.bounds();
        if b.x1 > max_x1 + 0.5 {
            w.set_bounds(Rect::new(b.x0, b.y0, max_x1.max(b.x0), b.y1));
        }
    }
    for kid in kids {
        clamp_subtree_right(store, kid, max_x1);
    }
}

/// Mount / recycle virtual ScrollView slots (after sticky/fixed).
/// Slot-builder mode pins slot 0; transparent-virt mode does not.
pub fn sync_virtual_scroll_views(store: &mut WidgetStore, text: &mut dyn TextMeasurer) {
    let ids = collect_all_widget_ids(store);
    for id in ids {
        sync_virtual_scroll_view(store, text, id);
    }
}

/// Sync a single virtual ScrollView (Phase 26).
pub fn sync_virtual_scroll_view(
    store: &mut WidgetStore,
    text: &mut dyn TextMeasurer,
    id: WidgetId,
) {
    let needs = store
        .get(id)
        .and_then(|w| w.as_any().downcast_ref::<ScrollView>())
        .map(|sv| sv.is_virtual())
        .unwrap_or(false);
    if !needs {
        return;
    }
    {
        let sv = store
            .get_mut(id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<ScrollView>()
            .unwrap();
        if sv.transparent_virt {
            sv.resync_transparent_geometry();
        }
        sv.rebuild_height_cache();
        sv.vp.viewport_height = sv.bounds.height();
        sv.vp.scroll_y = sv.offset_y;
        sv.vp.clamp_scroll();
        sv.offset_y = sv.vp.scroll_y;
    }
    let (
        start,
        end,
        bounds,
        scroll_y,
        slot_extent,
        slot_count,
        measure_heights,
        transparent,
        logical,
        wants_focus,
    ) = {
        let w = store.get(id).unwrap();
        let sv = w.as_any().downcast_ref::<ScrollView>().unwrap();
        let (a, b) = sv.vp.visible_range();
        (
            a,
            b,
            sv.bounds(),
            sv.offset_y,
            sv.slot_extent,
            sv.slot_count,
            sv.measure_heights,
            sv.transparent_virt,
            sv.logical_children.clone(),
            false, // ScrollView itself is not keyboard-focused host for rows
        )
    };
    let _ = wants_focus;
    let mut desired: Vec<usize> = (start..end).collect();
    // Sticky MVP for slot-builder virt only: always mount slot 0.
    if !transparent && slot_count > 0 && !desired.contains(&0) {
        desired.insert(0, 0);
    }
    if transparent {
        desired.retain(|&i| i < logical.len());
    }
    desired.sort_unstable();
    desired.dedup();

    let policy = if transparent {
        MountPoolPolicy::DetachOnly
    } else {
        MountPoolPolicy::RecycleDestroy
    };

    {
        let (mut mounted, mut pool, mut pool_order) = {
            let sv = store
                .get_mut(id)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<ScrollView>()
                .unwrap();
            (
                std::mem::take(&mut sv.mounted),
                std::mem::take(&mut sv.pool),
                std::mem::take(&mut sv.pool_order),
            )
        };
        let desired_set: HashSet<usize> = desired.iter().copied().collect();
        let keep_keys: HashSet<usize> = mounted
            .iter()
            .filter(|(k, _)| desired_set.contains(k))
            .map(|(k, _)| *k)
            .collect();
        let mut prebuilt: HashMap<usize, WidgetId> = HashMap::new();
        for &idx in &desired {
            if keep_keys.contains(&idx) {
                continue;
            }
            if !transparent && pool.contains_key(&idx) {
                continue;
            }
            if transparent {
                if let Some(wid) = logical.get(idx).copied() {
                    prebuilt.insert(idx, wid);
                }
            } else {
                let widget: Box<dyn synui_core::ErasedWidget> = {
                    let w = store.get(id).unwrap();
                    let sv = w.as_any().downcast_ref::<ScrollView>().unwrap();
                    if let Some(builder) = sv.slot_builder.as_ref() {
                        builder(idx)
                    } else {
                        Box::new(Label::new(format!("Slot {idx}")))
                    }
                };
                prebuilt.insert(idx, store.insert_erased(widget));
            }
        }
        let remove_ids = reconcile_mount_window(
            &mut mounted,
            &mut pool,
            &mut pool_order,
            VSCROLL_POOL_CAP,
            &desired,
            policy,
            &mut |&idx| prebuilt.remove(&idx).expect("prebuilt slot"),
        );
        for wid in remove_ids {
            store.remove(wid);
        }
        if let Some(w) = store.get_mut(id) {
            if let Some(sv) = w.as_any_mut().downcast_mut::<ScrollView>() {
                sv.mounted = mounted;
                sv.pool = pool;
                sv.pool_order = pool_order;
            }
        }
    }

    let keep = store
        .get(id)
        .and_then(|w| {
            w.as_any()
                .downcast_ref::<ScrollView>()
                .map(|sv| sv.mounted.clone())
        })
        .unwrap_or_default();
    let child_ids: Vec<WidgetId> = keep.iter().map(|(_, id)| *id).collect();
    store.set_children(id, child_ids.clone());

    let content_h = store
        .get(id)
        .and_then(|w| {
            w.as_any()
                .downcast_ref::<ScrollView>()
                .map(|sv| sv.vp.content_height())
        })
        .unwrap_or(slot_count as f64 * slot_extent);
    let gutter = v_scrollbar_gutter(bounds, content_h);
    let row_width = (bounds.width() - gutter).max(40.0);
    for &(idx, wid) in &keep {
        let (y0, extent) = {
            let w = store.get(id).unwrap();
            let sv = w.as_any().downcast_ref::<ScrollView>().unwrap();
            let extent = sv.vp.row_extent(idx);
            let y0 = if !transparent && idx == 0 {
                bounds.y0
            } else {
                bounds.y0 + sv.vp.offset_of(idx) - scroll_y
            };
            (y0, extent)
        };
        let avail_h = if measure_heights { 10_000.0 } else { extent };
        layout_row_subtree(
            store,
            text,
            wid,
            Point::new(bounds.x0, y0),
            Size::new(row_width, avail_h),
        );
        if measure_heights {
            let measured = store
                .get(wid)
                .map(|w| w.bounds().height().max(1.0))
                .unwrap_or(extent);
            if let Some(w) = store.get_mut(id) {
                if let Some(sv) = w.as_any_mut().downcast_mut::<ScrollView>() {
                    sv.vp.set_measured_height(idx, measured);
                }
            }
            let h2 = store
                .get(id)
                .and_then(|w| {
                    w.as_any()
                        .downcast_ref::<ScrollView>()
                        .map(|sv| sv.vp.row_extent(idx))
                })
                .unwrap_or(measured);
            let y1 = if !transparent && idx == 0 {
                bounds.y0
            } else {
                bounds.y0
                    + store
                        .get(id)
                        .and_then(|w| {
                            w.as_any()
                                .downcast_ref::<ScrollView>()
                                .map(|sv| sv.vp.offset_of(idx) - sv.offset_y)
                        })
                        .unwrap_or(y0 - bounds.y0)
            };
            layout_row_subtree(
                store,
                text,
                wid,
                Point::new(bounds.x0, y1),
                Size::new(row_width, h2),
            );
        }
    }

    if let Some(w) = store.get_mut(id) {
        if let Some(sv) = w.as_any_mut().downcast_mut::<ScrollView>() {
            sv.child_ids = child_ids;
            sv.vp.viewport_height = bounds.height();
            sv.vp.scroll_y = sv.offset_y;
            sv.content_height = sv.vp.content_height();
            sv.clamp_offset();
            sv.vp.scroll_y = sv.offset_y;
        }
    }
}

/// Phase 24/26: sync dirty virt hosts (registry); rediscover on structure change.
pub fn sync_virtual_mounts(store: &mut WidgetStore, text: &mut dyn TextMeasurer) {
    let targets = crate::virtual_mount::take_virt_sync_targets(store);
    for (id, kind) in targets {
        match kind {
            crate::virtual_mount::VirtHostKind::List => {
                crate::list_view::sync_list_view(store, text, id);
            }
            crate::virtual_mount::VirtHostKind::Tree => {
                crate::tree_view::sync_tree_view(store, text, id);
            }
            crate::virtual_mount::VirtHostKind::Table => {
                crate::data_table::sync_data_table(store, text, id);
            }
            crate::virtual_mount::VirtHostKind::Scroll => {
                sync_virtual_scroll_view(store, text, id);
            }
        }
        restore_host_focus_if_needed_for(store, id);
    }
}

/// Walk parents from `id` looking for a [`ScrollView`].
pub fn find_scroll_ancestor(store: &WidgetStore, mut id: WidgetId) -> Option<WidgetId> {
    loop {
        if store.get(id).is_some_and(|w| w.as_any().is::<ScrollView>()) {
            return Some(id);
        }
        match store.parent_of(id) {
            Some(p) => id = p,
            None => return None,
        }
    }
}

/// Phase 33: after a handled scroll, mark the ScrollView host (viewport bounds)
/// dirty instead of escalating Full paint.
pub fn mark_scroll_paint_dirty(store: &mut WidgetStore, point: Point) {
    let hit = store.hit_test(point);
    let scroll_id = hit.and_then(|id| find_scroll_ancestor(store, id));
    if let Some(id) = scroll_id {
        let bounds = store.get(id).map(|w| w.bounds());
        store.mark_paint_dirty_widget(id);
        if let Some(b) = bounds {
            if b.width() > 0.0 && b.height() > 0.0 {
                store.mark_paint_dirty_rect(b);
            }
        }
    } else if let Some(hit) = hit {
        store.mark_paint_dirty_widget(hit);
    } else {
        store.mark_paint_dirty();
    }
}

fn restore_host_focus_if_needed_for(store: &mut WidgetStore, id: WidgetId) {
    if let Some(w) = store.get(id) {
        if let Some(lv) = w.as_any().downcast_ref::<crate::list_view::ListView>() {
            let wants = lv.is_focused_host();
            restore_host_focus_if_needed(store, id, wants);
            return;
        }
        if let Some(dt) = w.as_any().downcast_ref::<crate::data_table::DataTable>() {
            let wants = dt.is_focused_host();
            restore_host_focus_if_needed(store, id, wants);
        }
    }
}

fn collect_ids(store: &WidgetStore, id: WidgetId) -> Vec<WidgetId> {
    collect_widget_ids(store, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use synui_core::TextMeasurer;

    struct DummyText;
    impl TextMeasurer for DummyText {
        fn measure_text(&mut self, _text: &str, font_size: f32, _max_width: Option<f32>) -> Size {
            Size::new(40.0, font_size as f64 * 1.2)
        }
    }

    #[test]
    fn thumb_drag_updates_scroll() {
        let bounds = Rect::new(0.0, 0.0, 200.0, 100.0);
        let content = 400.0;
        let mut bar = VScrollbarState::default();
        let geom = v_scrollbar_geom(bounds, 0.0, content).unwrap();
        let mid = Point::new(geom.thumb.x0 + 1.0, geom.thumb.y0 + 2.0);
        assert!(bar.pointer_down(mid, bounds, 0.0, content).is_some());
        let bottom = Point::new(geom.thumb.x0 + 1.0, geom.track.y1 - 2.0);
        let y = bar.pointer_move(bottom, bounds, content).unwrap();
        assert!(y > 100.0, "expected scroll near bottom, got {y}");
        assert!(bar.pointer_up());
    }

    #[test]
    fn track_click_jumps() {
        let bounds = Rect::new(0.0, 0.0, 200.0, 100.0);
        let content = 500.0;
        let mut bar = VScrollbarState::default();
        let geom = v_scrollbar_geom(bounds, 0.0, content).unwrap();
        let click = Point::new(geom.track.x0 + 1.0, geom.track.y1 - 4.0);
        let y = bar.pointer_down(click, bounds, 0.0, content).unwrap();
        assert!(y > 200.0, "track click should jump scroll, got {y}");
    }

    #[test]
    fn virtual_scroll_10k_mounts_under_40() {
        let mut store = WidgetStore::new();
        let mut sv = ScrollView::new().with_virtual(10_000, 32.0);
        sv.set_slot_builder(|i| Box::new(Label::new(format!("row {i}"))));
        sv.set_bounds(Rect::new(0.0, 0.0, 400.0, 400.0));
        sv.offset_y = 5000.0 * 32.0;
        let id = store.insert(sv);
        store.set_root(id);
        let mut text = DummyText;
        apply_scroll_views(&mut store);
        sync_virtual_scroll_views(&mut store, &mut text);
        let n = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<ScrollView>()
            .unwrap()
            .mounted_count();
        assert!(n <= 40, "expected ≤40 mounted for 10k slots, got {n}");
        assert!(n >= 2, "should mount viewport rows + sticky slot0, got {n}");
        let children = store.children_of(id).to_vec();
        assert!(!children.is_empty());
    }

    #[test]
    fn transparent_virt_10k_mounts_under_40() {
        let mut store = WidgetStore::new();
        let mut kids = Vec::with_capacity(10_000);
        for i in 0..10_000 {
            kids.push(store.insert(Label::new(format!("row {i}"))));
        }
        let mut sv = ScrollView::new()
            .with_children(kids)
            .enable_transparent_virt(true);
        sv.set_bounds(Rect::new(0.0, 0.0, 400.0, 400.0));
        // Fixed extents so scroll math is stable without prior measure.
        sv.measure_heights = false;
        sv.slot_extent = 32.0;
        sv.vp.clear_variable_heights();
        sv.vp.item_extent = 32.0;
        sv.vp.item_count = 10_000;
        sv.content_height = sv.vp.content_height();
        sv.offset_y = 5000.0 * 32.0;
        let id = store.insert(sv);
        store.set_root(id);
        store.set_children(id, vec![]);
        let mut text = DummyText;
        apply_scroll_views(&mut store);
        sync_virtual_scroll_views(&mut store, &mut text);
        let n = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<ScrollView>()
            .unwrap()
            .mounted_count();
        assert!(n <= 40, "expected ≤40 mounted for transparent 10k, got {n}");
        assert!(n >= 1, "should mount viewport rows, got {n}");
        // Logical children remain in the store (not destroyed).
        let logical_len = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<ScrollView>()
            .unwrap()
            .logical_children()
            .len();
        assert_eq!(logical_len, 10_000);
        assert_eq!(store.children_of(id).len(), n);
    }

    #[test]
    fn scroll_shifts_styled_subtree() {
        let mut store = WidgetStore::new();
        let inner = store.insert(Label::new("row"));
        let styled =
            crate::styled::StyledBox::new(synui_core::Style::from_css("padding:8").expect("css"))
                .with_children(vec![inner]);
        let styled_id = store.insert(styled);
        store.set_children(styled_id, vec![inner]);
        if let Some(w) = store.get_mut(inner) {
            w.set_bounds(Rect::new(0.0, 40.0, 100.0, 60.0));
        }
        if let Some(w) = store.get_mut(styled_id) {
            w.set_bounds(Rect::new(0.0, 20.0, 200.0, 80.0));
        }

        let mut sv = ScrollView::new().with_children(vec![styled_id]);
        sv.set_bounds(Rect::new(0.0, 0.0, 200.0, 50.0));
        sv.content_height = 200.0;
        sv.base_y = vec![20.0];
        sv.offset_y = 30.0;
        let id = store.insert(sv);
        store.set_root(id);
        store.set_children(id, vec![styled_id]);

        apply_scroll_views(&mut store);

        let styled_y = store.get(styled_id).unwrap().bounds().y0;
        let inner_y = store.get(inner).unwrap().bounds().y0;
        assert!((styled_y - (-10.0)).abs() < 0.5, "styled y={styled_y}");
        assert!(
            (inner_y - 10.0).abs() < 0.5,
            "inner must move with parent, y={inner_y}"
        );
    }

    #[test]
    fn virtual_scroll_variable_heights_10k() {
        let mut store = WidgetStore::new();
        let mut sv = ScrollView::new().with_virtual(10_000, 24.0);
        sv.set_slot_extent_fn(|i| if i % 2 == 0 { 24.0 } else { 48.0 });
        sv.set_slot_builder(|i| Box::new(Label::new(format!("v {i}"))));
        sv.set_bounds(Rect::new(0.0, 0.0, 400.0, 400.0));
        sv.offset_y = 2000.0;
        let id = store.insert(sv);
        store.set_root(id);
        let mut text = DummyText;
        apply_scroll_views(&mut store);
        sync_virtual_scroll_views(&mut store, &mut text);
        let sv = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<ScrollView>()
            .unwrap();
        assert!(sv.mounted_count() <= 40);
        assert!(sv.vp.has_variable_heights());
        assert!(sv.content_height > 10_000.0 * 24.0);
    }

    #[test]
    fn card_row_template_10k_mounts_under_40() {
        let mut store = WidgetStore::new();
        let mut sv = ScrollView::new().with_virtual(10_000, 48.0);
        // Row template = composed label (Card-like content height); same VirtualMountHost path.
        sv.set_slot_builder(|i| Box::new(Label::new(format!("▣ Card {i}"))));
        sv.set_bounds(Rect::new(0.0, 0.0, 400.0, 320.0));
        sv.offset_y = 1000.0 * 48.0;
        let id = store.insert(sv);
        store.set_root(id);
        let mut text = DummyText;
        sync_virtual_mounts(&mut store, &mut text);
        let n = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<ScrollView>()
            .unwrap()
            .mounted_count();
        assert!(n <= 40, "card-row templates mount bound, got {n}");
    }

    #[test]
    fn nested_scroll_chains_at_inner_edge() {
        let mut store = WidgetStore::new();
        let mut inner = ScrollView::new();
        inner.scroll_chaining = true;
        inner.set_bounds(Rect::new(10.0, 10.0, 190.0, 110.0));
        inner.content_height = 400.0;
        inner.offset_y = 0.0; // at top
        let inner_id = store.insert(inner);

        let mut outer = ScrollView::new().with_children(vec![inner_id]);
        outer.scroll_chaining = true;
        outer.set_bounds(Rect::new(0.0, 0.0, 200.0, 200.0));
        outer.content_height = 500.0;
        outer.offset_y = 50.0;
        let outer_id = store.insert(outer);
        store.set_root(outer_id);
        store.set_children(outer_id, vec![inner_id]);

        // Wheel toward start while inner is at top → inner ignores → outer scrolls.
        let result = store.dispatch_event(&Event::Scroll {
            position: Point::new(50.0, 50.0),
            delta_x: 0.0,
            delta_y: 20.0,
        });
        assert!(result.handled);
        let outer_off = store
            .get(outer_id)
            .unwrap()
            .as_any()
            .downcast_ref::<ScrollView>()
            .unwrap()
            .offset_y;
        assert!(
            (outer_off - 30.0).abs() < 0.5,
            "outer should chain-scroll when inner at edge, got {outer_off}"
        );
    }
}
