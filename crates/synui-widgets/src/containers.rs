//! Flex and other container widgets.

use synui_core::alloc_widget_id;
use synui_core::{
    AlignItems, Constraints, Dimension, Display, FlexDirection, JustifyContent, LayoutStyle,
    MeasureCtx, PaintCtx, Rect, Size, Stroke, Widget, WidgetId,
};
use synui_render::paint_simple_rect;
use synui_theme::elevation_shadow;
use vello::kurbo::RoundedRect;
use vello::peniko::Fill;
use vello::Scene;

/// Flexbox container.
pub struct Flex {
    id: WidgetId,
    bounds: Rect,
    pub direction: FlexDirection,
    pub justify: JustifyContent,
    pub align: AlignItems,
    pub gap: f32,
    pub padding: [f32; 4],
    child_ids: Vec<WidgetId>,
    layout_style: LayoutStyle,
}

impl Flex {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            direction: FlexDirection::Column,
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            gap: 8.0,
            padding: [0.0; 4],
            child_ids: Vec::new(),
            layout_style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                width: Dimension::Fill,
                height: Dimension::Fill,
                gap: 8.0,
                padding: [0.0; 4],
                ..LayoutStyle::default()
            },
        }
    }

    pub fn row(mut self) -> Self {
        self.direction = FlexDirection::Row;
        self.layout_style.flex_direction = FlexDirection::Row;
        self
    }

    pub fn column(mut self) -> Self {
        self.direction = FlexDirection::Column;
        self.layout_style.flex_direction = FlexDirection::Column;
        self
    }

    pub fn with_children(mut self, children: Vec<WidgetId>) -> Self {
        self.child_ids = children;
        self
    }

    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self.layout_style.gap = gap;
        self
    }

    pub fn with_padding(mut self, padding: [f32; 4]) -> Self {
        self.padding = padding;
        self.layout_style.padding = padding;
        self
    }

    pub fn with_justify(mut self, justify: JustifyContent) -> Self {
        self.justify = justify;
        self.layout_style.justify_content = justify;
        self
    }

    pub fn with_align(mut self, align: AlignItems) -> Self {
        self.align = align;
        self.layout_style.align_items = align;
        self
    }

    pub fn with_width(mut self, width: Dimension) -> Self {
        self.layout_style.width = width;
        self
    }

    pub fn with_height(mut self, height: Dimension) -> Self {
        self.layout_style.height = height;
        self
    }

    pub fn apply_layout_style(&mut self, style: &LayoutStyle) {
        self.direction = style.flex_direction;
        self.justify = style.justify_content;
        self.align = style.align_items;
        self.gap = style.gap;
        self.padding = style.padding;
        self.layout_style = style.clone();
    }

    /// Apply flex alignment from a parent `style_css` without clobbering column/row gap.
    ///
    /// Style defaults use `gap: 0`; pushing that onto `View::column` (default gap 8)
    /// made the first frame look cramped until a rebuild re-applied the View gap.
    pub fn apply_style_alignment(&mut self, style: &LayoutStyle) {
        self.justify = style.justify_content;
        self.align = style.align_items;
        self.layout_style.justify_content = style.justify_content;
        self.layout_style.align_items = style.align_items;
        if style.gap > 0.0 {
            self.gap = style.gap;
            self.layout_style.gap = style.gap;
        }
        // Padding stays on StyledBox; keep Flex Fill so justify/align fill the host.
        self.layout_style.width = Dimension::Fill;
        self.layout_style.height = Dimension::Fill;
    }

    pub fn set_gap_padding_direction(
        &mut self,
        gap: f32,
        padding: [f32; 4],
        direction: FlexDirection,
    ) {
        self.gap = gap;
        self.padding = padding;
        self.direction = direction;
        self.layout_style.gap = gap;
        self.layout_style.padding = padding;
        self.layout_style.flex_direction = direction;
    }
}

impl Default for Flex {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Flex {
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
        Size::new(
            constraints.max_width.min(800.0) as f64,
            constraints.max_height.min(600.0) as f64,
        )
    }

    fn paint(&self, _ctx: &mut PaintCtx, _scene: &mut Scene) {
        // Transparent by default — use StyledBox/Card/background for surfaces.
    }
}

/// Card container with elevation styling.
pub struct Card {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    pub elevation: u8,
}

impl Card {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            elevation: 1,
        }
    }

    pub fn with_children(mut self, children: Vec<WidgetId>) -> Self {
        self.child_ids = children;
        self
    }

    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.child_ids = children;
    }
}

