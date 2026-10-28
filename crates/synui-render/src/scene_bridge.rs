//! Bridge `vello::Scene` encodings into sparse-strips scene APIs.

use crate::backend::RenderError;
use peniko::color::PremulRgba8;
use peniko::kurbo::{BezPath, Point, Rect, Stroke};
use peniko::{BlendMode, Color, Compose, Fill, Mix, Style as PenikoStyle};
use vello::Scene;
use vello_encoding::{
    DrawBeginClip, DrawColor, DrawMonoid, DrawTag, Encoding, Monoid, PathTag, Resolver, Style,
    Transform,
};
use vello_encoding::{GlyphRun as EncodedGlyphRun, PathSegmentType};

/// Target for replaying a classic Vello scene encoding.
pub trait SparseSceneSink {
    fn clear(&mut self, color: Color);
    fn set_transform(&mut self, transform: Transform);
    fn set_fill_rule(&mut self, fill: Fill);
    fn set_stroke(&mut self, stroke: Stroke);
    fn set_paint_color(&mut self, color: Color);
    fn fill_path(&mut self, path: &BezPath);
    fn stroke_path(&mut self, path: &BezPath);
    fn push_clip_layer(&mut self, path: &BezPath);
    fn push_blend_layer(&mut self, blend: BlendMode, alpha: f32);
    fn pop_layer(&mut self);
    fn fill_glyph_run(
        &mut self,
        run: &EncodedGlyphRun,
        glyphs: &[vello_encoding::Glyph],
    ) -> Result<(), RenderError>;
}

pub fn replay_vello_scene(
    scene: &Scene,
    sink: &mut dyn SparseSceneSink,
) -> Result<(), RenderError> {
    let encoding = scene.encoding();
    if encoding.is_empty() {
        return Ok(());
    }

    let paths = build_paths(encoding)?;
    let draw_monoids = scan_draw_monoids(&encoding.draw_tags);

    let mut resolver = Resolver::new();
    let mut packed = Vec::new();
    let _ = resolver.resolve(encoding, &mut packed);

    let mut clip_depth = 0usize;
    let glyph_run_for_draw: std::collections::HashMap<usize, usize> = encoding
        .resources
        .glyph_runs
        .iter()
        .enumerate()
        .map(|(run_ix, run)| (run.stream_offsets.draw_tags, run_ix))
        .collect();
    let mut path_cursor = 0usize;
    for (draw_ix, &tag) in encoding.draw_tags.iter().enumerate() {
        if tag == DrawTag::NOP {
            continue;
        }
        let m = draw_monoids[draw_ix];
        match tag {
            DrawTag::COLOR => {
                let color = read_draw_color(encoding, m.scene_offset as usize)?;
                if let Some(&run_ix) = glyph_run_for_draw.get(&draw_ix) {
                    let run = &encoding.resources.glyph_runs[run_ix];
                    let glyphs = &encoding.resources.glyphs[run.glyphs.clone()];
                    sink.set_paint_color(color);
                    sink.fill_glyph_run(run, glyphs)?;
                    continue;
                }
                let Some(built) = next_shape_path(&paths, &mut path_cursor) else {
                    continue;
                };
                apply_style(sink, built.style, built.transform);
                sink.set_paint_color(color);
                if style_is_fill(built.style) {
                    sink.fill_path(&built.path);
                } else if let Some(stroke) = style_to_stroke(built.style) {
                    sink.set_stroke(stroke);
                    sink.stroke_path(&built.path);
                }
            }
            DrawTag::BEGIN_CLIP => {
                let Some(built) = next_shape_path(&paths, &mut path_cursor) else {
                    continue;
                };
                let clip = read_draw_begin_clip(encoding, m.scene_offset as usize)?;
                apply_style(sink, built.style, built.transform);
                if clip.blend_mode == DrawBeginClip::CLIP_BLEND_MODE {
                    sink.push_clip_layer(&built.path);
                } else {
                    sink.push_blend_layer(clip_to_blend(clip.blend_mode), clip.alpha);
                }
                clip_depth += 1;
            }
            DrawTag::END_CLIP => {
                if clip_depth > 0 {
                    sink.pop_layer();
                    clip_depth -= 1;
                }
                // `encode_end_clip` appends a dummy `PATH` marker with no geometry.
                skip_empty_paths(&paths, &mut path_cursor);
            }
            DrawTag::LINEAR_GRADIENT
            | DrawTag::RADIAL_GRADIENT
            | DrawTag::SWEEP_GRADIENT
            | DrawTag::IMAGE
            | DrawTag::BLUR_RECT => {
                let Some(built) = next_shape_path(&paths, &mut path_cursor) else {
                    continue;
                };
                apply_style(sink, built.style, built.transform);
                sink.set_paint_color(Color::from_rgb8(0x80, 0x80, 0x80));
                if style_is_fill(built.style) {
                    sink.fill_path(&built.path);
                }
            }
            _ => {}
        }
    }

    while clip_depth > 0 {
        sink.pop_layer();
        clip_depth -= 1;
    }

    Ok(())
}

