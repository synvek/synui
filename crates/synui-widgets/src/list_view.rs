//! Virtualized list with composable rows, measured variable height, sticky header (Phase 15–17).

use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Constraints, Dimension, Event, EventCtx, EventResult, Key,
    LayoutStyle, MeasureCtx, NamedKey, PaintCtx, Point, PointerButton, Rect, Size, TextAlign,
    TextMeasurer, Widget, WidgetId, WidgetStore,
};
use synui_render::paint_simple_rect;
use vello::Scene;

use std::collections::{HashMap, HashSet, VecDeque};

use crate::basic::Label;
use crate::scroll::{paint_v_scrollbar, VScrollbarState};
use crate::virtual_mount::{
    collect_all_widget_ids, reconcile_mount_window, scroll_at_edge, MountPoolPolicy, LIST_POOL_CAP,
};
use crate::virtual_viewport::{layout_row_subtree, VirtualViewport};

/// Virtualized list: viewport ± overscan; paint labels or mount child widgets.
pub struct ListView {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    mounted: Vec<(usize, WidgetId)>,
    /// Off-viewport rows retained by index for state preservation (Phase 18).
    pool: HashMap<usize, WidgetId>,
    pool_order: VecDeque<usize>,
    pub item_count: usize,
    pub item_extent: f64,
    extent_fn: Option<Box<dyn Fn(usize) -> f64>>,
    /// When true and mounting, measure rows to fill height cache.
    pub measure_heights: bool,
    vp: VirtualViewport,
    /// Multi-select set (Phase 19).
    pub selected: HashSet<usize>,
    /// Anchor for Shift-range selection.
    pub selection_anchor: Option<usize>,
    pub context_request: Option<(usize, f64, f64)>,
    pub activated: Option<usize>,
    label_fn: Box<dyn Fn(usize) -> String>,
    item_builder: Option<Box<dyn Fn(usize) -> Box<dyn synui_core::ErasedWidget>>>,
    /// Store-aware builder (preferred for multi-widget rows).
    item_builder_store: Option<Box<dyn Fn(&mut WidgetStore, usize) -> WidgetId>>,
    use_mount: bool,
    focused: bool,
    scrollbar: VScrollbarState,
    /// Sticky header label (widget-level, not CSS). Empty = none.
    pub sticky_header: Option<String>,
    pub sticky_header_height: f64,
}

