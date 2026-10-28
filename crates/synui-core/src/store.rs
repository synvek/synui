//! Widget tree storage and event dispatch.

use crate::event::{Event, EventCtx, EventResult, Gesture, Key, NamedKey};
use crate::focus::{FocusManager, GestureRecognizer};
use crate::geometry::{Point, Rect, Size};
use crate::overlay::{OverlayConfig, OverlayEntry, OverlayStack};
use crate::widget::{
    A11yAction, A11yOverride, A11yProps, ErasedWidget, LayoutDirection, PointerCursor, WidgetId,
};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

type SemanticActionHandler = Box<dyn FnMut(A11yAction, Option<String>) -> EventResult>;

/// Records where a widget lived before overlay reparenting.
#[derive(Debug, Clone)]
pub struct OverlayReparent {
    pub parent: WidgetId,
    pub index: usize,
}

/// Placement hint for positioned overlays (resolved by layout layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayPlacement {
    #[default]
    Below,
    Above,
    Start,
    End,
    Center,
    BottomSheet,
}

/// Phase 31: whether the next present must rebuild the whole scene or can retain it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintMode {
    /// Reset scene and paint every node.
    Full,
    /// Keep prior scene; erase damage rects and skip clean subtrees.
    Damaged,
}

/// Default consecutive [`PaintMode::Damaged`] presents before forcing Full compact (Phase 32).
pub const DAMAGED_FRAME_BUDGET: u32 = 32;

thread_local! {
    /// Debug: force next paints to ignore damage and paint the full tree.
    static FORCE_FULL_PAINT: Cell<bool> = Cell::new(false);
    /// Optional override for [`DAMAGED_FRAME_BUDGET`] (tests / `SYNUI_DAMAGED_FRAME_BUDGET`).
    static DAMAGED_BUDGET_OVERRIDE: Cell<Option<u32>> = Cell::new(None);
}

/// Debug: force full-tree paint (Phase 29).
pub fn force_full_paint(on: bool) {
    FORCE_FULL_PAINT.set(on);
}

pub fn force_full_paint_enabled() -> bool {
    FORCE_FULL_PAINT.get()
}

/// Effective Damaged-frame budget before Full compact (Phase 32).
pub fn damaged_frame_budget() -> u32 {
    if let Some(n) = DAMAGED_BUDGET_OVERRIDE.get() {
        return n.max(1);
    }
    if let Ok(s) = std::env::var("SYNUI_DAMAGED_FRAME_BUDGET") {
        if let Ok(n) = s.parse::<u32>() {
            return n.max(1);
        }
    }
    DAMAGED_FRAME_BUDGET
}

/// Test/debug: override Damaged-frame budget (`None` restores default/env).
pub fn set_damaged_frame_budget(budget: Option<u32>) {
    DAMAGED_BUDGET_OVERRIDE.set(budget);
}

/// Paint-path counters for damage MVP (Phase 29).
#[derive(Debug, Default, Clone, Copy)]
pub struct PaintStats {
    pub painted_nodes: u32,
    pub skipped_paint_nodes: u32,
}

/// Layout-path counters for dirty-subtree MVP (Phase 30).
#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutStats {
    /// Structure-stable passes that marked specific dirty widgets.
    pub partial_layout_passes: u32,
    /// Full / structure / unscoped layout passes.
    pub full_layout_passes: u32,
    /// Dirty widget marks consumed on the last partial pass.
    pub dirty_widgets_marked: u32,
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 0.0 && i.height() > 0.0
}

fn rect_hits_any(bounds: Rect, damage: &[Rect]) -> bool {
    damage.iter().any(|d| rects_overlap(bounds, *d))
}

static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a new widget identifier.
pub fn alloc_widget_id() -> WidgetId {
    WidgetId(NEXT_WIDGET_ID.fetch_add(1, Ordering::Relaxed))
}

/// Hover-tip session tracked by the host (delay + visible overlay owner).
#[derive(Debug, Clone)]
pub struct TooltipSession {
    /// Resolved tip owner under the cursor (may differ from raw hit via ancestors).
    pub candidate: Option<WidgetId>,
    /// Time spent hovering the current candidate.
    pub dwell_ms: f32,
    /// Anchor that currently owns the tip overlay.
    pub shown_anchor: Option<WidgetId>,
    /// Delay before showing (ms).
    pub show_delay_ms: f32,
}

impl Default for TooltipSession {
    fn default() -> Self {
        Self {
            candidate: None,
            dwell_ms: 0.0,
            shown_anchor: None,
            show_delay_ms: 500.0,
        }
    }
}

/// Stores the retained widget tree.
pub struct WidgetStore {
    widgets: HashMap<WidgetId, Box<dyn ErasedWidget>>,
    children: HashMap<WidgetId, Vec<WidgetId>>,
    /// Declarative Element semantics, keyed independently from native widget storage.
    semantic_overrides: HashMap<WidgetId, A11yOverride>,
    /// Reverse accessibility action routes installed by Engine adapters.
    semantic_action_handlers: HashMap<WidgetId, SemanticActionHandler>,
    /// Facade / host-assigned tip strings (in addition to [`Widget::tooltip`]).
    widget_tooltips: HashMap<WidgetId, String>,
    /// Hover tip controller state.
    tooltip_session: TooltipSession,
    root: Option<WidgetId>,
    focus: FocusManager,
    gestures: GestureRecognizer,
    overlays: OverlayStack,
    /// Owner widget (e.g. Dropdown) -> overlay stack root (ModalHost / PopupHost).
    overlay_by_owner: HashMap<WidgetId, WidgetId>,
    /// Overlay root -> owner widget.
    overlay_owner: HashMap<WidgetId, WidgetId>,
    /// Widget reparented into overlay; restore on pop.
    overlay_reparent: HashMap<WidgetId, OverlayReparent>,
    structure_version: u64,
    /// Measure-affecting content (text that changes leaf size, font, etc.).
    content_epoch: u64,
    /// Paint-only content (label chrome, selection, same-size visual props).
    paint_epoch: u64,
    pending_overlay_dismiss: Vec<WidgetId>,
    viewport: Size,
    layout_direction: LayoutDirection,
    needs_layout: bool,
    needs_paint: bool,
    /// When true, next layout is full-tree (structure / unscoped mark).
    layout_full: bool,
    /// Widgets marked layout-dirty (Phase 30); ignored when `layout_full`.
    dirty_layout_widgets: HashSet<WidgetId>,
    /// Last layout pass stats (Phase 30).
    last_layout_stats: LayoutStats,
    /// When true, next paint paints the full tree (layout / unmarked paint).
    damage_full: bool,
    /// Widgets marked paint-dirty (expanded to bounds at paint time).
    dirty_widgets: HashSet<WidgetId>,
    /// Explicit dirty rectangles (Phase 29).
    dirty_rects: Vec<Rect>,
    /// Last paint pass stats (Phase 29).
    last_paint_stats: PaintStats,
    /// Consecutive Damaged presents since last Full compact (Phase 32).
    damaged_frames_since_full: u32,
    /// Platform safe-area insets: [top, right, bottom, left] in px.
    safe_area: [f32; 4],
    /// When true, a11y tree should be rebuilt.
    a11y_dirty: bool,
    /// Widget receiving exclusive pointer move/up (scrollbar / slider drag).
    pointer_grab: Option<WidgetId>,
    /// Phase 40: widget under the cursor (for `:hover` cascade).
    hovered: Option<WidgetId>,
}

impl Default for WidgetStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetStore {
    pub fn new() -> Self {
        Self {
            widgets: HashMap::new(),
            children: HashMap::new(),
            semantic_overrides: HashMap::new(),
            semantic_action_handlers: HashMap::new(),
            widget_tooltips: HashMap::new(),
            tooltip_session: TooltipSession::default(),
            root: None,
            focus: FocusManager::default(),
            gestures: GestureRecognizer::new(),
            overlays: OverlayStack::new(),
            overlay_by_owner: HashMap::new(),
            overlay_owner: HashMap::new(),
            overlay_reparent: HashMap::new(),
            structure_version: 0,
            content_epoch: 0,
            paint_epoch: 0,
            pending_overlay_dismiss: Vec::new(),
            viewport: Size::new(800.0, 600.0),
            layout_direction: LayoutDirection::Ltr,
            needs_layout: true,
            needs_paint: true,
            layout_full: true,
            dirty_layout_widgets: HashSet::new(),
            last_layout_stats: LayoutStats::default(),
            damage_full: true,
            dirty_widgets: HashSet::new(),
            dirty_rects: Vec::new(),
            last_paint_stats: PaintStats::default(),
            damaged_frames_since_full: 0,
            safe_area: [0.0; 4],
            a11y_dirty: true,
            pointer_grab: None,
            hovered: None,
        }
    }

    pub fn structure_version(&self) -> u64 {
        self.structure_version
    }

    pub fn content_epoch(&self) -> u64 {
        self.content_epoch
    }

    pub fn paint_epoch(&self) -> u64 {
        self.paint_epoch
    }

    /// Paint-only prop change (selection chrome, label text treated as paint by DiffStats).
    pub fn bump_paint_epoch(&mut self) {
        self.paint_epoch = self.paint_epoch.wrapping_add(1);
        self.mark_paint_dirty();
    }