fn path_is_non_empty(path: &BezPath) -> bool {
    !path.elements().is_empty()
}

fn skip_empty_paths(paths: &[BuiltPath], path_cursor: &mut usize) {
    while *path_cursor < paths.len() && !path_is_non_empty(&paths[*path_cursor].path) {
        *path_cursor += 1;
    }
}

fn next_shape_path<'a>(paths: &'a [BuiltPath], path_cursor: &mut usize) -> Option<&'a BuiltPath> {
    skip_empty_paths(paths, path_cursor);
    if *path_cursor >= paths.len() {
        return None;
    }
    let built = &paths[*path_cursor];
    *path_cursor += 1;
    Some(built)
}

struct BuiltPath {
    path: BezPath,
    transform: Transform,
    style: Style,
}

fn build_paths(encoding: &Encoding) -> Result<Vec<BuiltPath>, RenderError> {
    let mut paths = Vec::new();
    let mut path = BezPath::new();
    let mut transform_ix = 0usize;
    let mut style_ix = 0usize;
    let mut data_ix = 0usize;
    let mut current_transform = Transform::IDENTITY;
    let mut current_style = Style::from_fill(Fill::NonZero);
    let mut needs_move = true;

    for &tag in &encoding.path_tags {
        if tag == PathTag::TRANSFORM {
            current_transform = *encoding
                .transforms
                .get(transform_ix)
                .ok_or_else(|| RenderError::SceneBridge("transform index out of range".into()))?;
            transform_ix += 1;
            needs_move = true;
            continue;
        }
        if tag == PathTag::STYLE {
            current_style = *encoding
                .styles
                .get(style_ix)
                .ok_or_else(|| RenderError::SceneBridge("style index out of range".into()))?;
            style_ix += 1;
            needs_move = true;
            continue;
        }
        if tag == PathTag::PATH {
            paths.push(BuiltPath {
                path: std::mem::take(&mut path),
                transform: current_transform,
                style: current_style,
            });
            path = BezPath::new();
            needs_move = true;
            continue;
        }
        if tag.is_path_segment() {
            append_segment(
                &mut path,
                tag,
                &encoding.path_data,
                &mut data_ix,
                &mut needs_move,
                tag.is_subpath_end(),
            );
        }
    }
    if !path.elements().is_empty() {
        paths.push(BuiltPath {
            path,
            transform: current_transform,
            style: current_style,
        });
    }
    Ok(paths)
}

