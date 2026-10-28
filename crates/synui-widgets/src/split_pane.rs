//! Draggable two-pane splitter (Phase 18 workspace shell).

use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Constraints, Dimension, Event, EventCtx, EventResult, Key,
    LayoutStyle, MeasureCtx, NamedKey, PaintCtx, Point, PointerButton, PointerCursor, Rect, Size,
    Style, TextMeasurer, VisualStyle, Widget, WidgetId, WidgetStore,
};
use synui_render::paint_simple_rect;
use vello::peniko::Color;
use vello::Scene;

use crate::styled::paint_border_sides;

use crate::virtual_viewport::layout_row_subtree;

const DIVIDER_THICKNESS: f64 = 6.0;
const DEFAULT_MIN: f64 = 80.0;

/// Split orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    /// Primary on the left, secondary on the right.
    Horizontal,
    /// Primary on top, secondary on the bottom.
    Vertical,
}

/// Two-child split pane with a draggable divider.
pub struct SplitPane {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    layout_style: LayoutStyle,
    pub direction: SplitDirection,
    /// Fraction of space given to the primary pane (0..=1), excluding divider.
    pub ratio: f32,
    pub min_primary: f64,
    pub min_secondary: f64,
    dragging: bool,
    drag_origin: f64,
    drag_ratio0: f32,
    focused: bool,
    /// Set when the user changes ratio (drag end or keyboard nudge).
    ratio_dirty: bool,
    /// Optional divider fill; `None` uses theme outline with hover/drag alpha.
    pub divider_color: Option<Color>,
    /// Optional CSS-subset chrome for the divider (e.g. `border-right:1px solid #888`).
    /// When per-side borders are set without a background, the default fill is skipped.
    pub divider_visual: VisualStyle,
}

