//! Styled box host for CSS-subset [`synui_core::Style`].

use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Constraints, Event, EventCtx, EventResult, LayoutStyle,
    MeasureCtx, PaintCtx, PointerButton, Rect, Size, Style, VisualStyle, Widget, WidgetId,
};
use synui_render::paint_simple_rect;
use vello::kurbo::{Line, RoundedRect, Stroke};
use vello::peniko::Color;
use vello::Scene;

/// Cascade inputs retained for Phase 40 pseudo / media re-resolve without `view_fn`.
#[derive(Debug, Clone)]
pub struct CascadeMeta {
    pub classes: Vec<String>,
    pub local: Style,
    pub ancestors: Vec<String>,
    /// Parent computed style at last apply (narrow inherit source).
    pub parent: Style,
}

/// Layout + visual host wrapping a single child (or more via store children).
pub struct StyledBox {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    layout_style: LayoutStyle,
    pub visual: VisualStyle,
    /// Phase 40: inputs for interaction-driven cascade refresh.
    pub cascade: Option<CascadeMeta>,
    pub on_press: Option<Box<dyn FnMut()>>,
    pressed: bool,
}

impl StyledBox {
    pub fn new(style: Style) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            layout_style: style.layout,
            visual: style.visual,
            cascade: None,
            on_press: None,
            pressed: false,
        }
    }

    pub fn on_press(mut self, f: impl FnMut() + 'static) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }

    pub fn with_cascade(mut self, meta: CascadeMeta) -> Self {
        self.cascade = Some(meta);
        self
    }

    pub fn with_children(mut self, children: Vec<WidgetId>) -> Self {
        self.child_ids = children;
        self
    }

    /// Replace direct children without changing widget identity.
    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.child_ids = children;
    }

    pub fn apply_style(&mut self, style: &Style) {
        self.layout_style = style.layout.clone();
        self.visual = style.visual.clone();
    }

    pub fn layout_style_mut(&mut self) -> &mut LayoutStyle {
        &mut self.layout_style
    }
}

impl Widget for StyledBox {
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

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        // Intrinsic size only — Fill/Percent stretch is applied by Taffy after
        // track/flex sizing. Returning max constraints here made `1fr` tracks
        // treat each cell as full-container min-content (A/B/C spill off-screen).
        let pad_w = (self.layout_style.padding[1] + self.layout_style.padding[3]) as f64;
        let pad_h = (self.layout_style.padding[0] + self.layout_style.padding[2]) as f64;
        let w = match self.layout_style.width {
            synui_core::Dimension::Length(v) => v as f64,
            synui_core::Dimension::Fill
            | synui_core::Dimension::Percent(_)
            | synui_core::Dimension::Auto => pad_w,
        };
        let h = match self.layout_style.height {
            synui_core::Dimension::Length(v) => v as f64,
            synui_core::Dimension::Fill
            | synui_core::Dimension::Percent(_)
            | synui_core::Dimension::Auto => pad_h,
        };
        Size::new(
            w.min(constraints.max_width as f64),
            h.min(constraints.max_height as f64),
        )
    }

    fn paint_inherit(&self) -> Option<(Option<synui_core::ColorValue>, Option<f32>)> {
        if self.visual.color.is_some() || self.visual.font_size.is_some() {
            Some((self.visual.color, self.visual.font_size))
        } else {
            None
        }
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let radius = self.visual.border_radius.unwrap_or(0.0) as f64;
        if let Some(bg) = self.visual.background {
            let mut color = bg.resolve(ctx.theme);
            if let Some(op) = self.visual.opacity {
                color = with_opacity(color, op);
            }
            paint_simple_rect(scene, self.bounds, color, radius);
        } else if let Some(op) = self.visual.opacity {
            // Opacity without background: draw a transparent surface tint (no-op if 1.0).
            if op < 1.0 {
                let mut c = ctx.theme.surface;
                c = with_opacity(c, op);
                paint_simple_rect(scene, self.bounds, c, radius);
            }
        }
        if self.visual.has_per_side_border() {
            paint_border_sides(scene, self.bounds, &self.visual, ctx.theme);
        } else if let (Some(w), Some(bc)) = (self.visual.border_width, self.visual.border_color) {
            if w > 0.0 {
                let color = bc.resolve(ctx.theme);
                let rr = RoundedRect::from_rect(self.bounds, radius);
                scene.stroke(&Stroke::new(w as f64), Default::default(), color, None, &rr);
            }
        }
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        if self.on_press.is_none() {
            return EventResult::IGNORED;
        }
        match event {
            Event::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                if self.bounds.contains(*position) {
                    self.pressed = true;
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            Event::PointerUp {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                if self.pressed && self.bounds.contains(*position) {
                    if let Some(ref mut f) = self.on_press {
                        f();
                    }
                    self.pressed = false;
                    EventResult::HANDLED
                } else {
                    self.pressed = false;
                    EventResult::IGNORED
                }
            }
            _ => EventResult::IGNORED,
        }
    }

    fn hit_test_prefer_self(&self, _point: synui_core::Point) -> bool {
        self.on_press.is_some()
    }

    fn a11y(&self) -> A11yProps {
        if self.on_press.is_some() {
            A11yProps {
                role: A11yRole::Button,
                focusable: true,
                actions: vec![A11yAction::Click, A11yAction::Focus],
                ..Default::default()
            }
        } else {
            A11yProps::default()
        }
    }

    fn is_focusable(&self) -> bool {
        self.on_press.is_some()
    }
}