fn append_segment(
    path: &mut BezPath,
    tag: PathTag,
    data: &[u32],
    data_ix: &mut usize,
    needs_move: &mut bool,
    subpath_end: bool,
) {
    let read_f32 = |data: &[u32], ix: &mut usize| -> Result<f64, RenderError> {
        let v = data
            .get(*ix)
            .ok_or_else(|| RenderError::SceneBridge("path data out of range".into()))?;
        *ix += 1;
        Ok(f32::from_bits(*v) as f64)
    };

    let read_i16_pair = |data: &[u32], ix: &mut usize| -> Result<(f64, f64), RenderError> {
        let word = data
            .get(*ix)
            .ok_or_else(|| RenderError::SceneBridge("path data out of range".into()))?;
        *ix += 1;
        let bytes = word.to_le_bytes();
        let x = i16::from_le_bytes([bytes[0], bytes[1]]) as f64;
        let y = i16::from_le_bytes([bytes[2], bytes[3]]) as f64;
        Ok((x, y))
    };

    let read_point = |data: &[u32], ix: &mut usize, f32: bool| -> Result<(f64, f64), RenderError> {
        if f32 {
            let x = read_f32(data, ix)?;
            let y = read_f32(data, ix)?;
            Ok((x, y))
        } else {
            read_i16_pair(data, ix)
        }
    };

    let ensure_move = |path: &mut BezPath,
                       data: &[u32],
                       data_ix: &mut usize,
                       needs_move: &mut bool,
                       f32: bool|
     -> Result<(), RenderError> {
        if *needs_move {
            let (x, y) = read_point(data, data_ix, f32)?;
            path.move_to(Point::new(x, y));
            *needs_move = false;
        }
        Ok(())
    };

    let seg = tag.path_segment_type();
    let f32 = tag.is_f32();
    let n_coords = match seg {
        PathSegmentType::LINE_TO => 2,
        PathSegmentType::QUAD_TO => 4,
        PathSegmentType::CUBIC_TO => 6,
        _ => 0,
    };

    if n_coords == 0 {
        return;
    }

    let result = (|| {
        ensure_move(path, data, data_ix, needs_move, f32)?;
        match seg {
            PathSegmentType::LINE_TO => {
                let (x, y) = read_point(data, data_ix, f32)?;
                path.line_to(Point::new(x, y));
            }
            PathSegmentType::QUAD_TO => {
                let (p1x, p1y) = read_point(data, data_ix, f32)?;
                let (x, y) = read_point(data, data_ix, f32)?;
                path.quad_to(Point::new(p1x, p1y), Point::new(x, y));
            }
            PathSegmentType::CUBIC_TO => {
                let (p1x, p1y) = read_point(data, data_ix, f32)?;
                let (p2x, p2y) = read_point(data, data_ix, f32)?;
                let (x, y) = read_point(data, data_ix, f32)?;
                path.curve_to(Point::new(p1x, p1y), Point::new(p2x, p2y), Point::new(x, y));
            }
            _ => {}
        }
        Ok::<(), RenderError>(())
    })();

    if result.is_err() {
        // Keep the path stream aligned even if a segment is malformed.
        if *needs_move {
            *data_ix += if f32 { 2 } else { 1 };
            *needs_move = false;
        }
        *data_ix += if f32 { n_coords } else { (n_coords + 1) / 2 };
    }

    if subpath_end {
        path.close_path();
        *needs_move = true;
    }
}

fn scan_draw_monoids(tags: &[DrawTag]) -> Vec<DrawMonoid> {
    let mut cur = DrawMonoid::default();
    let mut out = Vec::with_capacity(tags.len());
    for &tag in tags {
        out.push(cur);
        cur = DrawMonoid::combine(&cur, &DrawMonoid::new(tag));
    }
    out
}

fn read_draw_color(encoding: &Encoding, offset: usize) -> Result<Color, RenderError> {
    let words = encoding
        .draw_data
        .get(offset..offset + 1)
        .ok_or_else(|| RenderError::SceneBridge("draw color out of range".into()))?;
    let dc = bytemuck::cast_ref::<u32, DrawColor>(&words[0]);
    Ok(draw_color_to_peniko(*dc))
}

fn read_draw_begin_clip(encoding: &Encoding, offset: usize) -> Result<DrawBeginClip, RenderError> {
    let words = encoding
        .draw_data
        .get(offset..offset + 2)
        .ok_or_else(|| RenderError::SceneBridge("draw clip out of range".into()))?;
    Ok(*bytemuck::cast_ref::<[u32; 2], DrawBeginClip>(
        words
            .try_into()
            .map_err(|_| RenderError::SceneBridge("clip size".into()))?,
    ))
}