impl ListView {
    pub fn new(item_count: usize, item_extent: f64) -> Self {
        let extent = item_extent.max(1.0);
        let this = Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            mounted: Vec::new(),
            pool: HashMap::new(),
            pool_order: VecDeque::new(),
            item_count,
            item_extent: extent,
            extent_fn: None,
            measure_heights: false,
            vp: VirtualViewport::new(item_count, extent),
            selected: HashSet::new(),
            selection_anchor: None,
            context_request: None,
            activated: None,
            label_fn: Box::new(|i| format!("Item {i}")),
            item_builder: None,
            item_builder_store: None,
            use_mount: false,
            focused: false,
            scrollbar: VScrollbarState::default(),
            sticky_header: None,
            sticky_header_height: 28.0,
        };
        crate::virtual_mount::register_virt_host(this.id, crate::virtual_mount::VirtHostKind::List);
        this
    }

    pub fn take_context_request(&mut self) -> Option<(usize, f64, f64)> {
        self.context_request.take()
    }

    pub fn take_activated(&mut self) -> Option<usize> {
        self.activated.take()
    }

    pub fn with_label_fn(mut self, f: impl Fn(usize) -> String + 'static) -> Self {
        self.set_label_fn(f);
        self
    }

    pub fn set_label_fn(&mut self, f: impl Fn(usize) -> String + 'static) {
        self.label_fn = Box::new(f);
    }

    pub fn with_sticky_header(mut self, title: impl Into<String>) -> Self {
        self.sticky_header = Some(title.into());
        self
    }

    pub fn with_extent_fn(mut self, f: impl Fn(usize) -> f64 + 'static) -> Self {
        self.set_extent_fn(f);
        self
    }

    pub fn set_extent_fn(&mut self, f: impl Fn(usize) -> f64 + 'static) {
        self.extent_fn = Some(Box::new(f));
        self.measure_heights = false;
        self.sync_vp_meta();
        self.rebuild_heights();
    }

    pub fn with_measure_heights(mut self) -> Self {
        self.enable_measure_heights(true);
        self
    }

    pub fn enable_measure_heights(&mut self, on: bool) {
        self.measure_heights = on;
        if on {
            self.extent_fn = None;
            self.use_mount = true;
        }
        self.sync_vp_meta();
    }

    pub fn with_item_builder(
        mut self,
        f: impl Fn(usize) -> Box<dyn synui_core::ErasedWidget> + 'static,
    ) -> Self {
        self.set_item_builder(f);
        self
    }

    pub fn set_item_builder(
        &mut self,
        f: impl Fn(usize) -> Box<dyn synui_core::ErasedWidget> + 'static,
    ) {
        self.item_builder = Some(Box::new(f));
        self.item_builder_store = None;
        self.use_mount = true;
    }

    pub fn with_item_builder_store(
        mut self,
        f: impl Fn(&mut WidgetStore, usize) -> WidgetId + 'static,
    ) -> Self {
        self.item_builder_store = Some(Box::new(f));
        self.item_builder = None;
        self.use_mount = true;
        self
    }

    pub fn uses_mount(&self) -> bool {
        self.use_mount
    }

    pub fn with_mounted_labels(mut self) -> Self {
        self.set_item_builder(|i| Box::new(Label::new(format!("Item {i}"))));
        self
    }

    fn header_h(&self) -> f64 {
        if self.sticky_header.is_some() {
            self.sticky_header_height
        } else {
            0.0
        }
    }

    fn body_bounds(&self) -> Rect {
        let h = self.header_h();
        Rect::new(
            self.bounds.x0,
            self.bounds.y0 + h,
            self.bounds.x1,
            self.bounds.y1,
        )
    }

    fn sync_vp_meta(&mut self) {
        self.vp.item_count = self.item_count;
        self.vp.item_extent = self.item_extent;
        let body = self.body_bounds();
        self.vp.viewport_height = body.height().max(0.0);
    }

    fn rebuild_heights(&mut self) {
        self.sync_vp_meta();
        if let Some(f) = self.extent_fn.as_ref() {
            // Force rebuild by temporarily taking
            let count = self.item_count;
            let mut heights = Vec::with_capacity(count);
            for i in 0..count {
                heights.push(f(i).max(1.0));
            }
            self.vp.set_heights(heights);
        } else if !self.measure_heights {
            self.vp.clear_variable_heights();
        }
        self.vp.clamp_scroll();
    }

    pub fn ensure_height_cache(&mut self) {
        self.sync_vp_meta();
        if self.extent_fn.is_some() {
            if !self.vp.has_variable_heights() || self.vp.item_count != self.item_count {
                self.rebuild_heights();
            }
        } else if self.measure_heights {
            self.vp.item_count = self.item_count;
            self.vp.ensure_measured_slots();
        } else {
            self.vp.item_count = self.item_count;
            self.vp.item_extent = self.item_extent;
            self.vp.clear_variable_heights();
        }
    }

    pub fn row_extent(&self, index: usize) -> f64 {
        self.vp.row_extent(index)
    }

    pub fn offset_of(&self, index: usize) -> f64 {
        self.vp.offset_of(index)
    }

    pub fn content_height(&self) -> f64 {
        self.vp.content_height()
    }

    pub fn scroll_y(&self) -> f64 {
        self.vp.scroll_y
    }

    pub fn set_scroll_y(&mut self, y: f64) {
        self.vp.scroll_y = y;
        self.vp.clamp_scroll();
        crate::virtual_mount::mark_virt_host_dirty(self.id);
    }

    pub fn max_scroll(&self) -> f64 {
        self.vp.max_scroll()
    }

    pub fn clamp_scroll(&mut self) {
        self.sync_vp_meta();
        self.vp.clamp_scroll();
        crate::virtual_mount::mark_virt_host_dirty(self.id);
    }

    pub fn visible_range(&self) -> (usize, usize) {
        self.vp.visible_range()
    }

    pub fn painted_row_count(&self) -> usize {
        let (a, b) = self.visible_range();
        b.saturating_sub(a)
    }

    pub fn mounted_count(&self) -> usize {
        self.mounted.len()
    }

    pub fn mounted_id(&self, index: usize) -> Option<WidgetId> {
        self.mounted
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, id)| *id)
    }

    pub fn pool_len(&self) -> usize {
        self.pool.len()
    }

    fn index_at_y(&self, y: f64) -> Option<usize> {
        let body = self.body_bounds();
        if y < body.y0 {
            return None;
        }
        let local = y - body.y0 + self.vp.scroll_y;
        self.vp.index_at_offset(local)
    }

    fn ensure_selected_visible(&mut self) {
        if let Some(sel) = self.primary_selected() {
            self.vp.ensure_index_visible(sel);
            crate::virtual_mount::mark_virt_host_dirty(self.id);
        }
    }

    /// Whether this list currently owns keyboard focus (for post-recycle restore).
    pub fn is_focused_host(&self) -> bool {
        self.focused
    }

    /// Focus / primary row for keyboard and status (anchor, else any selected).
    pub fn primary_selected(&self) -> Option<usize> {
        self.selection_anchor
            .filter(|i| self.selected.contains(i))
            .or_else(|| self.selected.iter().copied().min())
    }

    fn apply_pointer_selection(&mut self, idx: usize, shift: bool, ctrl: bool) {
        if ctrl {
            if self.selected.contains(&idx) {
                self.selected.remove(&idx);
            } else {
                self.selected.insert(idx);
            }
            self.selection_anchor = Some(idx);
        } else if shift {
            let anchor = self.selection_anchor.unwrap_or(idx);
            let (a, b) = (anchor.min(idx), anchor.max(idx));
            self.selected = (a..=b).collect();
        } else {
            self.selected.clear();
            self.selected.insert(idx);
            self.selection_anchor = Some(idx);
        }
    }

    fn move_primary(&mut self, next: usize, shift: bool) {
        if self.item_count == 0 {
            return;
        }
        let next = next.min(self.item_count - 1);
        if shift {
            let anchor = self.selection_anchor.unwrap_or(next);
            let (a, b) = (anchor.min(next), anchor.max(next));
            self.selected = (a..=b).collect();
            // Keep anchor; move "focus" via ensuring last clicked end is visible — store end in anchor only on non-shift.
        } else {
            self.selected.clear();
            self.selected.insert(next);
            self.selection_anchor = Some(next);
        }
    }
}