    /// Paint-only change scoped to one widget (Phase 29 damage).
    pub fn bump_paint_epoch_widget(&mut self, id: WidgetId) {
        self.paint_epoch = self.paint_epoch.wrapping_add(1);
        self.mark_paint_dirty_widget(id);
    }

    /// Measure-affecting content change: invalidate leaf sizes on next layout.
    pub fn bump_measure_epoch(&mut self) {
        self.content_epoch = self.content_epoch.wrapping_add(1);
        self.mark_layout_dirty();
    }

    /// Measure-affecting change scoped to one widget (Phase 30).
    pub fn bump_measure_epoch_widget(&mut self, id: WidgetId) {
        self.content_epoch = self.content_epoch.wrapping_add(1);
        self.mark_layout_dirty_widget(id);
    }

    /// Alias for [`Self::bump_measure_epoch`] (historical name).
    pub fn bump_content_epoch(&mut self) {
        self.bump_measure_epoch();
    }

    pub fn mark_structure_changed(&mut self) {
        self.structure_version = self.structure_version.wrapping_add(1);
        self.mark_layout_dirty();
    }

    pub fn overlay_for_owner(&self, owner: WidgetId) -> Option<WidgetId> {
        self.overlay_by_owner.get(&owner).copied()
    }

    pub fn overlay_owner_of(&self, overlay_root: WidgetId) -> Option<WidgetId> {
        self.overlay_owner.get(&overlay_root).copied()
    }

    pub fn take_pending_overlay_dismiss(&mut self) -> Vec<WidgetId> {
        std::mem::take(&mut self.pending_overlay_dismiss)
    }

    fn notify_overlay_dismissed(&mut self, owner: WidgetId) {
        self.pending_overlay_dismiss.push(owner);
    }

    pub fn set_viewport(&mut self, size: Size) {
        self.viewport = size;
    }

    pub fn viewport(&self) -> Size {
        self.viewport
    }

    pub fn set_layout_direction(&mut self, direction: LayoutDirection) {
        self.layout_direction = direction;
    }

    pub fn layout_direction(&self) -> LayoutDirection {
        self.layout_direction
    }

    pub fn insert(&mut self, widget: impl ErasedWidget + 'static) -> WidgetId {
        let id = widget.id();
        let focusable = widget.is_focusable();
        self.widgets.insert(id, Box::new(widget));
        self.focus.register(id, focusable);
        id
    }

    /// Insert an already type-erased widget (e.g. from a list item builder).
    pub fn insert_erased(&mut self, widget: Box<dyn ErasedWidget>) -> WidgetId {
        let id = widget.id();
        let focusable = widget.is_focusable();
        self.widgets.insert(id, widget);
        self.focus.register(id, focusable);
        id
    }

    /// Number of widgets currently in the store (for layout capacity hints).
    pub fn widget_count(&self) -> usize {
        self.widgets.len()
    }

    /// Remove a widget and all descendants from the store.
    pub fn remove(&mut self, id: WidgetId) -> bool {
        if !self.widgets.contains_key(&id) {
            return false;
        }
        if self.overlay_by_owner.contains_key(&id) {
            self.remove_overlay_for_owner(id);
        }
        let mut to_remove = vec![id];
        let mut i = 0;
        while i < to_remove.len() {
            let current = to_remove[i];
            if let Some(kids) = self.children.get(&current) {
                to_remove.extend(kids.iter().copied());
            }
            i += 1;
        }
        for wid in &to_remove {
            self.widgets.remove(wid);
            self.children.remove(wid);
            self.semantic_overrides.remove(wid);
            self.semantic_action_handlers.remove(wid);
            self.widget_tooltips.remove(wid);
            self.focus.remove(*wid);
            self.overlays.remove_widget(*wid);
        }
        for kids in self.children.values_mut() {
            kids.retain(|c| !to_remove.contains(c));
        }
        if self.root == Some(id) {
            self.root = None;
        }
        self.mark_structure_changed();
        true
    }

    /// Replace widget at id, preserving children list.
    pub fn replace(&mut self, id: WidgetId, widget: impl ErasedWidget + 'static) -> bool {
        if !self.widgets.contains_key(&id) {
            return false;
        }
        let new_id = widget.id();
        let kids = self.children.remove(&id).unwrap_or_default();
        let focusable = widget.is_focusable();
        self.widgets.remove(&id);
        self.focus.remove(id);

        if new_id != id {
            if let Some(semantic) = self.semantic_overrides.remove(&id) {
                self.semantic_overrides.insert(new_id, semantic);
            }
            if let Some(handler) = self.semantic_action_handlers.remove(&id) {
                self.semantic_action_handlers.insert(new_id, handler);
            }
            if let Some(parent) = self.parent_of(id) {
                if let Some(siblings) = self.children.get_mut(&parent) {
                    if let Some(pos) = siblings.iter().position(|&c| c == id) {
                        siblings[pos] = new_id;
                    }
                }
            }
            if self.root == Some(id) {
                self.root = Some(new_id);
            }
            for entry in self.overlays.iter_mut() {
                if entry.root == id {
                    entry.root = new_id;
                }
                for trap_id in entry.focus_trap.iter_mut() {
                    if *trap_id == id {
                        *trap_id = new_id;
                    }
                }
            }
        }

        self.widgets.insert(new_id, Box::new(widget));
        let effective_focusable = self
            .effective_a11y(new_id)
            .map(|props| props.focusable && !props.disabled)
            .unwrap_or(focusable);
        self.focus.register(new_id, effective_focusable);
        if !kids.is_empty() {
            self.children.insert(new_id, kids);
        }
        self.mark_structure_changed();
        true
    }

    pub fn set_root(&mut self, id: WidgetId) {
        if self.root != Some(id) {
            self.root = Some(id);
            self.mark_structure_changed();
        }
    }

    pub fn root(&self) -> Option<WidgetId> {
        self.root
    }

    pub fn set_children(&mut self, parent: WidgetId, children: Vec<WidgetId>) {
        let unchanged = self
            .children
            .get(&parent)
            .is_some_and(|current| current.as_slice() == children.as_slice());
        self.children.insert(parent, children);
        // Same ordered ids are not a structural change — avoid full-tree layout/paint
        // on every facade State rebuild (selection/status text, etc.).
        if !unchanged {
            self.mark_structure_changed();
        }
    }

