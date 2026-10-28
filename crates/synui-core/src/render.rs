//! Traits for shared render resources (implemented in synui-render).

use crate::geometry::{Rect, Size};
use peniko::Color;
use std::ops::Range;
use vello::Scene;

/// Horizontal alignment when painting text in a rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
}

/// One styled run for rich-text painting (byte range into the source string).
#[derive(Debug, Clone)]
pub struct TextStyleRun {
    pub range: Range<usize>,
    pub font_size: Option<f32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<[u8; 4]>,
}

/// Text measurement and painting.
pub trait TextPainter {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: Option<f32>) -> Size;
    #[allow(clippy::too_many_arguments)]
    fn paint_text(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font_size: f32,
        color: Color,
        bounds: Rect,
        max_width: Option<f32>,
        align: TextAlign,
    );

    /// Paint text with per-range styles (falls back to plain `paint_text` when empty).
    #[allow(clippy::too_many_arguments)]
    fn paint_text_styled(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font_size: f32,
        color: Color,
        bounds: Rect,
        max_width: Option<f32>,
        align: TextAlign,
        _runs: &[TextStyleRun],
    ) {
        self.paint_text(scene, text, font_size, color, bounds, max_width, align);
    }

    /// Byte index nearest to `x` within the laid-out text (Start-aligned).
    fn hit_test_x(&mut self, text: &str, font_size: f32, x: f64) -> usize {
        self.hit_test_xy(text, font_size, None, x, 0.0)
    }

    /// Byte index nearest to local layout point `(x, y)`.
    fn hit_test_xy(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        x: f64,
        y: f64,
    ) -> usize {
        self.hit_test_xy_styled(text, font_size, max_width, x, y, &[])
    }

    /// Like [`hit_test_xy`], using the same style runs as rich-text paint.
    fn hit_test_xy_styled(
        &mut self,
        text: &str,
        font_size: f32,
        _max_width: Option<f32>,
        x: f64,
        _y: f64,
        _runs: &[TextStyleRun],
    ) -> usize {
        if text.is_empty() {
            return 0;
        }
        let mut best = 0;
        let mut best_dist = f64::MAX;
        for (i, _) in text
            .char_indices()
            .chain(std::iter::once((text.len(), '\0')))
        {
            let prefix = &text[..i];
            let w = self.measure_text(prefix, font_size, None).width;
            let dist = (w - x).abs();
            if dist < best_dist {
                best_dist = dist;
                best = i;
            }
        }
        best
    }

    /// X offset of caret at byte index `index` (single-line / unwrapped).
    fn caret_x(&mut self, text: &str, font_size: f32, index: usize) -> f64 {
        self.caret_rect(text, font_size, None, index).x0
    }

    /// Caret rectangle in layout-local coordinates.
    fn caret_rect(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        index: usize,
    ) -> Rect {
        self.caret_rect_styled(text, font_size, max_width, index, &[])
    }

    /// Like [`caret_rect`], using the same style runs as rich-text paint.
    fn caret_rect_styled(
        &mut self,
        text: &str,
        font_size: f32,
        _max_width: Option<f32>,
        index: usize,
        _runs: &[TextStyleRun],
    ) -> Rect {
        let index = index.min(text.len());
        let prefix = &text[..index];
        let x = self.measure_text(prefix, font_size, None).width;
        let h = self.measure_text(text, font_size, None).height.max(1.0);
        Rect::new(x, 0.0, x + 1.0, h)
    }

    /// Selection highlight rectangles in layout-local coordinates.
    fn selection_rects(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        range: Range<usize>,
    ) -> Vec<Rect> {
        self.selection_rects_styled(text, font_size, max_width, range, &[])
    }

    /// Like [`selection_rects`], using the same style runs as rich-text paint.
    fn selection_rects_styled(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        range: Range<usize>,
        runs: &[TextStyleRun],
    ) -> Vec<Rect> {
        let mut a = range.start.min(range.end).min(text.len());
        let mut b = range.start.max(range.end).min(text.len());
        while a > 0 && !text.is_char_boundary(a) {
            a -= 1;
        }
        while b < text.len() && !text.is_char_boundary(b) {
            b += 1;
        }
        if a >= b {
            return Vec::new();
        }
        let r0 = self.caret_rect_styled(text, font_size, max_width, a, runs);
        let r1 = self.caret_rect_styled(text, font_size, max_width, b, runs);
        vec![Rect::new(
            r0.x0.min(r1.x0),
            r0.y0.min(r1.y0),
            r0.x0.max(r1.x0).max(r0.x0.min(r1.x0) + 1.0),
            r0.y1.max(r1.y1),
        )]
    }
}

/// Icon painting from a string identifier.
pub trait IconPainter {
    fn paint_icon(
        &mut self,
        scene: &mut Scene,
        id: &str,
        bounds: Rect,
        color: Color,
    ) -> Result<(), String>;
}

/// How a raster or vector image is scaled inside its layout bounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Fill,
}

/// Image painting from a string identifier (raster or SVG).
pub trait ImagePainter {
    fn image_size(&self, id: &str) -> Option<Size>;

    fn paint_image(
        &mut self,
        scene: &mut Scene,
        id: &str,
        bounds: Rect,
        fit: ImageFit,
    ) -> Result<(), String> {
        self.paint_image_with_tint(scene, id, bounds, fit, None)
    }

    /// Paint an image. `tint` replaces SVG `currentColor` (icons); rasters ignore it.
    fn paint_image_with_tint(
        &mut self,
        scene: &mut Scene,
        id: &str,
        bounds: Rect,
        fit: ImageFit,
        tint: Option<Color>,
    ) -> Result<(), String>;
}

/// Read-only lookup of preloaded Lottie JSON by host asset id.
pub trait LottieJsonSource {
    fn lottie_json(&self, id: &str) -> Option<&str>;

    /// Host caches (e.g. `synui_render::LottieCache`) downcast for Composition clone.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Application-level render resources shared across widgets per frame.
pub struct PaintResources<'a> {
    pub text: &'a mut dyn TextPainter,
    pub icons: &'a mut dyn IconPainter,
    pub images: Option<&'a mut dyn ImagePainter>,
}