fn draw_color_to_peniko(dc: DrawColor) -> Color {
    // `DrawColor::rgba` is premultiplied RGBA8 (see `vello_encoding::DrawColor`).
    let premul: peniko::color::PremulColor<peniko::color::Srgb> =
        PremulRgba8::from_u32(dc.rgba).into();
    premul.un_premultiply()
}

fn style_is_fill(style: Style) -> bool {
    (style.flags_and_miter_limit & Style::FLAGS_STYLE_BIT) == 0
}

fn style_to_stroke(style: Style) -> Option<Stroke> {
    if style_is_fill(style) {
        return None;
    }
    let mut stroke = Stroke::new(style.line_width as f64);
    let join = style.flags_and_miter_limit & Style::FLAGS_JOIN_MASK;
    stroke.join = match join {
        Style::FLAGS_JOIN_BITS_BEVEL => peniko::kurbo::Join::Bevel,
        Style::FLAGS_JOIN_BITS_MITER => peniko::kurbo::Join::Miter,
        _ => peniko::kurbo::Join::Round,
    };
    Some(stroke)
}

fn apply_style(sink: &mut dyn SparseSceneSink, style: Style, transform: Transform) {
    if style_is_fill(style) {
        let fill = if (style.flags_and_miter_limit & Style::FLAGS_FILL_BIT) != 0 {
            Fill::EvenOdd
        } else {
            Fill::NonZero
        };
        sink.set_fill_rule(fill);
    }
    sink.set_transform(transform);
}

fn clip_to_blend(mode: u32) -> BlendMode {
    if mode == DrawBeginClip::CLIP_BLEND_MODE {
        return BlendMode::new(Mix::Normal, Compose::SrcOver);
    }
    // Porter-Duff modes are packed in the low 16 bits of `blend_mode`.
    let _ = mode;
    BlendMode::new(Mix::Normal, Compose::SrcOver)
}

fn to_common_color(color: Color) -> vello_common::peniko::Color {
    let c = color.to_rgba8();
    vello_common::peniko::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// CPU backend adapter for [`vello_cpu::RenderContext`].
pub struct CpuSceneSink<'a> {
    pub ctx: &'a mut vello_cpu::RenderContext,
    pub resources: &'a mut vello_cpu::Resources,
}

impl SparseSceneSink for CpuSceneSink<'_> {
    fn clear(&mut self, color: Color) {
        self.ctx.set_paint(to_common_color(color));
        let w = self.ctx.width() as f64;
        let h = self.ctx.height() as f64;
        self.ctx.fill_rect(&Rect::new(0.0, 0.0, w, h));
    }

    fn set_transform(&mut self, transform: Transform) {
        self.ctx.set_transform(transform.to_kurbo());
    }

    fn set_fill_rule(&mut self, fill: Fill) {
        self.ctx.set_fill_rule(fill);
    }

    fn set_stroke(&mut self, stroke: Stroke) {
        self.ctx.set_stroke(stroke);
    }

    fn set_paint_color(&mut self, color: Color) {
        self.ctx.set_paint(to_common_color(color));
    }

    fn fill_path(&mut self, path: &BezPath) {
        self.ctx.fill_path(path);
    }

    fn stroke_path(&mut self, path: &BezPath) {
        self.ctx.stroke_path(path);
    }

    fn push_clip_layer(&mut self, path: &BezPath) {
        self.ctx.push_clip_layer(path);
    }

    fn push_blend_layer(&mut self, blend: BlendMode, alpha: f32) {
        self.ctx
            .push_layer(None, Some(blend), Some(alpha.clamp(0.0, 1.0)), None, None);
    }

    fn pop_layer(&mut self) {
        self.ctx.pop_layer();
    }

    fn fill_glyph_run(
        &mut self,
        run: &EncodedGlyphRun,
        glyphs: &[vello_encoding::Glyph],
    ) -> Result<(), RenderError> {
        apply_peniko_style_cpu(self.ctx, &run.style);
        self.ctx.set_transform(run.transform.to_kurbo());
        let mut builder = self.ctx.glyph_run(self.resources, &run.font);
        builder = builder.font_size(run.font_size);
        if run.hint {
            builder = builder.hint(true);
        }
        let cpu_glyphs: Vec<vello_cpu::Glyph> = glyphs
            .iter()
            .map(|g| vello_cpu::Glyph {
                id: g.id,
                x: g.x,
                y: g.y,
            })
            .collect();
        builder.fill_glyphs(cpu_glyphs.into_iter());
        Ok(())
    }
}

