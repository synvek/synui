//! Bidi helpers wrapping Parley layout direction.

use synui_core::LayoutDirection;

/// Resolve paragraph base direction from locale/layout direction.
pub fn paragraph_direction(layout: LayoutDirection) -> LayoutDirection {
    layout
}

/// Map a visual x offset into a logical byte index.
///
/// For LTR, `measure_prefix` returns the advance of `text[..i]`.
/// For RTL, pass the same measure but `visual_x` is interpreted from the
/// right edge of the laid-out line (`total_width - visual_x` when
/// `direction == Rtl`).
pub fn logical_index_for_visual_x(
    text: &str,
    visual_x: f64,
    direction: LayoutDirection,
    measure_prefix: &mut dyn FnMut(&str) -> f64,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    let total = measure_prefix(text);
    let target = match direction {
        LayoutDirection::Ltr => visual_x,
        LayoutDirection::Rtl => (total - visual_x).max(0.0),
    };
    let mut best = 0;
    let mut best_dist = f64::MAX;
    for (i, _) in text
        .char_indices()
        .chain(std::iter::once((text.len(), '\0')))
    {
        let w = measure_prefix(&text[..i]);
        let dist = (w - target).abs();
        if dist < best_dist {
            best_dist = dist;
            best = i;
        }
    }
    best
}

/// Visual caret x (from the left of the layout box) for a logical byte index.
pub fn visual_caret_x(
    text: &str,
    index: usize,
    direction: LayoutDirection,
    measure_prefix: &mut dyn FnMut(&str) -> f64,
) -> f64 {
    let index = index.min(text.len());
    let prefix = measure_prefix(&text[..index]);
    match direction {
        LayoutDirection::Ltr => prefix,
        LayoutDirection::Rtl => {
            let total = measure_prefix(text);
            (total - prefix).max(0.0)
        }
    }
}

/// Whether ArrowLeft should move toward the start of the logical string.
pub fn arrow_left_moves_to_start(direction: LayoutDirection) -> bool {
    matches!(direction, LayoutDirection::Ltr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_empty() {
        let mut m = |_s: &str| 0.0;
        assert_eq!(
            logical_index_for_visual_x("", 10.0, LayoutDirection::Ltr, &mut m),
            0
        );
    }

    #[test]
    fn hit_mid_ltr() {
        let mut m = |s: &str| s.len() as f64 * 10.0;
        assert_eq!(
            logical_index_for_visual_x("abcd", 26.0, LayoutDirection::Ltr, &mut m),
            3
        );
    }

    #[test]
    fn rtl_caret_from_right() {
        let mut m = |s: &str| s.len() as f64 * 10.0;
        // total=40; visual x=10 from left → 30 from right → index 3
        assert_eq!(
            logical_index_for_visual_x("abcd", 10.0, LayoutDirection::Rtl, &mut m),
            3
        );
        assert!((visual_caret_x("abcd", 3, LayoutDirection::Rtl, &mut m) - 10.0).abs() < 1e-6);
    }
}
