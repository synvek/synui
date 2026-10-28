//! Parley text layout and Vello scene painting.

use std::cell::RefCell;

use parley::{
    Affinity, Alignment, AlignmentOptions, Cursor, FontContext, FontStyle, FontWeight, Layout,
    LayoutContext, LineHeight, PositionedLayoutItem, Selection, StyleProperty,
};
use synui_core::{Size, TextAlign, TextMeasurer, TextPainter, TextStyleRun};
use vello::kurbo::{Affine, Line, Rect, Stroke};
use vello::peniko::{Color, Fill};
use vello::{Glyph, Scene};

const DISPLAY_SCALE: f32 = 1.0;

/// Fixed line box as a multiple of font size (CSS-like unitless `line-height`).
///
/// Parley's default is metrics-relative, so swapping Latin → CJK (or mixed) jumps
/// layout height (~1.0em → ~1.4em). Pinning to font-size keeps labels / fields /
/// text areas stable while typing through an IME.
pub const LINE_HEIGHT_EM: f32 = 1.4;

/// Extra advance (in ems) after an italic run that is followed by upright text.
const ITALIC_CORRECTION_EM: f32 = 0.08;

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Optical vertical mid of a line for UI centering.
///
/// Geometric `(block_min + block_max) / 2` sits below the ink mass because Latin/CJK
/// line boxes reserve unused descent below the baseline — text then looks high inside
/// buttons, fields, and tips. Weight the ascent band more heavily.
fn optical_line_mid(m: &parley::LineMetrics) -> f64 {
    let baseline = m.baseline as f64;
    let ascent = m.ascent as f64;
    let descent = m.descent as f64;
    baseline - ascent * 0.5 + descent * 0.25
}

thread_local! {
    static SHARED_RENDERER: RefCell<TextRenderer> = RefCell::new(TextRenderer::new());
}

/// Borrow the process thread-local [`TextRenderer`] (avoids per-event FontContext init).
pub fn with_text_renderer<R>(f: impl FnOnce(&mut TextRenderer) -> R) -> R {
    SHARED_RENDERER.with(|cell| f(&mut cell.borrow_mut()))
}

const LAYOUT_CACHE_CAP: usize = 64;

/// Fixed brush for geometry-only layouts (measure / hit / caret). Never used for paint.
const GEOMETRY_BRUSH: [u8; 4] = [0, 0, 0, 255];

/// Fingerprint for measure/hit/caret layouts (Phase 58): **no** brush.
///
/// Paint must never read this cache — Parley bakes fill into glyphs, so a BLACK
/// geometry entry reused at paint would yield invisible text on dark surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GeometryLayoutKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub max_width_bits: u32,
    pub runs_fingerprint: u64,
}

impl GeometryLayoutKey {
    pub fn new(text: &str, font_size: f32, max_width: Option<f32>, runs: &[TextStyleRun]) -> Self {
        Self {
            text_hash: hash_bytes(text.as_bytes()),
            font_size_bits: font_size.to_bits(),
            max_width_bits: max_width.unwrap_or(-1.0).to_bits(),
            runs_fingerprint: runs_fingerprint(runs),
        }
    }
}

/// Fingerprint for paint layouts (Phase 16/58): brush is part of the key.
///
/// Omitting brush caused theme toggles (and measure-with-BLACK vs paint-with-
/// on_surface) to reuse glyphs with the wrong fill — intermittent invisible text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextLayoutKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub max_width_bits: u32,
    pub runs_fingerprint: u64,
    pub brush: [u8; 4],
}