impl Default for ListView {
    fn default() -> Self {
        Self::new(0, 32.0)
    }
}

impl Widget for ListView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn manages_child_layout(&self) -> bool {
        // Rows are placed by sync_list_view; taffy flex would stack them and can
        // grow the host past the window so the scrollbar leaves the viewport.
        true
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();
        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: synui_core::FlexDirection::Column,
            width: Dimension::Fill,
            height: Dimension::Fill,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..LayoutStyle::base()
        })
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.sync_vp_meta();
        self.vp.clamp_scroll();
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        let width = if constraints.max_width.is_finite() {
            constraints.max_width as f64
        } else {
            400.0
        };
        let height = if constraints.max_height.is_finite() {
            constraints.max_height as f64
        } else {
            320.0
        };
        Size::new(width, height)
    }

    fn clip_bounds(&self) -> Option<Rect> {
        // Clip body + mounted children below the sticky header.
        Some(self.body_bounds())
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let body = self.body_bounds();
        paint_simple_rect(scene, body, ctx.theme.surface, 0.0);
        if !self.use_mount {
            let (start, end) = self.visible_range();
            let text_right = body.x1 - 14.0;
            if let Some(text) = ctx.text.as_deref_mut() {
                for i in start..end {
                    let y0 = body.y0 + self.offset_of(i) - self.vp.scroll_y;
                    let h = self.row_extent(i);
                    let row = Rect::new(body.x0, y0, body.x1, y0 + h);
                    if row.y1 < body.y0 || row.y0 > body.y1 {
                        continue;
                    }
                    if self.selected.contains(&i) {
                        let mut hl = ctx.theme.primary;
                        hl.components[3] = 0.18;
                        paint_simple_rect(scene, row, hl, 0.0);
                    }
                    let label = (self.label_fn)(i);
                    let pad = Rect::new(row.x0 + 12.0, row.y0, text_right, row.y1);
                    text.paint_text(
                        scene,
                        &label,
                        14.0,
                        ctx.theme.on_surface,
                        pad,
                        Some(pad.width().max(1.0) as f32),
                        TextAlign::Start,
                    );
                }
            }
        } else {
            for &sel in &self.selected {
                let y0 = body.y0 + self.offset_of(sel) - self.vp.scroll_y;
                let h = self.row_extent(sel);
                let row = Rect::new(body.x0, y0, body.x1, y0 + h);
                if row.y1 < body.y0 || row.y0 > body.y1 {
                    continue;
                }
                let mut hl = ctx.theme.primary;
                hl.components[3] = 0.18;
                paint_simple_rect(scene, row, hl, 0.0);
            }
        }
        let (width, edge) = if self.focused {
            let mut edge = ctx.theme.primary;
            edge.components[3] = 0.5;
            (1.5, edge)
        } else {
            (1.0, ctx.theme.field_border())
        };
        let inset = Rect::new(
            self.bounds.x0 + 1.0,
            self.bounds.y0 + 1.0,
            self.bounds.x1 - 1.0,
            self.bounds.y1 - 1.0,
        );
        use vello::kurbo::{RoundedRect, Stroke};
        scene.stroke(
            &Stroke::new(width),
            vello::kurbo::Affine::IDENTITY,
            edge,
            None,
            &RoundedRect::from_rect(inset, 0.0),
        );
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
            body,
            self.vp.scroll_y,
            self.content_height(),
            track,
            thumb,
        );
    }

    fn paint_overlay(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let Some(title) = &self.sticky_header else {
            return;
        };
        let hdr = Rect::new(
            self.bounds.x0,
            self.bounds.y0,
            self.bounds.x1,
            self.bounds.y0 + self.header_h(),
        );
        paint_simple_rect(scene, hdr, ctx.theme.surface_container, 0.0);
        if let Some(text) = ctx.text.as_deref_mut() {
            text.paint_text(
                scene,
                title,
                13.0,
                ctx.theme.on_surface,
                Rect::new(hdr.x0 + 12.0, hdr.y0, hdr.x1 - 14.0, hdr.y1),
                Some((hdr.width() - 26.0).max(1.0) as f32),
                TextAlign::Start,
            );
        }
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        self.ensure_height_cache();
        let body = self.body_bounds();
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                shift,
                ctrl,
                ..
            } if self.bounds.contains(*position) => {
                if let Some(y) = self.scrollbar.pointer_down(
                    *position,
                    body,
                    self.vp.scroll_y,
                    self.content_height(),
                ) {
                    self.set_scroll_y(y);
                    self.focused = true;
                    ctx.grab_pointer();
                    return EventResult::HANDLED;
                }
                self.focused = true;
                if let Some(idx) = self.index_at_y(position.y) {
                    self.apply_pointer_selection(idx, *shift, *ctrl);
                } else if !*ctrl && !*shift {
                    self.selected.clear();
                    self.selection_anchor = None;
                }
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } => {
                if let Some(y) = self
                    .scrollbar
                    .pointer_move(*position, body, self.content_height())
                {
                    self.set_scroll_y(y);
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
            Event::PointerDown {
                button: PointerButton::Secondary,
                position,
                ..
            } if body.contains(*position) => {
                self.focused = true;
                if let Some(idx) = self.index_at_y(position.y) {
                    if !self.selected.contains(&idx) {
                        self.apply_pointer_selection(idx, false, false);
                    }
                    self.context_request = Some((idx, position.x, position.y));
                }
                EventResult::HANDLED
            }
            Event::Scroll {
                position, delta_y, ..
            } if self.bounds.contains(*position) || self.focused => {
                let max = self.vp.max_scroll();
                if scroll_at_edge(self.vp.scroll_y, max, *delta_y as f64) {
                    return EventResult::IGNORED;
                }
                // Must mark virt dirty — bare vp.clamp_scroll left rows frozen on screen.
                self.set_scroll_y(self.vp.scroll_y - *delta_y as f64);
                ctx.stop_propagation();
                EventResult::HANDLED
            }
            Event::FocusGained => {
                self.focused = true;
                EventResult::HANDLED
            }
            Event::FocusLost => {
                self.focused = false;
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            } if self.focused => {
                if let Some(sel) = self.primary_selected() {
                    self.activated = Some(sel);
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Space),
                ctrl,
                ..
            } if self.focused => {
                if let Some(sel) = self.primary_selected() {
                    if *ctrl {
                        self.apply_pointer_selection(sel, false, true);
                    } else {
                        self.activated = Some(sel);
                    }
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowDown),
                shift,
                ..
            } if self.focused => {
                let cur = self.primary_selected().unwrap_or(0);
                let next = (cur + 1).min(self.item_count.saturating_sub(1));
                self.move_primary(next, *shift);
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowUp),
                shift,
                ..
            } if self.focused => {
                let cur = self.primary_selected().unwrap_or(0);
                let next = cur.saturating_sub(1);
                self.move_primary(next, *shift);
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::PageDown),
                shift,
                ..
            } if self.focused => {
                let step = (body.height() / self.item_extent).floor() as usize;
                let cur = self.primary_selected().unwrap_or(0);
                let next = (cur + step.max(1)).min(self.item_count.saturating_sub(1));
                self.move_primary(next, *shift);
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::PageUp),
                shift,
                ..
            } if self.focused => {
                let step = (body.height() / self.item_extent).floor() as usize;
                let cur = self.primary_selected().unwrap_or(0);
                let next = cur.saturating_sub(step.max(1));
                self.move_primary(next, *shift);
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Home),
                shift,
                ..
            } if self.focused => {
                self.move_primary(0, *shift);
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::End),
                shift,
                ..
            } if self.focused => {
                if self.item_count > 0 {
                    self.move_primary(self.item_count - 1, *shift);
                    self.ensure_selected_visible();
                }
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::List,
            label: Some(format!("ListView ({} items)", self.item_count)),
            focusable: true,
            actions: vec![A11yAction::Focus],
            row_count: Some(self.item_count),
            row_index: self.primary_selected(),
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn hit_test_prefer_self(&self, point: Point) -> bool {
        let body = self.body_bounds();
        VScrollbarState::hit(body, self.vp.scroll_y, self.content_height(), point)
            || self.scrollbar.is_dragging()
    }
}

