//! Text input widgets with IME composition support.
//!
//! Selection indices are UTF-8 byte offsets into `text` (grapheme-safe moves via char boundaries).
//! Editing logic lives in [`crate::edit_core::EditCore`].

use std::cell::Cell;
use std::ops::{Deref, DerefMut};

use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Circle, Constraints, Dimension, Event, EventCtx, EventResult,
    Key, LayoutStyle, Line, MeasureCtx, NamedKey, PaintCtx, Point, PointerButton, Rect,
    RoundedRect, Size, Stroke, TextAlign, Widget, WidgetId, WidgetStore,
};
use synui_render::paint_simple_rect;
use vello::peniko::Color;
use vello::Scene;

use crate::edit_core::{handle_edit_shortcut, EditCore, EditHistory, floor_char_boundary};
use crate::scroll::{paint_v_scrollbar, VScrollbarState};

const TEXT_FIELD_FONT: f32 = 16.0;
const TEXT_AREA_FONT: f32 = 14.0;
/// Password mask glyph. ASCII `*` sits near the top of the em-box, so selection
/// highlights look top-heavy; U+2022 sits nearer the visual middle of the line.
const PASSWORD_MASK: char = '•';
const PASSWORD_MASK_UTF8_LEN: usize = PASSWORD_MASK.len_utf8();

fn password_mask(text: &str) -> String {
    PASSWORD_MASK.to_string().repeat(text.chars().count())
}

fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Map a UTF-8 byte range in the real secret to a byte range in the mask string.
fn password_mask_byte_range(text: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let len = text.len();
    let start = floor_char_boundary(text, range.start.min(range.end).min(len));
    let end = ceil_char_boundary(text, range.start.max(range.end).min(len));
    let start_chars = text[..start].chars().count();
    let end_chars = text[..end].chars().count();
    (start_chars * PASSWORD_MASK_UTF8_LEN)..(end_chars * PASSWORD_MASK_UTF8_LEN)
}

fn password_mask_byte_to_text_byte(text: &str, mask_byte: usize) -> usize {
    let char_idx = mask_byte / PASSWORD_MASK_UTF8_LEN;
    text.char_indices()
        .nth(char_idx)
        .map(|(offset, _)| offset)
        .unwrap_or(text.len())
}

/// Single-line text input.
pub struct TextField {
    id: WidgetId,
    bounds: Rect,
    pub edit: EditCore,
    pub placeholder: String,
    pub password: bool,
    /// Phase 41: real disabled state for `:disabled` cascade + input gating.
    pub disabled: bool,
    history: EditHistory<String>,
    focused: bool,
    cursor_visible: bool,
    cursor_blink_ms: f32,
    dragging_select: bool,
}

impl Deref for TextField {
    type Target = EditCore;
    fn deref(&self) -> &EditCore {
        &self.edit
    }
}

impl DerefMut for TextField {
    fn deref_mut(&mut self) -> &mut EditCore {
        &mut self.edit
    }
}

impl TextField {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            edit: EditCore::new(),
            placeholder: placeholder.into(),
            password: false,
            disabled: false,
            history: EditHistory::new(64),
            focused: false,
            cursor_visible: true,
            cursor_blink_ms: 0.0,
            dragging_select: false,
        }
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    pub fn multiline() -> TextArea {
        TextArea::new()
    }

    fn content_bounds(&self) -> Rect {
        const LINE_HEIGHT: f64 = TEXT_FIELD_FONT as f64 * synui_render::LINE_HEIGHT_EM as f64;
        let pad_x = 16.0;
        let line_h = LINE_HEIGHT.min(self.bounds.height().max(LINE_HEIGHT));
        // Typographic line boxes leave unused descent below ink; nudge the strip down
        // so glyphs sit optically centered in the field (matches tip / button paint).
        let optical_nudge = TEXT_FIELD_FONT as f64 * 0.08;
        let y = self.bounds.y0 + (self.bounds.height() - line_h) / 2.0 + optical_nudge;
        Rect::new(
            self.bounds.x0 + pad_x,
            y,
            self.bounds.x1 - pad_x,
            y + line_h,
        )
    }

    fn text_bounds_for(&self, layout_height: f64) -> Rect {
        let content = self.content_bounds();
        let h = layout_height.min(content.height()).max(1.0);
        let y0 = content.y0 + (content.height() - h) / 2.0;
        Rect::new(content.x0, y0, content.x1, y0 + h)
    }

    /// Logical rectangle for the platform IME candidate window (at caret).
    pub fn ime_cursor_area(&self) -> Rect {
        let content = self.content_bounds();
        let display = if self.password {
            password_mask(&self.text)
        } else {
            self.edit.display_with_ime()
        };
        let caret_idx = if self.password {
            password_mask_byte_range(&self.text, self.selection.end..self.selection.end).start
        } else if !self.ime_composing.is_empty() {
            self.selection.start.min(self.text.len()) + self.ime_composing.len()
        } else {
            self.selection.end.min(display.len())
        };
        let caret = synui_render::with_text_renderer(|renderer| {
            renderer.caret_bounds(&display, TEXT_FIELD_FONT, None, caret_idx)
        });
        let text_bounds = self.text_bounds_for(caret.height().max(content.height()));
        Rect::new(
            text_bounds.x0 + caret.x0,
            text_bounds.y0 + caret.y0,
            text_bounds.x0 + caret.x0 + 2.0,
            text_bounds.y0 + caret.y1,
        )
    }

    fn display_text(&self) -> (String, bool) {
        if self.password {
            return (password_mask(&self.text), false);
        }
        if !self.ime_composing.is_empty() {
            return (self.edit.display_with_ime(), false);
        }
        if self.text.is_empty() {
            if self.focused {
                return (String::new(), false);
            }
            return (self.placeholder.clone(), true);
        }
        (self.text.clone(), false)
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.cursor_blink_ms = 0.0;
    }

    /// Call after programmatic or IME text changes so the caret stays visible.
    pub fn notify_text_changed(&mut self) {
        self.edit.clamp_selection();
        self.reset_cursor_blink();
    }

    /// Advance caret blink while focused; returns true only when visibility toggles
    /// (so hosts do not request a redraw every frame while a field is focused).
    pub fn advance_cursor(&mut self, dt_ms: f32) -> bool {
        if !self.focused {
            return false;
        }
        self.cursor_blink_ms += dt_ms;
        if self.cursor_blink_ms >= 530.0 {
            self.cursor_blink_ms = 0.0;
            self.cursor_visible = !self.cursor_visible;
            return true;
        }
        false
    }

    /// Ms until the next caret blink toggle, if focused.
    pub fn cursor_blink_wake_ms(&self) -> Option<f32> {
        if !self.focused {
            return None;
        }
        Some((530.0 - self.cursor_blink_ms).max(1.0))
    }

    fn index_at(&self, x: f64) -> usize {
        let content = self.content_bounds();
        let local_x = (x - content.x0).max(0.0);
        let display = if self.password {
            password_mask(&self.text)
        } else {
            self.text.clone()
        };
        let hit = synui_render::with_text_renderer(|renderer| {
            renderer.hit_test_point(
                &display,
                TEXT_FIELD_FONT,
                None,
                local_x,
                content.height() / 2.0,
            )
        });
        if self.password {
            password_mask_byte_to_text_byte(&self.text, hit)
        } else {
            hit.min(self.text.len())
        }
    }

    fn push_undo(&mut self) {
        self.history.push(self.edit.text.clone());
    }

    /// Commit IME text with an undo snapshot.
    pub fn commit_ime(&mut self, text: &str) {
        if text.is_empty() {
            self.edit.ime_composing.clear();
            return;
        }
        self.history.end_coalesce();
        self.push_undo();
        self.edit.commit_ime(text);
        self.notify_text_changed();
    }
}

