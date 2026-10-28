//! SVG rendering: vello_svg primary, resvg fallback.

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use vello::Scene;

#[derive(Debug, Error)]
pub enum SvgError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("render error: {0}")]
    Render(String),
}

/// Cached SVG trees.
pub struct SvgCache {
    trees: HashMap<String, usvg::Tree>,
    raw: HashMap<String, String>,
}

impl Default for SvgCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SvgCache {
    pub fn new() -> Self {
        Self {
            trees: HashMap::new(),
            raw: HashMap::new(),
        }
    }

    pub fn parse(&mut self, key: impl Into<String>, svg: &str) -> Result<(), SvgError> {
        let key = key.into();
        if !self.trees.contains_key(&key) {
            let opt = usvg::Options::default();
            let tree =
                usvg::Tree::from_str(svg, &opt).map_err(|e| SvgError::Parse(e.to_string()))?;
            self.trees.insert(key.clone(), tree);
            self.raw.insert(key, svg.to_string());
        }
        Ok(())
    }

    /// Render SVG to scene via vello_svg.
    pub fn paint_to_scene(
        &mut self,
        scene: &mut Scene,
        svg: &str,
        key: &str,
    ) -> Result<(), SvgError> {
        self.parse(key, svg)?;
        let tree = self.trees.get(key).expect("parsed");
        vello_svg::append_tree(scene, tree);
        Ok(())
    }

    /// Paint SVG into bounds, falling back to resvg rasterization when vello_svg fails.
    pub fn paint_svg(
        &mut self,
        scene: &mut Scene,
        svg: &str,
        key: &str,
        bounds: Rect,
    ) -> Result<(), SvgError> {
        if self.paint_to_scene(scene, svg, key).is_err() {
            self.paint_fallback_raster(scene, svg, bounds)?;
        }
        Ok(())
    }

    /// Resvg fallback: rasterize to RGBA texture and draw via Vello `draw_image`.
    pub fn paint_fallback_raster(
        &self,
        scene: &mut Scene,
        svg: &str,
        bounds: Rect,
    ) -> Result<(), SvgError> {
        let opt = resvg::usvg::Options::default();
        let tree =
            resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| SvgError::Parse(e.to_string()))?;
        let width = bounds.width().max(1.0) as u32;
        let height = bounds.height().max(1.0) as u32;
        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| SvgError::Render("pixmap alloc failed".into()))?;
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        let scale_x = width as f32 / tree.size().width();
        let scale_y = height as f32 / tree.size().height();
        let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        let data = Arc::new(pixmap.data().to_vec());
        let image = ImageBrush::new(ImageData {
            data: Blob::new(data),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width,
            height,
        });
        let affine = Affine::translate((bounds.x0, bounds.y0))
            * Affine::scale_non_uniform(
                bounds.width() / width as f64,
                bounds.height() / height as f64,
            );
        scene.draw_image(image.as_ref(), affine);
        Ok(())
    }
}

pub fn render_svg_string(svg: &str) -> Result<Scene, SvgError> {
    vello_svg::render(svg).map_err(|e| SvgError::Parse(e.to_string()))
}

/// Widget-friendly SVG paint helper.
pub fn paint_svg(
    cache: &mut SvgCache,
    scene: &mut Scene,
    svg: &str,
    key: &str,
    bounds: Rect,
) -> Result<(), SvgError> {
    cache.paint_svg(scene, svg, key, bounds)
}
