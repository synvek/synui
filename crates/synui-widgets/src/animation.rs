//! Lottie and shape-morph animation widgets.

use std::cell::RefCell;
use synui_core::alloc_widget_id;
use synui_core::{
    Constraints, Dimension, LayoutStyle, MeasureCtx, PaintCtx, Rect, Size, Widget, WidgetId,
};
use synui_render::{LottieCache, LottiePlayer};
use synui_theme::morph_rounded_rect;
use vello::kurbo::Affine;
use vello::peniko::Fill;
use vello::Scene;

/// Lottie animation widget.
pub struct Lottie {
    id: WidgetId,
    bounds: Rect,
    /// When set, the player is created lazily from `PaintCtx.lotties` on first paint.
    asset_id: Option<String>,
    player: RefCell<Option<LottiePlayer>>,
    /// Cleared until the first paint attempts lazy asset resolve (avoids a stalled
    /// animation loop, and stops spinning after a cache miss).
    paint_attempted: std::cell::Cell<bool>,
    playing: bool,
    width: f64,
    height: f64,
}

impl Lottie {
    pub fn from_json(json: &str) -> Result<Self, velato::Error> {
        let mut player = LottiePlayer::from_json(json)?;
        player.set_playing(true);
        Ok(Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            asset_id: None,
            player: RefCell::new(Some(player)),
            paint_attempted: std::cell::Cell::new(true),
            playing: true,
            width: 160.0,
            height: 160.0,
        })
    }

    /// Creates a host that resolves JSON from a preloaded `LottieCache` id.
    ///
    /// Missing cache entries paint nothing (no panic). Callers should preload via
    /// `Window::with_lottie`.
    pub fn from_asset(id: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            asset_id: Some(id.into()),
            player: RefCell::new(None),
            paint_attempted: std::cell::Cell::new(false),
            playing: true,
            width: 160.0,
            height: 160.0,
        }
    }

    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if let Some(player) = self.player.borrow_mut().as_mut() {
            player.set_playing(playing);
        }
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn width(&self) -> f64 {
        self.width
    }

    pub fn height(&self) -> f64 {
        self.height
    }

    pub fn set_size(&mut self, width: f64, height: f64) {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
    }

    pub fn advance(&mut self, dt_secs: f32) -> bool {
        if !self.playing {
            return false;
        }
        if let Some(player) = self.player.borrow_mut().as_mut() {
            player.advance(dt_secs as f64);
            true
        } else {
            // Asset players are created lazily on paint. Keep requesting frames
            // until the first paint attempts resolve (then stop on cache miss).
            self.asset_id.is_some() && !self.paint_attempted.get()
        }
    }

    fn ensure_player(&self, ctx: &PaintCtx<'_>) {
        if self.player.borrow().is_some() {
            return;
        }
        let Some(asset_id) = self.asset_id.as_deref() else {
            return;
        };
        let Some(source) = ctx.lotties else {
            return;
        };
        // Prefer pre-parsed Composition from host LottieCache (Phase 58).
        if let Some(cache) = source.as_any().downcast_ref::<LottieCache>() {
            if let Some(composition) = cache.composition_str(asset_id) {
                let mut player = LottiePlayer::from_composition(composition.clone());
                player.set_playing(self.playing);
                *self.player.borrow_mut() = Some(player);
                return;
            }
        }
        // Fallback: parse JSON text (still works for non-LottieCache sources).
        let Some(json) = source.lottie_json(asset_id) else {
            return;
        };
        if let Ok(mut player) = LottiePlayer::from_json(json) {
            player.set_playing(self.playing);
            *self.player.borrow_mut() = Some(player);
        }
    }
}

impl Widget for Lottie {
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
            padding: [8.0, 8.0, 8.0, 8.0],
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

    fn measure(&mut self, _ctx: &mut MeasureCtx, _c: Constraints) -> Size {
        Size::new(self.width, self.height)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        self.ensure_player(ctx);
        self.paint_attempted.set(true);
        let mut player = self.player.borrow_mut();
        let Some(player) = player.as_mut() else {
            return;
        };
        let scale_x = self.bounds.width() / 100.0;
        let scale_y = self.bounds.height() / 100.0;
        let transform = Affine::translate((self.bounds.x0, self.bounds.y0))
            * Affine::scale_non_uniform(scale_x.max(0.01), scale_y.max(0.01));
        player.render_to_scene(scene, transform, 1.0);
    }
}

/// Morphing MD3 shape widget.
pub struct MorphingShape {
    id: WidgetId,
    bounds: Rect,
    progress: f32,
    speed: f32,
    playing: bool,
    size: f64,
}

impl MorphingShape {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            progress: 0.0,
            speed: 0.45,
            playing: true,
            size: 120.0,
        }
    }

    pub fn with_size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.max(0.0);
    }

    pub fn set_size(&mut self, size: f64) {
        self.size = size.max(1.0);
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn size(&self) -> f64 {
        self.size
    }

    pub fn advance(&mut self, dt_ms: f32) -> bool {
        if !self.playing {
            return false;
        }
        self.progress += (dt_ms / 1000.0) * self.speed;
        if self.progress > 1.0 {
            self.progress = 0.0;
        }
        true
    }
}

impl Widget for MorphingShape {
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
            padding: [8.0, 8.0, 8.0, 8.0],
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

    fn measure(&mut self, _ctx: &mut MeasureCtx, _c: Constraints) -> Size {
        Size::new(self.size, self.size)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let w = self.bounds.width().max(1.0);
        let h = self.bounds.height().max(1.0);
        let path = morph_rounded_rect(self.progress, w, h);
        let transform = Affine::translate((self.bounds.x0, self.bounds.y0));
        scene.fill(Fill::NonZero, transform, ctx.theme.primary, None, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_lottie_advance_requests_frame_until_paint() {
        let mut lottie = Lottie::from_asset("pulse").with_size(96.0, 96.0);
        assert!(
            lottie.advance(0.016),
            "pending paint should keep the anim loop alive"
        );
        lottie.paint_attempted.set(true);
        assert!(
            !lottie.advance(0.016),
            "cache miss after paint attempt should stop spinning"
        );
    }

    #[test]
    fn morph_advance_while_playing() {
        let mut morph = MorphingShape::new().with_size(96.0);
        assert!(morph.advance(16.0));
        morph.set_playing(false);
        assert!(!morph.advance(16.0));
    }
}
