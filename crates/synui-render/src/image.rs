//! Image cache: PNG/JPEG/GIF/animated WebP rasters + crisp SVG (vello_svg / resvg).
//!
//! SVG assets keep their source and are re-rendered into the widget bounds each paint
//! so scaling stays sharp. `currentColor` is substituted from an optional tint
//! (typically theme `on_surface`) the same way [`crate::icon::IconCache`] works.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops;
use image::{AnimationDecoder, Frame, ImageDecoder, ImageFormat, ImageReader};
use synui_core::Size;
use synui_core::{ImageFit, ImagePainter};
use thiserror::Error;
use vello::kurbo::{Affine, Rect, Vec2};
use vello::peniko::{
    Blob, Color, ImageAlphaType, ImageBrush, ImageData, ImageFormat as VelloImageFormat,
};
use vello::Scene;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImageId(pub String);

#[derive(Debug, Error)]
pub enum ImageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode error: {0}")]
    Decode(#[from] image::ImageError),
    #[error("svg error: {0}")]
    Svg(String),
    #[error("image not loaded: {0}")]
    NotLoaded(String),
}

struct Raster {
    width: u32,
    height: u32,
    pixels: Arc<dyn AsRef<[u8]> + Send + Sync>,
}

struct AnimatedRaster {
    width: u32,
    height: u32,
    frames: Vec<Raster>,
    delays_ms: Vec<u32>,
    frame_index: usize,
    elapsed_ms: f32,
    playing: bool,
}

impl AnimatedRaster {
    fn current(&self) -> &Raster {
        &self.frames[self.frame_index]
    }

    fn advance(&mut self, dt_ms: f32) -> bool {
        if !self.playing || self.frames.len() <= 1 {
            return false;
        }
        self.elapsed_ms += dt_ms;
        let delay = self.delays_ms[self.frame_index] as f32;
        if self.elapsed_ms >= delay {
            self.elapsed_ms -= delay;
            self.frame_index = (self.frame_index + 1) % self.frames.len();
        }
        true
    }
}

struct SvgAsset {
    raw: String,
    width: f32,
    height: f32,
}

enum ImageEntry {
    Static(Raster),
    Animated(AnimatedRaster),
    Svg(SvgAsset),
}

