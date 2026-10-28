//! Virtualized tree view — flatten visible nodes, expand/collapse, composable rows (Phase 16–17).

use std::collections::{HashMap, HashSet, VecDeque};

use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Constraints, Dimension, ErasedWidget, Event, EventCtx,
    EventResult, Key, LayoutStyle, MeasureCtx, NamedKey, PaintCtx, Point, PointerButton, Rect,
    Size, TextAlign, TextMeasurer, Widget, WidgetId, WidgetStore,
};
use synui_render::paint_simple_rect;
use vello::Scene;

use crate::basic::Label;
use crate::scroll::{paint_v_scrollbar, VScrollbarState};
use crate::virtual_viewport::{layout_row_subtree, VirtualViewport};

const ROW_EXTENT: f64 = 28.0;
const INDENT: f64 = 16.0;
/// Fixed column for expand/collapse glyphs so leaf labels align with folder labels.
const CHEVRON_WIDTH: f64 = 18.0;
const ROW_PAD_X: f64 = 8.0;
const POOL_CAP: usize = 64;

/// Tree node description (owned model).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }
}

#[derive(Clone)]
struct FlatRow {
    id: String,
    label: String,
    depth: usize,
    has_children: bool,
}

/// Virtualized tree: paint labels or mount composable row widgets per visible node.
pub struct TreeView {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    mounted: Vec<(usize, WidgetId)>,
    /// Off-viewport rows retained by node id (Phase 18).
    pool: HashMap<String, WidgetId>,
    pool_order: VecDeque<String>,
    pub roots: Vec<TreeNode>,
    expanded: HashSet<String>,
    flat: Vec<FlatRow>,
    vp: VirtualViewport,
    pub selected: Option<String>,
    pub activated: Option<String>,
    /// Last expand/collapse toggle (node id, now-expanded) for facade polling.
    pub expand_dirty: Option<(String, bool)>,
    item_builder: Option<Box<dyn Fn(&str) -> Box<dyn ErasedWidget>>>,
    item_builder_store: Option<Box<dyn Fn(&mut WidgetStore, &str) -> WidgetId>>,
    use_mount: bool,
    focused: bool,
    scrollbar: VScrollbarState,
}