impl Widget for TextField {
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
            width: Dimension::Fill,
            height: Dimension::Length(48.0),
            padding: [12.0, 16.0, 12.0, 16.0],
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
        Size::new(200.0, 48.0)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let r = ctx.theme.metrics.corner_field;
        paint_simple_rect(scene, self.bounds, ctx.theme.surface, r);
        let outline = RoundedRect::from_rect(self.bounds, r);
        let (width, border) = if self.focused {
            (2.0, ctx.theme.primary)
        } else {
            (1.0, ctx.theme.field_border())
        };
        scene.stroke(
            &Stroke::new(width),
            vello::kurbo::Affine::IDENTITY,
            border,
            None,
            &outline,
        );
        let content = self.content_bounds();
        let (display, is_placeholder) = self.display_text();
        let color = if is_placeholder {
            ctx.theme.outline
        } else {
            ctx.theme.on_surface
        };
        if let Some(text) = ctx.text.as_deref_mut() {
            // Prefer real Parley line height so Latin (shorter than LINE_HEIGHT) is
            // centered in the content strip; paint stays top-aligned to keep caret sync.
            const LINE_HEIGHT: f64 = TEXT_FIELD_FONT as f64 * synui_render::LINE_HEIGHT_EM as f64;
            let layout_h = if display.is_empty() {
                LINE_HEIGHT
            } else {
                let sample = if display.chars().all(|c| c.is_whitespace()) {
                    "Hg"
                } else {
                    display.as_str()
                };
                text.caret_rect(sample, TEXT_FIELD_FONT, None, 0)
                    .height()
                    .clamp(TEXT_FIELD_FONT as f64, LINE_HEIGHT)
            };
            let text_bounds = self.text_bounds_for(layout_h);

            if self.focused
                && !is_placeholder
                && self.has_selection()
                && self.ime_composing.is_empty()
            {
                let range = if self.password {
                    password_mask_byte_range(&self.text, self.sel_range())
                } else {
                    self.sel_range()
                };
                for rect in text.selection_rects(&display, TEXT_FIELD_FONT, None, range) {
                    let highlight = Rect::new(
                        text_bounds.x0 + rect.x0,
                        text_bounds.y0 + rect.y0,
                        text_bounds.x0 + rect.x1,
                        text_bounds.y0 + rect.y1,
                    );
                    let hl = ctx.theme.selection_fill();
                    paint_simple_rect(scene, highlight, hl, 2.0);
                }
            }

            if let Some(cr) = self.composition_range() {
                for rect in text.selection_rects(&display, TEXT_FIELD_FONT, None, cr) {
                    let highlight = Rect::new(
                        text_bounds.x0 + rect.x0,
                        text_bounds.y0 + rect.y0,
                        text_bounds.x0 + rect.x1,
                        text_bounds.y0 + rect.y1,
                    );
                    let hl = ctx.theme.selection_ime_fill();
                    paint_simple_rect(scene, highlight, hl, 2.0);
                }
            }

            if !display.is_empty() {
                text.paint_text(
                    scene,
                    &display,
                    TEXT_FIELD_FONT,
                    color,
                    text_bounds,
                    None,
                    TextAlign::Start,
                );
            }
            if self.focused && self.cursor_visible && !self.password && !self.has_selection() {
                let caret_idx = if !self.ime_composing.is_empty() {
                    self.selection.start.min(self.text.len()) + self.ime_composing.len()
                } else {
                    self.selection.end.min(display.len())
                };
                let caret = text.caret_rect(&display, TEXT_FIELD_FONT, None, caret_idx);
                let caret_line = Line::new(
                    (text_bounds.x0 + caret.x0, text_bounds.y0 + caret.y0 + 1.0),
                    (text_bounds.x0 + caret.x0, text_bounds.y0 + caret.y1 - 1.0),
                );
                scene.stroke(
                    &Stroke::new(1.5),
                    vello::kurbo::Affine::IDENTITY,
                    ctx.theme.primary,
                    None,
                    &caret_line,
                );
            }
        }
        if self.focused && !self.ime_composing.is_empty() {
            let underline = Line::new(
                (content.x0, content.y1 - 2.0),
                (content.x1, content.y1 - 2.0),
            );
            scene.stroke(
                &Stroke::new(2.0),
                vello::kurbo::Affine::IDENTITY,
                ctx.theme.primary,
                None,
                &underline,
            );
        }
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        if self.disabled {
            return EventResult::IGNORED;
        }
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                ..
            } if self.bounds.contains(*position) => {
                let idx = self.index_at(position.x);
                self.set_caret(idx);
                self.dragging_select = true;
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } if self.dragging_select => {
                let idx = self.index_at(position.x);
                self.selection.end = idx;
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::PointerUp { .. } if self.dragging_select => {
                self.dragging_select = false;
                EventResult::HANDLED
            }
            Event::FocusGained => {
                self.focused = true;
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::FocusLost => {
                self.focused = false;
                self.dragging_select = false;
                self.ime_composing.clear();
                self.history.end_coalesce();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Character(c),
                ctrl: true,
                shift,
                ..
            } => {
                if handle_edit_shortcut(&mut self.edit, &mut self.history, c, true, *shift) {
                    self.reset_cursor_blink();
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            Event::KeyDown {
                key: Key::Character(c),
                ctrl: false,
                ..
            } if self.ime_composing.is_empty() => {
                self.history.begin_coalesce();
                self.push_undo();
                self.insert_text(c);
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Space),
                ctrl: false,
                ..
            } if self.ime_composing.is_empty() => {
                // winit maps Space → NamedKey::Space (not Character(" ")).
                self.history.begin_coalesce();
                self.push_undo();
                self.insert_text(" ");
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Backspace),
                ..
            } if self.ime_composing.is_empty() => {
                self.history.end_coalesce();
                self.push_undo();
                self.backspace();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Delete),
                ..
            } if self.ime_composing.is_empty() => {
                self.history.end_coalesce();
                self.push_undo();
                self.delete_forward();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowLeft),
                shift,
                ctrl,
                ..
            } => {
                self.focused = true;
                self.history.end_coalesce();
                if *ctrl {
                    self.move_word_left(*shift);
                } else {
                    let to_start = synui_render::arrow_left_moves_to_start(ctx.layout_direction);
                    if to_start {
                        self.move_left(*shift);
                    } else {
                        self.move_right(*shift);
                    }
                }
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowRight),
                shift,
                ctrl,
                ..
            } => {
                self.focused = true;
                self.history.end_coalesce();
                if *ctrl {
                    self.move_word_right(*shift);
                } else {
                    let to_start = synui_render::arrow_left_moves_to_start(ctx.layout_direction);
                    if to_start {
                        self.move_right(*shift);
                    } else {
                        self.move_left(*shift);
                    }
                }
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Home),
                shift,
                ..
            } => {
                self.history.end_coalesce();
                self.move_line_start(*shift);
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::End),
                shift,
                ..
            } => {
                self.history.end_coalesce();
                self.move_line_end(*shift);
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            } => EventResult::HANDLED,
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        let range = self.sel_range();
        let value = if self.password {
            // Do not expose plaintext secrets to assistive technology.
            Some(password_mask(&self.text))
        } else {
            Some(self.text.clone())
        };
        A11yProps {
            role: A11yRole::TextInput,
            label: Some(self.placeholder.clone()),
            value,
            text_selection: Some(range),
            focusable: !self.disabled,
            actions: vec![A11yAction::Focus, A11yAction::SetValue],
            disabled: self.disabled,
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        !self.disabled
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }
}