/// Decoded bitmap / SVG cache with optional animation playback.
pub struct ImageCache {
    images: HashMap<ImageId, ImageEntry>,
    /// Tinted usvg trees for SVG entries: (id, color_key) → tree.
    svg_trees: HashMap<(ImageId, u32), usvg::Tree>,
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageCache {
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            svg_trees: HashMap::new(),
        }
    }

    pub fn load_bytes(&mut self, id: ImageId, bytes: &[u8]) -> Result<(), ImageError> {
        let entry = decode_image(bytes)?;
        self.insert_entry(id, entry);
        Ok(())
    }

    pub fn load_path(&mut self, id: ImageId, path: impl AsRef<Path>) -> Result<(), ImageError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        let entry = if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
            || looks_like_svg(&bytes)
        {
            decode_svg(&bytes)?
        } else {
            decode_image(&bytes)?
        };
        self.insert_entry(id, entry);
        Ok(())
    }

    fn insert_entry(&mut self, id: ImageId, entry: ImageEntry) {
        self.svg_trees.retain(|(cached_id, _), _| cached_id != &id);
        self.images.insert(id, entry);
    }

    pub fn contains(&self, id: &ImageId) -> bool {
        self.images.contains_key(id)
    }

    pub fn is_svg(&self, id: &ImageId) -> bool {
        matches!(self.images.get(id), Some(ImageEntry::Svg(_)))
    }

    pub fn size(&self, id: &ImageId) -> Option<(u32, u32)> {
        match self.images.get(id)? {
            ImageEntry::Static(r) => Some((r.width, r.height)),
            ImageEntry::Animated(a) => Some((a.width, a.height)),
            ImageEntry::Svg(s) => Some((s.width.max(1.0) as u32, s.height.max(1.0) as u32)),
        }
    }

    pub fn is_animated(&self, id: &ImageId) -> bool {
        matches!(self.images.get(id), Some(ImageEntry::Animated(_)))
    }

    pub fn set_playing(&mut self, id: &ImageId, playing: bool) {
        if let Some(ImageEntry::Animated(a)) = self.images.get_mut(id) {
            a.playing = playing;
            if playing {
                a.elapsed_ms = 0.0;
            }
        }
    }

    /// Advance all animated images; returns true while any animation is playing.
    pub fn advance_animations(&mut self, dt_ms: f32) -> bool {
        let mut needs_redraw = false;
        for entry in self.images.values_mut() {
            if let ImageEntry::Animated(a) = entry {
                if a.advance(dt_ms) {
                    needs_redraw = true;
                }
            }
        }
        needs_redraw
    }

    pub fn paint(
        &mut self,
        scene: &mut Scene,
        id: &ImageId,
        bounds: Rect,
        fit: ImageFit,
        tint: Option<Color>,
    ) -> Result<(), ImageError> {
        match self.images.get(id) {
            Some(ImageEntry::Static(_)) | Some(ImageEntry::Animated(_)) => {
                let raster = match self.images.get(id) {
                    Some(ImageEntry::Static(r)) => r,
                    Some(ImageEntry::Animated(a)) => a.current(),
                    _ => unreachable!(),
                };
                paint_raster(scene, raster, bounds, fit);
                Ok(())
            }
            Some(ImageEntry::Svg(_)) => self.paint_svg(scene, id, bounds, fit, tint),
            None => Err(ImageError::NotLoaded(id.0.clone())),
        }
    }

    fn paint_svg(
        &mut self,
        scene: &mut Scene,
        id: &ImageId,
        bounds: Rect,
        fit: ImageFit,
        tint: Option<Color>,
    ) -> Result<(), ImageError> {
        let key = (id.clone(), tint.map(color_key).unwrap_or(0));
        if !self.svg_trees.contains_key(&key) {
            let raw = match self.images.get(id) {
                Some(ImageEntry::Svg(s)) => s.raw.clone(),
                _ => return Err(ImageError::NotLoaded(id.0.clone())),
            };
            let source = match tint {
                Some(color) => raw.replace("currentColor", &color_to_hex(color)),
                None => raw,
            };
            let opt = usvg::Options::default();
            let tree = usvg::Tree::from_str(&source, &opt)
                .map_err(|e| ImageError::Svg(e.to_string()))?;
            self.svg_trees.insert(key.clone(), tree);
        }
        let tree = self.svg_trees.get(&key).expect("tree inserted");
        let size = tree.size();
        let iw = size.width() as f64;
        let ih = size.height() as f64;
        let transform = fit_affine_f64(bounds, iw, ih, fit);
        let mut svg_scene = Scene::new();
        vello_svg::append_tree(&mut svg_scene, tree);
        scene.append(&svg_scene, Some(transform));
        Ok(())
    }
}

impl ImagePainter for ImageCache {
    fn image_size(&self, id: &str) -> Option<Size> {
        self.size(&ImageId(id.to_string()))
            .map(|(w, h)| Size::new(w as f64, h as f64))
    }

    fn paint_image_with_tint(
        &mut self,
        scene: &mut Scene,
        id: &str,
        bounds: Rect,
        fit: ImageFit,
        tint: Option<Color>,
    ) -> Result<(), String> {
        self.paint(scene, &ImageId(id.to_string()), bounds, fit, tint)
            .map_err(|e| e.to_string())
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').trim_start(),
        Err(_) => return false,
    };
    let lower = text.to_ascii_lowercase();
    lower.starts_with("<svg")
        || (lower.starts_with("<?xml") && lower.contains("<svg"))
        || (lower.starts_with("<!doctype") && lower.contains("<svg"))
}