impl Widget for Card {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let radius = ctx.theme.metrics.corner_card;
        let m = &ctx.theme.metrics;
        if m.elevation_enabled {
            let (offset_y, blur, opacity) = elevation_shadow(self.elevation);
            if opacity > 0.0 {
                let shadow_bounds = Rect::new(
                    self.bounds.x0,
                    self.bounds.y0 + offset_y,
                    self.bounds.x1,
                    self.bounds.y1 + offset_y,
                );
                let mut shadow_color = vello::peniko::Color::BLACK;
                shadow_color.components[3] = opacity;
                let rr = RoundedRect::from_rect(shadow_bounds, radius + blur);
                scene.fill(
                    Fill::NonZero,
                    vello::kurbo::Affine::IDENTITY,
                    shadow_color,
                    None,
                    &rr,
                );
            }
        }
        paint_simple_rect(scene, self.bounds, ctx.theme.surface, radius);
        if m.bordered_surfaces {
            let rr = RoundedRect::from_rect(self.bounds, radius);
            scene.stroke(
                &Stroke::new(1.0),
                vello::kurbo::Affine::IDENTITY,
                ctx.theme.outline,
                None,
                &rr,
            );
        }
    }
}

/// Fixed-bounds host for positioned popup overlays (no full-viewport scrim).
pub struct PopupHost {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
}

impl PopupHost {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: WidgetId) -> Self {
        self.child_ids = vec![child];
        self
    }
}

impl Widget for PopupHost {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;

        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();

        STYLE.get_or_init(|| LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            flex_grow: 1.0,
            flex_shrink: 0.0,
            width: Dimension::Fill,
            height: Dimension::Fill,
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn paint(&self, _ctx: &mut PaintCtx, _scene: &mut Scene) {}
}

/// Full-viewport host for modal overlay content.
pub struct ModalHost {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
}

impl ModalHost {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: WidgetId) -> Self {
        self.child_ids = vec![child];
        self
    }
}

impl Default for ModalHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ModalHost {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;

        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();

        STYLE.get_or_init(|| LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_grow: 1.0,
            flex_shrink: 0.0,
            width: Dimension::Fill,
            height: Dimension::Fill,
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn paint(&self, ctx: &mut PaintCtx, _scene: &mut Scene) {
        // Scrim is painted by WidgetStore::paint_scrim; host is a transparent flex center.
        let _ = ctx;
    }
}

/// Dialog overlay container with title and actions.
pub struct Dialog {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    pub title: String,
    pub open: bool,
    pub on_dismiss: Option<Box<dyn FnMut()>>,
}

impl Dialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            title: title.into(),
            open: false,
            on_dismiss: None,
        }
    }

    pub fn with_children(mut self, children: Vec<WidgetId>) -> Self {
        self.child_ids = children;
        self
    }

    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.child_ids = children;
    }

    pub fn on_dismiss(mut self, f: impl FnMut() + 'static) -> Self {
        self.on_dismiss = Some(Box::new(f));
        self
    }

    pub fn show(&mut self) {
        self.open = true;
    }

    pub fn dismiss(&mut self) {
        self.open = false;
        if let Some(f) = self.on_dismiss.as_mut() {
            f();
        }
    }
}

