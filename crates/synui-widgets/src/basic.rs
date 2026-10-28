//! Basic widgets: Label, Button, Icon.

use peniko::Color;
use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Circle, Constraints, Dimension, Event, EventCtx, EventResult,
    FlexDirection, LayoutStyle, MeasureCtx, PaintCtx, Point, PointerButton, Rect, RoundedRect,
    Size, Stroke, TextAlign, Widget, WidgetId,
};
use synui_render::{paint_simple_rect, IconId};
use synui_theme::RippleState;
use vello::kurbo::Affine;
use vello::peniko::Fill;
use vello::Scene;

/// Text label widget.
pub struct Label {
    id: WidgetId,
    bounds: Rect,
    pub text: String,
    pub font_size: f32,
    /// When true and measure saw a width budget, paint/measure clip with `…`.
    pub truncate: bool,
    /// When true, label fills parent width and wraps (dialog body, etc.).
    pub wrap: bool,
    /// Max width from last measure — paint must use the same wrap contract.
    wrap_width: Option<f32>,
    /// Display string for paint (may include paint-width ellipsis).
    display_text: String,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            display_text: text.clone(),
            text,
            font_size: 16.0,
            truncate: false,
            wrap: false,
            wrap_width: None,
        }
    }

    pub fn with_wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    fn refresh_display(&mut self) {
        if !self.truncate {
            self.display_text = self.text.clone();
            return;
        }
        let Some(max_w) = self.wrap_width.filter(|w| *w > 0.0) else {
            self.display_text = self.text.clone();
            return;
        };
        self.display_text = synui_render::with_text_renderer(|r| {
            r.fit_text_with_ellipsis(&self.text, self.font_size, max_w)
        });
    }

    /// Replace label text and refresh display (paint-width ellipsis if truncated).
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.refresh_display();
    }

    /// Text currently painted (may include `…` when [`Self::truncate`] is set).
    pub fn display_text(&self) -> &str {
        &self.display_text
    }
}

impl Widget for Label {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;

        static HUG: OnceLock<LayoutStyle> = OnceLock::new();
        static FILL: OnceLock<LayoutStyle> = OnceLock::new();

        if self.wrap {
            FILL.get_or_init(|| LayoutStyle {
                display: synui_core::Display::Flex,
                flex_direction: FlexDirection::Row,
                justify_content: synui_core::JustifyContent::FlexStart,
                align_items: synui_core::AlignItems::Center,
                flex_grow: 0.0,
                flex_shrink: 0.0,
                width: Dimension::Fill,
                height: Dimension::Auto,
                padding: [4.0, 8.0, 4.0, 8.0],
                gap: 0.0,
                ..LayoutStyle::base()
            })
        } else {
            HUG.get_or_init(|| LayoutStyle {
                display: synui_core::Display::Flex,
                flex_direction: FlexDirection::Row,
                justify_content: synui_core::JustifyContent::FlexStart,
                align_items: synui_core::AlignItems::Center,
                flex_grow: 0.0,
                // Do not shrink chrome labels when a Fill sibling (table/editor) measures large.
                flex_shrink: 0.0,
                // Hug text so `align-items:center` columns (counter) center the label.
                // Stretch parents still expand Auto children on the cross axis.
                width: Dimension::Auto,
                height: Dimension::Auto,
                padding: [4.0, 8.0, 4.0, 8.0],
                gap: 0.0,
                ..LayoutStyle::base()
            })
        }
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, ctx: &mut MeasureCtx, _constraints: Constraints) -> Size {
        let pad_x = 16.0_f32; // layout padding L+R
                              // Wrap labels often live in nested panels (Dialog ~80% width). The layout
                              // engine only passes the root budget for Fill measure — leave headroom so
                              // multiline height is not underestimated.
        let max_w = if self.wrap {
            ctx.max_width
                .map(|w| (w * 0.72 - pad_x).max(160.0))
                .or(Some(400.0))
        } else if self.truncate {
            ctx.max_width
        } else {
            // Auto hug: huge wrap cap so Parley measures glyphs (ASCII fast-path
            // underestimates and used to make "Count: N" wrap on paint).
            ctx.max_width.or(Some(10_000.0))
        };
        self.wrap_width = max_w;
        self.refresh_display();
        let size = if let Some(m) = ctx.text_measurer.as_deref_mut() {
            m.measure_text(&self.display_text, self.font_size, max_w)
        } else {
            Size::new(
                self.display_text.len() as f64 * 8.0,
                self.font_size as f64 + 4.0,
            )
        };
        Size::new(
            size.width,
            (size.height + 8.0).max(self.font_size as f64 + 8.0),
        )
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        if let Some(text) = ctx.text.as_deref_mut() {
            // Hug labels: wrap only when measure saw an explicit width budget.
            // Wrap labels: always clamp to the laid-out box (root budget can be wider
            // than a Dialog/panel, which previously clipped the right edge).
            let pad_x = 16.0_f32;
            let bound_w = (self.bounds.width() as f32 - pad_x).max(1.0);
            let max_w = if self.wrap {
                Some(
                    self.wrap_width
                        .map(|w| w.min(bound_w))
                        .unwrap_or(bound_w)
                        .max(1.0),
                )
            } else {
                self.wrap_width
            };
            let color = ctx.text_color.unwrap_or(ctx.theme.on_surface);
            let font_size = ctx.font_size.unwrap_or(self.font_size);
            text.paint_text(
                scene,
                &self.display_text,
                font_size,
                color,
                self.bounds,
                max_w,
                TextAlign::Start,
            );
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Label,
            label: Some(self.text.clone()),
            ..Default::default()
        }
    }
}

