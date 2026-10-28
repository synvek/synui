//! Shape morphing via polymorpher (MD3).

use kurbo011::BezPath as MorphBezPath;
use polymorpher::geometry::Point;
use polymorpher::{CornerRounding, Morph, RoundedPoint, RoundedPolygon};
use vello::kurbo::BezPath;

/// Interpolate between two rounded rectangles for MD3 shape morph.
pub fn morph_rounded_rect(progress: f32, width: f64, height: f64) -> BezPath {
    let start = RoundedPolygon::from_points(
        &[
            RoundedPoint::new(Point::new(0.0, 0.0), CornerRounding::new(0.1)),
            RoundedPoint::new(Point::new(1.0, 0.0), CornerRounding::new(0.1)),
            RoundedPoint::new(Point::new(1.0, 1.0), CornerRounding::new(0.1)),
            RoundedPoint::new(Point::new(0.0, 1.0), CornerRounding::new(0.1)),
        ],
        1,
        true,
    )
    .normalized()
    .transformed(|p: Point| Point::new(p.x * width as f32, p.y * height as f32));

    let end = RoundedPolygon::from_points(
        &[
            RoundedPoint::new(Point::new(0.0, 0.0), CornerRounding::new(0.5)),
            RoundedPoint::new(Point::new(1.0, 0.0), CornerRounding::new(0.5)),
            RoundedPoint::new(Point::new(1.0, 1.0), CornerRounding::new(0.5)),
            RoundedPoint::new(Point::new(0.0, 1.0), CornerRounding::new(0.5)),
        ],
        1,
        true,
    )
    .normalized()
    .transformed(|p: Point| Point::new(p.x * width as f32, p.y * height as f32));

    let morph = Morph::new(start, end);
    let morph_path: MorphBezPath = morph.as_path::<MorphBezPath>(progress, false, true);
    morph_path_to_vello(&morph_path)
}

/// Interruptible morph progress (Req 9 subset — retarget without restarting from 0).
#[derive(Debug, Clone)]
pub struct MorphController {
    pub progress: f32,
    pub target: f32,
    pub speed: f32,
}

impl Default for MorphController {
    fn default() -> Self {
        Self {
            progress: 0.0,
            target: 1.0,
            speed: 2.0,
        }
    }
}

impl MorphController {
    pub fn new(target: f32) -> Self {
        Self {
            progress: 0.0,
            target,
            speed: 2.0,
        }
    }

    /// Retarget mid-flight (interrupt / redirect).
    pub fn retarget(&mut self, new_target: f32) {
        self.target = new_target.clamp(0.0, 1.0);
    }

    /// Advance toward `target`; returns current progress.
    pub fn tick(&mut self, dt_secs: f32) -> f32 {
        let dir = (self.target - self.progress).signum();
        if (self.target - self.progress).abs() < 0.001 {
            self.progress = self.target;
            return self.progress;
        }
        self.progress = (self.progress + dir * self.speed * dt_secs).clamp(0.0, 1.0);
        if (self.target - self.progress).abs() < 0.01 {
            self.progress = self.target;
        }
        self.progress
    }
}

fn morph_path_to_vello(path: &MorphBezPath) -> BezPath {
    let mut out = BezPath::new();
    for el in path.elements() {
        use kurbo011::PathEl;
        match el {
            PathEl::MoveTo(p) => out.move_to((p.x, p.y)),
            PathEl::LineTo(p) => out.line_to((p.x, p.y)),
            PathEl::QuadTo(p1, p2) => out.quad_to((p1.x, p1.y), (p2.x, p2.y)),
            PathEl::CurveTo(p1, p2, p3) => {
                out.curve_to((p1.x, p1.y), (p2.x, p2.y), (p3.x, p3.y));
            }
            PathEl::ClosePath => out.close_path(),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retarget_midway() {
        let mut c = MorphController::new(1.0);
        c.progress = 0.4;
        c.retarget(0.0);
        assert_eq!(c.target, 0.0);
        let p = c.tick(0.05);
        assert!(p < 0.4);
    }
}