impl TreeView {
    pub fn new(roots: Vec<TreeNode>) -> Self {
        let mut t = Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            mounted: Vec::new(),
            pool: HashMap::new(),
            pool_order: VecDeque::new(),
            roots,
            expanded: HashSet::new(),
            flat: Vec::new(),
            vp: VirtualViewport::new(0, ROW_EXTENT),
            selected: None,
            activated: None,
            expand_dirty: None,
            item_builder: None,
            item_builder_store: None,
            use_mount: false,
            focused: false,
            scrollbar: VScrollbarState::default(),
        };
        t.rebuild_flat();
        crate::virtual_mount::register_virt_host(t.id, crate::virtual_mount::VirtHostKind::Tree);
        t
    }

    pub fn take_activated(&mut self) -> Option<String> {
        self.activated.take()
    }

    pub fn take_expand_dirty(&mut self) -> Option<(String, bool)> {
        self.expand_dirty.take()
    }

    pub fn is_expanded(&self, id: &str) -> bool {
        self.expanded.contains(id)
    }

    pub fn set_roots(&mut self, roots: Vec<TreeNode>) {
        self.roots = roots;
        self.rebuild_flat();
    }

    pub fn with_item_builder(
        mut self,
        f: impl Fn(&str) -> Box<dyn ErasedWidget> + 'static,
    ) -> Self {
        self.set_item_builder(f);
        self
    }

    pub fn set_item_builder(&mut self, f: impl Fn(&str) -> Box<dyn ErasedWidget> + 'static) {
        self.item_builder = Some(Box::new(f));
        self.item_builder_store = None;
        self.use_mount = true;
    }

    pub fn with_item_builder_store(
        mut self,
        f: impl Fn(&mut WidgetStore, &str) -> WidgetId + 'static,
    ) -> Self {
        self.item_builder_store = Some(Box::new(f));
        self.item_builder = None;
        self.use_mount = true;
        self
    }

    pub fn uses_mount(&self) -> bool {
        self.use_mount
    }

    pub fn mounted_count(&self) -> usize {
        self.mounted.len()
    }

    fn sync_vp_meta(&mut self) {
        self.vp.item_count = self.flat.len();
        self.vp.item_extent = ROW_EXTENT;
        self.vp.viewport_height = self.bounds.height().max(0.0);
    }

    pub fn rebuild_flat(&mut self) {
        self.flat.clear();
        fn walk(
            nodes: &[TreeNode],
            depth: usize,
            expanded: &HashSet<String>,
            out: &mut Vec<FlatRow>,
        ) {
            for n in nodes {
                let has = !n.children.is_empty();
                out.push(FlatRow {
                    id: n.id.clone(),
                    label: n.label.clone(),
                    depth,
                    has_children: has,
                });
                if has && expanded.contains(&n.id) {
                    walk(&n.children, depth + 1, expanded, out);
                }
            }
        }
        walk(&self.roots, 0, &self.expanded, &mut self.flat);
        self.sync_vp_meta();
        self.vp.clamp_scroll();
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

    pub fn offset_of(&self, index: usize) -> f64 {
        self.vp.offset_of(index)
    }

    fn row_content_x(&self, depth: usize) -> f64 {
        self.bounds.x0 + ROW_PAD_X + depth as f64 * INDENT + CHEVRON_WIDTH
    }

    fn row_content_width(&self, depth: usize) -> f64 {
        (self.bounds.x1 - self.row_content_x(depth) - 14.0).max(40.0)
    }

    fn index_at_y(&self, y: f64) -> Option<usize> {
        if self.flat.is_empty() {
            return None;
        }
        let local = y - self.bounds.y0 + self.vp.scroll_y;
        self.vp.index_at_offset(local)
    }

    fn toggle_expand(&mut self, id: &str) {
        let now_expanded = if self.expanded.contains(id) {
            self.expanded.remove(id);
            false
        } else {
            self.expanded.insert(id.to_string());
            true
        };
        self.expand_dirty = Some((id.to_string(), now_expanded));
        self.rebuild_flat();
    }

    fn ensure_selected_visible(&mut self) {
        let Some(sel) = &self.selected else {
            return;
        };
        let Some(idx) = self.flat.iter().position(|r| &r.id == sel) else {
            return;
        };
        self.vp.ensure_index_visible(idx);
    }

    fn select_index(&mut self, idx: usize) {
        if let Some(row) = self.flat.get(idx) {
            self.selected = Some(row.id.clone());
            self.ensure_selected_visible();
        }
    }

    fn paint_chevron(&self, ctx: &mut PaintCtx, scene: &mut Scene, row: &FlatRow, y0: f64) {
        if !row.has_children {
            return;
        }
        let Some(text) = ctx.text.as_deref_mut() else {
            return;
        };
        let indent = row.depth as f64 * INDENT;
        let chevron_x = self.bounds.x0 + ROW_PAD_X + indent;
        let marker = if self.expanded.contains(&row.id) {
            "▼"
        } else {
            "▶"
        };
        let chevron_pad = Rect::new(chevron_x, y0, chevron_x + CHEVRON_WIDTH, y0 + ROW_EXTENT);
        text.paint_text(
            scene,
            marker,
            11.0,
            ctx.theme.on_surface,
            chevron_pad,
            Some(CHEVRON_WIDTH as f32),
            TextAlign::Center,
        );
    }
}