/// Multi-line text area (Phase 14: EditCore + Parley hit-test).
pub struct TextArea {
    id: WidgetId,
    bounds: Rect,
    pub edit: EditCore,
    history: EditHistory<String>,
    focused: bool,
    cursor_visible: bool,
    cursor_blink_ms: f32,
    dragging_select: bool,
    /// Vertical scroll offset in pixels (content above the viewport).
    scroll_y: f64,
    scrollbar: VScrollbarState,
    /// Last measured content height (paint / caret) — avoids Parley in hit-test.
    content_h_cache: Cell<f64>,
    /// Optional fill; `None` uses theme surface. Alpha 0 skips the fill.
    pub background: Option<Color>,
    /// When false, the field outline is not painted (focused or idle).
    pub show_border: bool,
}

impl Deref for TextArea {
    type Target = EditCore;
    fn deref(&self) -> &EditCore {
        &self.edit
    }
}

impl DerefMut for TextArea {
    fn deref_mut(&mut self) -> &mut EditCore {
        &mut self.edit
    }
}

impl TextArea {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            edit: EditCore::new(),
            history: EditHistory::new(64),
            focused: false,
            cursor_visible: true,
            cursor_blink_ms: 0.0,
            dragging_select: false,
            scroll_y: 0.0,
            scrollbar: VScrollbarState::default(),
            content_h_cache: Cell::new(0.0),
            background: None,
            show_border: true,
        }
    }

    /// Field fill. Pass a fully transparent color to skip the background.
    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }

    /// Transparent background (no fill).
    pub fn with_transparent_background(mut self) -> Self {
        self.background = Some(Color::TRANSPARENT);
        self
    }

    pub fn with_border(mut self, show: bool) -> Self {
        self.show_border = show;
        self
    }

    pub fn set_border(&mut self, show: bool) {
        self.show_border = show;
    }

    fn text_bounds(&self) -> Rect {
        let pad = 8.0;
        Rect::new(
            self.bounds.x0 + pad,
            self.bounds.y0 + pad,
            self.bounds.x1 - pad,
            self.bounds.y1 - pad,
        )
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.cursor_blink_ms = 0.0;
    }

    pub fn notify_text_changed(&mut self) {
        self.edit.clamp_selection();
        self.reset_cursor_blink();
    }

    pub fn advance_cursor(&mut self, dt_ms: f32) -> bool {
        if !self.focused {
            return false;
        }
        self.cursor_blink_ms += dt_ms;
        if self.cursor_blink_ms >= 530.0 {
            self.cursor_blink_ms = 0.0;
            self.cursor_visible = !self.cursor_visible;
            return true;
        }
        false
    }

    /// Ms until the next caret blink toggle, if focused.
    pub fn cursor_blink_wake_ms(&self) -> Option<f32> {
        if !self.focused {
            return None;
        }
        Some((530.0 - self.cursor_blink_ms).max(1.0))
    }

    pub fn ime_cursor_area(&self) -> Rect {
        let tb = self.text_bounds();
        let max_w = Some(tb.width() as f32);
        let display = self.edit.display_with_ime();
        let caret_idx = if !self.ime_composing.is_empty() {
            self.selection.start.min(self.text.len()) + self.ime_composing.len()
        } else {
            self.selection.end.min(display.len())
        };
        let caret = synui_render::with_text_renderer(|renderer| {
            renderer.caret_bounds(&display, TEXT_AREA_FONT, max_w, caret_idx)
        });
        Rect::new(
            tb.x0 + caret.x0,
            tb.y0 + caret.y0 - self.scroll_y,
            tb.x0 + caret.x0 + 2.0,
            tb.y0 + caret.y1 - self.scroll_y,
        )
    }

    fn index_at(&self, point: Point) -> usize {
        let tb = self.text_bounds();
        let max_w = Some(tb.width() as f32);
        let local_x = (point.x - tb.x0).max(0.0);
        let local_y = (point.y - tb.y0 + self.scroll_y).max(0.0);
        synui_render::with_text_renderer(|renderer| {
            renderer
                .hit_test_point(&self.text, TEXT_AREA_FONT, max_w, local_x, local_y)
                .min(self.text.len())
        })
    }

    fn ensure_caret_visible(&mut self) {
        let tb = self.text_bounds();
        let max_w = Some(tb.width() as f32);
        let end = self.selection.end;
        let (caret, content_h) = synui_render::with_text_renderer(|renderer| {
            renderer.caret_and_height(&self.text, TEXT_AREA_FONT, max_w, end)
        });
        self.content_h_cache.set(content_h);
        let view_h = tb.height();
        if caret.y1 - self.scroll_y > view_h {
            self.scroll_y = caret.y1 - view_h;
        }
        if caret.y0 - self.scroll_y < 0.0 {
            self.scroll_y = caret.y0;
        }
        let max_scroll = (content_h - view_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    fn push_undo(&mut self) {
        self.history.push(self.edit.text.clone());
    }

    /// Commit IME text with an undo snapshot.
    pub fn commit_ime(&mut self, text: &str) {
        if text.is_empty() {
            self.edit.ime_composing.clear();
            return;
        }
        self.history.end_coalesce();
        self.push_undo();
        self.edit.commit_ime(text);
        self.notify_text_changed();
    }
}

impl Default for TextArea {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextArea {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();
        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: synui_core::FlexDirection::Column,
            justify_content: synui_core::JustifyContent::FlexStart,
            align_items: synui_core::AlignItems::Stretch,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            width: Dimension::Fill,
            height: Dimension::Length(120.0),
            min_height: Dimension::Length(120.0),
            padding: [8.0; 4],
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

    fn clip_bounds(&self) -> Option<Rect> {
        Some(self.bounds)
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(
            constraints.max_width.min(400.0) as f64,
            120.0f64.min(constraints.max_height as f64).max(120.0),
        )
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let r = ctx.theme.metrics.corner_field;
        let bg = self.background.unwrap_or(ctx.theme.surface);
        if bg.components[3] > 0.001 {
            paint_simple_rect(scene, self.bounds, bg, r);
        }
        if self.show_border {
            let outline = RoundedRect::from_rect(self.bounds, r);
            let (width, border) = if self.focused {
                (2.0, ctx.theme.primary)
            } else {
                (1.0, ctx.theme.field_border())
            };
            scene.stroke(
                &Stroke::new(width),
                vello::kurbo::Affine::IDENTITY,
                border,
                None,
                &outline,
            );
        }
        let display = self.edit.display_with_ime();
        let text_bounds = self.text_bounds();
        let max_w = Some((text_bounds.width() - 10.0).max(1.0) as f32);
        let mut content_h = text_bounds.height();
        if let Some(text) = ctx.text.as_deref_mut() {
            let measured_h = text.measure_text(&display, TEXT_AREA_FONT, max_w).height;
            content_h = measured_h.max(content_h);
            self.content_h_cache.set(content_h);
            if self.focused && self.has_selection() && self.ime_composing.is_empty() {
                let range = self.sel_range();
                for rect in text.selection_rects(&display, TEXT_AREA_FONT, max_w, range) {
                    let highlight = Rect::new(
                        text_bounds.x0 + rect.x0,
                        text_bounds.y0 + rect.y0 - self.scroll_y,
                        text_bounds.x0 + rect.x1,
                        text_bounds.y0 + rect.y1 - self.scroll_y,
                    );
                    let hl = ctx.theme.selection_fill();
                    paint_simple_rect(scene, highlight, hl, 2.0);
                }
            }
            if let Some(cr) = self.composition_range() {
                for rect in text.selection_rects(&display, TEXT_AREA_FONT, max_w, cr) {
                    let highlight = Rect::new(
                        text_bounds.x0 + rect.x0,
                        text_bounds.y0 + rect.y0 - self.scroll_y,
                        text_bounds.x0 + rect.x1,
                        text_bounds.y0 + rect.y1 - self.scroll_y,
                    );
                    let hl = ctx.theme.selection_ime_fill();
                    paint_simple_rect(scene, highlight, hl, 2.0);
                }
            }
            if !display.is_empty() {
                // Tight height so paint top-aligns with selection/hit-test. Tall
                // paint_bounds trigger vertical centering and clip glyphs away.
                let paint_h = measured_h.max(TEXT_AREA_FONT as f64 * 1.2);
                let paint_bounds = Rect::new(
                    text_bounds.x0,
                    text_bounds.y0 - self.scroll_y,
                    text_bounds.x1 - 10.0,
                    text_bounds.y0 - self.scroll_y + paint_h,
                );
                text.paint_text(
                    scene,
                    &display,
                    TEXT_AREA_FONT,
                    ctx.theme.on_surface,
                    paint_bounds,
                    max_w,
                    TextAlign::Start,
                );
            }
            if self.focused && self.cursor_visible && !self.has_selection() {
                let caret_idx = if !self.ime_composing.is_empty() {
                    self.selection.start.min(self.text.len()) + self.ime_composing.len()
                } else {
                    self.selection.end.min(display.len())
                };
                let caret = text.caret_rect(&display, TEXT_AREA_FONT, max_w, caret_idx);
                let caret_line = Line::new(
                    (
                        text_bounds.x0 + caret.x0,
                        text_bounds.y0 + caret.y0 - self.scroll_y + 1.0,
                    ),
                    (
                        text_bounds.x0 + caret.x0,
                        text_bounds.y0 + caret.y1 - self.scroll_y - 1.0,
                    ),
                );
                scene.stroke(
                    &Stroke::new(1.5),
                    vello::kurbo::Affine::IDENTITY,
                    ctx.theme.primary,
                    None,
                    &caret_line,
                );
            }
        }
        let mut track = ctx.theme.outline;
        track.components[3] = 0.25;
        let mut thumb = ctx.theme.on_surface;
        thumb.components[3] = if self.scrollbar.is_dragging() {
            0.65
        } else {
            0.45
        };
        paint_v_scrollbar(scene, self.bounds, self.scroll_y, content_h, track, thumb);
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        let content_h = {
            let tb = self.text_bounds();
            let max_w = Some(tb.width() as f32);
            synui_render::with_text_renderer(|r| {
                r.measure(&self.text, TEXT_AREA_FONT, max_w).height
            })
        };
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                ..
            } if self.bounds.contains(*position) => {
                if let Some(y) =
                    self.scrollbar
                        .pointer_down(*position, self.bounds, self.scroll_y, content_h)
                {
                    let max_scroll = (content_h - self.text_bounds().height()).max(0.0);
                    self.scroll_y = y.clamp(0.0, max_scroll);
                    ctx.grab_pointer();
                    return EventResult::HANDLED;
                }
                let idx = self.index_at(*position);
                self.set_caret(idx);
                self.dragging_select = true;
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } => {
                if let Some(y) = self
                    .scrollbar
                    .pointer_move(*position, self.bounds, content_h)
                {
                    let max_scroll = (content_h - self.text_bounds().height()).max(0.0);
                    self.scroll_y = y.clamp(0.0, max_scroll);
                    return EventResult::HANDLED;
                }
                if self.dragging_select {
                    let idx = self.index_at(*position);
                    self.selection.end = idx;
                    self.reset_cursor_blink();
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::PointerUp { .. } => {
                if self.scrollbar.pointer_up() {
                    ctx.release_pointer();
                    return EventResult::HANDLED;
                }
                if self.dragging_select {
                    self.dragging_select = false;
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::Scroll { delta_y, .. } if self.focused => {
                let tb = self.text_bounds();
                let max_scroll = (content_h - tb.height()).max(0.0);
                self.scroll_y = (self.scroll_y - *delta_y as f64).clamp(0.0, max_scroll);
                EventResult::HANDLED
            }
            Event::FocusGained => {
                self.focused = true;
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::FocusLost => {
                self.focused = false;
                self.dragging_select = false;
                self.scrollbar.end_drag();
                self.ime_composing.clear();
                self.history.end_coalesce();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Character(c),
                ctrl: true,
                shift,
                ..
            } => {
                if handle_edit_shortcut(&mut self.edit, &mut self.history, c, true, *shift) {
                    self.reset_cursor_blink();
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            Event::KeyDown {
                key: Key::Character(c),
                ctrl: false,
                ..
            } if self.ime_composing.is_empty() => {
                self.history.begin_coalesce();
                self.push_undo();
                self.insert_text(c);
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Space),
                ctrl: false,
                ..
            } if self.ime_composing.is_empty() => {
                // winit maps Space → NamedKey::Space (not Character(" ")).
                self.history.begin_coalesce();
                self.push_undo();
                self.insert_text(" ");
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            } if self.ime_composing.is_empty() => {
                self.history.end_coalesce();
                self.push_undo();
                self.insert_text("\n");
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Backspace),
                ..
            } if self.ime_composing.is_empty() => {
                self.history.end_coalesce();
                self.push_undo();
                self.backspace();
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Delete),
                ..
            } if self.ime_composing.is_empty() => {
                self.history.end_coalesce();
                self.push_undo();
                self.delete_forward();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowLeft),
                shift,
                ctrl,
                ..
            } => {
                self.history.end_coalesce();
                if *ctrl {
                    self.move_word_left(*shift);
                } else {
                    let to_start = synui_render::arrow_left_moves_to_start(ctx.layout_direction);
                    if to_start {
                        self.move_left(*shift);
                    } else {
                        self.move_right(*shift);
                    }
                }
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowRight),
                shift,
                ctrl,
                ..
            } => {
                self.history.end_coalesce();
                if *ctrl {
                    self.move_word_right(*shift);
                } else {
                    let to_start = synui_render::arrow_left_moves_to_start(ctx.layout_direction);
                    if to_start {
                        self.move_right(*shift);
                    } else {
                        self.move_left(*shift);
                    }
                }
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Home),
                shift,
                ..
            } => {
                self.history.end_coalesce();
                self.move_line_start(*shift);
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::End),
                shift,
                ..
            } => {
                self.history.end_coalesce();
                self.move_line_end(*shift);
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowUp),
                shift,
                ..
            } => {
                self.history.end_coalesce();
                self.move_vertical(-1, *shift);
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowDown),
                shift,
                ..
            } => {
                self.history.end_coalesce();
                self.move_vertical(1, *shift);
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::TextInput,
            label: None,
            value: Some(self.text.clone()),
            text_selection: Some(self.sel_range()),
            focusable: true,
            actions: vec![A11yAction::Focus, A11yAction::SetValue],
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn hit_test_prefer_self(&self, point: Point) -> bool {
        let content_h = self.content_h_cache.get().max(self.text_bounds().height());
        VScrollbarState::hit(self.bounds, self.scroll_y, content_h, point)
            || self.scrollbar.is_dragging()
    }
}

/// Checkbox selection control.
pub struct Checkbox {
    id: WidgetId,
    bounds: Rect,
    pub label: String,
    pub checked: bool,
    /// Phase 42: real disabled for `:disabled` cascade + input gating.
    pub disabled: bool,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            label: label.into(),
            checked: false,
            disabled: false,
        }
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }
}