/// Paint CSS border edges. Per-side props win over uniform when any side is set.
pub fn paint_border_sides(
    scene: &mut Scene,
    bounds: Rect,
    visual: &VisualStyle,
    theme: &synui_core::ThemeRef,
) {
    if let Some((w, c)) = visual.border_side(0) {
        let y = bounds.y0 + (w as f64) * 0.5;
        let line = Line::new((bounds.x0, y), (bounds.x1, y));
        scene.stroke(
            &Stroke::new(w as f64),
            Default::default(),
            c.resolve(theme),
            None,
            &line,
        );
    }
    if let Some((w, c)) = visual.border_side(1) {
        let x = bounds.x1 - (w as f64) * 0.5;
        let line = Line::new((x, bounds.y0), (x, bounds.y1));
        scene.stroke(
            &Stroke::new(w as f64),
            Default::default(),
            c.resolve(theme),
            None,
            &line,
        );
    }
    if let Some((w, c)) = visual.border_side(2) {
        let y = bounds.y1 - (w as f64) * 0.5;
        let line = Line::new((bounds.x0, y), (bounds.x1, y));
        scene.stroke(
            &Stroke::new(w as f64),
            Default::default(),
            c.resolve(theme),
            None,
            &line,
        );
    }
    if let Some((w, c)) = visual.border_side(3) {
        let x = bounds.x0 + (w as f64) * 0.5;
        let line = Line::new((x, bounds.y0), (x, bounds.y1));
        scene.stroke(
            &Stroke::new(w as f64),
            Default::default(),
            c.resolve(theme),
            None,
            &line,
        );
    }
}

fn with_opacity(color: Color, opacity: f32) -> Color {
    let [r, g, b, a] = color.to_rgba8().to_u8_array();
    let na = ((a as f32) * opacity.clamp(0.0, 1.0)) as u8;
    Color::from_rgba8(r, g, b, na)
}

#[cfg(test)]
mod grid_layout_tests {
    use super::*;
    use crate::basic::Label;
    use synui_core::{Size, Style, WidgetStore};
    use synui_layout::LayoutEngine;
    use synui_render::TextRenderer;

    #[test]
    fn grid_three_equal_columns_with_fill_children() {
        let mut store = WidgetStore::new();
        let mut text = TextRenderer::new();
        let grid_style = Style::from_css(
            "display:grid;grid-template-columns:1fr 1fr 1fr;gap:8;padding:8;width:fill",
        )
        .unwrap();
        let cell_style =
            Style::from_css("padding:16;background:#e8def8;border-radius:8;width:fill").unwrap();
        let la = store.insert(Label::new("A"));
        let lb = store.insert(Label::new("B"));
        let lc = store.insert(Label::new("C"));
        let a = store.insert(StyledBox::new(cell_style.clone()).with_children(vec![la]));
        let b = store.insert(StyledBox::new(cell_style.clone()).with_children(vec![lb]));
        let c = store.insert(StyledBox::new(cell_style).with_children(vec![lc]));
        store.set_children(a, vec![la]);
        store.set_children(b, vec![lb]);
        store.set_children(c, vec![lc]);
        let grid = store.insert(StyledBox::new(grid_style).with_children(vec![a, b, c]));
        store.set_children(grid, vec![a, b, c]);
        store.set_root(grid);
        let mut layout = LayoutEngine::new();
        layout.compute(&mut store, &mut text, grid, Size::new(900.0, 200.0));
        let ba = store.get(a).unwrap().bounds();
        let bb = store.get(b).unwrap().bounds();
        let bc = store.get(c).unwrap().bounds();
        let grid_b = store.get(grid).unwrap().bounds();
        eprintln!("A={ba:?} B={bb:?} C={bc:?} grid={grid_b:?}");
        assert!(
            ba.width() > 200.0 && bb.width() > 200.0 && bc.width() > 200.0,
            "each cell should get ~1/3: A={ba:?} B={bb:?} C={bc:?}"
        );
        assert!((ba.width() - bb.width()).abs() < 20.0);
        assert!((bb.width() - bc.width()).abs() < 20.0);
        assert!(bc.x1 <= grid_b.x1 + 1.0, "C must stay inside grid");
    }
}