impl Widget for TreeView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn manages_child_layout(&self) -> bool {
        true
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();
        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
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
        Some(self.bounds)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        paint_simple_rect(scene, self.bounds, ctx.theme.surface, 0.0);
        let (start, end) = self.visible_range();
        if !self.use_mount {
            if let Some(text) = ctx.text.as_deref_mut() {
                for i in start..end {
                    let row = &self.flat[i];
                    let y0 = self.bounds.y0 + self.offset_of(i) - self.vp.scroll_y;
                    let rect = Rect::new(self.bounds.x0, y0, self.bounds.x1, y0 + ROW_EXTENT);
                    if rect.y1 < self.bounds.y0 || rect.y0 > self.bounds.y1 {
                        continue;
                    }
                    if self.selected.as_deref() == Some(row.id.as_str()) {
                        let mut hl = ctx.theme.primary;
                        hl.components[3] = 0.18;
                        paint_simple_rect(scene, rect, hl, 0.0);
                    }
                    if row.has_children {
                        let indent = row.depth as f64 * INDENT;
                        let chevron_x = self.bounds.x0 + ROW_PAD_X + indent;
                        let marker = if self.expanded.contains(&row.id) {
                            "▼"
                        } else {
                            "▶"
                        };
                        let chevron_pad =
                            Rect::new(chevron_x, y0, chevron_x + CHEVRON_WIDTH, y0 + ROW_EXTENT);
                        text.paint_text(
                            scene,
                            marker,
                            11.0,
                            ctx.theme.on_surface,
                            chevron_pad,
                            Some(CHEVRON_WIDTH as f32),
                            TextAlign::Center,
                        );
                    }
                    let label_pad = Rect::new(
                        self.row_content_x(row.depth),
                        rect.y0,
                        rect.x1 - 14.0,
                        rect.y1,
                    );
                    text.paint_text(
                        scene,
                        &row.label,
                        13.0,
                        ctx.theme.on_surface,
                        label_pad,
                        Some(label_pad.width().max(1.0) as f32),
                        TextAlign::Start,
                    );
                }
            }
        } else {
            for i in start..end {
                let row = &self.flat[i];
                let y0 = self.bounds.y0 + self.offset_of(i) - self.vp.scroll_y;
                let rect = Rect::new(self.bounds.x0, y0, self.bounds.x1, y0 + ROW_EXTENT);
                if rect.y1 < self.bounds.y0 || rect.y0 > self.bounds.y1 {
                    continue;
                }
                if self.selected.as_deref() == Some(row.id.as_str()) {
                    let mut hl = ctx.theme.primary;
                    hl.components[3] = 0.18;
                    paint_simple_rect(scene, rect, hl, 0.0);
                }
                self.paint_chevron(ctx, scene, row, y0);
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
            self.bounds,
            self.vp.scroll_y,
            self.content_height(),
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
            } if self.bounds.contains(*position) => {
                if let Some(y) = self.scrollbar.pointer_down(
                    *position,
                    self.bounds,
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
                    let row = self.flat[idx].clone();
                    let indent = row.depth as f64 * INDENT;
                    let chevron_x0 = self.bounds.x0 + ROW_PAD_X + indent;
                    let chevron_x1 = chevron_x0 + CHEVRON_WIDTH;
                    if row.has_children && position.x >= chevron_x0 && position.x < chevron_x1 {
                        self.toggle_expand(&row.id);
                    }
                    self.selected = Some(row.id);
                }
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } => {
                if let Some(y) =
                    self.scrollbar
                        .pointer_move(*position, self.bounds, self.content_height())
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
            Event::Scroll {
                position, delta_y, ..
            } if self.bounds.contains(*position) || self.focused => {
                self.set_scroll_y(self.vp.scroll_y - *delta_y as f64);
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
                key: Key::Named(NamedKey::ArrowDown),
                ..
            } if self.focused => {
                let cur = self
                    .selected
                    .as_ref()
                    .and_then(|id| self.flat.iter().position(|r| &r.id == id))
                    .unwrap_or(0);
                let next = (cur + 1).min(self.flat.len().saturating_sub(1));
                self.select_index(next);
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowUp),
                ..
            } if self.focused => {
                let cur = self
                    .selected
                    .as_ref()
                    .and_then(|id| self.flat.iter().position(|r| &r.id == id))
                    .unwrap_or(0);
                self.select_index(cur.saturating_sub(1));
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowRight),
                ..
            } if self.focused => {
                if let Some(id) = self.selected.clone() {
                    if let Some(row) = self.flat.iter().find(|r| r.id == id) {
                        if row.has_children && !self.expanded.contains(&id) {
                            self.toggle_expand(&id);
                        }
                    }
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowLeft),
                ..
            } if self.focused => {
                if let Some(id) = self.selected.clone() {
                    if self.expanded.contains(&id) {
                        self.toggle_expand(&id);
                    }
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            } if self.focused => {
                if let Some(id) = self.selected.clone() {
                    self.activated = Some(id);
                }
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::List,
            label: Some(format!("Tree ({} visible)", self.flat.len())),
            value: self.selected.clone(),
            focusable: true,
            actions: vec![A11yAction::Focus],
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn hit_test_prefer_self(&self, point: Point) -> bool {
        VScrollbarState::hit(self.bounds, self.vp.scroll_y, self.content_height(), point)
            || self.scrollbar.is_dragging()
    }
}

/// Mount / recycle TreeView row widgets and lay out row subtrees.
pub fn sync_tree_views(store: &mut WidgetStore, text: &mut dyn TextMeasurer) {
    let ids: Vec<WidgetId> = {
        let mut out = Vec::new();
        if let Some(root) = store.root() {
            fn walk(store: &WidgetStore, id: WidgetId, out: &mut Vec<WidgetId>) {
                out.push(id);
                for &c in store.children_of(id) {
                    walk(store, c, out);
                }
            }
            walk(store, root, &mut out);
        }
        out
    };
    for id in ids {
        sync_tree_view(store, text, id);
    }
}

/// Sync a single TreeView host (Phase 26).
pub fn sync_tree_view(store: &mut WidgetStore, text: &mut dyn TextMeasurer, id: WidgetId) {
    let needs = store
        .get(id)
        .map(|w| {
            w.as_any()
                .downcast_ref::<TreeView>()
                .map(|tv| tv.use_mount)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !needs {
        return;
    }
    {
        let w = store.get_mut(id).unwrap();
        let tv = w.as_any_mut().downcast_mut::<TreeView>().unwrap();
        tv.sync_vp_meta();
    }
    let (start, end, scroll_y, old_mounted) = {
        let w = store.get(id).unwrap();
        let tv = w.as_any().downcast_ref::<TreeView>().unwrap();
        let (a, b) = tv.visible_range();
        (a, b, tv.vp.scroll_y, tv.mounted.clone())
    };
    let desired: Vec<usize> = (start..end).collect();
    let desired_set: HashSet<usize> = desired.iter().copied().collect();
    let mut keep: Vec<(usize, WidgetId)> = Vec::new();
    let mut to_pool: Vec<(String, WidgetId)> = Vec::new();
    for (idx, wid) in old_mounted {
        if desired_set.contains(&idx) {
            keep.push((idx, wid));
        } else {
            let node_id = store
                .get(id)
                .and_then(|w| {
                    w.as_any()
                        .downcast_ref::<TreeView>()
                        .and_then(|tv| tv.flat.get(idx).map(|r| r.id.clone()))
                })
                .unwrap_or_default();
            to_pool.push((node_id, wid));
        }
    }
    let mut remove_ids: Vec<WidgetId> = Vec::new();
    {
        let tv = store
            .get_mut(id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<TreeView>()
            .unwrap();
        for (node_id, wid) in to_pool {
            if let Some(old) = tv.pool.insert(node_id.clone(), wid) {
                remove_ids.push(old);
            }
            tv.pool_order.retain(|k| k != &node_id);
            tv.pool_order.push_back(node_id);
        }
        while tv.pool.len() > POOL_CAP {
            if let Some(evict) = tv.pool_order.pop_front() {
                if let Some(wid) = tv.pool.remove(&evict) {
                    remove_ids.push(wid);
                }
            } else {
                break;
            }
        }
    }
    for wid in remove_ids {
        store.remove(wid);
    }
    let keep_set: HashSet<usize> = keep.iter().map(|(i, _)| *i).collect();
    for &idx in &desired {
        if keep_set.contains(&idx) {
            continue;
        }
        let node_id = store
            .get(id)
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<TreeView>()
                    .and_then(|tv| tv.flat.get(idx).map(|r| r.id.clone()))
            })
            .unwrap_or_default();
        let pooled = {
            let tv = store
                .get_mut(id)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<TreeView>()
                .unwrap();
            tv.pool_order.retain(|k| k != &node_id);
            tv.pool.remove(&node_id)
        };
        let wid = if let Some(wid) = pooled {
            wid
        } else {
            let store_builder = {
                let tv = store
                    .get_mut(id)
                    .unwrap()
                    .as_any_mut()
                    .downcast_mut::<TreeView>()
                    .unwrap();
                tv.item_builder_store.take()
            };
            if let Some(builder) = store_builder {
                let wid = builder(store, &node_id);
                if let Some(w) = store.get_mut(id) {
                    if let Some(tv) = w.as_any_mut().downcast_mut::<TreeView>() {
                        tv.item_builder_store = Some(builder);
                    }
                }
                wid
            } else {
                let widget: Box<dyn ErasedWidget> = {
                    let w = store.get(id).unwrap();
                    let tv = w.as_any().downcast_ref::<TreeView>().unwrap();
                    if let Some(builder) = tv.item_builder.as_ref() {
                        builder(&node_id)
                    } else {
                        let label = tv
                            .flat
                            .get(idx)
                            .map(|r| r.label.clone())
                            .unwrap_or_default();
                        Box::new(Label::new(label))
                    }
                };
                store.insert_erased(widget)
            }
        };
        keep.push((idx, wid));
    }
    keep.sort_by_key(|(i, _)| *i);
    let child_ids: Vec<WidgetId> = keep.iter().map(|(_, wid)| *wid).collect();
    store.set_children(id, child_ids.clone());

    for &(idx, wid) in &keep {
        let (origin, size) = {
            let w = store.get(id).unwrap();
            let tv = w.as_any().downcast_ref::<TreeView>().unwrap();
            let row = tv.flat.get(idx);
            let depth = row.map(|r| r.depth).unwrap_or(0);
            let y0 = tv.bounds.y0 + tv.offset_of(idx) - scroll_y;
            (
                Point::new(tv.row_content_x(depth), y0),
                Size::new(tv.row_content_width(depth), ROW_EXTENT),
            )
        };
        layout_row_subtree(store, text, wid, origin, size);
    }
    if let Some(w) = store.get_mut(id) {
        if let Some(tv) = w.as_any_mut().downcast_mut::<TreeView>() {
            tv.mounted = keep;
            tv.child_ids = child_ids;
            tv.vp.clamp_scroll();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synui_render::TextRenderer;

    #[test]
    fn expand_flattens_children() {
        let roots = vec![TreeNode::new("a", "A")
            .with_children(vec![TreeNode::new("a1", "A1"), TreeNode::new("a2", "A2")])];
        let mut tv = TreeView::new(roots);
        assert_eq!(tv.flat.len(), 1);
        tv.toggle_expand("a");
        assert_eq!(tv.flat.len(), 3);
    }

    #[test]
    fn mount_bound_for_large_flat_tree() {
        let mut children = Vec::new();
        for i in 0..10_000 {
            children.push(TreeNode::new(format!("n{i}"), format!("Node {i}")));
        }
        let roots = vec![TreeNode::new("root", "Root").with_children(children)];
        let mut store = WidgetStore::new();
        let mut text = TextRenderer::new();
        let mut tv = TreeView::new(roots)
            .with_item_builder(|id| Box::new(Label::new(id.to_string())) as Box<dyn ErasedWidget>);
        tv.toggle_expand("root");
        assert_eq!(tv.flat.len(), 10_001);
        tv.set_bounds(Rect::new(0.0, 0.0, 400.0, 320.0));
        let id = store.insert(tv);
        store.set_root(id);
        sync_tree_views(&mut store, &mut text);
        let n = store
            .get(id)
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<TreeView>()
                    .map(|t| t.mounted_count())
            })
            .unwrap_or(0);
        assert!(n <= 40, "mounted {n} for 10k flat tree");
    }
}