impl Widget for Checkbox {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        let height: f64 = 40.0;
        let text_w = if let Some(m) = ctx.text_measurer.as_deref_mut() {
            m.measure_text(&self.label, 14.0, None).width
        } else {
            self.label.len() as f64 * 8.0
        };
        Size::new(
            (text_w + 40.0).min(constraints.max_width as f64),
            height.min(constraints.max_height as f64),
        )
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
            height: Dimension::Length(40.0),
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let r = ctx.theme.metrics.corner_field.min(4.0);
        let box_size = 20.0;
        let box_y = self.bounds.y0 + (self.bounds.height() - box_size) / 2.0;
        let box_rect = Rect::new(
            self.bounds.x0 + 4.0,
            box_y,
            self.bounds.x0 + 4.0 + box_size,
            box_y + box_size,
        );
        if self.checked {
            paint_simple_rect(scene, box_rect, ctx.theme.primary, r);
        } else {
            paint_simple_rect(scene, box_rect, ctx.theme.surface_variant, r);
            let outline = RoundedRect::from_rect(box_rect, r);
            scene.stroke(
                &Stroke::new(1.5),
                vello::kurbo::Affine::IDENTITY,
                ctx.theme.outline,
                None,
                &outline,
            );
        }
        if let Some(text) = ctx.text.as_deref_mut() {
            let label_bounds = Rect::new(
                self.bounds.x0 + 32.0,
                self.bounds.y0,
                self.bounds.x1 - 4.0,
                self.bounds.y1,
            );
            text.paint_text(
                scene,
                &self.label,
                14.0,
                ctx.theme.on_surface,
                label_bounds,
                Some(label_bounds.width() as f32),
                TextAlign::Start,
            );
        }
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        if self.disabled {
            return EventResult::IGNORED;
        }
        match event {
            Event::PointerUp { position, .. } if self.bounds.contains(*position) => {
                self.checked = !self.checked;
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Space),
                ..
            } => {
                self.checked = !self.checked;
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn is_focusable(&self) -> bool {
        !self.disabled
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Checkbox,
            label: Some(self.label.clone()),
            value: Some(self.checked.to_string()),
            focusable: !self.disabled,
            actions: vec![A11yAction::Click, A11yAction::Focus],
            disabled: self.disabled,
            checked: Some(self.checked),
            ..Default::default()
        }
    }
}

