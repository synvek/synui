//! Geometry types re-exported from Vello's Kurbo with SynUI conveniences.

pub use vello::kurbo::{Affine, Circle, Line, Point, Rect, RoundedRect, Size, Stroke, Vec2};

/// Layout constraints passed during measure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl Constraints {
    pub const UNBOUNDED: Self = Self {
        min_width: 0.0,
        max_width: f32::INFINITY,
        min_height: 0.0,
        max_height: f32::INFINITY,
    };

    pub fn tight(size: Size) -> Self {
        Self {
            min_width: size.width as f32,
            max_width: size.width as f32,
            min_height: size.height as f32,
            max_height: size.height as f32,
        }
    }

    pub fn deflate(&self, width: f32, height: f32) -> Self {
        Self {
            min_width: (self.min_width - width).max(0.0),
            max_width: (self.max_width - width).max(0.0),
            min_height: (self.min_height - height).max(0.0),
            max_height: (self.max_height - height).max(0.0),
        }
    }
}

impl From<Size> for Constraints {
    fn from(size: Size) -> Self {
        Self::tight(size)
    }
}