impl Widget for Dialog {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        if self.open {
            &self.child_ids
        } else {
            &[]
        }
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static OPEN: OnceLock<LayoutStyle> = OnceLock::new();
        static CLOSED: OnceLock<LayoutStyle> = OnceLock::new();
        if self.open {
            OPEN.get_or_init(|| LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                // Center hug-width actions (Button Auto → measured length, so Stretch
                // never expands them). Wrapped Labels use width Fill and still span.
                align_items: AlignItems::Center,
                flex_grow: 0.0,
                flex_shrink: 0.0,
                width: Dimension::Percent(80.0),
                height: Dimension::Auto,
                padding: [56.0, 24.0, 24.0, 24.0],
                gap: 8.0,
                ..LayoutStyle::base()
            })
        } else {
            CLOSED.get_or_init(|| LayoutStyle {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                flex_grow: 0.0,
                flex_shrink: 0.0,
                width: Dimension::Auto,
                height: Dimension::Auto,
                padding: [0.0; 4],
                gap: 0.0,
                ..LayoutStyle::base()
            })
        }
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        if !self.open {
            return Size::ZERO;
        }
        let w = (constraints.max_width as f64 * 0.8).clamp(280.0, 560.0);
        // Tall enough for title + wrapped body + centered action.
        Size::new(w, 320.0_f64.min(constraints.max_height as f64))
    }

    fn hit_test(&self, point: synui_core::Point) -> bool {
        self.open && self.bounds.contains(point)
    }

    fn clip_bounds(&self) -> Option<Rect> {
        self.open.then_some(self.bounds)
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        if !self.open {
            return;
        }
        let radius = ctx.theme.metrics.corner_dialog;
        let m = &ctx.theme.metrics;
        if m.elevation_enabled {
            let (offset_y, blur, opacity) = elevation_shadow(8);
            if opacity > 0.0 {
                let shadow_bounds = Rect::new(
                    self.bounds.x0,
                    self.bounds.y0 + offset_y,
                    self.bounds.x1,
                    self.bounds.y1 + offset_y,
                );
                let mut shadow_color = vello::peniko::Color::BLACK;
                shadow_color.components[3] = opacity;
                let rr = RoundedRect::from_rect(shadow_bounds, radius + blur);
                scene.fill(
                    Fill::NonZero,
                    vello::kurbo::Affine::IDENTITY,
                    shadow_color,
                    None,
                    &rr,
                );
            }
        }
        paint_simple_rect(scene, self.bounds, ctx.theme.surface, radius);
        let outline = RoundedRect::from_rect(self.bounds, radius);
        scene.stroke(
            &Stroke::new(1.0),
            vello::kurbo::Affine::IDENTITY,
            ctx.theme.outline,
            None,
            &outline,
        );
        if let Some(text) = ctx.text.as_deref_mut() {
            let title_bounds = Rect::new(
                self.bounds.x0 + 24.0,
                self.bounds.y0 + 16.0,
                self.bounds.x1 - 24.0,
                self.bounds.y0 + 48.0,
            );
            text.paint_text(
                scene,
                &self.title,
                20.0,
                ctx.theme.on_surface,
                title_bounds,
                None,
                synui_core::TextAlign::Start,
            );
        }
    }

    fn event(
        &mut self,
        _ctx: &mut synui_core::EventCtx,
        event: &synui_core::Event,
    ) -> synui_core::EventResult {
        use synui_core::{Event, EventResult, Key, NamedKey};
        match event {
            Event::KeyDown {
                key: Key::Named(NamedKey::Escape),
                ..
            } => {
                self.dismiss();
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn is_focusable(&self) -> bool {
        self.open
    }

    fn a11y(&self) -> synui_core::A11yProps {
        synui_core::A11yProps {
            role: synui_core::A11yRole::Dialog,
            label: Some(self.title.clone()),
            ..Default::default()
        }
    }
}

/// Bottom sheet container with drag-to-dismiss.
pub struct BottomSheet {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    pub open: bool,
    pub drag_offset: f64,
    dragging: bool,
}

impl BottomSheet {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            open: false,
            drag_offset: 0.0,
            dragging: false,
        }
    }

    pub fn with_children(mut self, children: Vec<WidgetId>) -> Self {
        self.child_ids = children;
        self
    }

    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.child_ids = children;
    }
}