fn decode_svg(bytes: &[u8]) -> Result<ImageEntry, ImageError> {
    let raw = std::str::from_utf8(bytes)
        .map_err(|e| ImageError::Svg(format!("svg is not utf-8: {e}")))?
        .to_string();
    let opt = usvg::Options::default();
    let tree =
        usvg::Tree::from_str(&raw, &opt).map_err(|e| ImageError::Svg(e.to_string()))?;
    let size = tree.size();
    Ok(ImageEntry::Svg(SvgAsset {
        raw,
        width: size.width(),
        height: size.height(),
    }))
}

fn decode_image(bytes: &[u8]) -> Result<ImageEntry, ImageError> {
    if looks_like_svg(bytes) {
        return decode_svg(bytes);
    }
    if let Some(animated) = try_decode_animated(bytes)? {
        return Ok(ImageEntry::Animated(animated));
    }
    let (width, height, pixels) = load_static_rgba(bytes)?;
    Ok(ImageEntry::Static(Raster {
        width,
        height,
        pixels,
    }))
}

fn try_decode_animated(bytes: &[u8]) -> Result<Option<AnimatedRaster>, ImageError> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let format = match reader.format() {
        Some(f) => f,
        None => return Ok(None),
    };

    let animated = match format {
        ImageFormat::Gif => {
            let decoder = GifDecoder::new(Cursor::new(bytes))?;
            let (width, height) = decoder.dimensions();
            let frames = decoder.into_frames().collect_frames()?;
            if frames.len() <= 1 {
                None
            } else {
                Some(build_animation(width, height, frames)?)
            }
        }
        ImageFormat::WebP => {
            let decoder = WebPDecoder::new(Cursor::new(bytes))?;
            if !decoder.has_animation() {
                None
            } else {
                let (width, height) = decoder.dimensions();
                let frames = decoder.into_frames().collect_frames()?;
                if frames.len() <= 1 {
                    None
                } else {
                    Some(build_animation(width, height, frames)?)
                }
            }
        }
        _ => None,
    };
    Ok(animated)
}

fn build_animation(
    width: u32,
    height: u32,
    frames: Vec<Frame>,
) -> Result<AnimatedRaster, ImageError> {
    let mut canvas = image::RgbaImage::new(width, height);
    let mut raster_frames = Vec::with_capacity(frames.len());
    let mut delays_ms = Vec::with_capacity(frames.len());

    for frame in frames {
        let (num, denom) = frame.delay().numer_denom_ms();
        let delay_ms = if denom == 0 {
            100
        } else {
            ((u64::from(num) * 1000) / u64::from(denom)).max(1) as u32
        };
        delays_ms.push(delay_ms);
        imageops::overlay(
            &mut canvas,
            frame.buffer(),
            i64::from(frame.left()),
            i64::from(frame.top()),
        );
        let mut data = canvas.clone().into_raw();
        premultiply_rgba(&mut data);
        raster_frames.push(Raster {
            width,
            height,
            pixels: rgba_blob(data),
        });
    }

    Ok(AnimatedRaster {
        width,
        height,
        frames: raster_frames,
        delays_ms,
        frame_index: 0,
        elapsed_ms: 0.0,
        playing: true,
    })
}

fn rgba_blob(data: Vec<u8>) -> Arc<dyn AsRef<[u8]> + Send + Sync> {
    Arc::new(data)
}

fn load_static_rgba(
    bytes: &[u8],
) -> Result<(u32, u32, Arc<dyn AsRef<[u8]> + Send + Sync>), ImageError> {
    let img = image::load_from_memory(bytes)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut data = rgba.into_raw();
    premultiply_rgba(&mut data);
    Ok((width, height, rgba_blob(data)))
}