impl TextLayoutKey {
    pub fn new(
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        runs: &[TextStyleRun],
        brush: [u8; 4],
    ) -> Self {
        Self {
            text_hash: hash_bytes(text.as_bytes()),
            font_size_bits: font_size.to_bits(),
            max_width_bits: max_width.unwrap_or(-1.0).to_bits(),
            runs_fingerprint: runs_fingerprint(runs),
            brush,
        }
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    // FNV-1a 64
    let mut h = 0xcbf29ce484222325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn runs_fingerprint(runs: &[TextStyleRun]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for r in runs {
        h ^= r.range.start as u64;
        h = h.wrapping_mul(0x100000001b3);
        h ^= r.range.end as u64;
        h = h.wrapping_mul(0x100000001b3);
        h ^= (r.bold as u64) << 1 | (r.italic as u64) << 2 | (r.underline as u64) << 3;
        h = h.wrapping_mul(0x100000001b3);
        if let Some(sz) = r.font_size {
            h ^= sz.to_bits() as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        if let Some(c) = r.color {
            h ^= c[0] as u64;
            h = h.wrapping_mul(0x100000001b3);
            h ^= c[1] as u64;
            h = h.wrapping_mul(0x100000001b3);
            h ^= c[2] as u64;
            h = h.wrapping_mul(0x100000001b3);
            h ^= c[3] as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn color_brush(color: Color) -> [u8; 4] {
    [
        (color.components[0] * 255.0) as u8,
        (color.components[1] * 255.0) as u8,
        (color.components[2] * 255.0) as u8,
        (color.components[3] * 255.0) as u8,
    ]
}

/// LRU cache of Parley [`Layout`]s keyed by `K`.
pub struct LayoutLruCache<K> {
    entries: std::collections::HashMap<K, Layout<[u8; 4]>>,
    order: std::collections::VecDeque<K>,
    pub hits: u64,
    pub misses: u64,
}

impl<K> Default for LayoutLruCache<K> {
    fn default() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            order: std::collections::VecDeque::new(),
            hits: 0,
            misses: 0,
        }
    }
}

impl<K: Clone + Eq + std::hash::Hash> LayoutLruCache<K> {
    pub fn get(&mut self, key: &K) -> Option<Layout<[u8; 4]>> {
        if let Some(layout) = self.entries.get(key) {
            self.hits = self.hits.saturating_add(1);
            if let Some(i) = self.order.iter().position(|k| k == key) {
                self.order.remove(i);
            }
            self.order.push_back(key.clone());
            Some(layout.clone())
        } else {
            None
        }
    }

    pub fn insert(&mut self, key: K, layout: Layout<[u8; 4]>) {
        if self.entries.contains_key(&key) {
            self.entries.insert(key, layout);
            return;
        }
        while self.entries.len() >= LAYOUT_CACHE_CAP {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            } else {
                break;
            }
        }
        self.entries.insert(key.clone(), layout);
        self.order.push_back(key);
        self.misses = self.misses.saturating_add(1);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    pub fn clear_stats(&mut self) {
        self.hits = 0;
        self.misses = 0;
    }

    /// Drop all cached layouts (call on theme polarity change).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
}

/// Paint-side layout cache (brush in key). Alias for older call sites.
pub type TextLayoutCache = LayoutLruCache<TextLayoutKey>;

/// Text rendering context wrapping Parley.
pub struct TextRenderer {
    pub font_cx: FontContext,
    pub layout_cx: LayoutContext,
    /// Measure / hit / caret — brush-agnostic (Phase 58).
    pub geometry_cache: LayoutLruCache<GeometryLayoutKey>,
    /// Paint only — brush-keyed so theme colors never reuse BLACK glyphs.
    pub paint_cache: LayoutLruCache<TextLayoutKey>,
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            font_cx: FontContext::default(),
            layout_cx: LayoutContext::new(),
            geometry_cache: LayoutLruCache::default(),
            paint_cache: LayoutLruCache::default(),
        }
    }

    /// Geometry cache hit/miss counters (measure / hit / caret).
    pub fn layout_cache_stats(&self) -> (u64, u64) {
        (self.geometry_cache.hits, self.geometry_cache.misses)
    }

    /// Paint cache hit/miss counters.
    pub fn paint_cache_stats(&self) -> (u64, u64) {
        (self.paint_cache.hits, self.paint_cache.misses)
    }

    /// Invalidate Parley layouts after a theme change (brush is baked into layouts).
    ///
    /// Hosts must clear both Application `TextRenderer` and [`with_text_renderer`].
    pub fn clear_layout_cache(&mut self) {
        self.geometry_cache.clear();
        self.paint_cache.clear();
    }

    pub fn measure(&mut self, text: &str, font_size: f32, max_width: Option<f32>) -> Size {
        // Match [`LINE_HEIGHT_EM`] so CJK vs Latin never jumps control/label row heights.
        let single_line_h = font_size as f64 * LINE_HEIGHT_EM as f64;
        // Fast path for unwrapped ASCII: layout_10k Gate A — Parley dominates cold measure.
        if max_width.is_none() && text.is_ascii() && !text.contains('\n') {
            let w = text.len() as f64 * font_size as f64 * 0.55 + 4.0;
            return Size::new(w, single_line_h);
        }
        let layout = self.build_geometry_layout(text, font_size, max_width, &[]);
        let h = if max_width.is_none() && !text.contains('\n') {
            single_line_h
        } else {
            (layout.height() as f64).ceil() + 2.0
        };
        Size::new((layout.width() as f64).ceil() + 4.0, h)
    }

    /// Fit `text` into `max_width` (single line), appending `…` when clipped.
    ///
    /// Uses Parley glyph advances (not the ASCII measure fast-path). Char
    /// boundaries only — suitable for Label paint-width truncation.
    pub fn fit_text_with_ellipsis(&mut self, text: &str, font_size: f32, max_width: f32) -> String {
        if max_width <= 0.0 || text.is_empty() {
            return String::new();
        }
        // `Some(_)` bypasses the ASCII heuristic so prefix and `…` share one metric.
        let glyph_w = |r: &mut Self, s: &str| r.measure(s, font_size, Some(10_000.0)).width;
        let full_w = glyph_w(self, text);
        if full_w <= max_width as f64 {
            return text.to_owned();
        }
        let ellipsis = "…";
        let ellipsis_w = glyph_w(self, ellipsis);
        if ellipsis_w > max_width as f64 {
            return ellipsis.to_owned();
        }
        let chars: Vec<char> = text.chars().collect();
        let mut lo = 0usize;
        let mut hi = chars.len();
        while lo < hi {
            let mid = (lo + hi + 1) / 2;
            let mut candidate: String = chars[..mid].iter().collect();
            candidate.push_str(ellipsis);
            if glyph_w(self, &candidate) <= max_width as f64 {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let mut out: String = chars[..lo].iter().collect();
        out.push_str(ellipsis);
        out
    }

    /// Measure with style runs (bold / per-span font_size, etc.).
    pub fn measure_styled(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        runs: &[TextStyleRun],
    ) -> Size {
        if runs.is_empty() {
            return self.measure(text, font_size, max_width);
        }
        let layout = self.build_geometry_layout(text, font_size, max_width, runs);
        Size::new(
            (layout.width() as f64).ceil() + 4.0,
            (layout.height() as f64).ceil() + 2.0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font_size: f32,
        color: Color,
        bounds: Rect,
        max_width: Option<f32>,
        align: TextAlign,
    ) {
        self.paint_styled(scene, text, font_size, color, bounds, max_width, align, &[]);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_styled(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font_size: f32,
        color: Color,
        bounds: Rect,
        max_width: Option<f32>,
        align: TextAlign,
        runs: &[TextStyleRun],
    ) {
        let wrap_width = match max_width {
            Some(w) if w > 0.0 => Some(w),
            _ => None,
        };
        let layout = self.build_paint_layout(text, font_size, wrap_width, color, runs);
        let layout_width = layout.width() as f64;
        let layout_height = layout.height() as f64;

        let origin_x = match align {
            TextAlign::Start => bounds.x0,
            TextAlign::Center => bounds.x0 + ((bounds.width() - layout_width) / 2.0).max(0.0),
        };
        // Multiline and tight bounds (editors): top-align so selection/hit-test share
        // the same origin as glyphs. Only vertically center when the caller passes
        // explicitly tall bounds (buttons / tips / single-line labels).
        let origin_y = if layout.len() > 1 || bounds.height() <= layout_height * 1.35 + 2.0 {
            bounds.y0
        } else if let Some(line) = layout.lines().next() {
            let optical_mid = optical_line_mid(line.metrics());
            bounds.y0 + bounds.height() / 2.0 - optical_mid
        } else {
            bounds.y0 + ((bounds.height() - layout_height) / 2.0).max(0.0)
        };
        let layout_transform = Affine::translate((origin_x, origin_y));

        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let style = glyph_run.style();
                let run = glyph_run.run();
                let font = run.font().clone();
                let run_size = run.font_size();
                let brush = style.brush;
                let run_color = Color::from_rgba8(brush[0], brush[1], brush[2], brush[3]);

                // Underlines under glyphs (CSS text-decor painting order).
                if let Some(underline) = &style.underline {
                    let metrics = run.metrics();
                    let offset = underline.offset.unwrap_or(metrics.underline_offset);
                    let width = underline.size.unwrap_or(metrics.underline_size).max(1.0);
                    let y = glyph_run.baseline() - offset + width / 2.0;
                    let ul = Line::new(
                        (glyph_run.offset() as f64, y as f64),
                        ((glyph_run.offset() + glyph_run.advance()) as f64, y as f64),
                    );
                    let ul_brush = underline.brush;
                    let ul_color =
                        Color::from_rgba8(ul_brush[0], ul_brush[1], ul_brush[2], ul_brush[3]);
                    scene.stroke(
                        &Stroke::new(width as f64),
                        layout_transform,
                        ul_color,
                        None,
                        &ul,
                    );
                }

                // Faux italic: Parley keeps upright advances; Vello shears glyphs.
                let glyph_xform = run
                    .synthesis()
                    .skew()
                    .map(|angle| Affine::skew((angle as f64).to_radians().tan(), 0.0));
                let glyphs: Vec<Glyph> = glyph_run
                    .positioned_glyphs()
                    .map(|g| Glyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                    })
                    .collect();
                if glyphs.is_empty() {
                    continue;
                }
                scene
                    .draw_glyphs(&font)
                    .brush(run_color)
                    .font_size(run_size)
                    .transform(layout_transform)
                    .glyph_transform(glyph_xform)
                    .normalized_coords(run.normalized_coords())
                    .draw(Fill::NonZero, glyphs.into_iter());
            }
        }
    }

    /// Hit-test a point in local layout coordinates → UTF-8 byte index.
    pub fn hit_test_point(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        x: f64,
        y: f64,
    ) -> usize {
        self.hit_test_point_styled(text, font_size, max_width, x, y, &[])
    }

    /// Hit-test using the same style runs as rich-text paint (bold/italic/etc.).
    pub fn hit_test_point_styled(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        x: f64,
        y: f64,
        runs: &[TextStyleRun],
    ) -> usize {
        let layout = self.build_geometry_layout(text, font_size, max_width, runs);
        Cursor::from_point(&layout, x as f32, y as f32).index()
    }

    /// Caret rectangle (local to layout origin) for a byte index.
    pub fn caret_bounds(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        index: usize,
    ) -> Rect {
        self.caret_and_height(text, font_size, max_width, index).0
    }

    /// Caret bounds + layout height from a single Parley layout (TextArea scroll).
    pub fn caret_and_height(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        index: usize,
    ) -> (Rect, f64) {
        let index = floor_char_boundary(text, index.min(text.len()));
        // Always use Parley — paint_text does too. An ASCII `len * 0.55`
        // shortcut drifted from real glyph advances (worse with longer Latin
        // strings); CJK skipped that path and looked correct.
        let layout = self.build_geometry_layout(text, font_size, max_width, &[]);
        let cursor = Cursor::from_byte_index(&layout, index, Affinity::Downstream);
        let bb = cursor.geometry(&layout, 1.5);
        let caret = Rect::new(bb.x0, bb.y0, bb.x1.max(bb.x0 + 1.0), bb.y1.max(bb.y0 + 1.0));
        let height = (layout.height() as f64).ceil() + 2.0;
        (caret, height.max(caret.y1))
    }

    /// Caret bounds using the same style runs as rich-text paint.
    pub fn caret_bounds_styled(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        index: usize,
        runs: &[TextStyleRun],
    ) -> Rect {
        let index = floor_char_boundary(text, index.min(text.len()));
        let layout = self.build_geometry_layout(text, font_size, max_width, runs);
        let cursor = Cursor::from_byte_index(&layout, index, Affinity::Downstream);
        let bb = cursor.geometry(&layout, 1.5);
        Rect::new(bb.x0, bb.y0, bb.x1.max(bb.x0 + 1.0), bb.y1.max(bb.y0 + 1.0))
    }

    /// Selection highlight rects in layout-local coordinates.
    pub fn selection_bounds(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        range: std::ops::Range<usize>,
    ) -> Vec<Rect> {
        self.selection_bounds_styled(text, font_size, max_width, range, &[])
    }

    /// Selection bounds using the same style runs as rich-text paint.
    pub fn selection_bounds_styled(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        range: std::ops::Range<usize>,
        runs: &[TextStyleRun],
    ) -> Vec<Rect> {
        let a = floor_char_boundary(text, range.start.min(range.end).min(text.len()));
        let b = ceil_char_boundary(text, range.start.max(range.end).min(text.len()));
        if a >= b {
            return Vec::new();
        }
        let layout = self.build_geometry_layout(text, font_size, max_width, runs);
        let sel = Selection::from_byte_index(&layout, a, Affinity::Downstream)
            .extend(Cursor::from_byte_index(&layout, b, Affinity::Upstream));
        sel.geometry(&layout)
            .into_iter()
            .map(|(bb, _)| Rect::new(bb.x0, bb.y0, bb.x1, bb.y1))
            .collect()
    }

    /// Geometry layout for measure / hit / caret — never consulted by paint.
    fn build_geometry_layout(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        runs: &[TextStyleRun],
    ) -> Layout<[u8; 4]> {
        let key = GeometryLayoutKey::new(text, font_size, max_width, runs);
        if let Some(layout) = self.geometry_cache.get(&key) {
            return layout;
        }
        let layout = self.shape_layout(text, font_size, max_width, GEOMETRY_BRUSH, runs);
        self.geometry_cache.insert(key, layout.clone());
        layout
    }

    /// Paint layout keyed by brush so theme colors never reuse geometry BLACK glyphs.
    fn build_paint_layout(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        color: Color,
        runs: &[TextStyleRun],
    ) -> Layout<[u8; 4]> {
        let brush = color_brush(color);
        let key = TextLayoutKey::new(text, font_size, max_width, runs, brush);
        if let Some(layout) = self.paint_cache.get(&key) {
            return layout;
        }
        let layout = self.shape_layout(text, font_size, max_width, brush, runs);
        self.paint_cache.insert(key, layout.clone());
        layout
    }

    fn shape_layout(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        brush: [u8; 4],
        runs: &[TextStyleRun],
    ) -> Layout<[u8; 4]> {
        let mut builder =
            self.layout_cx
                .ranged_builder(&mut self.font_cx, text, DISPLAY_SCALE, true);
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::Brush(brush));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            LINE_HEIGHT_EM,
        )));
        for run in runs {
            let start = run.range.start.min(text.len());
            let end = run.range.end.min(text.len());
            if start >= end {
                continue;
            }
            let range = start..end;
            if let Some(sz) = run.font_size {
                builder.push(StyleProperty::FontSize(sz), range.clone());
            }
            if run.bold {
                builder.push(StyleProperty::FontWeight(FontWeight::BOLD), range.clone());
            }
            if run.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), range.clone());
            }
            if run.underline {
                builder.push(StyleProperty::Underline(true), range.clone());
            }
            if let Some(c) = run.color {
                builder.push(StyleProperty::Brush(c), range);
            }
        }
        push_italic_corrections(&mut builder, text, font_size, runs);
        let mut layout = builder.build(text);
        layout.break_all_lines(max_width);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn layout_cache_hits_on_repeat() {
        let mut r = TextRenderer::new();
        let _ = r.measure("Hello cache", 14.0, Some(200.0));
        let (h0, m0) = r.layout_cache_stats();
        assert_eq!(h0, 0);
        assert!(m0 >= 1);
        let _ = r.measure("Hello cache", 14.0, Some(200.0));
        let (h1, _) = r.layout_cache_stats();
        assert!(h1 >= 1, "expected cache hit, hits={h1}");
    }

    #[test]
    fn layout_cache_misses_when_brush_color_changes() {
        let mut r = TextRenderer::new();
        let bounds = Rect::new(0.0, 0.0, 200.0, 40.0);
        let mut scene = Scene::new();
        r.paint(
            &mut scene,
            "Theme text",
            14.0,
            Color::from_rgb8(0, 0, 0),
            bounds,
            Some(200.0),
            TextAlign::Start,
        );
        let (_, m0) = r.paint_cache_stats();
        r.paint(
            &mut scene,
            "Theme text",
            14.0,
            Color::from_rgb8(230, 225, 229),
            bounds,
            Some(200.0),
            TextAlign::Start,
        );
        let (hits, misses) = r.paint_cache_stats();
        assert!(
            misses > m0,
            "paint with a new brush must not reuse the old layout, misses={misses} m0={m0} hits={hits}"
        );
    }

    #[test]
    fn measure_does_not_populate_paint_cache() {
        let mut r = TextRenderer::new();
        let _ = r.measure("Separate caches", 14.0, Some(200.0));
        assert_eq!(
            r.paint_cache.len(),
            0,
            "measure must not insert paint entries"
        );
        assert!(r.geometry_cache.len() >= 1);
        let mut scene = Scene::new();
        r.paint(
            &mut scene,
            "Separate caches",
            14.0,
            Color::from_rgb8(230, 225, 229),
            Rect::new(0.0, 0.0, 200.0, 40.0),
            Some(200.0),
            TextAlign::Start,
        );
        let (_, paint_misses) = r.paint_cache_stats();
        assert!(
            paint_misses >= 1,
            "paint after measure must miss paint cache (not reuse geometry BLACK)"
        );
    }

    #[test]
    fn geometry_cache_survives_paint_lru_pressure() {
        let mut r = TextRenderer::new();
        let text = "Pinned geometry";
        let _ = r.measure(text, 14.0, Some(200.0));
        let mut scene = Scene::new();
        // Flood paint LRU (cap 64) with distinct brush colors — must not evict geometry.
        for i in 0..80u8 {
            r.paint(
                &mut scene,
                &format!("paint flood {i}"),
                14.0,
                Color::from_rgb8(i, 40, 80),
                Rect::new(0.0, 0.0, 200.0, 40.0),
                Some(200.0),
                TextAlign::Start,
            );
        }
        r.geometry_cache.clear_stats();
        let _ = r.measure(text, 14.0, Some(200.0));
        let (hits, _) = r.layout_cache_stats();
        assert!(
            hits >= 1,
            "geometry entry must survive paint-only LRU pressure, hits={hits}"
        );
    }

    #[test]
    fn dark_on_surface_paint_uses_light_brush_not_black() {
        let mut r = TextRenderer::new();
        // MD3 dark on_surface-ish (light glyphs on dark surface).
        let on_surface = Color::from_rgb8(230, 225, 229);
        let _ = r.measure("Label", 14.0, Some(120.0));
        let mut scene = Scene::new();
        r.paint(
            &mut scene,
            "Label",
            14.0,
            on_surface,
            Rect::new(0.0, 0.0, 120.0, 24.0),
            Some(120.0),
            TextAlign::Start,
        );
        let key = TextLayoutKey::new("Label", 14.0, Some(120.0), &[], color_brush(on_surface));
        assert!(
            r.paint_cache.contains(&key),
            "dark-theme Label paint must cache the light brush layout"
        );
        let black_key = TextLayoutKey::new("Label", 14.0, Some(120.0), &[], GEOMETRY_BRUSH);
        assert!(
            !r.paint_cache.contains(&black_key),
            "paint cache must not hold geometry BLACK under the paint key space"
        );
    }

    #[test]
    fn clear_layout_cache_drops_geometry_and_paint() {
        let mut r = TextRenderer::new();
        let _ = r.measure("Clear me", 14.0, Some(100.0));
        let mut scene = Scene::new();
        r.paint(
            &mut scene,
            "Clear me",
            14.0,
            Color::from_rgb8(1, 2, 3),
            Rect::new(0.0, 0.0, 100.0, 20.0),
            Some(100.0),
            TextAlign::Start,
        );
        assert!(r.geometry_cache.len() > 0);
        assert!(r.paint_cache.len() > 0);
        r.clear_layout_cache();
        assert_eq!(r.geometry_cache.len(), 0);
        assert_eq!(r.paint_cache.len(), 0);
    }

    #[test]
    fn ascii_caret_x_tracks_glyph_width_not_char_count() {
        let mut r = TextRenderer::new();
        let font = 16.0;
        // Same byte length — the old `len * 0.55` shortcut put both carets at
        // the same x, while painted glyphs differ a lot (i vs W).
        let thin = "iiiiiiiiii";
        let wide = "WWWWWWWWWW";
        assert_eq!(thin.len(), wide.len());
        let thin_x = r.caret_bounds(thin, font, None, thin.len()).x0;
        let wide_x = r.caret_bounds(wide, font, None, wide.len()).x0;
        assert!(
            (wide_x - thin_x).abs() > 10.0,
            "ASCII caret must follow glyph advances, not char count: thin={thin_x} wide={wide_x}"
        );

        // Mixing in CJK must not be required for correctness (regression of the
        // "works only after typing Chinese" bug).
        let latin = "Hello todo item";
        let mixed = "Hello todo item中";
        let latin_mid = r.caret_bounds(latin, font, None, 5).x0;
        let mixed_mid = r.caret_bounds(mixed, font, None, 5).x0;
        assert!(
            (latin_mid - mixed_mid).abs() < 1.0,
            "prefix caret before CJK must match pure ASCII: latin={latin_mid} mixed={mixed_mid}"
        );
    }

    #[test]
    fn fit_text_with_ellipsis_clips_to_max_width() {
        let mut r = TextRenderer::new();
        let font = 16.0;
        let long = "WWWWWWWWWWWWWWWWWWWW";
        let fitted = r.fit_text_with_ellipsis(long, font, 60.0);
        assert!(
            fitted.ends_with('…'),
            "narrow budget must append ellipsis, got {fitted:?}"
        );
        assert!(
            fitted.chars().count() < long.chars().count(),
            "fitted text must be shorter than source"
        );
        let fitted_w = r.measure(&fitted, font, None).width;
        assert!(
            fitted_w <= 60.0 + 2.0,
            "fitted glyphs must fit budget, width={fitted_w}"
        );
        let short = "Hi";
        assert_eq!(r.fit_text_with_ellipsis(short, font, 200.0), short);
    }
}