/// Radio button.
pub struct Radio {
    id: WidgetId,
    bounds: Rect,
    pub label: String,
    pub selected: bool,
}

impl Radio {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            label: label.into(),
            selected: false,
        }
    }
}

impl Widget for Radio {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        let height: f64 = 40.0;
        let text_w = if let Some(m) = ctx.text_measurer.as_deref_mut() {
            m.measure_text(&self.label, 14.0, None).width
        } else {
            self.label.len() as f64 * 8.0
        };
        Size::new(
            (text_w + 40.0).min(constraints.max_width as f64),
            height.min(constraints.max_height as f64),
        )
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
            height: Dimension::Length(40.0),
            min_height: Dimension::Length(40.0),
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        // Match Checkbox: solid primary when selected; outline-only when not
        // (no ring/dot gap).
        let diam = 20.0;
        let box_y = self.bounds.y0 + (self.bounds.height() - diam) / 2.0;
        let disc = Rect::new(
            self.bounds.x0 + 4.0,
            box_y,
            self.bounds.x0 + 4.0 + diam,
            box_y + diam,
        );
        if self.selected {
            paint_simple_rect(scene, disc, ctx.theme.primary, diam / 2.0);
        } else {
            let outline = RoundedRect::from_rect(disc, diam / 2.0);
            scene.stroke(
                &Stroke::new(2.0),
                vello::kurbo::Affine::IDENTITY,
                ctx.theme.outline,
                None,
                &outline,
            );
        }
        if let Some(text) = ctx.text.as_deref_mut() {
            let label_bounds = Rect::new(
                self.bounds.x0 + 28.0,
                self.bounds.y0,
                self.bounds.x1 - 4.0,
                self.bounds.y1,
            );
            text.paint_text(
                scene,
                &self.label,
                14.0,
                ctx.theme.on_surface,
                label_bounds,
                Some(label_bounds.width() as f32),
                TextAlign::Start,
            );
        }
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        match event {
            Event::PointerUp { position, .. } if self.bounds.contains(*position) => {
                // Bool-controlled Radio (gallery / facade) toggles like Checkbox.
                self.selected = !self.selected;
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Space),
                ..
            } => {
                self.selected = !self.selected;
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Checkbox,
            label: Some(self.label.clone()),
            value: Some(self.selected.to_string()),
            focusable: true,
            actions: vec![A11yAction::Click, A11yAction::Focus],
            checked: Some(self.selected),
            ..Default::default()
        }
    }
}