impl SplitPane {
    pub fn new(direction: SplitDirection) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            layout_style: LayoutStyle {
                display: synui_core::Display::Flex,
                flex_direction: synui_core::FlexDirection::Column,
                width: Dimension::Fill,
                height: Dimension::Fill,
                flex_grow: 1.0,
                flex_shrink: 1.0,
                ..LayoutStyle::base()
            },
            direction,
            ratio: 0.35,
            min_primary: DEFAULT_MIN,
            min_secondary: DEFAULT_MIN,
            dragging: false,
            drag_origin: 0.0,
            drag_ratio0: 0.35,
            focused: false,
            ratio_dirty: false,
            divider_color: None,
            divider_visual: VisualStyle::default(),
        }
    }

    pub fn horizontal() -> Self {
        Self::new(SplitDirection::Horizontal)
    }

    pub fn vertical() -> Self {
        Self::new(SplitDirection::Vertical)
    }

    pub fn with_ratio(mut self, ratio: f32) -> Self {
        self.ratio = ratio.clamp(0.05, 0.95);
        self
    }

    pub fn with_min_sizes(mut self, primary: f64, secondary: f64) -> Self {
        self.min_primary = primary.max(0.0);
        self.min_secondary = secondary.max(0.0);
        self
    }

    /// Solid divider color (RGBA). When unset, uses theme outline at reduced alpha.
    pub fn with_divider_color(mut self, color: Color) -> Self {
        self.divider_color = Some(color);
        self
    }

    pub fn set_divider_color(&mut self, color: Option<Color>) {
        self.divider_color = color;
    }

    /// Apply a CSS-subset string to the divider chrome (`border-right`, `background`, …).
    pub fn with_divider_css(mut self, css: &str) -> Result<Self, synui_core::StyleParseError> {
        self.set_divider_css(css)?;
        Ok(self)
    }

    pub fn set_divider_css(&mut self, css: &str) -> Result<(), synui_core::StyleParseError> {
        let style = Style::from_css(css)?;
        self.divider_visual = style.visual;
        Ok(())
    }

    pub fn set_divider_visual(&mut self, visual: VisualStyle) {
        self.divider_visual = visual;
    }

    pub fn with_children(mut self, children: Vec<WidgetId>) -> Self {
        self.child_ids = children;
        self
    }

    /// Replace direct children without changing widget identity.
    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.child_ids = children;
    }

    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio.clamp(0.05, 0.95);
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn take_ratio_dirty(&mut self) -> bool {
        let d = self.ratio_dirty;
        self.ratio_dirty = false;
        d
    }

    fn mark_ratio_dirty(&mut self) {
        self.ratio_dirty = true;
    }

    /// Clamp `ratio` so both panes respect min sizes for the given outer extent.
    pub fn clamp_ratio_for_extent(&self, extent: f64) -> f32 {
        let usable = (extent - DIVIDER_THICKNESS).max(0.0);
        if usable <= 0.0 {
            return self.ratio;
        }
        let min_p = self.min_primary.min(usable);
        let min_s = self.min_secondary.min(usable - min_p);
        let max_p = (usable - min_s).max(min_p);
        let primary = (self.ratio as f64 * usable).clamp(min_p, max_p);
        (primary / usable).clamp(0.05, 0.95) as f32
    }

    pub fn primary_bounds(&self) -> Rect {
        let ratio = self.clamp_ratio_for_extent(self.split_extent());
        match self.direction {
            SplitDirection::Horizontal => {
                let w = (self.bounds.width() - DIVIDER_THICKNESS).max(0.0) * ratio as f64;
                Rect::new(
                    self.bounds.x0,
                    self.bounds.y0,
                    self.bounds.x0 + w,
                    self.bounds.y1,
                )
            }
            SplitDirection::Vertical => {
                let h = (self.bounds.height() - DIVIDER_THICKNESS).max(0.0) * ratio as f64;
                Rect::new(
                    self.bounds.x0,
                    self.bounds.y0,
                    self.bounds.x1,
                    self.bounds.y0 + h,
                )
            }
        }
    }

    pub fn secondary_bounds(&self) -> Rect {
        let ratio = self.clamp_ratio_for_extent(self.split_extent());
        match self.direction {
            SplitDirection::Horizontal => {
                let usable = (self.bounds.width() - DIVIDER_THICKNESS).max(0.0);
                let w = usable * ratio as f64;
                Rect::new(
                    self.bounds.x0 + w + DIVIDER_THICKNESS,
                    self.bounds.y0,
                    self.bounds.x1,
                    self.bounds.y1,
                )
            }
            SplitDirection::Vertical => {
                let usable = (self.bounds.height() - DIVIDER_THICKNESS).max(0.0);
                let h = usable * ratio as f64;
                Rect::new(
                    self.bounds.x0,
                    self.bounds.y0 + h + DIVIDER_THICKNESS,
                    self.bounds.x1,
                    self.bounds.y1,
                )
            }
        }
    }

    pub fn divider_bounds(&self) -> Rect {
        let ratio = self.clamp_ratio_for_extent(self.split_extent());
        match self.direction {
            SplitDirection::Horizontal => {
                let usable = (self.bounds.width() - DIVIDER_THICKNESS).max(0.0);
                let x = self.bounds.x0 + usable * ratio as f64;
                Rect::new(x, self.bounds.y0, x + DIVIDER_THICKNESS, self.bounds.y1)
            }
            SplitDirection::Vertical => {
                let usable = (self.bounds.height() - DIVIDER_THICKNESS).max(0.0);
                let y = self.bounds.y0 + usable * ratio as f64;
                Rect::new(self.bounds.x0, y, self.bounds.x1, y + DIVIDER_THICKNESS)
            }
        }
    }

    fn split_extent(&self) -> f64 {
        match self.direction {
            SplitDirection::Horizontal => self.bounds.width(),
            SplitDirection::Vertical => self.bounds.height(),
        }
    }

    fn nudge_ratio(&mut self, delta: f32) {
        let extent = self.split_extent();
        self.ratio = self.clamp_ratio_for_extent(extent);
        self.ratio = (self.ratio + delta).clamp(0.05, 0.95);
        self.ratio = self.clamp_ratio_for_extent(extent);
        self.mark_ratio_dirty();
    }
}