/// Button variants per MD3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Filled,
    Outlined,
    Text,
    Icon,
    Fab,
}

pub struct Button {
    id: WidgetId,
    bounds: Rect,
    pub label: String,
    pub variant: ButtonVariant,
    pub on_press: Option<Box<dyn FnMut()>>,
    /// Stable action id (Phase 26); dispatched when set, before `on_press`.
    pub action: Option<synui_core::ActionId>,
    /// Phase 41: real disabled state for `:disabled` cascade + input gating.
    pub disabled: bool,
    ripple: Option<RippleState>,
    pressed: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            label: label.into(),
            variant: ButtonVariant::Filled,
            on_press: None,
            action: None,
            disabled: false,
            ripple: None,
            pressed: false,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn on_press(mut self, f: impl FnMut() + 'static) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }

    pub fn with_action(mut self, id: synui_core::ActionId) -> Self {
        self.action = Some(id);
        self
    }

    pub fn set_action(&mut self, id: Option<synui_core::ActionId>) {
        self.action = id;
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }
}

impl Widget for Button {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;

        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();

        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: FlexDirection::Row,
            justify_content: synui_core::JustifyContent::FlexStart,
            align_items: synui_core::AlignItems::Center,
            // Hug label text. `Fill` → percent(1) collapses under Auto +
            // align-items:center parents (e.g. counter `style_css`).
            // Keep flex_shrink:1 so Row toolbars (gallery category strip) can
            // compress toward min_width instead of overflowing the window.
            flex_grow: 0.0,
            flex_shrink: 1.0,
            width: Dimension::Auto,
            height: Dimension::Length(40.0),
            min_width: Dimension::Length(64.0),
            min_height: Dimension::Length(40.0),
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn measure(&mut self, ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        // MD3 filled button: 24px horizontal padding, min width 64.
        let text_w = if let Some(m) = ctx.text_measurer.as_deref_mut() {
            m.measure_text(&self.label, 14.0, None).width + 48.0
        } else {
            self.label.len() as f64 * 8.0 + 48.0
        };
        Size::new(
            text_w.min(constraints.max_width as f64).max(64.0),
            40.0f64.min(constraints.max_height as f64),
        )
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let cupertino = ctx.theme.cupertino;
        let radius = if cupertino {
            ctx.theme.metrics.corner_button.max(10.0)
        } else {
            ctx.theme.metrics.corner_button
        };
        let (bg, fg) = match self.variant {
            ButtonVariant::Filled | ButtonVariant::Fab => (ctx.theme.primary, ctx.theme.on_primary),
            ButtonVariant::Outlined | ButtonVariant::Text | ButtonVariant::Icon => (
                vello::peniko::Color::from_rgba8(0, 0, 0, 0),
                ctx.theme.primary,
            ),
        };
        if bg != Color::TRANSPARENT {
            paint_simple_rect(scene, self.bounds, bg, radius);
        } else if self.variant == ButtonVariant::Outlined {
            let rr = RoundedRect::from_rect(self.bounds, radius);
            scene.stroke(
                &Stroke::new(1.0),
                Affine::IDENTITY,
                ctx.theme.outline,
                None,
                &rr,
            );
        }
        if let Some(ripple) = &self.ripple {
            if ctx.theme.metrics.ripple_enabled {
                let origin = Point::new(
                    self.bounds.x0 + ripple.origin.x,
                    self.bounds.y0 + ripple.origin.y,
                );
                let max_r = synui_theme::max_ripple_radius(ripple.origin, self.bounds);
                let r = ripple.current_radius(max_r);
                let mut ripple_color = ctx.theme.on_primary;
                ripple_color.components[3] = ripple.current_opacity();
                let circle = Circle::new(origin, r);
                let clip = RoundedRect::from_rect(self.bounds, radius);
                scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &clip);
                scene.fill(Fill::NonZero, Affine::IDENTITY, ripple_color, None, &circle);
                scene.pop_layer();
            }
        }
        if let Some(text) = ctx.text.as_deref_mut() {
            text.paint_text(
                scene,
                &self.label,
                14.0,
                fg,
                self.bounds,
                Some(self.bounds.width() as f32),
                TextAlign::Center,
            );
        }
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        if self.disabled {
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
                    let local =
                        Point::new(position.x - self.bounds.x0, position.y - self.bounds.y0);
                    self.ripple = Some(RippleState::new(local, Color::WHITE));
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
                    if let Some(aid) = self.action {
                        synui_core::dispatch_action(aid);
                    } else if let Some(ref mut f) = self.on_press {
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

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Button,
            label: Some(self.label.clone()),
            text_selection: None,
            focusable: !self.disabled,
            actions: vec![A11yAction::Click, A11yAction::Focus],
            disabled: self.disabled,
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        !self.disabled
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }
}

impl Button {
    /// Advance ripple animation; returns true while active.
    pub fn advance_ripple(&mut self, dt_ms: f32) -> bool {
        if let Some(ref mut ripple) = self.ripple {
            let active = ripple.advance(dt_ms);
            if !active {
                self.ripple = None;
            }
            return active;
        }
        false
    }
}

/// Icon widget using cached SVG (curated Material Symbols or [`IconCache::load`]).
///
/// Color follows style cascade [`PaintCtx::text_color`], then theme `on_surface`,
/// unless [`Self::color`] is set. SVG `currentColor` is replaced at paint time.
pub struct Icon {
    id: WidgetId,
    bounds: Rect,
    pub icon_id: IconId,
    pub size: f32,
    /// Explicit tint; when `None`, uses cascade `text_color` / `on_surface`.
    pub color: Option<Color>,
}

impl Icon {
    pub fn new(icon_id: IconId) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            icon_id,
            size: 24.0,
            color: None,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for Icon {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, _c: Constraints) -> Size {
        Size::new(self.size as f64, self.size as f64)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        if let Some(icons) = ctx.icons.as_deref_mut() {
            let color = self
                .color
                .or(ctx.text_color)
                .unwrap_or(ctx.theme.on_surface);
            let _ = icons.paint_icon(scene, &self.icon_id.0, self.bounds, color);
        }
    }
}