fn premultiply_rgba(data: &mut [u8]) {
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as f32 / 255.0;
        px[0] = (px[0] as f32 * a) as u8;
        px[1] = (px[1] as f32 * a) as u8;
        px[2] = (px[2] as f32 * a) as u8;
    }
}

fn paint_raster(scene: &mut Scene, raster: &Raster, bounds: Rect, fit: ImageFit) {
    let brush = ImageBrush::new(ImageData {
        data: Blob::new(Arc::clone(&raster.pixels)),
        format: VelloImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: raster.width,
        height: raster.height,
    });
    let affine = fit_affine_f64(bounds, raster.width as f64, raster.height as f64, fit);
    scene.draw_image(brush.as_ref(), affine);
}

fn fit_affine_f64(bounds: Rect, iw: f64, ih: f64, fit: ImageFit) -> Affine {
    if iw <= 0.0 || ih <= 0.0 {
        return Affine::IDENTITY;
    }

    let (scale_x, scale_y) = match fit {
        ImageFit::Fill => (bounds.width() / iw, bounds.height() / ih),
        ImageFit::Contain => {
            let s = (bounds.width() / iw).min(bounds.height() / ih);
            (s, s)
        }
        ImageFit::Cover => {
            let s = (bounds.width() / iw).max(bounds.height() / ih);
            (s, s)
        }
    };

    let dw = iw * scale_x;
    let dh = ih * scale_y;
    let tx = bounds.x0 + (bounds.width() - dw) / 2.0;
    let ty = bounds.y0 + (bounds.height() - dh) / 2.0;
    Affine::translate(Vec2::new(tx, ty)) * Affine::scale_non_uniform(scale_x, scale_y)
}

fn color_key(color: Color) -> u32 {
    let c = color.to_rgba8();
    u32::from_be_bytes([c.r, c.g, c.b, c.a])
}

fn color_to_hex(color: Color) -> String {
    let c = color.to_rgba8();
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::png::PngEncoder;
    use image::{ExtendedColorType, ImageEncoder};

    fn png_1x1() -> Vec<u8> {
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&[255, 0, 0, 255], 1, 1, ExtendedColorType::Rgba8)
            .unwrap();
        png
    }

    const HOME_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><path fill="currentColor" d="M10 20v-6h4v6h5v-8h3L12 3L2 12h3v8z"/></svg>"#;

    #[test]
    fn loads_static_png() {
        let mut cache = ImageCache::new();
        let id = ImageId("dot".into());
        cache.load_bytes(id.clone(), &png_1x1()).unwrap();
        assert_eq!(cache.size(&id), Some((1, 1)));
        assert!(!cache.is_animated(&id));
        assert!(!cache.is_svg(&id));
    }

    #[test]
    fn loads_svg_as_vector_asset() {
        let mut cache = ImageCache::new();
        let id = ImageId("home".into());
        cache.load_bytes(id.clone(), HOME_SVG.as_bytes()).unwrap();
        assert!(cache.is_svg(&id));
        assert_eq!(cache.size(&id), Some((24, 24)));
        let mut scene = Scene::new();
        cache
            .paint(
                &mut scene,
                &id,
                Rect::new(0.0, 0.0, 48.0, 48.0),
                ImageFit::Contain,
                Some(Color::from_rgb8(0x67, 0x50, 0xa4)),
            )
            .unwrap();
        // Second paint at a different size must still succeed (crisp re-render).
        cache
            .paint(
                &mut scene,
                &id,
                Rect::new(0.0, 0.0, 96.0, 96.0),
                ImageFit::Contain,
                Some(Color::from_rgb8(0x67, 0x50, 0xa4)),
            )
            .unwrap();
    }

    #[test]
    fn looks_like_svg_detects_markup() {
        assert!(looks_like_svg(HOME_SVG.as_bytes()));
        assert!(looks_like_svg(
            b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>"
        ));
        assert!(!looks_like_svg(&png_1x1()));
    }
}