impl Widget for SplitPane {
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
        &self.layout_style
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        // Do not persist clamp into `ratio` — that fights controlled State and makes
        // the secondary pane jump for a frame when taffy touches SplitPane bounds.
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(constraints.max_width as f64, constraints.max_height as f64)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let bounds = self.divider_bounds();
        let vis = &self.divider_visual;
        let sides_only = vis.has_per_side_border() && vis.background.is_none();
        if let Some(bg) = vis.background {
            let mut color = bg.resolve(ctx.theme);
            if self.dragging {
                color.components[3] = (color.components[3] * 1.25).min(1.0);
            }
            paint_simple_rect(scene, bounds, color, 0.0);
        } else if !sides_only {
            let fill = if let Some(mut color) = self.divider_color {
                if self.dragging {
                    color.components[3] = (color.components[3] * 1.25).min(1.0);
                }
                color
            } else {
                let mut fill = ctx.theme.outline;
                fill.components[3] = if self.dragging { 0.55 } else { 0.28 };
                fill
            };
            paint_simple_rect(scene, bounds, fill, 0.0);
        }
        if vis.has_border() {
            paint_border_sides(scene, bounds, vis, ctx.theme);
        }
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        match event {
            Event::FocusGained => {
                self.focused = true;
                EventResult::HANDLED
            }
            Event::FocusLost => {
                self.focused = false;
                EventResult::HANDLED
            }
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                ..
            } => {
                if self.divider_bounds().contains(*position) {
                    self.dragging = true;
                    self.focused = true;
                    self.drag_ratio0 = self.ratio;
                    self.drag_origin = match self.direction {
                        SplitDirection::Horizontal => position.x,
                        SplitDirection::Vertical => position.y,
                    };
                    ctx.grab_pointer();
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            Event::PointerMove { position, .. } => {
                if self.dragging {
                    let extent = self.split_extent();
                    let usable = (extent - DIVIDER_THICKNESS).max(1.0);
                    let delta = match self.direction {
                        SplitDirection::Horizontal => position.x - self.drag_origin,
                        SplitDirection::Vertical => position.y - self.drag_origin,
                    };
                    let next = self.drag_ratio0 as f64 + delta / usable;
                    self.ratio = next.clamp(0.05, 0.95) as f32;
                    self.ratio = self.clamp_ratio_for_extent(extent);
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            Event::PointerUp {
                button: PointerButton::Primary,
                ..
            } => {
                if self.dragging {
                    self.dragging = false;
                    self.mark_ratio_dirty();
                    ctx.release_pointer();
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            Event::KeyDown { key, .. } if self.focused => match (self.direction, key) {
                (SplitDirection::Horizontal, Key::Named(NamedKey::ArrowLeft)) => {
                    self.nudge_ratio(-0.02);
                    EventResult::HANDLED
                }
                (SplitDirection::Horizontal, Key::Named(NamedKey::ArrowRight)) => {
                    self.nudge_ratio(0.02);
                    EventResult::HANDLED
                }
                (SplitDirection::Vertical, Key::Named(NamedKey::ArrowUp)) => {
                    self.nudge_ratio(-0.02);
                    EventResult::HANDLED
                }
                (SplitDirection::Vertical, Key::Named(NamedKey::ArrowDown)) => {
                    self.nudge_ratio(0.02);
                    EventResult::HANDLED
                }
                _ => EventResult::IGNORED,
            },
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Generic,
            label: Some("Split pane".into()),
            value: Some(format!("{:.0}%", self.ratio * 100.0)),
            actions: vec![A11yAction::Focus],
            ..A11yProps::default()
        }
    }

    fn hit_test_prefer_self(&self, point: Point) -> bool {
        self.divider_bounds().contains(point) || self.dragging
    }

    fn pointer_cursor(&self) -> Option<PointerCursor> {
        Some(match self.direction {
            SplitDirection::Horizontal => PointerCursor::ColResize,
            SplitDirection::Vertical => PointerCursor::RowResize,
        })
    }
}

fn bounds_match(actual: Rect, target: Rect) -> bool {
    const EPS: f64 = 0.5;
    (actual.x0 - target.x0).abs() < EPS
        && (actual.y0 - target.y0).abs() < EPS
        && (actual.x1 - target.x1).abs() < EPS
        && (actual.y1 - target.y1).abs() < EPS
}

fn place_split_child(
    store: &mut WidgetStore,
    text: &mut dyn TextMeasurer,
    child: WidgetId,
    target: Rect,
    force: bool,
) {
    let already = store
        .get(child)
        .map(|w| bounds_match(w.bounds(), target))
        .unwrap_or(false);
    // Shell bounds can match after Tab swaps Table↔Preview while the new child
    // is still at (0,0) — force relayout when the store structure changed.
    if already && !force {
        return;
    }
    let is_nested_split = store
        .get(child)
        .map(|w| w.as_any().is::<SplitPane>())
        .unwrap_or(false);
    if is_nested_split {
        if let Some(w) = store.get_mut(child) {
            w.set_bounds(target);
        }
        return;
    }
    layout_row_subtree(
        store,
        text,
        child,
        Point::new(target.x0, target.y0),
        Size::new(target.width(), target.height()),
    );
}

thread_local! {
    static LAST_SPLIT_STRUCTURE: std::cell::Cell<u64> = const { std::cell::Cell::new(u64::MAX) };
}

/// Lay out SplitPane children into primary/secondary regions (overwrites taffy child bounds).
pub fn sync_split_panes(store: &mut WidgetStore, text: &mut dyn TextMeasurer) {
    let structure = store.structure_version();
    let force = LAST_SPLIT_STRUCTURE.get() != structure;
    LAST_SPLIT_STRUCTURE.set(structure);

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
        let is_split = store
            .get(id)
            .and_then(|w| w.as_any().downcast_ref::<SplitPane>())
            .is_some();
        if !is_split {
            continue;
        }
        let (primary, secondary, kids) = {
            let w = store.get(id).unwrap();
            let sp = w.as_any().downcast_ref::<SplitPane>().unwrap();
            (
                sp.primary_bounds(),
                sp.secondary_bounds(),
                store.children_of(id).to_vec(),
            )
        };
        // Paint-only frames (status/detail text) must not re-layout panes — that
        // flash-rebuilds DataTable cells along the divider.
        // Nested SplitPane: only place the shell. `layout_row_subtree` would run
        // taffy flex on its children (~50/50) and park Detail over the table
        // bottom until the nested iteration fixes it — one-frame upward jump.
        if let Some(&cid) = kids.first() {
            place_split_child(store, text, cid, primary, force);
        }
        if let Some(&cid) = kids.get(1) {
            place_split_child(store, text, cid, secondary, force);
        }
        if let Some(w) = store.get_mut(id) {
            if let Some(sp) = w.as_any_mut().downcast_mut::<SplitPane>() {
                sp.child_ids = kids;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_clamped_by_min_sizes() {
        let pane = SplitPane::horizontal()
            .with_ratio(0.1)
            .with_min_sizes(120.0, 120.0);
        let r = pane.clamp_ratio_for_extent(400.0);
        assert!(r >= 0.30, "ratio {r}");
        assert!(r <= 0.70, "ratio {r}");
    }

    #[test]
    fn child_bounds_follow_ratio() {
        let mut pane = SplitPane::horizontal()
            .with_ratio(0.4)
            .with_min_sizes(40.0, 40.0);
        pane.set_bounds(Rect::new(0.0, 0.0, 500.0, 300.0));
        let p = pane.primary_bounds();
        let s = pane.secondary_bounds();
        let d = pane.divider_bounds();
        assert!((p.width() + d.width() + s.width() - 500.0).abs() < 1.0);
        assert_eq!(p.height(), 300.0);
        assert_eq!(s.height(), 300.0);
        assert!(p.x1 <= d.x0 + 0.01);
        assert!(d.x1 <= s.x0 + 0.01);
    }

    #[test]
    fn divider_reports_resize_cursor_and_accepts_color() {
        let h = SplitPane::horizontal().with_divider_color(Color::from_rgb8(200, 100, 50));
        assert_eq!(h.pointer_cursor(), Some(PointerCursor::ColResize));
        assert!(h.divider_color.is_some());
        let v = SplitPane::vertical();
        assert_eq!(v.pointer_cursor(), Some(PointerCursor::RowResize));
        assert!(v.divider_color.is_none());
    }

    #[test]
    fn divider_css_border_right_skips_default_fill_flag() {
        let pane = SplitPane::horizontal()
            .with_divider_css("border-right:1px solid #888888")
            .expect("css");
        assert!(pane.divider_visual.has_per_side_border());
        assert!(pane.divider_visual.background.is_none());
    }

    #[test]
    fn vertical_split_bounds() {
        let mut pane = SplitPane::vertical().with_ratio(0.25);
        pane.set_bounds(Rect::new(10.0, 20.0, 210.0, 420.0));
        let p = pane.primary_bounds();
        let s = pane.secondary_bounds();
        assert!(p.height() < s.height());
        assert!((p.height() + DIVIDER_THICKNESS + s.height() - 400.0).abs() < 1.0);
    }

    #[test]
    fn nested_split_child_is_independent() {
        // Outer horizontal + inner vertical: both clamp independently.
        let mut outer = SplitPane::horizontal().with_ratio(0.3);
        outer.set_bounds(Rect::new(0.0, 0.0, 800.0, 600.0));
        let mut inner = SplitPane::vertical().with_ratio(0.5);
        let sec = outer.secondary_bounds();
        inner.set_bounds(sec);
        assert!(inner.primary_bounds().height() > 100.0);
        assert!(inner.secondary_bounds().height() > 100.0);
        outer.set_ratio(0.5);
        outer.mark_ratio_dirty();
        assert!(outer.take_ratio_dirty());
        assert!(!outer.take_ratio_dirty());
    }

    #[test]
    fn set_bounds_does_not_mutate_ratio() {
        let mut pane = SplitPane::vertical().with_ratio(0.72);
        pane.set_bounds(Rect::new(0.0, 0.0, 200.0, 100.0)); // mins would clamp if persisted
        assert!((pane.ratio - 0.72).abs() < f32::EPSILON);
    }

    #[test]
    fn structure_change_relayouts_split_child_even_when_shell_bounds_match() {
        use crate::{Flex, Label};
        use synui_core::WidgetStore;

        struct DummyText;
        impl TextMeasurer for DummyText {
            fn measure_text(
                &mut self,
                _text: &str,
                font_size: f32,
                _max_width: Option<f32>,
            ) -> Size {
                Size::new(40.0, font_size as f64 * 1.2)
            }
        }

        let mut store = WidgetStore::new();
        let mut text = DummyText;
        let placeholder = store.insert(Label::new("preview"));
        let mut main = Flex::new().column();
        main = main.with_children(vec![placeholder]);
        let primary = store.insert(main);
        store.set_children(primary, vec![placeholder]);
        let secondary = store.insert(Label::new("detail"));
        let mut split = SplitPane::vertical().with_ratio(0.72);
        split.replace_children(vec![primary, secondary]);
        let split_id = store.insert(split);
        store.set_children(split_id, vec![primary, secondary]);
        store.set_root(split_id);
        if let Some(w) = store.get_mut(split_id) {
            w.set_bounds(Rect::new(0.0, 0.0, 200.0, 300.0));
        }
        sync_split_panes(&mut store, &mut text);
        let primary_bounds = store.get(primary).unwrap().bounds();
        assert!(primary_bounds.height() > 1.0);

        // Tab swap: main shell stays in the primary slot; new table mounts at origin.
        let table = store.insert(Label::new("Name"));
        store.set_children(primary, vec![table]);
        assert!(
            bounds_match(store.get(primary).unwrap().bounds(), primary_bounds),
            "shell still matches primary slot (would skip without force)"
        );
        assert!(
            store.get(table).unwrap().bounds().x0.abs() < 0.5
                && store.get(table).unwrap().bounds().y0.abs() < 0.5,
            "new table starts at origin"
        );

        sync_split_panes(&mut store, &mut text);
        let table_bounds = store.get(table).unwrap().bounds();
        assert!(
            table_bounds.x0 >= primary_bounds.x0 - 0.5
                && table_bounds.y0 >= primary_bounds.y0 - 0.5,
            "table must leave origin after structure-forced split sync, got {table_bounds:?}"
        );
    }

    #[test]
    fn split_pane_children_are_not_taffy_flex_children() {
        use crate::Label;
        use synui_core::WidgetStore;
        use synui_layout::LayoutEngine;

        struct DummyText;
        impl TextMeasurer for DummyText {
            fn measure_text(
                &mut self,
                _text: &str,
                font_size: f32,
                _max_width: Option<f32>,
            ) -> Size {
                Size::new(40.0, font_size as f64 * 1.2)
            }
        }

        let mut store = WidgetStore::new();
        let primary = store.insert(Label::new("primary"));
        let secondary = store.insert(Label::new("Detail — folder"));
        // Park secondary at the correct bottom slot before layout.
        if let Some(w) = store.get_mut(secondary) {
            w.set_bounds(Rect::new(0.0, 220.0, 200.0, 300.0));
        }
        let mut split = SplitPane::vertical().with_ratio(0.72);
        split.replace_children(vec![primary, secondary]);
        let split_id = store.insert(split);
        store.set_children(split_id, vec![primary, secondary]);
        store.set_root(split_id);

        let before = store.get(secondary).unwrap().bounds();
        let mut engine = LayoutEngine::new();
        let mut text = DummyText;
        engine.compute(&mut store, &mut text, split_id, Size::new(200.0, 300.0));
        let after = store.get(secondary).unwrap().bounds();
        // Taffy must not park Detail at ~50% (the table-bottom flash).
        assert!(
            (after.y0 - before.y0).abs() < 0.5,
            "layout moved secondary from {before:?} to {after:?}"
        );
        assert!(
            !store.should_layout_as_child(split_id, secondary),
            "split children must be excluded from taffy"
        );
    }
}
