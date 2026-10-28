//! Popup / overlay positioning relative to anchor bounds.

use synui_core::{LayoutDirection, OverlayPlacement, Point, Rect, Size};

/// Computes popup position clamped to the viewport.
pub struct PopupAnchor;

impl PopupAnchor {
    /// Resolve placement using the shared [`OverlayPlacement`] enum (Phase 13).
    pub fn position(
        anchor: Rect,
        popup_size: Size,
        placement: OverlayPlacement,
        viewport: Size,
        direction: LayoutDirection,
    ) -> Point {
        let pw = popup_size.width;
        let ph = popup_size.height;
        let vw = viewport.width;
        let vh = viewport.height;
        let rtl = direction == LayoutDirection::Rtl;

        let (mut x, mut y) = match placement {
            OverlayPlacement::Below => (anchor.x0, anchor.y1),
            OverlayPlacement::Above => (anchor.x0, anchor.y0 - ph),
            OverlayPlacement::Start if rtl => (anchor.x1 - pw, anchor.y0),
            OverlayPlacement::Start => (anchor.x0, anchor.y0),
            OverlayPlacement::End if rtl => (anchor.x0, anchor.y0),
            OverlayPlacement::End => (anchor.x1 - pw, anchor.y0),
            OverlayPlacement::Center => ((vw - pw).max(0.0) * 0.5, (vh - ph).max(0.0) * 0.5),
            OverlayPlacement::BottomSheet => (0.0, (vh - ph).max(0.0)),
        };

        if x + pw > vw {
            x = (vw - pw).max(0.0);
        }
        if x < 0.0 {
            x = 0.0;
        }
        if y + ph > vh {
            y = (vh - ph).max(0.0);
        }
        if y < 0.0 {
            y = 0.0;
        }

        Point::new(x, y)
    }

    /// Bounds for a bottom sheet covering the lower `fraction` of the viewport.
    pub fn bottom_sheet_bounds(viewport: Size, fraction: f64) -> Rect {
        let h = (viewport.height * fraction).max(1.0);
        let origin = Self::position(
            Rect::ZERO,
            Size::new(viewport.width, h),
            OverlayPlacement::BottomSheet,
            viewport,
            LayoutDirection::Ltr,
        );
        Rect::new(origin.x, origin.y, origin.x + viewport.width, origin.y + h)
    }

    /// Centered dialog-like bounds (fraction of viewport, clamped).
    pub fn center_bounds(viewport: Size, width_frac: f64, height_frac: f64) -> Rect {
        let w = (viewport.width * width_frac).clamp(1.0, viewport.width);
        let h = (viewport.height * height_frac).clamp(1.0, viewport.height);
        let origin = Self::position(
            Rect::ZERO,
            Size::new(w, h),
            OverlayPlacement::Center,
            viewport,
            LayoutDirection::Ltr,
        );
        Rect::new(origin.x, origin.y, origin.x + w, origin.y + h)
    }
}

/// Legacy alias kept for docs; prefer [`OverlayPlacement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupPlacement {
    #[default]
    Below,
    Above,
    Start,
    End,
}

impl From<PopupPlacement> for OverlayPlacement {
    fn from(p: PopupPlacement) -> Self {
        match p {
            PopupPlacement::Below => OverlayPlacement::Below,
            PopupPlacement::Above => OverlayPlacement::Above,
            PopupPlacement::Start => OverlayPlacement::Start,
            PopupPlacement::End => OverlayPlacement::End,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_anchor() {
        let anchor = Rect::new(10.0, 20.0, 110.0, 50.0);
        let pos = PopupAnchor::position(
            anchor,
            Size::new(80.0, 100.0),
            OverlayPlacement::Below,
            Size::new(400.0, 300.0),
            LayoutDirection::Ltr,
        );
        assert!((pos.x - 10.0).abs() < 0.01);
        assert!((pos.y - 50.0).abs() < 0.01);
    }

    #[test]
    fn clamps_to_viewport() {
        let anchor = Rect::new(350.0, 250.0, 390.0, 280.0);
        let pos = PopupAnchor::position(
            anchor,
            Size::new(100.0, 80.0),
            OverlayPlacement::Below,
            Size::new(400.0, 300.0),
            LayoutDirection::Ltr,
        );
        assert!(pos.x + 100.0 <= 400.0);
        assert!(pos.y + 80.0 <= 300.0);
    }

    #[test]
    fn bottom_sheet_uses_overlay_placement() {
        let b = PopupAnchor::bottom_sheet_bounds(Size::new(400.0, 300.0), 0.4);
        assert!((b.height() - 120.0).abs() < 0.01);
        assert!((b.y0 - 180.0).abs() < 0.01);
    }
}