/// Mount / recycle ListView row widgets and lay out row subtrees.
pub fn sync_list_views(store: &mut WidgetStore, text: &mut dyn TextMeasurer) {
    let ids = collect_all_widget_ids(store);
    for id in ids {
        sync_list_view(store, text, id);
    }
}

/// Sync a single ListView host (Phase 26 dirty virt).
pub fn sync_list_view(store: &mut WidgetStore, text: &mut dyn TextMeasurer, id: WidgetId) {
    let needs = store
        .get(id)
        .map(|w| {
            w.as_any()
                .downcast_ref::<ListView>()
                .map(|lv| lv.use_mount)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !needs {
        return;
    }
    {
        let w = store.get_mut(id).unwrap();
        let lv = w.as_any_mut().downcast_mut::<ListView>().unwrap();
        lv.ensure_height_cache();
    }
    let (start, end, body, scroll_y, measure_heights) = {
        let w = store.get(id).unwrap();
        let lv = w.as_any().downcast_ref::<ListView>().unwrap();
        let (a, b) = lv.visible_range();
        (a, b, lv.body_bounds(), lv.vp.scroll_y, lv.measure_heights)
    };
    let desired: Vec<usize> = (start..end).collect();
    let (mut mounted, mut pool, mut pool_order) = {
        let lv = store
            .get_mut(id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<ListView>()
            .unwrap();
        (
            std::mem::take(&mut lv.mounted),
            std::mem::take(&mut lv.pool),
            std::mem::take(&mut lv.pool_order),
        )
    };
    // Create rows on demand inside reconcile. Prebuilding while skipping keys
    // that are "in pool" races with pool eviction and panicked with
    // "prebuilt list row" during fast scrolls.
    let remove_ids = reconcile_mount_window(
        &mut mounted,
        &mut pool,
        &mut pool_order,
        LIST_POOL_CAP,
        &desired,
        MountPoolPolicy::RecycleDestroy,
        &mut |&idx| {
            let store_builder = {
                let lv = store
                    .get_mut(id)
                    .unwrap()
                    .as_any_mut()
                    .downcast_mut::<ListView>()
                    .unwrap();
                lv.item_builder_store.take()
            };
            if let Some(builder) = store_builder {
                let wid = builder(store, idx);
                if let Some(w) = store.get_mut(id) {
                    if let Some(lv) = w.as_any_mut().downcast_mut::<ListView>() {
                        lv.item_builder_store = Some(builder);
                    }
                }
                wid
            } else {
                let widget: Box<dyn synui_core::ErasedWidget> = {
                    let w = store.get(id).unwrap();
                    let lv = w.as_any().downcast_ref::<ListView>().unwrap();
                    if let Some(builder) = lv.item_builder.as_ref() {
                        builder(idx)
                    } else {
                        Box::new(Label::new((lv.label_fn)(idx)))
                    }
                };
                store.insert_erased(widget)
            }
        },
    );
    for wid in remove_ids {
        store.remove(wid);
    }
    let keep = mounted.clone();
    let child_ids: Vec<WidgetId> = keep.iter().map(|(_, id)| *id).collect();
    store.set_children(id, child_ids.clone());
    if let Some(w) = store.get_mut(id) {
        if let Some(lv) = w.as_any_mut().downcast_mut::<ListView>() {
            lv.mounted = mounted;
            lv.pool = pool;
            lv.pool_order = pool_order;
        }
    }

    let row_width = (body.width() - 18.0).max(40.0);
    for &(idx, wid) in &keep {
        let y0 = body.y0 + {
            let w = store.get(id).unwrap();
            let lv = w.as_any().downcast_ref::<ListView>().unwrap();
            lv.offset_of(idx) - scroll_y
        };
        let h = {
            let w = store.get(id).unwrap();
            let lv = w.as_any().downcast_ref::<ListView>().unwrap();
            lv.row_extent(idx)
        };
        let avail_h = if measure_heights {
            10_000.0
        } else {
            h.max(1.0)
        };
        layout_row_subtree(
            store,
            text,
            wid,
            Point::new(body.x0 + 4.0, y0),
            Size::new(row_width, avail_h),
        );
        if measure_heights {
            if let Some(cw) = store.get(wid) {
                let measured = cw.bounds().height().max(1.0);
                if let Some(w) = store.get_mut(id) {
                    if let Some(lv) = w.as_any_mut().downcast_mut::<ListView>() {
                        lv.vp.set_measured_height(idx, measured);
                    }
                }
            }
            let h2 = store
                .get(id)
                .and_then(|w| {
                    w.as_any()
                        .downcast_ref::<ListView>()
                        .map(|l| l.row_extent(idx))
                })
                .unwrap_or(h);
            let y1 = body.y0
                + store
                    .get(id)
                    .and_then(|w| {
                        w.as_any()
                            .downcast_ref::<ListView>()
                            .map(|l| l.offset_of(idx) - l.vp.scroll_y)
                    })
                    .unwrap_or(y0 - body.y0);
            layout_row_subtree(
                store,
                text,
                wid,
                Point::new(body.x0 + 4.0, y1),
                Size::new(row_width, h2),
            );
        }
    }
    if let Some(w) = store.get_mut(id) {
        if let Some(lv) = w.as_any_mut().downcast_mut::<ListView>() {
            lv.child_ids = child_ids;
            lv.vp.clamp_scroll();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synui_render::TextRenderer;

    #[test]
    fn painted_rows_bounded_for_10k() {
        let mut list = ListView::new(10_000, 32.0);
        list.set_bounds(Rect::new(0.0, 0.0, 400.0, 320.0));
        let n = list.painted_row_count();
        assert!(n <= 40, "painted {n} rows for 10k list");
        assert!(n >= 8);
    }

    #[test]
    fn deep_scroll_scrollbar_thumb_stays_in_track() {
        use crate::scroll::v_scrollbar_geom;

        let mut list = ListView::new(10_000, 32.0);
        // Mimic list_demo: chrome above the list, host inside a window.
        let host = Rect::new(0.0, 72.0, 800.0, 600.0);
        list.set_bounds(host);
        list.set_scroll_y(8_200.0 * 32.0);
        let body = list.body_bounds();
        let geom = v_scrollbar_geom(body, list.scroll_y(), list.content_height())
            .expect("overflowing list must show scrollbar");
        assert!(
            geom.track.y0 >= host.y0 - 0.5 && geom.track.y1 <= host.y1 + 0.5,
            "track left host: track={:?} host={:?}",
            geom.track,
            host
        );
        assert!(
            geom.thumb.y0 >= geom.track.y0 - 0.5 && geom.thumb.y1 <= geom.track.y1 + 0.5,
            "thumb left track: thumb={:?} track={:?}",
            geom.thumb,
            geom.track
        );
    }

    #[test]
    fn layout_does_not_grow_host_from_mounted_rows() {
        use synui_layout::LayoutEngine;
        use synui_render::TextRenderer;

        let mut store = WidgetStore::new();
        let mut text = TextRenderer::new();
        let mut list = ListView::new(10_000, 32.0).with_item_builder(|i| {
            Box::new(Label::new(format!("Row {i}"))) as Box<dyn synui_core::ErasedWidget>
        });
        list.set_bounds(Rect::new(0.0, 0.0, 400.0, 320.0));
        let id = store.insert(list);
        store.set_root(id);
        sync_list_views(&mut store, &mut text);
        assert!(
            !store.should_layout_as_child(id, store.children_of(id)[0]),
            "mounted rows must not participate in taffy under ListView"
        );

        let mut engine = LayoutEngine::new();
        engine.compute(&mut store, &mut text, id, Size::new(400.0, 320.0));
        sync_list_views(&mut store, &mut text);
        {
            let w = store.get_mut(id).unwrap();
            let lv = w.as_any_mut().downcast_mut::<ListView>().unwrap();
            lv.set_scroll_y(8_200.0 * 32.0);
        }
        sync_list_views(&mut store, &mut text);
        engine.compute(&mut store, &mut text, id, Size::new(400.0, 320.0));
        let bounds = store.get(id).unwrap().bounds();
        assert!(
            (bounds.height() - 320.0).abs() < 1.0,
            "deep scroll must not grow ListView host, got height {}",
            bounds.height()
        );
        assert!(
            (bounds.y1 - 320.0).abs() < 1.0,
            "ListView bottom must stay in viewport, y1={}",
            bounds.y1
        );
    }

    #[test]
    fn variable_height_prefix() {
        let mut list =
            ListView::new(5, 20.0).with_extent_fn(|i| if i % 2 == 0 { 20.0 } else { 40.0 });
        list.ensure_height_cache();
        assert_eq!(list.content_height(), 140.0);
        assert_eq!(list.offset_of(2), 60.0);
    }

    #[test]
    fn sticky_header_shrinks_body() {
        let mut list = ListView::new(100, 32.0).with_sticky_header("Title");
        list.set_bounds(Rect::new(0.0, 0.0, 400.0, 320.0));
        assert_eq!(list.body_bounds().y0, 28.0);
        assert!(list.body_bounds().height() < 320.0);
    }

    #[test]
    fn mount_bound_for_10k() {
        let mut store = WidgetStore::new();
        let mut text = TextRenderer::new();
        let mut list = ListView::new(10_000, 32.0).with_item_builder(|i| {
            Box::new(Label::new(format!("Row {i}"))) as Box<dyn synui_core::ErasedWidget>
        });
        list.set_bounds(Rect::new(0.0, 0.0, 400.0, 320.0));
        let id = store.insert(list);
        store.set_root(id);
        sync_list_views(&mut store, &mut text);
        let n = store
            .get(id)
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<ListView>()
                    .map(|l| l.mounted_count())
            })
            .unwrap_or(0);
        assert!(n <= 40, "mounted {n} for 10k list");
    }

    #[test]
    fn stable_key_pool_reuses_widget_after_scroll() {
        let mut store = WidgetStore::new();
        let mut text = TextRenderer::new();
        let mut list = ListView::new(200, 32.0).with_item_builder(|i| {
            Box::new(Label::new(format!("Row {i}"))) as Box<dyn synui_core::ErasedWidget>
        });
        list.set_bounds(Rect::new(0.0, 0.0, 400.0, 200.0));
        let id = store.insert(list);
        store.set_root(id);
        sync_list_views(&mut store, &mut text);
        let first_id = store
            .get(id)
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<ListView>()
                    .and_then(|l| l.mounted_id(0))
            })
            .expect("row 0 mounted");
        {
            let w = store.get_mut(id).unwrap();
            let lv = w.as_any_mut().downcast_mut::<ListView>().unwrap();
            lv.set_scroll_y(5_000.0);
        }
        sync_list_views(&mut store, &mut text);
        assert!(
            store
                .get(id)
                .and_then(|w| {
                    w.as_any()
                        .downcast_ref::<ListView>()
                        .map(|l| l.mounted_id(0).is_none() && l.pool_len() > 0)
                })
                .unwrap_or(false),
            "row 0 should leave viewport into pool"
        );
        {
            let w = store.get_mut(id).unwrap();
            let lv = w.as_any_mut().downcast_mut::<ListView>().unwrap();
            lv.set_scroll_y(0.0);
        }
        sync_list_views(&mut store, &mut text);
        let reused = store
            .get(id)
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<ListView>()
                    .and_then(|l| l.mounted_id(0))
            })
            .expect("row 0 remounted");
        assert_eq!(first_id, reused, "pool must reuse the same widget id");
        assert!(store.get(reused).is_some());
    }

    #[test]
    fn fast_scroll_does_not_panic_on_pool_eviction() {
        let mut store = WidgetStore::new();
        let mut text = TextRenderer::new();
        let mut list = ListView::new(10_000, 32.0).with_item_builder(|i| {
            Box::new(Label::new(format!("Row {i}"))) as Box<dyn synui_core::ErasedWidget>
        });
        list.set_bounds(Rect::new(0.0, 0.0, 400.0, 320.0));
        let id = store.insert(list);
        store.set_root(id);
        // Jump through many windows so pool fills and evicts while new rows are needed.
        for step in 0..80 {
            {
                let w = store.get_mut(id).unwrap();
                let lv = w.as_any_mut().downcast_mut::<ListView>().unwrap();
                lv.set_scroll_y(step as f64 * 48.0 * 32.0);
            }
            sync_list_views(&mut store, &mut text);
        }
        let mounted = store
            .get(id)
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<ListView>()
                    .map(|l| l.mounted_count())
            })
            .unwrap_or(0);
        assert!(mounted > 0 && mounted <= 40, "mounted={mounted}");
    }
}