impl Widget for BottomSheet {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        if self.open {
            &self.child_ids
        } else {
            &[]
        }
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static OPEN: OnceLock<LayoutStyle> = OnceLock::new();
        static CLOSED: OnceLock<LayoutStyle> = OnceLock::new();
        if self.open {
            OPEN.get_or_init(|| LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                // Center hug-width actions (same as Dialog: Button Auto → measured length).
                align_items: AlignItems::Center,
                flex_grow: 0.0,
                flex_shrink: 0.0,
                width: Dimension::Fill,
                height: Dimension::Percent(45.0),
                padding: [16.0, 16.0, 16.0, 16.0],
                gap: 8.0,
                ..LayoutStyle::base()
            })
        } else {
            CLOSED.get_or_init(|| LayoutStyle {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                flex_grow: 0.0,
                flex_shrink: 0.0,
                width: Dimension::Auto,
                height: Dimension::Auto,
                padding: [0.0; 4],
                gap: 0.0,
                ..LayoutStyle::base()
            })
        }
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        if !self.open {
            return Size::ZERO;
        }
        let h = (constraints.max_height as f64 * 0.45).max(120.0);
        Size::new(constraints.max_width as f64, h)
    }

    fn bounds(&self) -> Rect {
        if self.open {
            self.bounds
        } else {
            Rect::ZERO
        }
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn hit_test(&self, point: synui_core::Point) -> bool {
        self.open && self.bounds.contains(point)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        if !self.open {
            return;
        }
        let y_off = self.drag_offset;
        let sheet = Rect::new(
            self.bounds.x0,
            self.bounds.y0 + y_off,
            self.bounds.x1,
            self.bounds.y1 + y_off,
        );
        paint_simple_rect(scene, sheet, ctx.theme.surface, 16.0);
        let handle = Rect::new(
            sheet.x0 + sheet.width() / 2.0 - 20.0,
            sheet.y0 + 8.0,
            sheet.x0 + sheet.width() / 2.0 + 20.0,
            sheet.y0 + 12.0,
        );
        paint_simple_rect(scene, handle, ctx.theme.on_surface_variant, 2.0);
    }

    fn event(
        &mut self,
        _ctx: &mut synui_core::EventCtx,
        event: &synui_core::Event,
    ) -> synui_core::EventResult {
        use synui_core::{Event, EventResult, Gesture};
        match event {
            Event::PointerDown { position, .. } if self.bounds.contains(*position) => {
                self.dragging = true;
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } if self.dragging => {
                self.drag_offset = (position.y - self.bounds.y0).max(0.0);
                EventResult::HANDLED
            }
            Event::PointerUp { .. } if self.dragging => {
                self.dragging = false;
                if self.drag_offset > self.bounds.height() * 0.3 {
                    self.open = false;
                    self.drag_offset = 0.0;
                } else {
                    self.drag_offset = 0.0;
                }
                EventResult::HANDLED
            }
            Event::Gesture(Gesture::Drag { delta, .. }) if self.open => {
                if delta.y > 0.0 {
                    self.drag_offset += delta.y;
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: synui_core::Key::Named(synui_core::NamedKey::Escape),
                ..
            } => {
                self.open = false;
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }
}

/// Snack bar notification with auto-dismiss.
pub struct SnackBar {
    id: WidgetId,
    bounds: Rect,
    pub message: String,
    pub action_label: Option<String>,
    pub visible: bool,
    pub on_action: Option<Box<dyn FnMut()>>,
    show_ms: f32,
    elapsed_ms: f32,
    slide_in: f32,
}

impl SnackBar {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            message: message.into(),
            action_label: None,
            visible: false,
            on_action: None,
            show_ms: 4000.0,
            elapsed_ms: 0.0,
            slide_in: 0.0,
        }
    }

    pub fn with_action(mut self, label: impl Into<String>, f: impl FnMut() + 'static) -> Self {
        self.action_label = Some(label.into());
        self.on_action = Some(Box::new(f));
        self
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.elapsed_ms = 0.0;
        self.slide_in = 0.0;
    }

    pub fn tick(&mut self, dt_ms: f32) -> bool {
        if !self.visible {
            return false;
        }
        self.elapsed_ms += dt_ms;
        self.slide_in = (self.slide_in + dt_ms / 200.0).min(1.0);
        if self.elapsed_ms >= self.show_ms {
            self.visible = false;
            return true;
        }
        false
    }
}

impl Widget for SnackBar {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        if !self.visible {
            return;
        }
        let slide = (1.0 - self.slide_in as f64) * 48.0;
        let bar = Rect::new(
            self.bounds.x0,
            self.bounds.y0 + slide,
            self.bounds.x1,
            self.bounds.y1 + slide,
        );
        paint_simple_rect(scene, bar, ctx.theme.inverse_surface(), 8.0);
        if let Some(text) = ctx.text.as_deref_mut() {
            text.paint_text(
                scene,
                &self.message,
                14.0,
                ctx.theme.inverse_on_surface(),
                bar,
                None,
                synui_core::TextAlign::Start,
            );
        }
    }

    fn event(
        &mut self,
        _ctx: &mut synui_core::EventCtx,
        event: &synui_core::Event,
    ) -> synui_core::EventResult {
        use synui_core::{Event, EventResult};
        if let Event::PointerUp { position, .. } = event {
            if self.visible && self.bounds.contains(*position) {
                if let (Some(label), Some(action)) = (&self.action_label, &mut self.on_action) {
                    let action_w = label.len() as f64 * 8.0 + 16.0;
                    let action_rect = Rect::new(
                        self.bounds.x1 - action_w - 8.0,
                        self.bounds.y0,
                        self.bounds.x1 - 8.0,
                        self.bounds.y1,
                    );
                    if action_rect.contains(*position) {
                        action();
                        self.visible = false;
                        return EventResult::HANDLED;
                    }
                }
            }
        }
        EventResult::IGNORED
    }
}

trait InverseSurface {
    fn inverse_surface(&self) -> vello::peniko::Color;
    fn inverse_on_surface(&self) -> vello::peniko::Color;
}

impl InverseSurface for synui_core::ThemeRef {
    fn inverse_surface(&self) -> vello::peniko::Color {
        if self.is_dark {
            self.surface_variant
        } else {
            self.on_surface
        }
    }

    fn inverse_on_surface(&self) -> vello::peniko::Color {
        if self.is_dark {
            self.on_surface_variant
        } else {
            self.surface
        }
    }
}
