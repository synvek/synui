//! Progress indicators.

use synui_core::alloc_widget_id;
use synui_core::{
    A11yProps, A11yRole, Circle, Dimension, LayoutStyle, PaintCtx, Rect, Stroke, Widget, WidgetId,
};
use synui_render::paint_simple_rect;
use vello::Scene;

pub struct LinearProgress {
    id: WidgetId,
    bounds: Rect,
    pub value: f32,
    pub indeterminate: bool,
    phase: f32,
}

impl LinearProgress {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            value: 0.0,
            indeterminate: false,
            phase: 0.0,
        }
    }

    pub fn advance_animation(&mut self, dt_ms: f32) -> bool {
        if !self.indeterminate {
            return false;
        }
        self.phase = (self.phase + dt_ms / 1200.0) % 1.0;
        true
    }
}

impl Widget for LinearProgress {
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
            align_items: synui_core::AlignItems::Center,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            width: Dimension::Fill,
            height: Dimension::Length(16.0),
            padding: [8.0, 16.0, 8.0, 16.0],
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

    fn measure(
        &mut self,
        _ctx: &mut synui_core::MeasureCtx,
        constraints: synui_core::Constraints,
    ) -> synui_core::Size {
        synui_core::Size::new(constraints.max_width.min(400.0) as f64, 16.0)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let cupertino = ctx.theme.cupertino;
        let track_h = if cupertino { 4.0 } else { 6.0 };
        let track_r = if cupertino { 2.0 } else { 3.0 };
        let track = Rect::new(
            self.bounds.x0 + 16.0,
            self.bounds.center().y - track_h / 2.0,
            self.bounds.x1 - 16.0,
            self.bounds.center().y + track_h / 2.0,
        );
        paint_simple_rect(scene, track, ctx.theme.progress_track(), track_r);
        let progress = if self.indeterminate {
            0.15 + self.phase * 0.5
        } else {
            self.value.clamp(0.0, 1.0)
        };
        let x0 = if self.indeterminate {
            track.x0 + track.width() * (self.phase as f64)
        } else {
            track.x0
        };
        let fill = Rect::new(
            x0,
            track.y0,
            (x0 + track.width() * progress as f64).min(track.x1),
            track.y1,
        );
        paint_simple_rect(scene, fill, ctx.theme.primary, track_r);
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::ProgressBar,
            value: Some(format!("{:.0}%", self.value * 100.0)),
            ..Default::default()
        }
    }
}

pub struct CircularProgress {
    id: WidgetId,
    bounds: Rect,
    pub value: f32,
    pub indeterminate: bool,
    phase: f32,
}

impl CircularProgress {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            value: 0.0,
            indeterminate: true,
            phase: 0.0,
        }
    }

    pub fn advance_animation(&mut self, dt_ms: f32) -> bool {
        if !self.indeterminate {
            return false;
        }
        self.phase = (self.phase + dt_ms / 1000.0) % 1.0;
        true
    }
}

impl Widget for CircularProgress {
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
        let center = self.bounds.center();
        let r = self.bounds.width().min(self.bounds.height()) / 2.0 - 2.0;
        if r <= 1.0 {
            return;
        }
        let stroke = Stroke::new(3.0);
        let identity = vello::kurbo::Affine::IDENTITY;
        let track = Circle::new(center, r);
        scene.stroke(&stroke, identity, ctx.theme.progress_track(), None, &track);

        let (start_angle, sweep_angle) = if self.indeterminate {
            (
                self.phase as f64 * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2,
                0.25 * std::f64::consts::TAU,
            )
        } else {
            let value = self.value.clamp(0.0, 1.0) as f64;
            (-std::f64::consts::FRAC_PI_2, value * std::f64::consts::TAU)
        };
        if sweep_angle.abs() < 1e-4 {
            return;
        }
        let arc = vello::kurbo::Arc::new(center, (r, r), start_angle, sweep_angle, 0.0);
        scene.stroke(&stroke, identity, ctx.theme.primary, None, &arc);
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::ProgressBar,
            value: Some(format!("{:.0}%", self.value.clamp(0.0, 1.0) * 100.0)),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_progress_advance_only_when_indeterminate() {
        let mut progress = CircularProgress::new();
        progress.value = 0.45;
        progress.indeterminate = false;
        assert!(!progress.advance_animation(16.0));
        progress.indeterminate = true;
        assert!(progress.advance_animation(16.0));
        assert!(progress.phase > 0.0);
    }
}