/// Add letter-spacing on the last character of each italic run when the next
/// character is upright (classic italic correction / kerning at style breaks).
fn push_italic_corrections(
    builder: &mut parley::RangedBuilder<'_, [u8; 4]>,
    text: &str,
    font_size: f32,
    runs: &[TextStyleRun],
) {
    let corr = font_size * ITALIC_CORRECTION_EM;
    if corr <= 0.0 || text.is_empty() {
        return;
    }
    for run in runs {
        if !run.italic {
            continue;
        }
        let end = run.range.end.min(text.len());
        if end == 0 || end >= text.len() {
            continue;
        }
        let next_italic = runs
            .iter()
            .any(|r| r.italic && r.range.start <= end && end < r.range.end);
        if next_italic {
            continue;
        }
        match text[end..].chars().next() {
            None | Some('\n') | Some('\r') => continue,
            Some(_) => {}
        }
        if let Some((i, _)) = text[..end].char_indices().next_back() {
            builder.push(StyleProperty::LetterSpacing(corr), i..end);
        }
    }
}

impl TextMeasurer for TextRenderer {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: Option<f32>) -> Size {
        self.measure(text, font_size, max_width)
    }
}

impl TextPainter for TextRenderer {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: Option<f32>) -> Size {
        self.measure(text, font_size, max_width)
    }

    fn paint_text(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font_size: f32,
        color: Color,
        bounds: Rect,
        max_width: Option<f32>,
        align: TextAlign,
    ) {
        self.paint_styled(scene, text, font_size, color, bounds, max_width, align, &[]);
    }

    fn paint_text_styled(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font_size: f32,
        color: Color,
        bounds: Rect,
        max_width: Option<f32>,
        align: TextAlign,
        runs: &[TextStyleRun],
    ) {
        self.paint_styled(
            scene, text, font_size, color, bounds, max_width, align, runs,
        );
    }

    fn hit_test_xy(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        x: f64,
        y: f64,
    ) -> usize {
        self.hit_test_point(text, font_size, max_width, x, y)
    }

    fn hit_test_xy_styled(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        x: f64,
        y: f64,
        runs: &[TextStyleRun],
    ) -> usize {
        self.hit_test_point_styled(text, font_size, max_width, x, y, runs)
    }

    fn caret_rect(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        index: usize,
    ) -> Rect {
        self.caret_bounds(text, font_size, max_width, index)
    }

    fn caret_rect_styled(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        index: usize,
        runs: &[TextStyleRun],
    ) -> Rect {
        self.caret_bounds_styled(text, font_size, max_width, index, runs)
    }

    fn selection_rects(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        range: std::ops::Range<usize>,
    ) -> Vec<Rect> {
        self.selection_bounds(text, font_size, max_width, range)
    }

    fn selection_rects_styled(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
        range: std::ops::Range<usize>,
        runs: &[TextStyleRun],
    ) -> Vec<Rect> {
        self.selection_bounds_styled(text, font_size, max_width, range, runs)
    }
}