/// Toggle switch.
pub struct Switch {
    id: WidgetId,
    bounds: Rect,
    pub on: bool,
    /// Phase 42: real disabled for `:disabled` cascade + input gating.
    pub disabled: bool,
}

impl Switch {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            on: false,
            disabled: false,
        }
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }
}

impl Widget for Switch {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(
            52.0f64.min(constraints.max_width as f64),
            32.0f64.min(constraints.max_height as f64),
        )
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
            width: Dimension::Length(52.0),
            height: Dimension::Length(32.0),
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let cupertino = ctx.theme.cupertino;
        let track_h = if cupertino { 31.0 } else { 20.0 };
        let track_w = if cupertino { 51.0 } else { 52.0 };
        let track = Rect::new(
            self.bounds.x0,
            self.bounds.y0 + (self.bounds.height() - track_h) / 2.0,
            self.bounds.x0 + track_w,
            self.bounds.y0 + (self.bounds.height() + track_h) / 2.0,
        );
        let track_color = if self.on {
            if cupertino {
                peniko::Color::from_rgb8(52, 199, 89)
            } else {
                ctx.theme.primary
            }
        } else if cupertino {
            peniko::Color::from_rgb8(229, 229, 234)
        } else {
            ctx.theme.outline
        };
        let corner = if cupertino {
            track_h / 2.0
        } else {
            ctx.theme.metrics.corner_switch
        };
        paint_simple_rect(scene, track, track_color, corner);
        let thumb_r = if cupertino { 13.0 } else { 10.0 };
        let thumb_x = if self.on {
            track.x1 - thumb_r - 2.0
        } else {
            track.x0 + thumb_r + 2.0
        };
        let thumb_y = track.y0 + track_h / 2.0;
        let thumb = Rect::new(
            thumb_x - thumb_r,
            thumb_y - thumb_r,
            thumb_x + thumb_r,
            thumb_y + thumb_r,
        );
        paint_simple_rect(scene, thumb, peniko::Color::WHITE, thumb_r);
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        if self.disabled {
            return EventResult::IGNORED;
        }
        match event {
            Event::PointerUp { position, .. } if self.bounds.contains(*position) => {
                self.on = !self.on;
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Space),
                ..
            } => {
                self.on = !self.on;
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn is_focusable(&self) -> bool {
        !self.disabled
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Switch,
            value: Some(self.on.to_string()),
            focusable: !self.disabled,
            actions: vec![A11yAction::Click, A11yAction::Focus],
            disabled: self.disabled,
            checked: Some(self.on),
            ..Default::default()
        }
    }
}

/// Dropdown selector with expandable option list.
pub struct Dropdown {
    id: WidgetId,
    bounds: Rect,
    pub options: Vec<String>,
    pub selected: usize,
    pub expanded: bool,
    pub highlighted: usize,
    pub item_height: f64,
    field_height: f64,
    pub overlay_popup_bounds: Option<Rect>,
    /// Phase 42: real disabled for `:disabled` cascade + input gating.
    pub disabled: bool,
}

impl Dropdown {
    pub fn new(options: Vec<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            options,
            selected: 0,
            expanded: false,
            highlighted: 0,
            item_height: 36.0,
            field_height: 40.0,
            overlay_popup_bounds: None,
            disabled: false,
        }
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    pub fn field_bounds(&self) -> Rect {
        let fh = self.field_height.min(self.bounds.height());
        Rect::new(
            self.bounds.x0,
            self.bounds.y0,
            self.bounds.x1,
            self.bounds.y0 + fh,
        )
    }

    pub fn popup_bounds(&self) -> Rect {
        if let Some(bounds) = self.overlay_popup_bounds {
            return bounds;
        }
        if !self.expanded {
            return Rect::ZERO;
        }
        let h = self.item_height * self.options.len() as f64;
        Rect::new(
            self.bounds.x0,
            self.bounds.y0 + self.field_height,
            self.bounds.x1,
            self.bounds.y0 + self.field_height + h,
        )
    }

    fn item_at(&self, position: Point) -> Option<usize> {
        if !self.expanded {
            return None;
        }
        let popup = self.popup_bounds();
        if !popup.contains(position) {
            return None;
        }
        let idx = ((position.y - popup.y0) / self.item_height) as usize;
        if idx < self.options.len() {
            Some(idx)
        } else {
            None
        }
    }

    fn close(&mut self) {
        self.expanded = false;
        self.overlay_popup_bounds = None;
    }
}

impl Widget for Dropdown {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn hit_test(&self, point: Point) -> bool {
        self.bounds.contains(point)
    }

    fn measure(&mut self, ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        let width = if let Some(opt) = self.options.get(self.selected) {
            if let Some(m) = ctx.text_measurer.as_deref_mut() {
                m.measure_text(opt, 14.0, None).width + 32.0
            } else {
                200.0
            }
        } else {
            200.0
        };
        Size::new(
            width.min(constraints.max_width as f64).max(160.0),
            self.field_height.min(constraints.max_height as f64),
        )
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;

        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();

        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: synui_core::FlexDirection::Column,
            justify_content: synui_core::JustifyContent::FlexStart,
            align_items: synui_core::AlignItems::Stretch,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            width: Dimension::Fill,
            height: Dimension::Auto,
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let field = self.field_bounds();
        paint_simple_rect(
            scene,
            field,
            ctx.theme.surface_variant,
            ctx.theme.metrics.corner_field,
        );
        if let Some(opt) = self.options.get(self.selected) {
            if let Some(text) = ctx.text.as_deref_mut() {
                let inner = Rect::new(field.x0 + 12.0, field.y0, field.x1 - 12.0, field.y1);
                text.paint_text(
                    scene,
                    opt,
                    14.0,
                    ctx.theme.on_surface,
                    inner,
                    Some(inner.width() as f32),
                    TextAlign::Start,
                );
            }
        }
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        if self.disabled {
            return EventResult::IGNORED;
        }
        match event {
            Event::PointerMove { position, .. } if self.expanded => {
                let idx = self.item_at(*position).unwrap_or(self.highlighted);
                if idx != self.highlighted {
                    self.highlighted = idx;
                }
                EventResult::HANDLED
            }
            Event::PointerUp { position, .. } => {
                if self.field_bounds().contains(*position) {
                    self.expanded = !self.expanded;
                    self.highlighted = self.selected;
                    return EventResult::HANDLED;
                }
                if self.expanded {
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowDown),
                ..
            } if self.expanded => {
                if !self.options.is_empty() {
                    self.highlighted = (self.highlighted + 1) % self.options.len();
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowUp),
                ..
            } if self.expanded => {
                if !self.options.is_empty() {
                    let n = self.options.len();
                    self.highlighted = (self.highlighted + n - 1) % n;
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            } if self.expanded => {
                self.selected = self.highlighted;
                self.close();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Escape),
                ..
            } if self.expanded => {
                self.close();
                EventResult::HANDLED
            }
            Event::FocusLost => {
                // Popup menu receives clicks without holding focus; closing here races
                // with PointerUp selection on the overlay menu.
                if self.overlay_popup_bounds.is_none() {
                    self.close();
                }
                EventResult::IGNORED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn is_focusable(&self) -> bool {
        !self.disabled
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Generic,
            label: Some("Dropdown".into()),
            value: self.options.get(self.selected).cloned(),
            focusable: !self.disabled,
            actions: vec![A11yAction::Click, A11yAction::Focus, A11yAction::SetValue],
            disabled: self.disabled,
            selected: Some(self.selected < self.options.len()),
            expanded: Some(self.expanded),
            ..Default::default()
        }
    }
}

/// Slider value control.
pub struct Slider {
    id: WidgetId,
    bounds: Rect,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    dragging: bool,
    /// Phase 42: real disabled for `:disabled` cascade + input gating.
    pub disabled: bool,
}

impl Slider {
    pub fn new(min: f32, max: f32) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            value: min,
            min,
            max,
            dragging: false,
            disabled: false,
        }
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    fn value_at_x(&self, x: f64) -> f32 {
        // Keep in sync with paint track inset (max Cupertino thumb radius).
        let inset = 14.0_f64;
        let x0 = self.bounds.x0 + inset;
        let width = (self.bounds.width() - 2.0 * inset).max(1.0);
        let t = ((x - x0) / width) as f32;
        self.min + (self.max - self.min) * t.clamp(0.0, 1.0)
    }
}

impl Widget for Slider {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;

        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();