fn apply_peniko_style_cpu(ctx: &mut vello_cpu::RenderContext, style: &PenikoStyle) {
    match style {
        PenikoStyle::Fill(fill) => ctx.set_fill_rule(*fill),
        PenikoStyle::Stroke(stroke) => ctx.set_stroke(stroke.clone()),
    }
}

/// Hybrid backend adapter for [`vello_hybrid::Scene`].
#[cfg(not(target_arch = "wasm32"))]
pub struct HybridSceneSink<'a> {
    pub scene: &'a mut vello_hybrid::Scene,
    pub resources: &'a mut vello_hybrid::Resources,
}

#[cfg(not(target_arch = "wasm32"))]
impl SparseSceneSink for HybridSceneSink<'_> {
    fn clear(&mut self, color: Color) {
        self.scene
            .set_paint(vello_common::paint::PaintType::Solid(to_common_color(
                color,
            )));
        let w = self.scene.width() as f64;
        let h = self.scene.height() as f64;
        self.scene.fill_rect(&Rect::new(0.0, 0.0, w, h));
    }

    fn set_transform(&mut self, transform: Transform) {
        self.scene.set_transform(transform.to_kurbo());
    }

    fn set_fill_rule(&mut self, fill: Fill) {
        self.scene.set_fill_rule(fill);
    }

    fn set_stroke(&mut self, stroke: Stroke) {
        self.scene.set_stroke(stroke);
    }

    fn set_paint_color(&mut self, color: Color) {
        self.scene
            .set_paint(vello_common::paint::PaintType::Solid(to_common_color(
                color,
            )));
    }

    fn fill_path(&mut self, path: &BezPath) {
        self.scene.fill_path(path);
    }

    fn stroke_path(&mut self, path: &BezPath) {
        self.scene.stroke_path(path);
    }

    fn push_clip_layer(&mut self, path: &BezPath) {
        self.scene.push_clip_layer(path);
    }

    fn push_blend_layer(&mut self, blend: BlendMode, alpha: f32) {
        self.scene
            .push_layer(None, Some(blend), Some(alpha.clamp(0.0, 1.0)), None, None);
    }

    fn pop_layer(&mut self) {
        self.scene.pop_layer();
    }

    fn fill_glyph_run(
        &mut self,
        run: &EncodedGlyphRun,
        glyphs: &[vello_encoding::Glyph],
    ) -> Result<(), RenderError> {
        apply_peniko_style_hybrid(self.scene, &run.style);
        self.scene.set_transform(run.transform.to_kurbo());
        let mut builder = self.scene.glyph_run(self.resources, &run.font);
        builder = builder.font_size(run.font_size);
        if run.hint {
            builder = builder.hint(true);
        }
        let hybrid_glyphs: Vec<glifo::Glyph> = glyphs
            .iter()
            .map(|g| glifo::Glyph {
                id: g.id,
                x: g.x,
                y: g.y,
            })
            .collect();
        builder.fill_glyphs(hybrid_glyphs.into_iter());
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_peniko_style_hybrid(scene: &mut vello_hybrid::Scene, style: &PenikoStyle) {
    match style {
        PenikoStyle::Fill(fill) => scene.set_fill_rule(*fill),
        PenikoStyle::Stroke(stroke) => scene.set_stroke(stroke.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::kurbo::{Affine, Rect};
    use vello::Scene;
    use vello_cpu::{Pixmap, RenderContext, Resources};

    #[test]
    fn replay_simple_fill_produces_visible_pixels() {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(103, 80, 164),
            None,
            &Rect::new(0.0, 0.0, 200.0, 56.0),
        );
        let encoding = scene.encoding();
        assert!(!encoding.is_empty(), "scene should not be empty");
        let paths = build_paths(encoding).expect("build paths");
        assert!(!paths.is_empty(), "expected at least one path, got 0");

        let mut ctx = RenderContext::new(200, 56);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::from_rgb8(255, 251, 254));
        replay_vello_scene(&scene, &mut sink).expect("replay");
        ctx.flush();
        let mut pm = Pixmap::new(200, 56);
        ctx.render_to_pixmap(&mut res, &mut pm);
        let data = pm.data_as_u8_slice();
        let mut non_zero = 0usize;
        for b in data {
            if *b != 0 {
                non_zero += 1;
            }
        }
        assert!(non_zero > 100, "expected painted pixels, got {non_zero}");
    }

    #[test]
    fn replay_clip_layer_does_not_error() {
        use peniko::kurbo::RoundedRect;
        let mut scene = Scene::new();
        let rr = RoundedRect::from_rect(Rect::new(10.0, 10.0, 100.0, 100.0), 8.0);
        scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &rr);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 0, 0),
            None,
            &Rect::new(0.0, 0.0, 200.0, 200.0),
        );
        scene.pop_layer();
        let mut ctx = RenderContext::new(200, 200);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::WHITE);
        replay_vello_scene(&scene, &mut sink).expect("replay with clip");
    }

    #[test]
    fn replay_wide_surface_produces_pixels() {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(103, 80, 164),
            None,
            &Rect::new(0.0, 0.0, 800.0, 56.0),
        );
        let mut ctx = RenderContext::new(800, 600);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::from_rgb8(255, 251, 254));
        replay_vello_scene(&scene, &mut sink).expect("replay");
        ctx.flush();
        let mut pm = Pixmap::new(800, 600);
        ctx.render_to_pixmap(&mut res, &mut pm);
        let data = pm.data_as_u8_slice();
        let row = &data[0..(800 * 4) as usize];
        assert!(row
            .iter()
            .any(|&b| b != 0 && b != 254 && b != 251 && b != 255));
    }

    #[test]
    fn replay_gallery_like_scene() {
        use peniko::kurbo::{RoundedRect, Stroke};
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(103, 80, 164),
            None,
            &Rect::new(0.0, 0.0, 800.0, 56.0),
        );
        let rr = RoundedRect::from_rect(Rect::new(16.0, 72.0, 200.0, 120.0), 20.0);
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            Color::from_rgb8(121, 116, 126),
            None,
            &rr,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 251, 254),
            None,
            &RoundedRect::from_rect(Rect::new(0.0, 56.0, 800.0, 600.0), 0.0),
        );
        let mut ctx = RenderContext::new(800, 600);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::from_rgb8(255, 251, 254));
        replay_vello_scene(&scene, &mut sink).expect("gallery-like replay");
    }

    #[test]
    fn replay_translucent_fill_over_opaque_stays_opaque() {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 251, 254),
            None,
            &Rect::new(0.0, 0.0, 100.0, 40.0),
        );
        let mut highlight = Color::from_rgb8(103, 80, 164);
        highlight.components[3] = 0.15;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            highlight,
            None,
            &Rect::new(0.0, 10.0, 100.0, 30.0),
        );
        let mut ctx = RenderContext::new(100, 40);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::BLACK);
        replay_vello_scene(&scene, &mut sink).expect("translucent replay");
        ctx.flush();
        let mut pm = Pixmap::new(100, 40);
        ctx.render_to_pixmap(&mut res, &mut pm);
        let data = pm.data_as_u8_slice();
        let px = 20 * 100 * 4 + 50 * 4;
        assert_eq!(
            data[px + 3],
            255,
            "src-over onto opaque surface should stay opaque, got alpha {}",
            data[px + 3]
        );
    }

    #[test]
    fn replay_menu_like_highlight_then_text_order() {
        use crate::text::TextRenderer;
        use synui_core::TextAlign;
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 251, 254),
            None,
            &Rect::new(0.0, 0.0, 160.0, 108.0),
        );
        let mut text = TextRenderer::new();
        text.paint(
            &mut scene,
            "Option A",
            14.0,
            Color::from_rgb8(28, 27, 31),
            Rect::new(0.0, 0.0, 160.0, 36.0),
            None,
            TextAlign::Start,
        );
        let mut highlight = Color::from_rgb8(103, 80, 164);
        highlight.components[3] = 0.15;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            highlight,
            None,
            &Rect::new(0.0, 36.0, 160.0, 72.0),
        );
        text.paint(
            &mut scene,
            "Option B",
            14.0,
            Color::from_rgb8(28, 27, 31),
            Rect::new(0.0, 36.0, 160.0, 72.0),
            None,
            TextAlign::Start,
        );
        text.paint(
            &mut scene,
            "Option C",
            14.0,
            Color::from_rgb8(28, 27, 31),
            Rect::new(0.0, 72.0, 160.0, 108.0),
            None,
            TextAlign::Start,
        );
        let mut ctx = RenderContext::new(160, 108);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::BLACK);
        replay_vello_scene(&scene, &mut sink).expect("menu-like replay");
        ctx.flush();
        let mut pm = Pixmap::new(160, 108);
        ctx.render_to_pixmap(&mut res, &mut pm);
        let data = pm.data_as_u8_slice();
        // Highlight band for option B (no glyph ink): should not reveal black backdrop.
        let px = 54 * 160 * 4 + 80 * 4;
        assert!(
            data[px + 3] == 255 && data[px] > 200,
            "highlight row leaked backdrop: rgba=({}, {}, {}, {})",
            data[px],
            data[px + 1],
            data[px + 2],
            data[px + 3]
        );
    }

    #[test]
    fn draw_color_premultiplied_decodes_correctly() {
        let opaque = DrawColor {
            rgba: Color::from_rgb8(103, 80, 164)
                .premultiply()
                .to_rgba8()
                .to_u32(),
        };
        let decoded = draw_color_to_peniko(opaque);
        let c = decoded.to_rgba8();
        assert_eq!(c.r, 103);
        assert_eq!(c.g, 80);
        assert_eq!(c.b, 164);
        assert_eq!(c.a, 255);

        let translucent = DrawColor {
            rgba: Color::from_rgba8(255, 0, 0, 128)
                .premultiply()
                .to_rgba8()
                .to_u32(),
        };
        let decoded = draw_color_to_peniko(translucent);
        let c = decoded.to_rgba8();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 128);
    }

    #[test]
    fn replay_shape_after_text_uses_correct_path_and_color() {
        use crate::text::TextRenderer;
        use synui_core::TextAlign;
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(103, 80, 164),
            None,
            &Rect::new(0.0, 0.0, 200.0, 40.0),
        );
        let mut text = TextRenderer::new();
        text.paint(
            &mut scene,
            "Title",
            16.0,
            Color::from_rgb8(28, 27, 31),
            Rect::new(8.0, 8.0, 192.0, 32.0),
            None,
            TextAlign::Start,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 251, 254),
            None,
            &Rect::new(0.0, 40.0, 200.0, 120.0),
        );
        let mut ctx = RenderContext::new(200, 120);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::BLACK);
        replay_vello_scene(&scene, &mut sink).expect("shape after text replay");
        ctx.flush();
        let mut pm = Pixmap::new(200, 120);
        ctx.render_to_pixmap(&mut res, &mut pm);
        let data = pm.data_as_u8_slice();
        // Body pixel at (100, 80) should be the light surface color, not text ink or header purple.
        let px = 80 * 200 * 4 + 100 * 4;
        let r = data[px];
        let g = data[px + 1];
        let b = data[px + 2];
        assert!(
            r > 240 && g > 245 && b > 250,
            "body pixel got ({r},{g},{b})"
        );
    }

    #[test]
    fn replay_text_glyphs_does_not_error() {
        use crate::text::TextRenderer;
        use synui_core::TextAlign;
        let mut scene = Scene::new();
        let mut text = TextRenderer::new();
        text.paint(
            &mut scene,
            "SynUI Widget Gallery",
            20.0,
            Color::from_rgb8(28, 27, 31),
            Rect::new(16.0, 16.0, 400.0, 48.0),
            None,
            TextAlign::Start,
        );
        let mut ctx = RenderContext::new(800, 600);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::from_rgb8(255, 251, 254));
        replay_vello_scene(&scene, &mut sink).expect("text glyph replay");
        ctx.flush();
        let mut pm = Pixmap::new(800, 600);
        ctx.render_to_pixmap(&mut res, &mut pm);
        let nz = pm.data_as_u8_slice().iter().filter(|&&b| b != 0).count();
        assert!(nz > 1000, "expected text pixels, got {nz}");
    }

    #[test]
    fn replay_appended_icon_scene() {
        use peniko::kurbo::Vec2;
        use vello_svg;
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 251, 254),
            None,
            &Rect::new(0.0, 0.0, 800.0, 600.0),
        );
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(crate::icon::ICON_ADD_SVG, &opt).expect("parse icon svg");
        let mut icon_scene = Scene::new();
        vello_svg::append_tree(&mut icon_scene, &tree);
        scene.append(&icon_scene, Some(Affine::translate(Vec2::new(16.0, 16.0))));
        let mut ctx = RenderContext::new(800, 600);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::from_rgb8(255, 251, 254));
        replay_vello_scene(&scene, &mut sink).expect("icon append replay");
    }

    fn count_path_markers(encoding: &Encoding) -> usize {
        encoding
            .path_tags
            .iter()
            .filter(|&&t| t == PathTag::PATH)
            .count()
    }

    fn count_path_draws(encoding: &Encoding) -> usize {
        encoding
            .draw_tags
            .iter()
            .filter(|&&t| {
                matches!(
                    t,
                    DrawTag::COLOR
                        | DrawTag::BEGIN_CLIP
                        | DrawTag::LINEAR_GRADIENT
                        | DrawTag::RADIAL_GRADIENT
                        | DrawTag::SWEEP_GRADIENT
                        | DrawTag::IMAGE
                        | DrawTag::BLUR_RECT
                )
            })
            .count()
    }

    #[test]
    fn path_count_matches_draws_for_clip_stack() {
        use peniko::kurbo::RoundedRect;
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 0, 0),
            None,
            &Rect::new(0., 0., 100., 100.),
        );
        let rr = RoundedRect::from_rect(Rect::new(10., 10., 90., 90.), 8.);
        scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &rr);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(0, 0, 255),
            None,
            &Rect::new(0., 0., 100., 100.),
        );
        scene.pop_layer();
        let enc = scene.encoding();
        let built = build_paths(enc).unwrap().len();
        let markers = count_path_markers(enc);
        let draws = count_path_draws(enc);
        eprintln!("built={built} markers={markers} draws={draws}");
        assert_eq!(
            built, markers,
            "every PATH marker should produce a built path"
        );
        assert!(built >= draws, "built={built} draws={draws}");
    }

    #[test]
    fn replay_text_with_clip_stack_flushes() {
        use crate::text::TextRenderer;
        use peniko::kurbo::RoundedRect;
        use synui_core::TextAlign;
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 251, 254),
            None,
            &Rect::new(0.0, 0.0, 200.0, 200.0),
        );
        let rr = RoundedRect::from_rect(Rect::new(20.0, 20.0, 180.0, 180.0), 8.0);
        scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &rr);
        let mut text = TextRenderer::new();
        text.paint(
            &mut scene,
            "Clipped",
            16.0,
            Color::from_rgb8(0, 0, 0),
            Rect::new(24.0, 24.0, 176.0, 56.0),
            None,
            TextAlign::Start,
        );
        scene.pop_layer();
        let mut ctx = RenderContext::new(200, 200);
        let mut res = Resources::new();
        let mut sink = CpuSceneSink {
            ctx: &mut ctx,
            resources: &mut res,
        };
        sink.clear(Color::WHITE);
        replay_vello_scene(&scene, &mut sink).expect("text+clip replay");
        ctx.flush();
    }
}