/// Simple filled rounded rectangle.
pub fn paint_simple_rect(scene: &mut Scene, bounds: Rect, color: Color, radius: f64) {
    let rr = vello::kurbo::RoundedRect::from_rect(bounds, radius);
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rr);
}


#[cfg(test)]
mod line_height_stability {
    use super::*;

    #[test]
    fn cjk_and_latin_same_layout_height() {
        let mut r = TextRenderer::new();
        let a = r.build_geometry_layout("Hello world", 16.0, None, &[]).height();
        let b = r.build_geometry_layout("Hello 世界", 16.0, None, &[]).height();
        let c = r.build_geometry_layout("保存文档", 16.0, None, &[]).height();
        assert!(
            (a - b).abs() < 0.01 && (a - c).abs() < 0.01,
            "latin={a} mixed={b} cjk={c}"
        );
        assert!(
            (a - 16.0 * LINE_HEIGHT_EM).abs() < 0.01,
            "expected {em}em line box, got {a}",
            em = LINE_HEIGHT_EM
        );
    }

    #[test]
    fn wrapped_mixed_script_line_height_stable() {
        let mut r = TextRenderer::new();
        let latin = r
            .build_geometry_layout("aaaa bbbb cccc dddd", 14.0, Some(80.0), &[])
            .height();
        let mixed = r
            .build_geometry_layout("aaaa 中文 cccc dddd", 14.0, Some(80.0), &[])
            .height();
        // Same wrap width + fixed em line-height → equal total height when line count matches.
        let latin_lines = r
            .build_geometry_layout("aaaa bbbb cccc dddd", 14.0, Some(80.0), &[])
            .len();
        let mixed_lines = r
            .build_geometry_layout("aaaa 中文 cccc dddd", 14.0, Some(80.0), &[])
            .len();
        if latin_lines == mixed_lines {
            assert!(
                (latin - mixed).abs() < 0.01,
                "latin={latin} mixed={mixed} lines={latin_lines}"
            );
        }
    }
}