        // flex_shrink: 0 — Fill siblings (VirtualList / DataTable) must not crush the
        // track to zero height in tall facade galleries (drag then "vanishes" until Full paint).
        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: synui_core::FlexDirection::Row,
            justify_content: synui_core::JustifyContent::FlexStart,
            align_items: synui_core::AlignItems::Center,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            width: Dimension::Fill,
            height: Dimension::Length(40.0),
            min_height: Dimension::Length(40.0),
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(
            constraints.max_width.min(f32::MAX) as f64,
            40.0_f64.min(constraints.max_height as f64).max(32.0),
        )
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let cupertino = ctx.theme.cupertino;
        let track_h = if cupertino { 4.0 } else { 4.0 };
        let track_r = if cupertino { 2.0 } else { 2.0 };
        let thumb_r = if cupertino { 14.0 } else { 8.0 };
        let cy = self.bounds.center().y;
        // Inset matches `value_at_x` (max thumb radius) so Damaged erase/clip covers the thumb.
        let inset = 14.0_f64;
        let track = Rect::new(
            self.bounds.x0 + inset,
            cy - track_h / 2.0,
            (self.bounds.x1 - inset).max(self.bounds.x0 + inset),
            cy + track_h / 2.0,
        );
        let t = ((self.value - self.min) / (self.max - self.min).max(0.001)).clamp(0.0, 1.0);
        let filled = Rect::new(
            track.x0,
            track.y0,
            track.x0 + track.width() * t as f64,
            track.y1,
        );
        let track_color = ctx.theme.progress_track();
        paint_simple_rect(scene, track, track_color, track_r);
        paint_simple_rect(scene, filled, ctx.theme.primary, track_r);
        let thumb_x = track.x0 + track.width() * t as f64;
        let thumb = Circle::new((thumb_x, cy), thumb_r);
        scene.fill(
            vello::peniko::Fill::NonZero,
            vello::kurbo::Affine::IDENTITY,
            if cupertino {
                ctx.theme.surface
            } else {
                ctx.theme.primary
            },
            None,
            &thumb,
        );
        if cupertino {
            // Soft outline for iOS thumb.
            scene.stroke(
                &vello::kurbo::Stroke::new(0.5),
                vello::kurbo::Affine::IDENTITY,
                ctx.theme.outline.multiply_alpha(0.4),
                None,
                &thumb,
            );
        }
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        if self.disabled {
            return EventResult::IGNORED;
        }
        match event {
            Event::PointerDown { position, .. } if self.bounds.contains(*position) => {
                self.dragging = true;
                self.value = self.value_at_x(position.x);
                ctx.grab_pointer();
                ctx.request_paint();
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } if self.dragging => {
                self.value = self.value_at_x(position.x);
                ctx.request_paint();
                EventResult::HANDLED
            }
            Event::PointerUp { .. } if self.dragging => {
                self.dragging = false;
                ctx.release_pointer();
                ctx.request_paint();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowLeft),
                ..
            } => {
                self.value = (self.value - (self.max - self.min) * 0.05).max(self.min);
                ctx.request_paint();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowRight),
                ..
            } => {
                self.value = (self.value + (self.max - self.min) * 0.05).min(self.max);
                ctx.request_paint();
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn is_focusable(&self) -> bool {
        !self.disabled
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Slider,
            value: Some(format!("{:.2}", self.value)),
            focusable: !self.disabled,
            actions: vec![A11yAction::Focus, A11yAction::SetValue],
            disabled: self.disabled,
            ..Default::default()
        }
    }
}

/// Overlay popup list for a parent [`Dropdown`].
pub struct DropdownMenu {
    id: WidgetId,
    bounds: Rect,
    owner: WidgetId,
    options: Vec<String>,
    pub selected: usize,
    pub highlighted: usize,
    item_height: f64,
    pub pending_select: Option<usize>,
    pub pending_highlight: Option<usize>,
}

impl DropdownMenu {
    pub fn new(owner: WidgetId) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            owner,
            options: Vec::new(),
            selected: 0,
            highlighted: 0,
            item_height: 36.0,
            pending_select: None,
            pending_highlight: None,
        }
    }

    pub fn with_selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_options(mut self, options: Vec<String>) -> Self {
        self.options = options;
        self
    }

    pub fn with_highlighted(mut self, highlighted: usize) -> Self {
        self.highlighted = highlighted;
        self
    }

    fn item_at(&self, position: Point) -> Option<usize> {
        let idx = ((position.y - self.bounds.y0) / self.item_height) as usize;
        if idx < self.options.len() {
            Some(idx)
        } else {
            None
        }
    }
}

