//! Material ripple effect state machine (300–600ms).

use peniko::Color;
use synui_core::{Point, Rect};

/// Corner-distance max radius so ripples stay within the widget silhouette.
pub fn max_ripple_radius(local_origin: Point, bounds: Rect) -> f64 {
    let w = bounds.width();
    let h = bounds.height();
    [
        Point::new(0.0, 0.0),
        Point::new(w, 0.0),
        Point::new(0.0, h),
        Point::new(w, h),
    ]
    .iter()
    .map(|corner| {
        let dx = corner.x - local_origin.x;
        let dy = corner.y - local_origin.y;
        (dx * dx + dy * dy).sqrt()
    })
    .fold(0.0_f64, f64::max)
}

#[derive(Debug, Clone)]
pub struct RippleState {
    pub origin: Point,
    pub color: Color,
    pub progress: f32,
    pub duration_ms: f32,
    pub active: bool,
}

impl RippleState {
    pub fn new(origin: Point, color: Color) -> Self {
        Self {
            origin,
            color,
            progress: 0.0,
            duration_ms: 400.0,
            active: true,
        }
    }

    pub fn advance(&mut self, dt_ms: f32) -> bool {
        if !self.active {
            return false;
        }
        self.progress += dt_ms / self.duration_ms;
        if self.progress >= 1.0 {
            self.active = false;
            false
        } else {
            true
        }
    }

    pub fn current_radius(&self, max_radius: f64) -> f64 {
        let t = ease_out(self.progress);
        max_radius * t as f64
    }

    pub fn current_opacity(&self) -> f32 {
        (1.0 - self.progress).max(0.0) * 0.3
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(2)
}
