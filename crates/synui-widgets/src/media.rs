//! Image widget: raster (PNG/JPEG/GIF/WebP) or crisp SVG from the app `ImageCache`.

use synui_core::alloc_widget_id;
use synui_core::{
    Constraints, Dimension, Event, EventCtx, EventResult, ImageFit, LayoutStyle, MeasureCtx,
    PaintCtx, Rect, Size, Widget, WidgetId,
};
use synui_render::ImageId;
use vello::Scene;

/// Displays a raster or SVG image from the app `ImageCache`.
///
/// SVG assets are re-rendered into the layout bounds each paint (sharp at any
/// size). `currentColor` in the SVG is filled with theme `on_surface` so icon
/// assets tint with the theme like [`crate::basic::Icon`].
pub struct Image {
    id: WidgetId,
    bounds: Rect,
    pub image_id: ImageId,
    pub fit: ImageFit,
    width: Option<f32>,
    height: Option<f32>,
    /// Pinch zoom factor (1.0 = natural).
    pub scale: f32,
}

impl Image {
    pub fn new(image_id: impl Into<ImageId>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            image_id: image_id.into(),
            fit: ImageFit::Contain,
            width: None,
            height: None,
            scale: 1.0,
        }
    }

    pub fn with_fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }
}

impl From<ImageId> for Image {
    fn from(image_id: ImageId) -> Self {
        Self::new(image_id)
    }
}

impl Widget for Image {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;

        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();

        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: synui_core::FlexDirection::Row,
            justify_content: synui_core::JustifyContent::FlexStart,
            align_items: synui_core::AlignItems::FlexStart,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            width: Dimension::Auto,
            height: Dimension::Auto,
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

    fn measure(&mut self, _ctx: &mut MeasureCtx, _constraints: Constraints) -> Size {
        let w = self.width.unwrap_or(120.0) * self.scale;
        let h = self.height.unwrap_or(120.0) * self.scale;
        Size::new(w as f64, h as f64)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        if let Some(images) = ctx.images.as_deref_mut() {
            let _ = images.paint_image_with_tint(
                scene,
                &self.image_id.0,
                self.bounds,
                self.fit,
                Some(ctx.theme.on_surface),
            );
        }
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        if let Event::Gesture(synui_core::Gesture::Pinch { scale, center }) = event {
            if self.bounds.contains(*center) {
                self.scale = (*scale).clamp(0.25, 4.0);
                ctx.stop_propagation();
                return EventResult::HANDLED;
            }
        }
        EventResult::IGNORED
    }
}