impl Widget for DropdownMenu {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, _constraints: Constraints) -> Size {
        Size::new(
            self.bounds.width().max(160.0),
            self.item_height * self.options.len() as f64,
        )
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let menu_r = ctx.theme.metrics.corner_menu;
        paint_simple_rect(scene, self.bounds, ctx.theme.surface, menu_r);
        let outline = RoundedRect::from_rect(self.bounds, menu_r);
        scene.stroke(
            &Stroke::new(1.0),
            vello::kurbo::Affine::IDENTITY,
            ctx.theme.outline,
            None,
            &outline,
        );
        for (i, opt) in self.options.iter().enumerate() {
            let item = Rect::new(
                self.bounds.x0,
                self.bounds.y0 + i as f64 * self.item_height,
                self.bounds.x1,
                self.bounds.y0 + (i + 1) as f64 * self.item_height,
            );
            if i == self.selected {
                paint_simple_rect(scene, item, ctx.theme.surface_container, 0.0);
            }
            if i == self.highlighted && i != self.selected {
                let mut hover = ctx.theme.primary;
                hover.components[3] = 0.12;
                paint_simple_rect(scene, item, hover, 0.0);
            }
            if let Some(text) = ctx.text.as_deref_mut() {
                let inner = Rect::new(item.x0 + 12.0, item.y0, item.x1 - 12.0, item.y1);
                text.paint_text(
                    scene,
                    opt,
                    14.0,
                    ctx.theme.on_surface,
                    inner,
                    Some(inner.width() as f32),
                    TextAlign::Start,
                );
            }
        }
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        match event {
            Event::PointerMove { position, .. } => {
                if let Some(idx) = self.item_at(*position) {
                    if idx != self.highlighted {
                        self.highlighted = idx;
                        self.pending_highlight = Some(idx);
                    }
                }
                EventResult::HANDLED
            }
            Event::PointerUp { position, .. } => {
                if let Some(idx) = self.item_at(*position) {
                    self.pending_select = Some(idx);
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            } => EventResult::HANDLED,
            _ => EventResult::IGNORED,
        }
    }
}

impl DropdownMenu {
    pub fn apply_pending_selections(store: &mut WidgetStore) {
        let updates: Vec<(WidgetId, usize)> = store
            .widgets_mut()
            .filter_map(|w| {
                let menu = w.as_any_mut().downcast_mut::<DropdownMenu>()?;
                menu.pending_select.map(|idx| {
                    menu.pending_select = None;
                    (menu.owner, idx)
                })
            })
            .collect();
        for (owner, idx) in updates {
            if let Some(owner_w) = store.get_mut(owner) {
                if let Some(dd) = owner_w.as_any_mut().downcast_mut::<Dropdown>() {
                    dd.selected = idx;
                    dd.highlighted = idx;
                    dd.close();
                }
            }
        }

        let highlights: Vec<(WidgetId, usize)> = store
            .widgets_mut()
            .filter_map(|w| {
                let menu = w.as_any_mut().downcast_mut::<DropdownMenu>()?;
                menu.pending_highlight.map(|idx| {
                    menu.pending_highlight = None;
                    (menu.owner, idx)
                })
            })
            .collect();
        for (owner, idx) in highlights {
            if let Some(owner_w) = store.get_mut(owner) {
                if let Some(dd) = owner_w.as_any_mut().downcast_mut::<Dropdown>() {
                    dd.highlighted = idx;
                }
            }
        }
    }
}

#[cfg(test)]
mod phase42_disabled_tests {
    use super::*;
    use synui_core::{Event, EventCtx, Widget};

    #[test]
    fn input_controls_report_is_disabled() {
        assert!(Checkbox::new("x").with_disabled(true).is_disabled());
        assert!(Switch::new().with_disabled(true).is_disabled());
        assert!(Slider::new(0.0, 1.0).with_disabled(true).is_disabled());
        assert!(Dropdown::new(vec!["a".into()])
            .with_disabled(true)
            .is_disabled());
        assert!(!Checkbox::new("x").is_disabled());
    }

    #[test]
    fn caret_blink_only_dirties_on_toggle() {
        let mut tf = TextField::new("ph");
        tf.focused = true;
        assert!(!tf.advance_cursor(100.0));
        assert!(!tf.advance_cursor(400.0));
        assert!(tf.advance_cursor(50.0), "toggle at ~530ms");
        assert!(!tf.cursor_visible);
        assert!(tf.cursor_blink_wake_ms().unwrap() > 500.0);
    }

    #[test]
    fn password_mask_uses_bullet_and_maps_byte_ranges() {
        let ascii = password_mask("secret");
        assert_eq!(ascii, "••••••");
        assert_eq!(password_mask_byte_range("secret", 0..6), 0..18);

        let cjk = "密码";
        assert_eq!(password_mask(cjk), "••");
        // Both CJK chars are 3 bytes each; full selection → two mask glyphs.
        assert_eq!(password_mask_byte_range(cjk, 0..cjk.len()), 0..6);
        assert_eq!(password_mask_byte_to_text_byte(cjk, 0), 0);
        assert_eq!(password_mask_byte_to_text_byte(cjk, 3), 3);
        assert_eq!(password_mask_byte_to_text_byte(cjk, 6), cjk.len());
    }

    #[test]
    fn text_editors_insert_named_space() {
        let space = Event::KeyDown {
            key: Key::Named(NamedKey::Space),
            repeat: false,
            shift: false,
            ctrl: false,
            alt: false,
        };

        let mut tf = TextField::new("ph");
        let path = [tf.id()];
        let mut ctx = EventCtx::new(tf.id(), &path, Default::default());
        assert!(tf.event(&mut ctx, &space).handled);
        assert_eq!(tf.text, " ");

        let mut ta = TextArea::new();
        let path = [ta.id()];
        let mut ctx = EventCtx::new(ta.id(), &path, Default::default());
        assert!(ta.event(&mut ctx, &space).handled);
        assert_eq!(ta.text, " ");
    }

    #[test]
    fn slider_rejects_flex_shrink_and_grabs_pointer_while_dragging() {
        let slider = Slider::new(0.0, 100.0);
        assert_eq!(
            slider.layout_style().flex_shrink,
            0.0,
            "gallery VirtualList/DataTable must not crush the track"
        );
        assert!(matches!(
            slider.layout_style().height,
            Dimension::Length(h) if (h - 40.0).abs() < f32::EPSILON
        ));

        let mut store = WidgetStore::new();
        let mut slider = Slider::new(0.0, 100.0);
        slider.set_bounds(Rect::new(0.0, 0.0, 200.0, 40.0));
        let id = store.insert(slider);
        store.set_root(id);
        assert!(
            store
                .dispatch_event(&Event::PointerDown {
                    id: 0,
                    position: Point::new(100.0, 20.0),
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                })
                .handled
        );
        assert_eq!(
            store.pointer_grab(),
            Some(id),
            "drag must grab pointer so Damaged paint targets the slider"
        );

        assert!(
            store
                .dispatch_event(&Event::PointerMove {
                    id: 0,
                    position: Point::new(160.0, 5.0), // outside height — grab still delivers
                })
                .handled
        );
        let value = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<Slider>()
            .unwrap()
            .value;
        assert!(
            value > 50.0,
            "grabbed move outside bounds must still update value, got {value}"
        );

        assert!(
            store
                .dispatch_event(&Event::PointerUp {
                    id: 0,
                    position: Point::new(160.0, 5.0),
                    button: PointerButton::Primary,
                })
                .handled
        );
        assert_eq!(store.pointer_grab(), None);
    }
}
