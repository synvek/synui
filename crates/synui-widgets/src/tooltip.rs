//! Hover tip bubble (non-interactive overlay).

use synui_core::alloc_widget_id;
use synui_core::{
    Constraints, MeasureCtx, PaintCtx, Point, Rect, Size, Stroke, TextAlign, Widget, WidgetId,
    WidgetStore,
};
use synui_render::paint_simple_rect;
use vello::kurbo::{Affine, RoundedRect};
use vello::Scene;

const TIP_FONT: f32 = 12.0;
const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 10.0;
const MAX_TEXT_W: f64 = 240.0;
/// Approximate Latin advance at [`TIP_FONT`].
const CHAR_W: f64 = 7.0;
/// CJK / fullwidth glyphs are ~1em wide — ASCII `CHAR_W` underestimates them
/// and shrinks the bubble until paint wraps (e.g. 「通讯录」→ last char alone).
const WIDE_CHAR_W: f64 = TIP_FONT as f64;
const LINE_H: f64 = 16.0;

fn char_advance(c: char) -> f64 {
    if c.is_ascii() {
        CHAR_W
    } else {
        WIDE_CHAR_W
    }
}

fn line_width(s: &str) -> f64 {
    s.chars().map(char_advance).sum()
}

/// Estimate tip size without a TextPainter (sync path).
pub fn estimate_tooltip_size(text: &str) -> Size {
    let lines = wrap_tooltip_lines(text, MAX_TEXT_W);
    let text_w = lines.iter().map(|l| line_width(l)).fold(0.0_f64, f64::max);
    Size::new(
        (text_w + PAD_X).clamp(48.0, MAX_TEXT_W + PAD_X),
        lines.len().max(1) as f64 * LINE_H + PAD_Y,
    )
}

fn wrap_tooltip_lines(text: &str, max_w: f64) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                push_fitting(&mut lines, &mut current, word, max_w);
            } else if line_width(&current) + CHAR_W + line_width(word) <= max_w {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                push_fitting(&mut lines, &mut current, word, max_w);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Append `chunk` to `current`, breaking on glyph boundaries when wider than `max_w`.
fn push_fitting(lines: &mut Vec<String>, current: &mut String, chunk: &str, max_w: f64) {
    for c in chunk.chars() {
        let next_w = line_width(current) + char_advance(c);
        if !current.is_empty() && next_w > max_w {
            lines.push(std::mem::take(current));
        }
        current.push(c);
    }
}

/// Small floating tip painted as an overlay root (hit-test transparent).
pub struct TooltipBubble {
    id: WidgetId,
    bounds: Rect,
    pub text: String,
}

impl TooltipBubble {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            text: text.into(),
        }
    }
}

impl Widget for TooltipBubble {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, _c: Constraints) -> Size {
        estimate_tooltip_size(&self.text)
    }

    fn hit_test(&self, _point: Point) -> bool {
        // Tips must not steal pointer hits from the widget underneath.
        false
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let r = 6.0_f64;
        paint_simple_rect(scene, self.bounds, ctx.theme.surface_container, r);
        let outline = RoundedRect::from_rect(self.bounds, r);
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            ctx.theme.field_border(),
            None,
            &outline,
        );
        if let Some(text) = ctx.text.as_deref_mut() {
            // Horizontal inset only: full bubble height so paint_text optically
            // centers the line (tight padded height previously forced top-align).
            let pad = Rect::new(
                self.bounds.x0 + PAD_X * 0.5,
                self.bounds.y0,
                self.bounds.x1 - PAD_X * 0.5,
                self.bounds.y1,
            );
            text.paint_text(
                scene,
                &self.text,
                TIP_FONT,
                ctx.theme.on_surface,
                pad,
                Some(pad.width().max(1.0) as f32),
                TextAlign::Start,
            );
        }
    }
}

/// Notify the tip session that the hovered widget changed (or cleared).
pub fn tooltip_on_hover(store: &mut WidgetStore, hit: Option<WidgetId>) {
    let resolved = hit.and_then(|id| store.resolve_tooltip(id).map(|(anchor, _)| anchor));
    let session = store.tooltip_session_mut();
    if session.candidate != resolved {
        session.candidate = resolved;
        session.dwell_ms = 0.0;
    }
}

/// Advance tip dwell time. Returns true when a show/hide transition may be needed.
pub fn tooltip_tick(store: &mut WidgetStore, dt_ms: f32) -> bool {
    if store.overlay_stack().iter().any(|e| e.config.modal) {
        let session = store.tooltip_session_mut();
        let changed = session.shown_anchor.is_some() || session.candidate.is_some();
        session.candidate = None;
        session.dwell_ms = 0.0;
        return changed;
    }
    let session = store.tooltip_session_mut();
    if session.candidate.is_none() {
        return session.shown_anchor.is_some();
    }
    if session.shown_anchor == session.candidate {
        return false;
    }
    let before = session.dwell_ms;
    session.dwell_ms = (session.dwell_ms + dt_ms).min(session.show_delay_ms + 1.0);
    before < session.show_delay_ms && session.dwell_ms >= session.show_delay_ms
}

/// Hide any visible tip immediately (pointer down / leave window).
pub fn tooltip_dismiss(store: &mut WidgetStore) {
    let session = store.tooltip_session_mut();
    session.candidate = None;
    session.dwell_ms = 0.0;
    session.shown_anchor = None;
}

/// Ms until tip should appear, if hovering a candidate that is not yet shown.
pub fn next_tooltip_wake_ms(store: &WidgetStore) -> Option<f32> {
    let session = store.tooltip_session();
    if store.overlay_stack().iter().any(|e| e.config.modal) {
        return None;
    }
    let candidate = session.candidate?;
    if session.shown_anchor == Some(candidate) {
        return None;
    }
    let remaining = (session.show_delay_ms - session.dwell_ms).max(0.0);
    Some(remaining.max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_size_grows_with_text() {
        let small = estimate_tooltip_size("Hi");
        let large = estimate_tooltip_size("Save the current document to disk");
        assert!(large.width >= small.width);
        assert!(large.height >= small.height);
    }

    #[test]
    fn cjk_short_tip_stays_single_line() {
        // 「通讯录」must not shrink below glyph advance or paint wraps the last char.
        let size = estimate_tooltip_size("通讯录");
        let expected_w = 3.0 * WIDE_CHAR_W + PAD_X;
        assert!((size.width - expected_w).abs() < 0.5, "width={}", size.width);
        assert_eq!(size.height, LINE_H + PAD_Y);
        assert_eq!(wrap_tooltip_lines("通讯录", MAX_TEXT_W), vec!["通讯录".to_string()]);
    }
}