    pub fn children_of(&self, id: WidgetId) -> &[WidgetId] {
        self.children.get(&id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// True when a widget was moved into an overlay host and should be laid out there.
    pub fn is_reparented_to_overlay(&self, id: WidgetId) -> bool {
        self.overlay_reparent.contains_key(&id)
    }

    /// Whether `child` should participate in layout under `parent`.
    pub fn should_layout_as_child(&self, parent: WidgetId, child: WidgetId) -> bool {
        // SplitPane (and similar) place children in sync_split_panes — if taffy
        // flex-lays them out, Detail parks on the table bottom for a frame.
        if self.get(parent).is_some_and(|w| w.manages_child_layout()) {
            return false;
        }
        if !self.is_reparented_to_overlay(child) {
            return true;
        }
        self.overlay_for_owner(child) == Some(parent)
    }

    /// Widget ids that own an active overlay entry.
    pub fn overlay_owners(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.overlay_by_owner.keys().copied()
    }

    pub fn get(&self, id: WidgetId) -> Option<&dyn ErasedWidget> {
        self.widgets.get(&id).map(|w| w.as_ref())
    }

    /// Replaces the sparse declarative accessibility override for one retained widget.
    pub fn set_semantic_override(&mut self, id: WidgetId, semantic: A11yOverride) -> bool {
        if !self.widgets.contains_key(&id) {
            return false;
        }
        self.semantic_overrides.insert(id, semantic);
        if let Some(props) = self.effective_a11y(id) {
            self.focus.register(id, props.focusable && !props.disabled);
        }
        self.mark_a11y_dirty();
        true
    }

    pub fn semantic_override(&self, id: WidgetId) -> Option<&A11yOverride> {
        self.semantic_overrides.get(&id)
    }

    /// Returns native widget semantics with explicitly declared Element fields layered on top.
    pub fn effective_a11y(&self, id: WidgetId) -> Option<A11yProps> {
        let native = self.widgets.get(&id)?.a11y();
        Some(match self.semantic_overrides.get(&id) {
            Some(semantic) => semantic.merge(native),
            None => native,
        })
    }

    /// Installs a direct reverse route for accessibility actions.
    pub fn set_semantic_action_handler(
        &mut self,
        id: WidgetId,
        handler: impl FnMut(A11yAction, Option<String>) -> EventResult + 'static,
    ) -> bool {
        if !self.widgets.contains_key(&id) {
            return false;
        }
        self.semantic_action_handlers.insert(id, Box::new(handler));
        true
    }

    pub fn clear_semantic_action_handler(&mut self, id: WidgetId) {
        self.semantic_action_handlers.remove(&id);
    }

    /// Dispatches an assistive-technology action without manufacturing facade pointer input.
    /// Native widgets retain the historical click fallback when no Engine route is installed.
    pub fn dispatch_a11y_action(
        &mut self,
        id: WidgetId,
        action: A11yAction,
        value: Option<String>,
    ) -> EventResult {
        if let Some(handler) = self.semantic_action_handlers.get_mut(&id) {
            let result = handler(action, value.clone());
            if result.handled {
                return result;
            }
        }
        match action {
            A11yAction::Click => self.dispatch_a11y_click(id),
            A11yAction::Focus => {
                if self.focus.focus(id) {
                    self.mark_a11y_dirty();
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            A11yAction::SetValue => EventResult::IGNORED,
        }
    }

    pub fn get_mut(&mut self, id: WidgetId) -> Option<&mut (dyn ErasedWidget + '_)> {
        self.widgets
            .get_mut(&id)
            .map(|w| w.as_mut() as &mut dyn ErasedWidget)
    }

    pub fn widgets_mut(&mut self) -> impl Iterator<Item = &mut (dyn ErasedWidget + '_)> {
        self.widgets
            .values_mut()
            .map(|w| w.as_mut() as &mut dyn ErasedWidget)
    }

    pub fn widgets(&self) -> impl Iterator<Item = &(dyn ErasedWidget + '_)> {
        self.widgets
            .values()
            .map(|w| w.as_ref() as &(dyn ErasedWidget + '_))
    }

    pub fn focus_manager(&self) -> &FocusManager {
        &self.focus
    }

    pub fn focus_manager_mut(&mut self) -> &mut FocusManager {
        &mut self.focus
    }

    /// Phase 40: widget currently under the pointer (for `:hover`).
    pub fn hovered(&self) -> Option<WidgetId> {
        self.hovered
    }

    /// Update hovered widget; returns true when the id changed.
    pub fn set_hovered(&mut self, id: Option<WidgetId>) -> bool {
        if self.hovered != id {
            self.hovered = id;
            true
        } else {
            false
        }
    }

    /// Resolve the cursor icon for the current pointer target (grab wins over hit).
    pub fn pointer_cursor_at(&self, point: Point) -> PointerCursor {
        if let Some(id) = self.pointer_grab() {
            if let Some(cursor) = self.get(id).and_then(|w| w.pointer_cursor()) {
                return cursor;
            }
        }
        self.hit_test(point)
            .and_then(|id| self.get(id).and_then(|w| w.pointer_cursor()))
            .unwrap_or(PointerCursor::Default)
    }

    /// Host-assigned tip text for `id`.
    pub fn set_widget_tooltip(&mut self, id: WidgetId, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            self.widget_tooltips.remove(&id);
        } else {
            self.widget_tooltips.insert(id, text);
        }
    }

    /// Clears a host-assigned tip.
    pub fn clear_widget_tooltip(&mut self, id: WidgetId) {
        self.widget_tooltips.remove(&id);
    }

    /// Tip text from the host map (not [`Widget::tooltip`]).
    pub fn widget_tooltip(&self, id: WidgetId) -> Option<&str> {
        self.widget_tooltips.get(&id).map(String::as_str)
    }

    /// Resolve tip text walking from `id` up to the root. Returns `(anchor, text)`.
    pub fn resolve_tooltip(&self, id: WidgetId) -> Option<(WidgetId, String)> {
        let mut cur = Some(id);
        while let Some(wid) = cur {
            if let Some(text) = self.widget_tooltips.get(&wid) {
                if !text.is_empty() {
                    return Some((wid, text.clone()));
                }
            }
            if let Some(text) = self.get(wid).and_then(|w| w.tooltip().map(str::to_string)) {
                if !text.is_empty() {
                    return Some((wid, text));
                }
            }
            cur = self.parent_of(wid);
        }
        None
    }

    pub fn tooltip_session(&self) -> &TooltipSession {
        &self.tooltip_session
    }

    pub fn tooltip_session_mut(&mut self) -> &mut TooltipSession {
        &mut self.tooltip_session
    }

    /// Whether `target` is `root` or a descendant of `root`.
    pub fn subtree_contains(&self, root: WidgetId, target: WidgetId) -> bool {
        if root == target {
            return true;
        }
        self.children_of(root)
            .iter()
            .any(|&c| self.subtree_contains(c, target))
    }

    pub fn overlay_stack(&self) -> &OverlayStack {
        &self.overlays
    }

    pub fn overlay_stack_mut(&mut self) -> &mut OverlayStack {
        &mut self.overlays
    }

    /// Push an overlay layer onto the stack.
    pub fn push_overlay(
        &mut self,
        root: WidgetId,
        config: OverlayConfig,
        focus_trap: Vec<WidgetId>,
    ) {
        if !focus_trap.is_empty() {
            self.focus.set_focus_trap(Some(focus_trap.clone()));
        }
        self.overlays.push(OverlayEntry {
            root,
            config,
            focus_trap,
        });
        // Overlay hosts/panels are new widgets outside the main tree. Bump
        // structure so the host LayoutEngine full-rebuilds instead of an
        // incremental pass that can walk Display::None drawer/dialog children
        // and create orphan taffy nodes (blank columns under MenuBar).
        self.mark_structure_changed();
    }

    /// Pop the top overlay and restore focus trap from the new top (if any).
    pub fn pop_overlay(&mut self) -> Option<OverlayEntry> {
        let entry = self.overlays.pop()?;
        self.on_overlay_removed(entry.root);
        if let Some(top) = self.overlays.top() {
            if !top.focus_trap.is_empty() {
                self.focus.set_focus_trap(Some(top.focus_trap.clone()));
            } else {
                self.focus.set_focus_trap(None);
            }
        } else {
            self.focus.set_focus_trap(None);
        }
        self.mark_structure_changed();
        Some(entry)
    }

    /// Position an overlay host and push it onto the stack.
    pub fn insert_overlay(
        &mut self,
        host: WidgetId,
        bounds: Rect,
        config: OverlayConfig,
        focus_trap: Vec<WidgetId>,
        owner: WidgetId,
    ) {
        if let Some(w) = self.get_mut(host) {
            w.set_bounds(bounds);
        }
        self.overlay_by_owner.insert(owner, host);
        self.overlay_owner.insert(host, owner);
        self.push_overlay(host, config, focus_trap);
    }

    /// Remove overlay owned by `owner`, cleaning up overlay widget subtree.
    pub fn remove_overlay_for_owner(&mut self, owner: WidgetId) {
        if let Some(host) = self.overlay_by_owner.remove(&owner) {
            while self.overlays.top().map(|e| e.root) == Some(host) {
                let entry = self.overlays.pop().expect("overlay entry");
                self.on_overlay_removed(entry.root);
            }
            self.remove_overlay_subtree(host);
            if self.overlays.top().is_some() {
                if let Some(top) = self.overlays.top() {
                    if !top.focus_trap.is_empty() {
                        self.focus.set_focus_trap(Some(top.focus_trap.clone()));
                    }
                }
            } else {
                self.focus.set_focus_trap(None);
            }
            self.mark_structure_changed();
        }
    }

    fn on_overlay_removed(&mut self, root: WidgetId) {
        if let Some(owner) = self.overlay_owner.remove(&root) {
            self.overlay_by_owner.remove(&owner);
            if let Some(reparent) = self.overlay_reparent.remove(&owner) {
                let mut kids = self
                    .children
                    .get(&reparent.parent)
                    .cloned()
                    .unwrap_or_default();
                let idx = reparent.index.min(kids.len());
                kids.insert(idx, owner);
                self.children.insert(reparent.parent, kids);
            }
            // Owner was reparented back; detach it before overlay host subtree removal.
            if let Some(kids) = self.children.get_mut(&root) {
                kids.retain(|&id| id != owner);
            }
        }
        for (child_owner, reparent) in self.overlay_reparent.clone() {
            if reparent.parent == root {
                self.overlay_reparent.remove(&child_owner);
            }
        }
    }

    fn remove_overlay_subtree(&mut self, root: WidgetId) {
        let mut to_remove = vec![root];
        let mut i = 0;
        while i < to_remove.len() {
            let current = to_remove[i];
            if let Some(kids) = self.children.get(&current) {
                to_remove.extend(kids.iter().copied());
            }
            i += 1;
        }
        for wid in to_remove {
            self.widgets.remove(&wid);
            self.children.remove(&wid);
            self.semantic_overrides.remove(&wid);
            self.semantic_action_handlers.remove(&wid);
            self.focus.remove(wid);
            self.overlays.remove_widget(wid);
            self.overlay_owner.remove(&wid);
            self.overlay_by_owner.remove(&wid);
            self.overlay_reparent.remove(&wid);
        }
    }

    /// Reparent `widget` under `overlay_host` for modal display; records original parent.
    pub fn reparent_into_overlay(&mut self, widget: WidgetId, overlay_host: WidgetId) {
        if self.overlay_reparent.contains_key(&widget) {
            return;
        }
        if let Some(parent) = self.parent_of(widget) {
            let siblings = self.children.get(&parent).cloned().unwrap_or_default();
            if let Some(index) = siblings.iter().position(|&c| c == widget) {
                let kids: Vec<WidgetId> = siblings.into_iter().filter(|&c| c != widget).collect();
                self.children.insert(parent, kids);
                self.overlay_reparent
                    .insert(widget, OverlayReparent { parent, index });
            }
        }
        self.set_children(overlay_host, vec![widget]);
    }

    /// Collect focusable widget ids in subtree (for focus trap).
    pub fn collect_focusable(&self, root: WidgetId) -> Vec<WidgetId> {
        let mut out = Vec::new();
        self.collect_focusable_recursive(root, &mut out);
        out
    }

    fn collect_focusable_recursive(&self, id: WidgetId, out: &mut Vec<WidgetId>) {
        if let Some(w) = self.widgets.get(&id) {
            if w.is_focusable() && !w.is_disabled() {
                out.push(id);
            }
        }
        for &child in self.children_of(id) {
            self.collect_focusable_recursive(child, out);
        }
    }

    /// Widget receiving exclusive pointer move/up (scrollbar / slider drag).
    pub fn pointer_grab(&self) -> Option<WidgetId> {
        self.pointer_grab
    }

    pub fn mark_dirty(&mut self) {
        self.needs_layout = true;
        self.needs_paint = true;
        self.layout_full = true;
        self.damage_full = true;
        self.a11y_dirty = true;
    }

    /// Request layout (and paint) on the next frame (full-tree).
    pub fn mark_layout_dirty(&mut self) {
        self.needs_layout = true;
        self.needs_paint = true;
        self.layout_full = true;
        self.damage_full = true;
        self.a11y_dirty = true;
    }

    /// Request layout for one widget (and ancestors via LayoutEngine) — Phase 30.
    pub fn mark_layout_dirty_widget(&mut self, id: WidgetId) {
        self.needs_layout = true;
        self.needs_paint = true;
        self.a11y_dirty = true;
        if !self.layout_full {
            self.dirty_layout_widgets.insert(id);
        }
        // Bounds may change — paint that widget; siblings may also move after layout,
        // so escalate paint to full for safety once layout runs (winit sets damage).
        self.mark_paint_dirty_widget(id);
    }

    /// True when the next layout must cover the full tree.
    pub fn layout_is_full(&self) -> bool {
        self.layout_full
    }

    /// Drain layout-dirty widget ids (empty when `layout_full`).
    pub fn take_dirty_layout_widgets(&mut self) -> HashSet<WidgetId> {
        std::mem::take(&mut self.dirty_layout_widgets)
    }

    pub fn last_layout_stats(&self) -> LayoutStats {
        self.last_layout_stats
    }

    pub fn take_layout_stats(&mut self) -> LayoutStats {
        std::mem::take(&mut self.last_layout_stats)
    }

    /// Record a partial layout pass (Phase 30).
    pub fn note_partial_layout(&mut self, dirty_count: u32) {
        self.last_layout_stats.partial_layout_passes = self
            .last_layout_stats
            .partial_layout_passes
            .saturating_add(1);
        self.last_layout_stats.dirty_widgets_marked = dirty_count;
    }

    /// Record a full layout pass (Phase 30).
    pub fn note_full_layout(&mut self) {
        self.last_layout_stats.full_layout_passes =
            self.last_layout_stats.full_layout_passes.saturating_add(1);
        self.last_layout_stats.dirty_widgets_marked = 0;
    }

    /// Request paint without forcing a full relayout (animations, caret blink).
    /// Escalates to full-tree damage (unknown region).
    pub fn mark_paint_dirty(&mut self) {
        self.needs_paint = true;
        self.damage_full = true;
    }

    /// Request paint for one widget's current bounds (Phase 29).
    pub fn mark_paint_dirty_widget(&mut self, id: WidgetId) {
        self.needs_paint = true;
        if !self.damage_full {
            self.dirty_widgets.insert(id);
        }
    }

    /// Phase 40: mark every widget paint-dirty **without** escalating to Full/`scene.reset`.
    ///
    /// Used for light/dark theme token refresh (text cache cleared separately).
    pub fn mark_all_widgets_paint_dirty(&mut self) {
        self.needs_paint = true;
        if self.damage_full {
            // Already Full — nothing more to scope.
            return;
        }
        for &id in self.widgets.keys() {
            self.dirty_widgets.insert(id);
        }
    }

    /// Request paint for an explicit rectangle (Phase 29).
    pub fn mark_paint_dirty_rect(&mut self, rect: Rect) {
        self.needs_paint = true;
        if !self.damage_full {
            self.dirty_rects.push(rect);
        }
    }

    /// True when the next paint must cover the full tree.
    pub fn damage_is_full(&self) -> bool {
        self.damage_full || force_full_paint_enabled()
    }

    /// Phase 31 paint policy for the upcoming present.
    pub fn paint_mode(&self) -> PaintMode {
        if self.damage_is_full() {
            return PaintMode::Full;
        }
        if self.dirty_widgets.is_empty() && self.dirty_rects.is_empty() {
            return PaintMode::Full;
        }
        PaintMode::Damaged
    }

    /// Consecutive Damaged presents since the last Full compact (Phase 32).
    pub fn damaged_frames_since_full(&self) -> u32 {
        self.damaged_frames_since_full
    }

    /// If Damaged-frame budget is exhausted, escalate to Full so the scene can reset.
    ///
    /// Call once per present, before reading [`Self::paint_mode`].
    pub fn prepare_damage_compact(&mut self) {
        if self.damaged_frames_since_full >= damaged_frame_budget() {
            self.mark_paint_dirty();
        }
    }

    /// Record that a present completed with `mode` (Phase 32 counter).
    pub fn note_paint_presented(&mut self, mode: PaintMode) {
        match mode {
            PaintMode::Full => self.damaged_frames_since_full = 0,
            PaintMode::Damaged => {
                self.damaged_frames_since_full = self.damaged_frames_since_full.saturating_add(1);
            }
        }
    }

    pub fn last_paint_stats(&self) -> PaintStats {
        self.last_paint_stats
    }

    pub fn take_paint_stats(&mut self) -> PaintStats {
        std::mem::take(&mut self.last_paint_stats)
    }

    pub fn needs_layout(&self) -> bool {
        self.needs_layout
    }

    pub fn needs_paint(&self) -> bool {
        self.needs_paint
    }

    pub fn is_dirty(&self) -> bool {
        self.needs_layout || self.needs_paint
    }

    pub fn clear_dirty(&mut self) {
        self.needs_layout = false;
        self.needs_paint = false;
        self.clear_layout_marks();
        self.clear_damage();
    }

    pub fn clear_layout_dirty(&mut self) {
        self.needs_layout = false;
        self.clear_layout_marks();
    }

    pub fn clear_paint_dirty(&mut self) {
        self.needs_paint = false;
        self.clear_damage();
    }

    fn clear_layout_marks(&mut self) {
        self.layout_full = false;
        self.dirty_layout_widgets.clear();
    }

    fn clear_damage(&mut self) {
        self.damage_full = false;
        self.dirty_widgets.clear();
        self.dirty_rects.clear();
    }

    pub fn set_safe_area(&mut self, insets: [f32; 4]) {
        if self.safe_area != insets {
            self.safe_area = insets;
            self.mark_layout_dirty();
        }
    }

    pub fn safe_area(&self) -> [f32; 4] {
        self.safe_area
    }

    pub fn mark_a11y_dirty(&mut self) {
        self.a11y_dirty = true;
    }

    pub fn take_a11y_dirty(&mut self) -> bool {
        let d = self.a11y_dirty;
        self.a11y_dirty = false;
        d
    }

    pub fn parent_of(&self, id: WidgetId) -> Option<WidgetId> {
        self.children.iter().find_map(|(parent, kids)| {
            if kids.contains(&id) {
                Some(*parent)
            } else {
                None
            }
        })
    }

    pub fn find_by_role(&self, role: crate::widget::A11yRole) -> Vec<WidgetId> {
        let mut found = Vec::new();
        if let Some(root) = self.root {
            self.find_by_role_recursive(root, role, &mut found);
        }
        for entry in self.overlays.iter() {
            self.find_by_role_recursive(entry.root, role, &mut found);
        }
        found
    }

    fn find_by_role_recursive(
        &self,
        id: WidgetId,
        role: crate::widget::A11yRole,
        found: &mut Vec<WidgetId>,
    ) {
        if self
            .effective_a11y(id)
            .is_some_and(|props| props.role == role)
        {
            found.push(id);
        }
        if let Some(kids) = self.children.get(&id) {
            for &child in kids {
                self.find_by_role_recursive(child, role, found);
            }
        }
    }

    pub fn find_by_label(&self, label: &str) -> Option<WidgetId> {
        if let Some(root) = self.root {
            if let Some(id) = self.find_by_label_recursive(root, label) {
                return Some(id);
            }
        }
        for entry in self.overlays.iter() {
            if let Some(id) = self.find_by_label_recursive(entry.root, label) {
                return Some(id);
            }
        }
        None
    }

    fn find_by_label_recursive(&self, id: WidgetId, label: &str) -> Option<WidgetId> {
        if self
            .effective_a11y(id)
            .is_some_and(|props| props.label.as_deref() == Some(label))
        {
            return Some(id);
        }
        for &child in self.children_of(id) {
            if let Some(found) = self.find_by_label_recursive(child, label) {
                return Some(found);
            }
        }
        None
    }

    pub fn hit_test(&self, point: Point) -> Option<WidgetId> {
        if let Some(id) = self.hit_test_overlays(point) {
            return Some(id);
        }
        let root = self.root?;
        self.hit_test_recursive(root, point)
    }

    fn hit_test_overlays(&self, point: Point) -> Option<WidgetId> {
        for entry in self.overlays.iter().rev() {
            if let Some(id) = self.hit_test_recursive(entry.root, point) {
                return Some(id);
            }
            if entry.config.modal {
                let vp = Rect::new(0.0, 0.0, self.viewport.width, self.viewport.height);
                if vp.contains(point) {
                    return Some(entry.root);
                }
            }
        }
        None
    }

    fn hit_test_recursive(&self, id: WidgetId, point: Point) -> Option<WidgetId> {
        let widget = self.widgets.get(&id)?;
        if !widget.hit_test(point) {
            return None;
        }
        if widget.hit_test_prefer_self(point) {
            return Some(id);
        }
        let mut deepest = id;
        if let Some(kids) = self.children.get(&id) {
            let mut ordered: Vec<WidgetId> = kids.clone();
            ordered.sort_by(|&a, &b| {
                let za = self
                    .widgets
                    .get(&a)
                    .and_then(|w| w.layout_style().z_index)
                    .unwrap_or(0);
                let zb = self
                    .widgets
                    .get(&b)
                    .and_then(|w| w.layout_style().z_index)
                    .unwrap_or(0);
                za.cmp(&zb).then_with(|| {
                    let ia = kids.iter().position(|&x| x == a);
                    let ib = kids.iter().position(|&x| x == b);
                    ia.cmp(&ib)
                })
            });
            // Hit-test high z first.
            for &child in ordered.iter().rev() {
                if let Some(found) = self.hit_test_recursive(child, point) {
                    deepest = found;
                    break;
                }
            }
        }
        Some(deepest)
    }

    fn overlay_backdrop_hit(&self, point: Point) -> Option<&OverlayEntry> {
        for entry in self.overlays.iter().rev() {
            if !entry.config.backdrop_dismiss {
                continue;
            }
            // Full-viewport ModalHost always contains the point — only treat a
            // *descendant* hit as content. Hitting the host alone (scrim) dismisses.
            let hit = self.hit_test_recursive(entry.root, point);
            let hit_content = hit.is_some_and(|id| id != entry.root);
            if !hit_content {
                let vp = Rect::new(0.0, 0.0, self.viewport.width, self.viewport.height);
                if vp.contains(point) {
                    return Some(entry);
                }
            }
        }
        None
    }

    pub fn dispatch_event(&mut self, event: &Event) -> EventResult {
        // Pointer / scroll need a rooted tree; ignore until the host builds UI.
        if self.root.is_none()
            && matches!(
                event,
                Event::PointerDown { .. }
                    | Event::PointerUp { .. }
                    | Event::PointerMove { .. }
                    | Event::PointerCancel { .. }
                    | Event::Scroll { .. }
            )
        {
            return EventResult::IGNORED;
        }

        if let Event::KeyDown {
            key: Key::Named(NamedKey::Tab),
            shift,
            ..
        } = event
        {
            let prev = self.focus.focused();
            if *shift {
                self.focus.focus_prev();
            } else {
                self.focus.focus_next();
            }
            self.emit_focus_change(prev);
            return EventResult::HANDLED;
        }

        if let Event::KeyDown {
            key: Key::Named(NamedKey::Escape),
            ..
        } = event
        {
            if let Some(top) = self.overlays.top() {
                let owner = self.overlay_owner.get(&top.root).copied();
                self.pop_overlay();
                if let Some(owner) = owner {
                    self.notify_overlay_dismissed(owner);
                }
                return EventResult::HANDLED;
            }
        }

        let mut gesture_event: Option<Event> = None;
        let target = match event {
            Event::PointerDown {
                position,
                id,
                button: _,
                ..
            } => {
                self.gestures.on_pointer_down(*id, *position);
                if let Some(_entry) = self.overlay_backdrop_hit(*position) {
                    let owner = self
                        .overlays
                        .top()
                        .and_then(|e| self.overlay_owner.get(&e.root).copied());
                    // Clicks on the overlay owner (MenuBar title / Dropdown field)
                    // must reach the owner so it can switch/toggle — not dismiss-only.
                    let click_on_owner = owner
                        .map(|o| self.hit_test_recursive(o, *position).is_some())
                        .unwrap_or(false);
                    if !click_on_owner {
                        self.pop_overlay();
                        if let Some(owner) = owner {
                            self.notify_overlay_dismissed(owner);
                        }
                        return EventResult::HANDLED;
                    }
                }
                if let Some(widget_id) = self.hit_test(*position) {
                    if let Some(w) = self.widgets.get(&widget_id) {
                        if w.is_disabled() {
                            return EventResult::IGNORED;
                        }
                    }
                    let prev = self.focus.focused();
                    if self
                        .widgets
                        .get(&widget_id)
                        .map(|w| w.is_focusable())
                        .unwrap_or(false)
                    {
                        self.focus.focus(widget_id);
                    } else {
                        self.focus.clear_focus();
                    }
                    self.emit_focus_change(prev);
                    widget_id
                } else if !self.overlays.is_empty() {
                    let prev = self.focus.focused();
                    self.focus.clear_focus();
                    self.emit_focus_change(prev);
                    return EventResult::HANDLED;
                } else {
                    let prev = self.focus.focused();
                    self.focus.clear_focus();
                    self.emit_focus_change(prev);
                    return EventResult::IGNORED;
                }
            }
            Event::PointerUp { position, id, .. } => {
                if let Some(gesture) = self.gestures.on_pointer_up(*id, *position) {
                    gesture_event = Some(Event::Gesture(gesture));
                }
                if let Some(grab) = self.pointer_grab {
                    grab
                } else {
                    self.hit_test(*position).unwrap_or_else(|| {
                        self.focus.focused().unwrap_or(self.root.expect("no root"))
                    })
                }
            }
            Event::PointerMove { position, id } => {
                if let Some(gesture) = self.gestures.on_pointer_move(*id, *position) {
                    gesture_event = Some(Event::Gesture(gesture));
                }
                if let Some(grab) = self.pointer_grab {
                    grab
                } else {
                    self.hit_test(*position).unwrap_or_else(|| {
                        self.focus
                            .focused()
                            .or(self.root)
                            .expect("no root: build UI before dispatching pointer events")
                    })
                }
            }
            Event::PointerCancel { id } => {
                self.gestures.on_pointer_cancel(*id);
                return EventResult::HANDLED;
            }
            Event::KeyDown { .. } | Event::KeyUp { .. } => {
                if let Some(id) = self.focus.focused() {
                    id
                } else {
                    return EventResult::IGNORED;
                }
            }
            Event::Scroll { position, .. } => self
                .hit_test(*position)
                .unwrap_or_else(|| self.root.expect("no root")),
            Event::Gesture(g) => {
                if let Some(pos) = gesture_position(g) {
                    self.hit_test(pos)
                        .unwrap_or_else(|| self.root.expect("no root"))
                } else {
                    self.root.expect("no root")
                }
            }
            Event::FocusGained | Event::FocusLost => {
                return EventResult::IGNORED;
            }
        };

        let result = self.dispatch_to_widget(target, event);
        if result.handled {
            return result;
        }
        if let Some(gesture_ev) = gesture_event {
            return self.dispatch_to_widget(target, &gesture_ev);
        }
        result
    }

    fn emit_focus_change(&mut self, previous: Option<WidgetId>) {
        let current = self.focus.focused();
        if previous == current {
            return;
        }
        if let Some(prev) = previous {
            let _ = self.dispatch_to_widget(prev, &Event::FocusLost);
            // Focus chrome (e.g. TextField outline) must repaint when focus leaves.
            self.mark_paint_dirty_widget(prev);
        }
        if let Some(cur) = current {
            let _ = self.dispatch_to_widget(cur, &Event::FocusGained);
            self.mark_paint_dirty_widget(cur);
        }
    }

    fn dispatch_to_widget(&mut self, target: WidgetId, event: &Event) -> EventResult {
        let path = self.path_to_root(target);
        for &widget_id in path.iter().rev() {
            let mut ctx = EventCtx {
                target: widget_id,
                propagation_stopped: false,
                path: &path,
                layout_direction: self.layout_direction,
                want_pointer_grab: false,
                want_pointer_release: false,
                want_paint: false,
            };
            let result = if let Some(w) = self.widgets.get_mut(&widget_id) {
                w.event(&mut ctx, event)
            } else {
                EventResult::IGNORED
            };
            if ctx.want_pointer_grab {
                self.pointer_grab = Some(widget_id);
            }
            if ctx.want_pointer_release && self.pointer_grab == Some(widget_id) {
                self.pointer_grab = None;
            }
            if ctx.want_paint {
                self.mark_paint_dirty_widget(widget_id);
            }
            if result.handled || ctx.propagation_stopped {
                // Always clear grab after PointerUp reaching the grab target.
                if matches!(event, Event::PointerUp { .. } | Event::PointerCancel { .. })
                    && self.pointer_grab == Some(widget_id)
                {
                    self.pointer_grab = None;
                }
                return EventResult {
                    handled: result.handled,
                    stop_propagation: true,
                };
            }
        }
        if matches!(event, Event::PointerUp { .. } | Event::PointerCancel { .. }) {
            self.pointer_grab = None;
        }
        EventResult::IGNORED
    }

    /// Dispatch an accessibility click to the widget mapped to `node_id`.
    pub fn dispatch_a11y_click(&mut self, widget_id: WidgetId) -> EventResult {
        if let Some(pos) = self.widgets.get(&widget_id).map(|w| w.bounds().center()) {
            let down = Event::PointerDown {
                id: 0,
                position: pos,
                button: crate::event::PointerButton::Primary,
                shift: false,
                ctrl: false,
            };
            let up = Event::PointerUp {
                id: 0,
                position: pos,
                button: crate::event::PointerButton::Primary,
            };
            let _ = self.dispatch_event(&down);
            self.dispatch_event(&up)
        } else {
            EventResult::IGNORED
        }
    }

    fn path_to_root(&self, mut id: WidgetId) -> Vec<WidgetId> {
        let mut path = vec![id];
        loop {
            let parent = self
                .children
                .iter()
                .find(|(_, kids)| kids.contains(&id))
                .map(|(p, _)| *p);
            match parent {
                Some(p) => {
                    id = p;
                    path.push(id);
                }
                None => {
                    for entry in self.overlays.iter() {
                        if entry.root == id {
                            break;
                        }
                        if self.is_descendant_of(entry.root, id) {
                            path.push(entry.root);
                            break;
                        }
                    }
                    break;
                }
            }
        }
        path.reverse();
        path
    }

    fn is_descendant_of(&self, ancestor: WidgetId, id: WidgetId) -> bool {
        if ancestor == id {
            return true;
        }
        for &child in self.children_of(ancestor) {
            if self.is_descendant_of(child, id) {
                return true;
            }
        }
        false
    }

    pub fn paint_all(&self, scene: &mut vello::Scene, paint_ctx: &mut crate::widget::PaintCtx) {
        let mut stats = PaintStats::default();
        if let Some(root) = self.root {
            self.paint_recursive(root, scene, paint_ctx, None, &mut stats);
        }
        for entry in self.overlays.iter() {
            if entry.config.modal {
                self.paint_scrim(scene, paint_ctx);
            }
            self.paint_recursive(entry.root, scene, paint_ctx, None, &mut stats);
        }
        // Note: paint_all is &self — stats written via paint_frame for mutable path.
        let _ = stats;
    }

    /// Paint with Phase 29/31 damage on a **retained** scene: erase dirty rects, then
    /// skip subtrees that miss them when not full.
    ///
    /// Each dirty rect is erased and redrawn under **its own clip** (not the AABB union of
    /// all dirty rects). A union clip would let ancestor opaque fills wipe clean widgets that
    /// sit in the “holes” between disjoint dirty regions (e.g. Morph + Lottie with a label
    /// between them, or a category-button ripple unioned with an animation below).
    ///
    /// Callers that `scene.reset()` / allocate a fresh `Scene` must use [`Self::paint_all`]
    /// (or call this only after a Full reset+paint_all baseline exists).
    pub fn paint_frame(
        &mut self,
        scene: &mut vello::Scene,
        paint_ctx: &mut crate::widget::PaintCtx,
    ) -> PaintStats {
        let use_damage = !self.damage_is_full();
        let damage = if use_damage {
            self.collect_damage_rects()
        } else {
            Vec::new()
        };
        let mut stats = PaintStats::default();
        if use_damage && !damage.is_empty() {
            // One erase+clip+paint pass per dirty rect so holes between disjoint
            // regions keep their retained encoding.
            for r in &damage {
                if r.width() <= 0.0 || r.height() <= 0.0 {
                    continue;
                }
                Self::erase_rects(scene, std::slice::from_ref(r), paint_ctx.theme.surface);
                {
                    use vello::kurbo::{Affine, RoundedRect};
                    use vello::peniko::Fill;
                    scene.push_clip_layer(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        &RoundedRect::from_rect(*r, 0.0),
                    );
                }
                let filter = std::slice::from_ref(r);
                if let Some(root) = self.root {
                    self.paint_recursive(root, scene, paint_ctx, Some(filter), &mut stats);
                }
                for entry in self.overlays.iter() {
                    if entry.config.modal {
                        self.paint_scrim(scene, paint_ctx);
                    }
                    self.paint_recursive(entry.root, scene, paint_ctx, Some(filter), &mut stats);
                }
                scene.pop_layer();
            }
        } else {
            if let Some(root) = self.root {
                self.paint_recursive(root, scene, paint_ctx, None, &mut stats);
            }
            for entry in self.overlays.iter() {
                if entry.config.modal {
                    self.paint_scrim(scene, paint_ctx);
                }
                self.paint_recursive(entry.root, scene, paint_ctx, None, &mut stats);
            }
        }
        self.last_paint_stats = stats;
        stats
    }

    fn erase_rects(scene: &mut vello::Scene, rects: &[Rect], color: vello::peniko::Color) {
        use vello::kurbo::{Affine, RoundedRect};
        use vello::peniko::Fill;
        for r in rects {
            if r.width() <= 0.0 || r.height() <= 0.0 {
                continue;
            }
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color,
                None,
                &RoundedRect::from_rect(*r, 0.0),
            );
        }
    }

    fn collect_damage_rects(&self) -> Vec<Rect> {
        // Inflate slightly so glyph antialiasing / subpixel ink outside the
        // tight layout box is erased (counter "6"→"7" ghosts on retained scenes).
        const PAD: f64 = 2.0;
        let inflate = |r: Rect| Rect::new(r.x0 - PAD, r.y0 - PAD, r.x1 + PAD, r.y1 + PAD);
        let mut out: Vec<Rect> = self.dirty_rects.iter().copied().map(inflate).collect();
        for &id in &self.dirty_widgets {
            if let Some(w) = self.widgets.get(&id) {
                let b = w.bounds();
                if b.width() > 0.0 && b.height() > 0.0 {
                    out.push(inflate(b));
                }
            }
        }
        out
    }

    fn paint_scrim(&self, scene: &mut vello::Scene, _paint_ctx: &mut crate::widget::PaintCtx) {
        use vello::kurbo::RoundedRect;
        use vello::peniko::Fill;
        let vp = Rect::new(0.0, 0.0, self.viewport.width, self.viewport.height);
        let mut scrim = vello::peniko::Color::BLACK;
        scrim.components[3] = 0.4;
        scene.fill(
            Fill::NonZero,
            vello::kurbo::Affine::IDENTITY,
            scrim,
            None,
            &RoundedRect::from_rect(vp, 0.0),
        );
    }

    fn paint_recursive(
        &self,
        id: WidgetId,
        scene: &mut vello::Scene,
        paint_ctx: &mut crate::widget::PaintCtx,
        damage: Option<&[Rect]>,
        stats: &mut PaintStats,
    ) {
        let Some(widget) = self.widgets.get(&id) else {
            return;
        };
        // Closed overlays / drawers use Display::None — skip the whole subtree
        // so children are not painted in the main flow.
        if widget.layout_style().display == crate::widget::Display::None {
            return;
        }
        let bounds = widget.bounds();
        if let Some(d) = damage {
            if !rect_hits_any(bounds, d) {
                stats.skipped_paint_nodes = stats.skipped_paint_nodes.saturating_add(1);
                self.count_skipped_descendants(id, stats);
                return;
            }
        }
        stats.painted_nodes = stats.painted_nodes.saturating_add(1);
        let saved = paint_ctx.bounds;
        let saved_color = paint_ctx.text_color;
        let saved_font = paint_ctx.font_size;
        paint_ctx.bounds = bounds;
        if let Some((color, font_size)) = widget.paint_inherit() {
            if let Some(c) = color {
                paint_ctx.text_color = Some(c.resolve(paint_ctx.theme));
            }
            if let Some(fs) = font_size {
                paint_ctx.font_size = Some(fs);
            }
        }
        let clip = widget.clip_bounds();
        if let Some(clip_rect) = clip {
            use vello::kurbo::RoundedRect;
            use vello::peniko::Fill;
            scene.push_clip_layer(
                Fill::NonZero,
                vello::kurbo::Affine::IDENTITY,
                &RoundedRect::from_rect(clip_rect, 0.0),
            );
        }
        widget.paint(paint_ctx, scene);
        if let Some(kids) = self.children.get(&id) {
            let mut ordered: Vec<WidgetId> = kids.clone();
            ordered.sort_by(|&a, &b| {
                let za = self
                    .widgets
                    .get(&a)
                    .and_then(|w| w.layout_style().z_index)
                    .unwrap_or(0);
                let zb = self
                    .widgets
                    .get(&b)
                    .and_then(|w| w.layout_style().z_index)
                    .unwrap_or(0);
                za.cmp(&zb).then_with(|| {
                    let ia = kids.iter().position(|&x| x == a);
                    let ib = kids.iter().position(|&x| x == b);
                    ia.cmp(&ib)
                })
            });
            for &child in &ordered {
                self.paint_recursive(child, scene, paint_ctx, damage, stats);
            }
        }
        if clip.is_some() {
            scene.pop_layer();
        }
        // Overlay (scrollbar / sticky header) paints above clipped body + children.
        if let Some(widget) = self.widgets.get(&id) {
            widget.paint_overlay(paint_ctx, scene);
        }
        paint_ctx.bounds = saved;
        paint_ctx.text_color = saved_color;
        paint_ctx.font_size = saved_font;
    }

    fn count_skipped_descendants(&self, id: WidgetId, stats: &mut PaintStats) {
        for &child in self.children_of(id) {
            if self
                .widgets
                .get(&child)
                .is_some_and(|w| w.layout_style().display != crate::widget::Display::None)
            {
                stats.skipped_paint_nodes = stats.skipped_paint_nodes.saturating_add(1);
                self.count_skipped_descendants(child, stats);
            }
        }
    }
}

fn gesture_position(g: &Gesture) -> Option<Point> {
    match g {
        Gesture::Tap { position } => Some(*position),
        Gesture::Drag { current, .. } => Some(*current),
        Gesture::Scroll { position, .. } => Some(*position),
        Gesture::Pinch { center, .. } => Some(*center),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Widget;

    struct Dummy {
        id: WidgetId,
        bounds: Rect,
    }

    impl Dummy {
        fn new() -> Self {
            Self {
                id: alloc_widget_id(),
                bounds: Rect::ZERO,
            }
        }
    }

    impl Widget for Dummy {
        fn id(&self) -> WidgetId {
            self.id
        }
        fn bounds(&self) -> Rect {
            self.bounds
        }
        fn set_bounds(&mut self, bounds: Rect) {
            self.bounds = bounds;
        }
        fn paint(&self, _ctx: &mut crate::widget::PaintCtx, _scene: &mut vello::Scene) {}
        fn a11y(&self) -> A11yProps {
            A11yProps {
                role: crate::A11yRole::Button,
                label: Some("native".into()),
                focusable: true,
                actions: vec![A11yAction::Focus],
                required: true,
                ..Default::default()
            }
        }
    }

    struct FocusDummy {
        inner: Dummy,
    }

    impl FocusDummy {
        fn new() -> Self {
            Self {
                inner: Dummy::new(),
            }
        }
    }

    impl Widget for FocusDummy {
        fn id(&self) -> WidgetId {
            self.inner.id()
        }
        fn bounds(&self) -> Rect {
            self.inner.bounds()
        }
        fn set_bounds(&mut self, bounds: Rect) {
            self.inner.set_bounds(bounds);
        }
        fn paint(&self, ctx: &mut crate::widget::PaintCtx, scene: &mut vello::Scene) {
            self.inner.paint(ctx, scene);
        }
        fn a11y(&self) -> A11yProps {
            self.inner.a11y()
        }
        fn is_focusable(&self) -> bool {
            true
        }
    }

    #[test]
    fn remove_widget_and_children() {
        let mut store = WidgetStore::new();
        let child = store.insert(Dummy::new());
        let parent = store.insert(Dummy::new());
        store.set_children(parent, vec![child]);
        store.set_root(parent);
        assert!(store.remove(child));
        assert!(store.get(child).is_none());
        assert_eq!(store.children_of(parent), &[] as &[WidgetId]);
    }

    #[test]
    fn semantic_sidecars_merge_migrate_and_are_removed_with_the_widget() {
        use std::cell::Cell;
        use std::rc::Rc;

        let mut store = WidgetStore::new();
        let original = store.insert(Dummy::new());
        let override_props = A11yOverride {
            role: Some(crate::A11yRole::Grid),
            label: Some("declared".into()),
            value: Some("42".into()),
            actions: Some(vec![A11yAction::Click]),
            disabled: Some(true),
            checked: Some(false),
            selected: Some(true),
            expanded: Some(false),
            read_only: Some(true),
            invalid: Some(true),
            row_count: Some(100_000),
            col_count: Some(12),
            row_index: Some(41),
            col_index: Some(3),
            ..Default::default()
        };
        assert!(store.set_semantic_override(original, override_props.clone()));

        let effective = store.effective_a11y(original).unwrap();
        assert_eq!(effective.role, crate::A11yRole::Grid);
        assert_eq!(effective.label.as_deref(), Some("declared"));
        assert_eq!(effective.value.as_deref(), Some("42"));
        assert!(
            effective.required,
            "unspecified native state must survive merge"
        );
        assert_eq!(effective.row_count, Some(100_000));
        assert_eq!(effective.col_count, Some(12));

        let calls = Rc::new(Cell::new(0));
        let routed_calls = Rc::clone(&calls);
        assert!(
            store.set_semantic_action_handler(original, move |action, _| {
                if action == A11yAction::Click {
                    routed_calls.set(routed_calls.get() + 1);
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            })
        );

        let replacement = Dummy::new();
        let replacement_id = replacement.id();
        assert!(store.replace(original, replacement));
        assert!(store.semantic_override(original).is_none());
        assert_eq!(
            store.semantic_override(replacement_id),
            Some(&override_props)
        );
        assert_eq!(
            store.effective_a11y(replacement_id).unwrap().row_count,
            Some(100_000)
        );
        assert_eq!(
            store.dispatch_a11y_action(replacement_id, A11yAction::Click, None),
            EventResult::HANDLED
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(
            store.dispatch_a11y_action(replacement_id, A11yAction::Focus, None),
            EventResult::IGNORED,
            "migrated disabled semantics must also govern focus registration"
        );

        assert!(store.remove(replacement_id));
        assert!(store.semantic_override(replacement_id).is_none());
        assert_eq!(
            store.dispatch_a11y_action(replacement_id, A11yAction::Click, None),
            EventResult::IGNORED
        );
        assert_eq!(
            calls.get(),
            1,
            "remove must clear the reverse-route sidecar"
        );
    }

    #[test]
    fn overlay_stack_push_pop() {
        let mut store = WidgetStore::new();
        let overlay = store.insert(Dummy::new());
        store.push_overlay(overlay, OverlayConfig::modal(), vec![]);
        assert_eq!(store.overlay_stack().len(), 1);
        store.pop_overlay();
        assert!(store.overlay_stack().is_empty());
    }

    #[test]
    fn paint_damage_skips_disjoint_subtree() {
        let mut store = WidgetStore::new();
        let mut a = Dummy::new();
        a.bounds = Rect::new(0.0, 0.0, 50.0, 50.0);
        let mut b = Dummy::new();
        b.bounds = Rect::new(200.0, 200.0, 250.0, 250.0);
        let mut parent = Dummy::new();
        parent.bounds = Rect::new(0.0, 0.0, 400.0, 400.0);
        let id_a = store.insert(a);
        let id_b = store.insert(b);
        let id_parent = store.insert(parent);
        store.set_children(id_parent, vec![id_a, id_b]);
        store.set_root(id_parent);
        store.clear_paint_dirty();
        store.mark_paint_dirty_widget(id_a);

        let theme = crate::widget::ThemeRef::default();
        let mut scene = vello::Scene::new();
        let mut paint_ctx = crate::widget::PaintCtx {
            bounds: Rect::new(0.0, 0.0, 400.0, 400.0),
            theme: &theme,
            text: None,
            icons: None,
            images: None,
            lotties: None,
            text_color: None,
            font_size: None,
        };
        let stats = store.paint_frame(&mut scene, &mut paint_ctx);
        assert!(
            stats.skipped_paint_nodes >= 1,
            "expected skip of disjoint child, got {stats:?}"
        );
        assert!(
            stats.painted_nodes >= 2,
            "parent + dirty child should paint, got {stats:?}"
        );
        let _ = id_b;
    }

    #[test]
    fn focus_change_repaints_previous_and_new_widget() {
        use crate::{Event, PointerButton};

        let mut store = WidgetStore::new();
        let mut a = FocusDummy::new();
        a.set_bounds(Rect::new(0.0, 0.0, 50.0, 50.0));
        let mut b = FocusDummy::new();
        b.set_bounds(Rect::new(60.0, 0.0, 110.0, 50.0));
        let mut parent = FocusDummy::new();
        parent.set_bounds(Rect::new(0.0, 0.0, 200.0, 50.0));
        let id_a = store.insert(a);
        let id_b = store.insert(b);
        let id_parent = store.insert(parent);
        store.set_children(id_parent, vec![id_a, id_b]);
        store.set_root(id_parent);
        store.set_viewport(Size::new(200.0, 50.0));

        store.dispatch_event(&Event::PointerDown {
            id: 0,
            position: Point::new(25.0, 25.0),
            button: PointerButton::Primary,
            shift: false,
            ctrl: false,
        });
        assert_eq!(store.focus_manager().focused(), Some(id_a));
        store.clear_paint_dirty();

        store.dispatch_event(&Event::PointerDown {
            id: 0,
            position: Point::new(85.0, 25.0),
            button: PointerButton::Primary,
            shift: false,
            ctrl: false,
        });
        assert_eq!(store.focus_manager().focused(), Some(id_b));
        assert!(
            store.needs_paint(),
            "switching focus must repaint the previous and new widgets"
        );
        let _ = id_parent;
    }

    #[test]
    fn paint_mode_damaged_when_scoped() {
        let mut store = WidgetStore::new();
        let id = store.insert(Dummy::new());
        store.set_root(id);
        store.clear_paint_dirty();
        store.mark_paint_dirty_widget(id);
        assert_eq!(store.paint_mode(), PaintMode::Damaged);
        store.mark_paint_dirty();
        assert_eq!(store.paint_mode(), PaintMode::Full);
    }

    #[test]
    fn mark_all_widgets_paint_dirty_stays_damaged() {
        let mut store = WidgetStore::new();
        let a = store.insert(Dummy::new());
        let b = store.insert(Dummy::new());
        store.set_root(a);
        store.set_children(a, vec![b]);
        store.clear_paint_dirty();
        store.mark_all_widgets_paint_dirty();
        assert_eq!(store.paint_mode(), PaintMode::Damaged);
        assert!(store.needs_paint());
    }

    #[test]
    fn ime_commit_like_path_stays_damaged() {
        // Phase 38: Commit uses bump_measure_epoch_widget + mark_paint_dirty_widget (not Full).
        let mut store = WidgetStore::new();
        let id = store.insert(Dummy::new());
        store.set_root(id);
        store.clear_paint_dirty();
        store.bump_measure_epoch_widget(id);
        store.mark_paint_dirty_widget(id);
        assert_eq!(store.paint_mode(), PaintMode::Damaged);
        assert!(!store.damage_is_full());
    }

    #[test]
    fn retained_paint_frame_second_pass_skips() {
        let mut store = WidgetStore::new();
        let mut a = Dummy::new();
        a.bounds = Rect::new(0.0, 0.0, 50.0, 50.0);
        let mut b = Dummy::new();
        b.bounds = Rect::new(200.0, 200.0, 250.0, 250.0);
        let mut parent = Dummy::new();
        parent.bounds = Rect::new(0.0, 0.0, 400.0, 400.0);
        let id_a = store.insert(a);
        let id_b = store.insert(b);
        let id_parent = store.insert(parent);
        store.set_children(id_parent, vec![id_a, id_b]);
        store.set_root(id_parent);

        let theme = crate::widget::ThemeRef::default();
        let mut scene = vello::Scene::new();
        let mut paint_ctx = crate::widget::PaintCtx {
            bounds: Rect::new(0.0, 0.0, 400.0, 400.0),
            theme: &theme,
            text: None,
            icons: None,
            images: None,
            lotties: None,
            text_color: None,
            font_size: None,
        };
        store.mark_paint_dirty();
        store.paint_all(&mut scene, &mut paint_ctx);
        store.clear_paint_dirty();

        store.mark_paint_dirty_widget(id_a);
        assert_eq!(store.paint_mode(), PaintMode::Damaged);
        let stats = store.paint_frame(&mut scene, &mut paint_ctx);
        assert!(
            stats.skipped_paint_nodes >= 1,
            "retained damaged pass should skip clean sibling, got {stats:?}"
        );
        let _ = id_b;
    }

    /// Ancestor fills must not wipe retained siblings: damage paint is clipped
    /// per dirty rect (not AABB union of all dirty regions).
    #[test]
    fn damaged_paint_clips_to_dirty_union() {
        let mut store = WidgetStore::new();
        let mut leaf = Dummy::new();
        leaf.bounds = Rect::new(10.0, 10.0, 40.0, 40.0);
        let mut parent = Dummy::new();
        parent.bounds = Rect::new(0.0, 0.0, 400.0, 400.0);
        let id_leaf = store.insert(leaf);
        let id_parent = store.insert(parent);
        store.set_children(id_parent, vec![id_leaf]);
        store.set_root(id_parent);
        store.clear_paint_dirty();
        store.mark_paint_dirty_widget(id_leaf);
        let theme = crate::widget::ThemeRef::default();
        let mut scene = vello::Scene::new();
        let mut paint_ctx = crate::widget::PaintCtx {
            bounds: Rect::new(0.0, 0.0, 400.0, 400.0),
            theme: &theme,
            text: None,
            icons: None,
            images: None,
            lotties: None,
            text_color: None,
            font_size: None,
        };
        // Baseline + damaged pass must succeed (clip push/pop balanced).
        // Without per-rect clip, ancestor surface fills would blank retained
        // siblings outside the dirty leaf (gallery blank-UI class of bug).
        store.mark_paint_dirty();
        store.paint_all(&mut scene, &mut paint_ctx);
        store.clear_paint_dirty();
        store.mark_paint_dirty_widget(id_leaf);
        assert_eq!(store.paint_mode(), PaintMode::Damaged);
        let stats = store.paint_frame(&mut scene, &mut paint_ctx);
        assert!(stats.painted_nodes >= 1);
        assert!(
            !store.damage_is_full(),
            "clip regression guard runs on Damaged path only"
        );
    }

    /// Disjoint dirty regions must not use an AABB union clip: a clean sibling
    /// in the gap (Morph title / Lottie label) stays skipped and is not wiped.
    #[test]
    fn damaged_paint_preserves_gap_between_disjoint_dirty() {
        let mut store = WidgetStore::new();
        let mut top = Dummy::new();
        top.bounds = Rect::new(0.0, 0.0, 100.0, 40.0);
        let mut mid = Dummy::new();
        mid.bounds = Rect::new(0.0, 50.0, 100.0, 70.0);
        let mut bot = Dummy::new();
        bot.bounds = Rect::new(0.0, 80.0, 100.0, 120.0);
        let mut parent = Dummy::new();
        parent.bounds = Rect::new(0.0, 0.0, 100.0, 120.0);
        let id_top = store.insert(top);
        let id_mid = store.insert(mid);
        let id_bot = store.insert(bot);
        let id_parent = store.insert(parent);
        store.set_children(id_parent, vec![id_top, id_mid, id_bot]);
        store.set_root(id_parent);

        let theme = crate::widget::ThemeRef::default();
        let mut scene = vello::Scene::new();
        let mut paint_ctx = crate::widget::PaintCtx {
            bounds: Rect::new(0.0, 0.0, 100.0, 120.0),
            theme: &theme,
            text: None,
            icons: None,
            images: None,
            lotties: None,
            text_color: None,
            font_size: None,
        };
        store.mark_paint_dirty();
        store.paint_all(&mut scene, &mut paint_ctx);
        store.clear_paint_dirty();

        store.mark_paint_dirty_widget(id_top);
        store.mark_paint_dirty_widget(id_bot);
        assert_eq!(store.paint_mode(), PaintMode::Damaged);
        let stats = store.paint_frame(&mut scene, &mut paint_ctx);
        // Mid sits in the AABB hole between top and bot — must be skipped (retained).
        // Parent + top (+ancestors) paint in pass 1; parent + bot in pass 2.
        assert!(
            stats.skipped_paint_nodes >= 1,
            "gap sibling must stay skipped under per-rect clips, got {stats:?}"
        );
        let _ = id_mid;
    }

    /// Phase 32: consecutive Damaged presents hit budget → force Full compact.
    #[test]
    fn damaged_frame_budget_forces_full_compact() {
        set_damaged_frame_budget(Some(3));
        let mut store = WidgetStore::new();
        let mut a = Dummy::new();
        a.bounds = Rect::new(0.0, 0.0, 50.0, 50.0);
        let mut b = Dummy::new();
        b.bounds = Rect::new(200.0, 200.0, 250.0, 250.0);
        let mut parent = Dummy::new();
        parent.bounds = Rect::new(0.0, 0.0, 400.0, 400.0);
        let id_a = store.insert(a);
        let id_b = store.insert(b);
        let id_parent = store.insert(parent);
        store.set_children(id_parent, vec![id_a, id_b]);
        store.set_root(id_parent);

        let theme = crate::widget::ThemeRef::default();
        let mut scene = vello::Scene::new();
        let mut paint_ctx = crate::widget::PaintCtx {
            bounds: Rect::new(0.0, 0.0, 400.0, 400.0),
            theme: &theme,
            text: None,
            icons: None,
            images: None,
            lotties: None,
            text_color: None,
            font_size: None,
        };
        store.mark_paint_dirty();
        store.paint_all(&mut scene, &mut paint_ctx);
        store.note_paint_presented(PaintMode::Full);
        store.clear_paint_dirty();

        let mut saw_damaged = 0u32;
        let mut forced_full = false;
        for _ in 0..5 {
            store.mark_paint_dirty_widget(id_a);
            store.prepare_damage_compact();
            let mode = store.paint_mode();
            match mode {
                PaintMode::Damaged => {
                    saw_damaged += 1;
                    let stats = store.paint_frame(&mut scene, &mut paint_ctx);
                    assert!(stats.skipped_paint_nodes >= 1);
                }
                PaintMode::Full => {
                    forced_full = true;
                    scene.reset();
                    store.paint_all(&mut scene, &mut paint_ctx);
                }
            }
            store.note_paint_presented(mode);
            store.clear_paint_dirty();
            if mode == PaintMode::Full {
                assert_eq!(
                    store.damaged_frames_since_full(),
                    0,
                    "Full present must clear Damaged counter"
                );
            }
        }
        assert!(
            saw_damaged >= 3,
            "budget allows Damaged frames before compact, got {saw_damaged}"
        );
        assert!(
            forced_full,
            "after budget, prepare_damage_compact must escalate to Full"
        );
        set_damaged_frame_budget(None);
        let _ = id_b;
    }

    #[test]
    fn mark_layout_dirty_widget_is_partial() {
        let mut store = WidgetStore::new();
        let child = store.insert(Dummy::new());
        let parent = store.insert(Dummy::new());
        store.set_children(parent, vec![child]);
        store.set_root(parent);
        store.clear_dirty();
        assert!(!store.layout_is_full());
        store.mark_layout_dirty_widget(child);
        assert!(store.needs_layout());
        assert!(!store.layout_is_full());
        let dirty = store.take_dirty_layout_widgets();
        assert!(dirty.contains(&child));
        store.note_partial_layout(1);
        assert_eq!(store.last_layout_stats().partial_layout_passes, 1);
        store.mark_layout_dirty();
        assert!(store.layout_is_full());
    }

    #[test]
    fn set_children_same_ids_does_not_bump_structure() {
        let mut store = WidgetStore::new();
        let child = store.insert(Dummy::new());
        let parent = store.insert(Dummy::new());
        store.set_children(parent, vec![child]);
        let version = store.structure_version();
        store.clear_dirty();
        store.set_children(parent, vec![child]);
        assert_eq!(store.structure_version(), version);
        assert!(!store.layout_is_full());
        store.set_children(parent, vec![]);
        assert_ne!(store.structure_version(), version);
    }
}
