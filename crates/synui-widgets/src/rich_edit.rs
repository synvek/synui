//! Block-level rich text editor — EditCore on the active inline block (Phase 20).

use std::ops::{Deref, DerefMut};

use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Block, BlockDoc, Constraints, Dimension, Event, EventCtx,
    EventResult, Key, LayoutStyle, Line, MeasureCtx, NamedKey, PaintCtx, Point, PointerButton,
    Rect, RoundedRect, Size, Stroke, TextAlign, TextStyleRun, Widget, WidgetId,
};
use synui_render::paint_simple_rect;
use vello::Scene;

use crate::edit_core::{
    clipboard_get, clipboard_get_rich, clipboard_set_rich, EditCore, EditHistory,
};
use crate::scroll::{paint_v_scrollbar, VScrollbarState};

const FONT: f32 = 15.0;
const BLOCK_GAP: f64 = 6.0;
const MARKER_PAD: f64 = 22.0;

/// Caret / selection anchor in a [`BlockDoc`] (Phase 25 cross-block selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocPos {
    pub block: usize,
    pub offset: usize,
}

impl DocPos {
    pub fn new(block: usize, offset: usize) -> Self {
        Self { block, offset }
    }
}

/// Multi-line block editor: paragraphs, headings, lists, images.
///
/// Focus is `(block_index, byte_offset)` via `active_block` + [`EditCore`] on that inline.
/// Cross-block selection uses [`Self::sel_anchor`] + focus at `(active_block, edit.selection.end)`.
pub struct RichEdit {
    id: WidgetId,
    bounds: Rect,
    pub doc: BlockDoc,
    /// Index of the block owning the caret / EditCore.
    pub active_block: usize,
    pub edit: EditCore,
    /// Fixed end of the selection (Phase 25). Focus is `active_block` + `edit.selection.end`.
    pub sel_anchor: DocPos,
    history: EditHistory<BlockDoc>,
    focused: bool,
    cursor_visible: bool,
    cursor_blink_ms: f32,
    dragging_select: bool,
    /// Wall-clock time of the last primary press (not frame dt — idle frames
    /// would otherwise leave `last_click_ms` near 0 and turn every click into
    /// a multi-click word select).
    last_click_at: Option<std::time::Instant>,
    last_click_pos: synui_core::Point,
    click_count: u8,
    scroll_y: f64,
    scrollbar: VScrollbarState,
    /// Cached `(content_width, rows)`; invalidated on doc/width change.
    layout_cache: std::cell::RefCell<Option<(f64, Vec<(f64, f64, f64)>)>>,
}

impl Deref for RichEdit {
    type Target = EditCore;
    fn deref(&self) -> &EditCore {
        &self.edit
    }
}

impl DerefMut for RichEdit {
    fn deref_mut(&mut self) -> &mut EditCore {
        &mut self.edit
    }
}

impl RichEdit {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            doc: BlockDoc::new(),
            active_block: 0,
            edit: EditCore::new(),
            sel_anchor: DocPos::new(0, 0),
            history: EditHistory::new(64),
            focused: false,
            cursor_visible: true,
            cursor_blink_ms: 0.0,
            dragging_select: false,
            last_click_at: None,
            last_click_pos: synui_core::Point::new(0.0, 0.0),
            click_count: 0,
            scroll_y: 0.0,
            scrollbar: VScrollbarState::default(),
            layout_cache: std::cell::RefCell::new(None),
        }
    }

    pub fn with_doc(doc: BlockDoc) -> Self {
        let mut e = Self::new();
        e.set_doc(doc);
        e
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        Self::with_doc(BlockDoc::from_plain(text))
    }

    /// Replace document and sync EditCore to first text block.
    pub fn set_doc(&mut self, doc: BlockDoc) {
        self.doc = doc;
        *self.layout_cache.borrow_mut() = None;
        self.doc.ensure_nonempty();
        self.active_block = self
            .doc
            .blocks
            .iter()
            .position(|b| b.is_text())
            .unwrap_or(0);
        self.sync_edit_from_active();
        let len = self.edit.text.len();
        self.edit.selection = len..len;
        self.sel_anchor = DocPos::new(self.active_block, len);
        self.edit.ime_composing.clear();
    }

    /// Flushes the active EditCore buffer into [`Self::doc`] (facade poll / export).
    pub fn commit_pending_edit(&mut self) {
        self.flush_edit_to_doc();
    }

    fn sync_edit_from_active(&mut self) {
        if let Some(inline) = self.doc.get_inline(self.active_block) {
            self.edit.text = inline.text.clone();
            self.edit.clamp_selection();
        } else {
            self.edit.text.clear();
            self.edit.selection = 0..0;
        }
    }

    fn flush_edit_to_doc(&mut self) {
        if let Some(inline) = self.doc.get_inline_mut(self.active_block) {
            // Preserve spans; only replace text if EditCore mutated plain buffer.
            // Spans stay in sync via insert/delete helpers below.
            if inline.text != self.edit.text {
                // Prefer EditCore text when out of sync (edge); keep spans clamped.
                inline.text = self.edit.text.clone();
                inline.spans.retain(|s| s.range.start < inline.text.len());
                for s in &mut inline.spans {
                    s.range.end = s.range.end.min(inline.text.len());
                }
                inline.normalize();
            }
        }
    }

    fn text_bounds(&self) -> Rect {
        let pad = 10.0;
        Rect::new(
            self.bounds.x0 + pad,
            self.bounds.y0 + pad,
            self.bounds.x1 - pad,
            self.bounds.y1 - pad,
        )
    }

    fn push_undo(&mut self) {
        self.flush_edit_to_doc();
        *self.layout_cache.borrow_mut() = None;
        self.history.push(self.doc.clone());
    }

    fn selected_doc(&self) -> BlockDoc {
        let (s, e) = self.ordered_sel();
        if s == e {
            return BlockDoc::from_plain("");
        }
        self.doc.slice_doc(s.block, s.offset, e.block, e.offset)
    }

    fn copy_selection_to_clipboard(&self) {
        let plain = self.selected_plain();
        if plain.is_empty() {
            return;
        }
        let rich = self.selected_doc();
        clipboard_set_rich(&plain, rich.to_clipboard_json());
    }

    fn insert_doc(&mut self, paste: BlockDoc) {
        if paste.blocks.is_empty() {
            return;
        }
        if self.has_doc_selection() || self.edit.has_selection() {
            self.delete_selection_rich();
        }
        let single = paste.blocks.len() == 1;
        for (i, block) in paste.blocks.into_iter().enumerate() {
            if i > 0 {
                self.split_block_at_caret();
                if self.active_block < self.doc.blocks.len() {
                    self.doc.blocks[self.active_block] = block;
                    self.sync_edit_from_active();
                    let len = self.edit.text.len();
                    self.edit.selection = len..len;
                    self.sel_anchor = DocPos::new(self.active_block, len);
                }
                continue;
            }
            match block {
                Block::Image { .. } => {
                    let insert_at = self.active_block + 1;
                    self.doc.blocks.insert(insert_at, block);
                    self.active_block = insert_at;
                    // Prefer following text block for caret.
                    let next = insert_at + 1;
                    if next < self.doc.blocks.len() && self.doc.blocks[next].is_text() {
                        self.active_block = next;
                    } else {
                        self.doc.blocks.insert(next, Block::paragraph(""));
                        self.active_block = next;
                    }
                    self.sync_edit_from_active();
                    self.edit.selection = 0..0;
                    self.sel_anchor = DocPos::new(self.active_block, 0);
                }
                Block::Heading { .. }
                    if single
                        && self
                            .doc
                            .get_inline(self.active_block)
                            .map(|t| t.text.is_empty())
                            .unwrap_or(true)
                        && matches!(
                            self.doc.blocks.get(self.active_block),
                            Some(Block::Paragraph(_))
                        ) =>
                {
                    // Empty paragraph → keep heading block type (layout font size).
                    self.doc.blocks[self.active_block] = block;
                    self.sync_edit_from_active();
                    let len = self.edit.text.len();
                    self.edit.selection = len..len;
                    self.sel_anchor = DocPos::new(self.active_block, len);
                }
                other => {
                    // Merge into current block; heading size is already baked into spans
                    // by slice_doc / clipboard JSON.
                    if let Some(inl) = other.inline() {
                        let mut inl = inl.clone();
                        if matches!(&other, Block::Heading { .. }) && !inl.text.is_empty() {
                            let has_size = inl.spans.iter().any(|s| s.font_size.is_some());
                            if !has_size {
                                let tenths = (other.font_size() * 10.0).round() as u32;
                                inl.apply_font_size(0..inl.text.len(), Some(tenths));
                            }
                        }
                        let at = self.edit.selection.start;
                        if let Some(target) = self.doc.get_inline_mut(self.active_block) {
                            target.insert_rich(at, &inl);
                            self.edit.text = target.text.clone();
                            let end = at + inl.text.len();
                            self.edit.selection = end..end;
                            self.sel_anchor = DocPos::new(self.active_block, end);
                        }
                    }
                }
            }
        }
        *self.layout_cache.borrow_mut() = None;
    }

    fn content_width(&self) -> f64 {
        (self.text_bounds().width() - 10.0).max(40.0)
    }

    /// Layout: (y0 relative to content top, height, text_x inset) per block.
    fn block_layout(&self) -> Vec<(f64, f64, f64)> {
        let max_w = self.content_width();
        if let Some((w, ref cached)) = *self.layout_cache.borrow() {
            if (w - max_w).abs() < 0.5 {
                return cached.clone();
            }
        }
        let mut y = 0.0;
        let mut out = Vec::with_capacity(self.doc.blocks.len());
        for b in &self.doc.blocks {
            let (h, inset) = match b {
                Block::Image { height, width, .. } => {
                    let h = height.unwrap_or(120.0);
                    let _ = width;
                    (h, 0.0)
                }
                other => {
                    let font = other.font_size();
                    let indent = other.indent() as f64 * 16.0;
                    let marker_w = if other.marker().is_some() {
                        MARKER_PAD
                    } else {
                        0.0
                    };
                    let inset = indent + marker_w;
                    let text_w = (max_w - inset).max(20.0) as f32;
                    let text = other.inline().map(|i| i.text.as_str()).unwrap_or("");
                    let runs = other.inline().map(|i| i.style_runs()).unwrap_or_default();
                    let h = synui_render::with_text_renderer(|r| {
                        r.measure_styled(text, font, Some(text_w), &runs)
                            .height
                            .max(font as f64 * 1.2)
                    });
                    (h, inset)
                }
            };
            out.push((y, h, inset));
            y += h + BLOCK_GAP;
        }
        *self.layout_cache.borrow_mut() = Some((max_w, out.clone()));
        out
    }

    fn content_height(&self) -> f64 {
        let layout = self.block_layout();
        layout
            .last()
            .map(|(y, h, _)| y + h)
            .unwrap_or(0.0)
            .max(self.text_bounds().height())
    }

    fn active_inline_runs(&self) -> Vec<TextStyleRun> {
        if !self.edit.ime_composing.is_empty() {
            return Vec::new();
        }
        self.doc
            .get_inline(self.active_block)
            .map(|i| i.style_runs())
            .unwrap_or_default()
    }

    fn delete_selection_rich(&mut self) {
        if !self.has_doc_selection() {
            return;
        }
        let (start, end) = self.ordered_sel();
        if start.block == end.block {
            let r = start.offset.min(end.offset)..start.offset.max(end.offset);
            if let Some(inline) = self.doc.get_inline_mut(start.block) {
                inline.delete_range(r.start, r.end);
                self.active_block = start.block;
                self.sync_edit_from_active();
                self.edit.selection = r.start..r.start;
                self.sel_anchor = DocPos::new(start.block, r.start);
            }
            return;
        }
        let (block, offset) =
            self.doc
                .delete_range(start.block, start.offset, end.block, end.offset);
        self.active_block = block;
        self.sync_edit_from_active();
        self.edit.selection = offset..offset;
        self.sel_anchor = DocPos::new(block, offset);
    }

    /// Focus position (moving end of the selection).
    pub fn focus_pos(&self) -> DocPos {
        DocPos::new(self.active_block, self.edit.selection.end)
    }

    /// Ordered `(start, end)` of the document selection (half-open conceptually for delete).
    pub fn ordered_sel(&self) -> (DocPos, DocPos) {
        // For an intra-block selection, EditCore is also part of RichEdit's
        // public editing surface and may be updated directly by callers. Use
        // its normalized range so the document selection cannot diverge from
        // the visible selection when sel_anchor was not updated alongside it.
        if self.sel_anchor.block == self.active_block {
            let range = self.edit.sel_range();
            return (
                DocPos::new(self.active_block, range.start),
                DocPos::new(self.active_block, range.end),
            );
        }

        // Cross-block selections keep their anchor separately because the
        // active block's EditCore selection is collapsed at the focus.
        let a = self.sel_anchor;
        let f = self.focus_pos();
        if (a.block, a.offset) <= (f.block, f.offset) {
            (a, f)
        } else {
            (f, a)
        }
    }

    pub fn has_cross_block_sel(&self) -> bool {
        self.sel_anchor.block != self.active_block
    }

    pub fn has_doc_selection(&self) -> bool {
        let (s, e) = self.ordered_sel();
        s != e
    }

    fn selected_plain(&self) -> String {
        let (s, e) = self.ordered_sel();
        if s == e {
            return String::new();
        }
        self.doc.plain_slice(s.block, s.offset, e.block, e.offset)
    }

    fn collapse_sel_to_focus(&mut self) {
        let f = self.focus_pos();
        self.sel_anchor = f;
        self.edit.selection = f.offset..f.offset;
    }

    fn sync_anchor_from_intra_edit(&mut self) {
        if self.sel_anchor.block == self.active_block || !self.has_cross_block_sel() {
            self.sel_anchor = DocPos::new(self.active_block, self.edit.selection.start);
        }
    }

    pub fn insert_text(&mut self, s: &str) {
        if s.contains('\n') {
            for (i, part) in s.split('\n').enumerate() {
                if i > 0 {
                    self.split_block_at_caret();
                }
                if !part.is_empty() {
                    self.insert_plain(part);
                }
            }
            return;
        }
        self.insert_plain(s);
    }

    fn insert_plain(&mut self, s: &str) {
        if !self
            .doc
            .blocks
            .get(self.active_block)
            .map(|b| b.is_text())
            .unwrap_or(false)
        {
            // Jump to next text block or insert paragraph after image
            let insert_at = self.active_block + 1;
            self.doc.blocks.insert(insert_at, Block::paragraph(""));
            self.active_block = insert_at;
            self.sync_edit_from_active();
        }
        if self.edit.has_selection() || self.has_cross_block_sel() {
            self.delete_selection_rich();
        }
        let i = self.edit.selection.start;
        if let Some(inline) = self.doc.get_inline_mut(self.active_block) {
            inline.insert(i, s);
            self.edit.text = inline.text.clone();
            let end = i + s.len();
            self.edit.selection = end..end;
            self.sel_anchor = DocPos::new(self.active_block, end);
        }
    }

    fn split_block_at_caret(&mut self) {
        if self.has_doc_selection() {
            self.delete_selection_rich();
        }
        let offset = self.edit.selection.start.min(self.edit.text.len());
        self.doc.split_at(self.active_block, offset);
        self.active_block += 1;
        self.sync_edit_from_active();
        self.edit.selection = 0..0;
        self.sel_anchor = DocPos::new(self.active_block, 0);
    }

    pub fn commit_ime(&mut self, text: &str) {
        if text.is_empty() {
            self.edit.ime_composing.clear();
            return;
        }
        self.history.end_coalesce();
        self.push_undo();
        self.edit.ime_composing.clear();
        self.insert_text(text);
        self.notify_text_changed();
    }

    fn display_text(&self) -> String {
        self.edit.display_with_ime()
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.cursor_blink_ms = 0.0;
    }

    pub fn notify_text_changed(&mut self) {
        self.sync_edit_from_active();
        self.reset_cursor_blink();
        self.ensure_caret_visible();
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
        let layout = self.block_layout();
        let Some(&(y0, _, inset)) = layout.get(self.active_block) else {
            return Rect::new(tb.x0, tb.y0, tb.x0 + 2.0, tb.y0 + FONT as f64);
        };
        let font = self
            .doc
            .blocks
            .get(self.active_block)
            .map(|b| b.font_size())
            .unwrap_or(FONT);
        let text_w = (self.content_width() - inset).max(20.0) as f32;
        let display = self.display_text();
        let caret_idx = if !self.edit.ime_composing.is_empty() {
            self.edit.selection.start.min(self.edit.text.len()) + self.edit.ime_composing.len()
        } else {
            self.edit.selection.end.min(display.len())
        };
        let runs = self.active_inline_runs();
        let caret = synui_render::with_text_renderer(|renderer| {
            renderer.caret_bounds_styled(&display, font, Some(text_w), caret_idx, &runs)
        });
        Rect::new(
            tb.x0 + inset + caret.x0,
            tb.y0 + y0 + caret.y0 - self.scroll_y,
            tb.x0 + inset + caret.x0 + 2.0,
            tb.y0 + y0 + caret.y1 - self.scroll_y,
        )
    }

    fn hit_block_and_index(&self, point: Point) -> (usize, usize) {
        let tb = self.text_bounds();
        let local_y = (point.y - tb.y0 + self.scroll_y).max(0.0);
        let layout = self.block_layout();
        let mut bi = layout.len().saturating_sub(1);
        for (i, &(y0, h, _)) in layout.iter().enumerate() {
            if local_y < y0 + h + BLOCK_GAP * 0.5 {
                bi = i;
                break;
            }
        }
        let inset = layout.get(bi).map(|l| l.2).unwrap_or(0.0);
        let font = self
            .doc
            .blocks
            .get(bi)
            .map(|b| b.font_size())
            .unwrap_or(FONT);
        let text = self
            .doc
            .get_inline(bi)
            .map(|i| i.text.as_str())
            .unwrap_or("");
        let runs = self
            .doc
            .get_inline(bi)
            .map(|i| i.style_runs())
            .unwrap_or_default();
        let y0 = layout.get(bi).map(|l| l.0).unwrap_or(0.0);
        let text_w = (self.content_width() - inset).max(20.0) as f32;
        let local_x = (point.x - tb.x0 - inset).max(0.0);
        let local_ty = (local_y - y0).max(0.0);
        let idx = synui_render::with_text_renderer(|renderer| {
            renderer
                .hit_test_point_styled(text, font, Some(text_w), local_x, local_ty, &runs)
                .min(text.len())
        });
        (bi, idx)
    }

    fn ensure_caret_visible(&mut self) {
        let tb = self.text_bounds();
        let layout = self.block_layout();
        let Some(&(y0, h, inset)) = layout.get(self.active_block) else {
            return;
        };
        let font = self
            .doc
            .blocks
            .get(self.active_block)
            .map(|b| b.font_size())
            .unwrap_or(FONT);
        let text_w = (self.content_width() - inset).max(20.0) as f32;
        let text = self.edit.text.as_str();
        let runs = self.active_inline_runs();
        let end = self.edit.selection.end;
        let caret = synui_render::with_text_renderer(|renderer| {
            renderer.caret_bounds_styled(text, font, Some(text_w), end, &runs)
        });
        let caret_top = y0 + caret.y0;
        let caret_bot = y0 + caret.y1.max(h.min(font as f64));
        let view_h = tb.height();
        if caret_bot - self.scroll_y > view_h {
            self.scroll_y = caret_bot - view_h;
        }
        if caret_top - self.scroll_y < 0.0 {
            self.scroll_y = caret_top;
        }
        let max_scroll = (self.content_height() - view_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    fn activate_block(&mut self, index: usize, offset: usize) {
        self.flush_edit_to_doc();
        self.active_block = index.min(self.doc.blocks.len().saturating_sub(1));
        if !self
            .doc
            .blocks
            .get(self.active_block)
            .map(|b| b.is_text())
            .unwrap_or(false)
        {
            // Prefer nearest text block
            if let Some(i) = self.doc.blocks.iter().position(|b| b.is_text()) {
                self.active_block = i;
            }
        }
        self.sync_edit_from_active();
        let len = self.edit.text.len();
        let o = offset.min(len);
        self.edit.selection = o..o;
        self.sel_anchor = DocPos::new(self.active_block, o);
    }

    /// Move focus to `index`/`offset`, optionally extending selection from `sel_anchor`.
    fn set_focus_block(&mut self, index: usize, offset: usize, extend: bool) {
        if !extend {
            self.activate_block(index, offset);
            return;
        }
        if !self.edit.ime_composing.is_empty() {
            return;
        }
        self.flush_edit_to_doc();
        let mut bi = index.min(self.doc.blocks.len().saturating_sub(1));
        if !self
            .doc
            .blocks
            .get(bi)
            .map(|b| b.is_text())
            .unwrap_or(false)
        {
            // Skip images when extending.
            if let Some(i) = self
                .doc
                .blocks
                .iter()
                .enumerate()
                .skip(bi)
                .find_map(|(i, b)| b.is_text().then_some(i))
                .or_else(|| {
                    self.doc
                        .blocks
                        .iter()
                        .enumerate()
                        .take(bi.saturating_add(1))
                        .rev()
                        .find_map(|(i, b)| b.is_text().then_some(i))
                })
            {
                bi = i;
            } else {
                return;
            }
        }
        self.active_block = bi;
        self.sync_edit_from_active();
        let len = self.edit.text.len();
        let o = offset.min(len);
        if self.sel_anchor.block == bi {
            // Intra-block: EditCore selection keeps anchor..focus.
            self.edit.selection = self.sel_anchor.offset.min(len)..o;
        } else {
            // Cross-block: caret at focus end; highlight via sel_anchor + focus_pos.
            self.edit.selection = o..o;
        }
    }

    pub fn toggle_bold(&mut self) {
        if !self.has_doc_selection() {
            return;
        }
        self.history.end_coalesce();
        if self.has_cross_block_sel() {
            // Format each text block's selected slice.
            self.push_undo();
            let (start, end) = self.ordered_sel();
            for bi in start.block..=end.block {
                if let Some(inline) = self.doc.get_inline_mut(bi) {
                    let a = if bi == start.block {
                        start.offset.min(inline.text.len())
                    } else {
                        0
                    };
                    let b = if bi == end.block {
                        end.offset.min(inline.text.len())
                    } else {
                        inline.text.len()
                    };
                    if a < b {
                        inline.toggle_bold(a..b);
                    }
                }
            }
            self.sync_edit_from_active();
            return;
        }
        self.push_undo();
        let r = self.edit.sel_range();
        if let Some(inline) = self.doc.get_inline_mut(self.active_block) {
            inline.toggle_bold(r);
        }
    }

    pub fn toggle_italic(&mut self) {
        if !self.has_doc_selection() {
            return;
        }
        self.history.end_coalesce();
        self.push_undo();
        if self.has_cross_block_sel() {
            let (start, end) = self.ordered_sel();
            for bi in start.block..=end.block {
                if let Some(inline) = self.doc.get_inline_mut(bi) {
                    let a = if bi == start.block {
                        start.offset.min(inline.text.len())
                    } else {
                        0
                    };
                    let b = if bi == end.block {
                        end.offset.min(inline.text.len())
                    } else {
                        inline.text.len()
                    };
                    if a < b {
                        inline.toggle_italic(a..b);
                    }
                }
            }
            self.sync_edit_from_active();
            return;
        }
        let r = self.edit.sel_range();
        if let Some(inline) = self.doc.get_inline_mut(self.active_block) {
            inline.toggle_italic(r);
        }
    }

    pub fn toggle_underline(&mut self) {
        if !self.has_doc_selection() {
            return;
        }
        self.history.end_coalesce();
        self.push_undo();
        if self.has_cross_block_sel() {
            let (start, end) = self.ordered_sel();
            for bi in start.block..=end.block {
                if let Some(inline) = self.doc.get_inline_mut(bi) {
                    let a = if bi == start.block {
                        start.offset.min(inline.text.len())
                    } else {
                        0
                    };
                    let b = if bi == end.block {
                        end.offset.min(inline.text.len())
                    } else {
                        inline.text.len()
                    };
                    if a < b {
                        inline.toggle_underline(a..b);
                    }
                }
            }
            self.sync_edit_from_active();
            return;
        }
        let r = self.edit.sel_range();
        if let Some(inline) = self.doc.get_inline_mut(self.active_block) {
            inline.toggle_underline(r);
        }
    }

    fn backspace_rich(&mut self) {
        if self.has_doc_selection() {
            self.delete_selection_rich();
            return;
        }
        if self.edit.selection.start > 0 {
            let start =
                crate::edit_core::prev_char_boundary(&self.edit.text, self.edit.selection.start);
            if let Some(inline) = self.doc.get_inline_mut(self.active_block) {
                inline.delete_range(start, self.edit.selection.start);
                self.edit.text = inline.text.clone();
                self.edit.set_caret(start);
                self.sel_anchor = DocPos::new(self.active_block, start);
            }
            return;
        }
        // At start of block: merge / outdent
        let idx = self.active_block;
        if let Some(caret) = self.doc.merge_with_previous(idx) {
            self.active_block = idx.saturating_sub(1);
            self.sync_edit_from_active();
            self.edit.set_caret(caret);
            self.sel_anchor = DocPos::new(self.active_block, caret);
        }
    }

    fn delete_forward_rich(&mut self) {
        if self.has_doc_selection() {
            self.delete_selection_rich();
            return;
        }
        if self.edit.selection.end < self.edit.text.len() {
            let end =
                crate::edit_core::next_char_boundary(&self.edit.text, self.edit.selection.end);
            if let Some(inline) = self.doc.get_inline_mut(self.active_block) {
                inline.delete_range(self.edit.selection.end, end);
                self.edit.text = inline.text.clone();
            }
            return;
        }
        // End of block: merge next into this
        let next = self.active_block + 1;
        if next < self.doc.blocks.len() {
            if let Some(caret) = self.doc.merge_with_previous(next) {
                let _ = caret;
                self.sync_edit_from_active();
                self.sel_anchor = DocPos::new(self.active_block, self.edit.selection.start);
            }
        }
    }

    fn move_to_adjacent_block(&mut self, dir: i32, shift: bool) {
        let target = if dir < 0 {
            self.active_block.checked_sub(1)
        } else if self.active_block + 1 < self.doc.blocks.len() {
            Some(self.active_block + 1)
        } else {
            None
        };
        let Some(ti) = target else {
            return;
        };
        if !self.doc.blocks[ti].is_text() {
            // skip images
            if dir < 0 && ti > 0 {
                self.active_block = ti;
                self.move_to_adjacent_block(dir, shift);
            } else if dir > 0 && ti + 1 < self.doc.blocks.len() {
                self.active_block = ti;
                self.move_to_adjacent_block(dir, shift);
            }
            return;
        }
        let offset = if dir < 0 {
            self.doc.get_inline(ti).map(|i| i.text.len()).unwrap_or(0)
        } else {
            0
        };
        self.set_focus_block(ti, offset, shift);
    }

    /// Byte range to highlight inside block `bi`, if any.
    fn selection_range_for_block(
        &self,
        bi: usize,
        text_len: usize,
    ) -> Option<std::ops::Range<usize>> {
        if !self.has_doc_selection() {
            return None;
        }
        let (start, end) = self.ordered_sel();
        if bi < start.block || bi > end.block {
            return None;
        }
        if !self
            .doc
            .blocks
            .get(bi)
            .map(|b| b.is_text())
            .unwrap_or(false)
        {
            return None;
        }
        let a = if bi == start.block {
            start.offset.min(text_len)
        } else {
            0
        };
        let b = if bi == end.block {
            end.offset.min(text_len)
        } else {
            text_len
        };
        if a < b {
            Some(a..b)
        } else {
            None
        }
    }
}

impl Default for RichEdit {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for RichEdit {
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
            flex_grow: 1.0,
            flex_shrink: 1.0,
            width: Dimension::Fill,
            // Fill the pane like DataTable so tab switches do not reflow chrome labels.
            height: Dimension::Fill,
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
        let width = if constraints.max_width.is_finite() {
            constraints.max_width as f64
        } else {
            320.0
        };
        let height = if constraints.max_height.is_finite() {
            constraints.max_height as f64
        } else {
            180.0
        };
        Size::new(width, height.max(120.0))
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
        let tb = self.text_bounds();
        let layout = self.block_layout();
        let max_w = self.content_width();

        if let Some(text) = ctx.text.as_deref_mut() {
            for (i, b) in self.doc.blocks.iter().enumerate() {
                let Some(&(y0, h, inset)) = layout.get(i) else {
                    continue;
                };
                let top = tb.y0 + y0 - self.scroll_y;
                if top + h < tb.y0 - 20.0 || top > tb.y1 + 20.0 {
                    continue;
                }
                match b {
                    Block::Image {
                        id,
                        fit,
                        width,
                        height,
                    } => {
                        let w = width.unwrap_or(max_w).min(max_w);
                        let hh = height.unwrap_or(120.0);
                        let img_bounds = Rect::new(tb.x0, top, tb.x0 + w, top + hh);
                        if let Some(images) = ctx.images.as_deref_mut() {
                            let _ = images.paint_image(scene, id, img_bounds, *fit);
                        } else {
                            paint_simple_rect(scene, img_bounds, ctx.theme.surface_variant, 4.0);
                            text.paint_text(
                                scene,
                                &format!("[{id}]"),
                                12.0,
                                ctx.theme.on_surface,
                                img_bounds,
                                Some(w as f32),
                                TextAlign::Start,
                            );
                        }
                    }
                    other => {
                        let font = other.font_size();
                        let marker = other.marker();
                        if let Some(ref m) = marker {
                            let indent = other.indent() as f64 * 16.0;
                            let mb =
                                Rect::new(tb.x0 + indent, top, tb.x0 + inset, top + font as f64);
                            text.paint_text(
                                scene,
                                m,
                                font,
                                ctx.theme.on_surface,
                                mb,
                                Some(MARKER_PAD as f32),
                                TextAlign::Start,
                            );
                        }
                        let is_active = i == self.active_block && self.focused;
                        let display = if is_active {
                            self.display_text()
                        } else {
                            other
                                .inline()
                                .map(|inl| inl.text.clone())
                                .unwrap_or_default()
                        };
                        let runs: Vec<TextStyleRun> = if is_active {
                            self.active_inline_runs()
                        } else {
                            other
                                .inline()
                                .map(|inl| inl.style_runs())
                                .unwrap_or_default()
                        };
                        let text_w = (max_w - inset).max(20.0) as f32;
                        // Tight bounds = measured line height so paint top-aligns with
                        // selection/hit-test (no vertical centering into +40 padding).
                        let paint_bounds = Rect::new(
                            tb.x0 + inset,
                            top,
                            tb.x1 - 10.0,
                            top + h.max(font as f64 * 1.2),
                        );
                        let sel_range = self.selection_range_for_block(i, display.len());
                        if self.focused && self.edit.ime_composing.is_empty() {
                            if let Some(range) = sel_range {
                                for rect in text.selection_rects_styled(
                                    &display,
                                    font,
                                    Some(text_w),
                                    range,
                                    &runs,
                                ) {
                                    let highlight = Rect::new(
                                        paint_bounds.x0 + rect.x0,
                                        paint_bounds.y0 + rect.y0,
                                        paint_bounds.x0 + rect.x1,
                                        paint_bounds.y0 + rect.y1,
                                    );
                                    let hl = ctx.theme.selection_fill();
                                    paint_simple_rect(scene, highlight, hl, 2.0);
                                }
                            }
                        }
                        if !display.is_empty() {
                            text.paint_text_styled(
                                scene,
                                &display,
                                font,
                                ctx.theme.on_surface,
                                paint_bounds,
                                Some(text_w),
                                TextAlign::Start,
                                &runs,
                            );
                        }
                        if is_active && self.cursor_visible && !self.has_doc_selection() {
                            let caret_idx = if !self.edit.ime_composing.is_empty() {
                                self.edit.selection.start.min(self.edit.text.len())
                                    + self.edit.ime_composing.len()
                            } else {
                                self.edit.selection.end.min(display.len())
                            };
                            let caret = text.caret_rect_styled(
                                &display,
                                font,
                                Some(text_w),
                                caret_idx,
                                &runs,
                            );
                            let caret_line = Line::new(
                                (paint_bounds.x0 + caret.x0, paint_bounds.y0 + caret.y0 + 1.0),
                                (paint_bounds.x0 + caret.x0, paint_bounds.y0 + caret.y1 - 1.0),
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
                }
            }
        }
        let content_h = self.content_height();
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
        let content_h = || self.content_height();
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                ..
            } if self.bounds.contains(*position) => {
                let ch = content_h();
                if let Some(y) =
                    self.scrollbar
                        .pointer_down(*position, self.bounds, self.scroll_y, ch)
                {
                    let max_scroll = (ch - self.text_bounds().height()).max(0.0);
                    self.scroll_y = y.clamp(0.0, max_scroll);
                    ctx.grab_pointer();
                    return EventResult::HANDLED;
                }
                let (bi, idx) = self.hit_block_and_index(*position);
                let now = std::time::Instant::now();
                const DOUBLE_CLICK_MS: u128 = 400;
                const DOUBLE_CLICK_SLOP: f64 = 6.0;
                let near_prev = (position.x - self.last_click_pos.x).abs() <= DOUBLE_CLICK_SLOP
                    && (position.y - self.last_click_pos.y).abs() <= DOUBLE_CLICK_SLOP;
                let within_window = self
                    .last_click_at
                    .is_some_and(|t| now.duration_since(t).as_millis() < DOUBLE_CLICK_MS);
                if within_window && near_prev {
                    self.click_count = self.click_count.saturating_add(1);
                } else {
                    self.click_count = 1;
                }
                self.last_click_at = Some(now);
                self.last_click_pos = *position;
                self.activate_block(bi, idx);
                if self.click_count >= 2 {
                    self.edit.select_word_at(idx);
                    // activate_block left sel_anchor at the click; sync to the word
                    // start so paint/ordered_sel stay consistent.
                    let range = self.edit.sel_range();
                    self.sel_anchor = DocPos::new(self.active_block, range.start);
                    // Do not arm drag-select: a 1px PointerMove would call
                    // set_focus_block(extend) from the mid-word click and collapse
                    // the just-selected word back to a caret.
                    self.dragging_select = false;
                } else {
                    self.dragging_select = true;
                }
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } => {
                let ch = content_h();
                if let Some(y) = self.scrollbar.pointer_move(*position, self.bounds, ch) {
                    let max_scroll = (ch - self.text_bounds().height()).max(0.0);
                    self.scroll_y = y.clamp(0.0, max_scroll);
                    return EventResult::HANDLED;
                }
                if self.dragging_select {
                    let (bi, idx) = self.hit_block_and_index(*position);
                    self.set_focus_block(bi, idx, true);
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
            Event::Scroll {
                position, delta_y, ..
            } if self.bounds.contains(*position) || self.focused => {
                let tb = self.text_bounds();
                let ch = content_h();
                let max_scroll = (ch - tb.height()).max(0.0);
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
                self.edit.ime_composing.clear();
                self.flush_edit_to_doc();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Character(c),
                ctrl: true,
                shift,
                ..
            } => match c.as_str() {
                "a" | "A" => {
                    if let Some(last) = self
                        .doc
                        .blocks
                        .iter()
                        .enumerate()
                        .rev()
                        .find_map(|(i, b)| b.is_text().then_some(i))
                    {
                        let first = self
                            .doc
                            .blocks
                            .iter()
                            .position(|b| b.is_text())
                            .unwrap_or(0);
                        self.sel_anchor = DocPos::new(first, 0);
                        let len = self.doc.get_inline(last).map(|i| i.text.len()).unwrap_or(0);
                        self.set_focus_block(last, len, true);
                    } else {
                        self.edit.select_all();
                        self.sel_anchor = DocPos::new(self.active_block, 0);
                    }
                    self.reset_cursor_blink();
                    EventResult::HANDLED
                }
                "b" | "B" => {
                    self.toggle_bold();
                    self.reset_cursor_blink();
                    EventResult::HANDLED
                }
                "i" | "I" => {
                    self.toggle_italic();
                    self.reset_cursor_blink();
                    EventResult::HANDLED
                }
                "u" | "U" => {
                    self.toggle_underline();
                    self.reset_cursor_blink();
                    EventResult::HANDLED
                }
                "c" | "C" => {
                    self.copy_selection_to_clipboard();
                    EventResult::HANDLED
                }
                "x" | "X" => {
                    let plain = self.selected_plain();
                    if !plain.is_empty() {
                        self.history.end_coalesce();
                        self.push_undo();
                        self.copy_selection_to_clipboard();
                        self.delete_selection_rich();
                    }
                    EventResult::HANDLED
                }
                "v" | "V" => {
                    self.history.end_coalesce();
                    if let Some(json) = clipboard_get_rich() {
                        if let Ok(doc) = BlockDoc::from_clipboard_json(&json) {
                            self.push_undo();
                            self.insert_doc(doc);
                            self.ensure_caret_visible();
                            return EventResult::HANDLED;
                        }
                    }
                    let paste = clipboard_get();
                    if !paste.is_empty() {
                        self.push_undo();
                        self.insert_text(&paste);
                        self.ensure_caret_visible();
                    }
                    EventResult::HANDLED
                }
                "z" | "Z" if *shift => {
                    self.flush_edit_to_doc();
                    if let Some(next) = self.history.redo(self.doc.clone()) {
                        self.set_doc(next);
                    }
                    EventResult::HANDLED
                }
                "y" | "Y" => {
                    self.flush_edit_to_doc();
                    if let Some(next) = self.history.redo(self.doc.clone()) {
                        self.set_doc(next);
                    }
                    EventResult::HANDLED
                }
                "z" | "Z" => {
                    self.flush_edit_to_doc();
                    if let Some(prev) = self.history.undo(self.doc.clone()) {
                        self.set_doc(prev);
                    }
                    EventResult::HANDLED
                }
                _ => EventResult::IGNORED,
            },
            Event::KeyDown {
                key: Key::Named(NamedKey::Tab),
                shift,
                ..
            } => {
                self.history.end_coalesce();
                self.push_undo();
                if *shift {
                    self.doc.outdent_list(self.active_block);
                } else {
                    self.doc.indent_list(self.active_block);
                }
                self.sync_edit_from_active();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Character(c),
                ctrl: false,
                ..
            } if self.edit.ime_composing.is_empty() => {
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
            } if self.edit.ime_composing.is_empty() => {
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
            } if self.edit.ime_composing.is_empty() => {
                self.history.end_coalesce();
                self.push_undo();
                self.split_block_at_caret();
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Backspace),
                ..
            } if self.edit.ime_composing.is_empty() => {
                self.history.end_coalesce();
                self.push_undo();
                self.backspace_rich();
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Delete),
                ..
            } if self.edit.ime_composing.is_empty() => {
                self.history.end_coalesce();
                self.push_undo();
                self.delete_forward_rich();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowLeft),
                shift,
                ctrl,
                ..
            } => {
                if !*shift && !*ctrl && self.edit.selection.start == 0 && !self.has_doc_selection()
                {
                    self.move_to_adjacent_block(-1, *shift);
                } else if *ctrl {
                    self.edit.move_word_left(*shift);
                    self.sync_anchor_from_intra_edit();
                    if !*shift {
                        self.collapse_sel_to_focus();
                    }
                } else if !*shift
                    && self.has_doc_selection()
                    && (self.has_cross_block_sel() || self.edit.has_selection())
                {
                    let (start, _) = self.ordered_sel();
                    self.set_focus_block(start.block, start.offset, false);
                } else {
                    self.edit.move_left(*shift);
                    self.sync_anchor_from_intra_edit();
                    if !*shift {
                        self.collapse_sel_to_focus();
                    } else if self.edit.selection.start == 0
                        && self.edit.selection.end == 0
                        && self.active_block > 0
                    {
                        self.move_to_adjacent_block(-1, true);
                    }
                }
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowRight),
                shift,
                ctrl,
                ..
            } => {
                let at_end =
                    self.edit.selection.end >= self.edit.text.len() && !self.has_doc_selection();
                if !*shift && !*ctrl && at_end {
                    self.move_to_adjacent_block(1, *shift);
                } else if *ctrl {
                    self.edit.move_word_right(*shift);
                    self.sync_anchor_from_intra_edit();
                    if !*shift {
                        self.collapse_sel_to_focus();
                    }
                } else if !*shift
                    && self.has_doc_selection()
                    && (self.has_cross_block_sel() || self.edit.has_selection())
                {
                    let (_, end) = self.ordered_sel();
                    self.set_focus_block(end.block, end.offset, false);
                } else {
                    self.edit.move_right(*shift);
                    self.sync_anchor_from_intra_edit();
                    if !*shift {
                        self.collapse_sel_to_focus();
                    } else if self.edit.selection.end >= self.edit.text.len()
                        && self.active_block + 1 < self.doc.blocks.len()
                    {
                        self.move_to_adjacent_block(1, true);
                    }
                }
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowUp),
                shift,
                ..
            } => {
                let before = self.edit.selection.end;
                self.edit.move_vertical(-1, *shift);
                self.sync_anchor_from_intra_edit();
                if self.edit.selection.end == before {
                    self.move_to_adjacent_block(-1, *shift);
                } else if !*shift {
                    self.collapse_sel_to_focus();
                }
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowDown),
                shift,
                ..
            } => {
                let before = self.edit.selection.end;
                self.edit.move_vertical(1, *shift);
                self.sync_anchor_from_intra_edit();
                if self.edit.selection.end == before {
                    self.move_to_adjacent_block(1, *shift);
                } else if !*shift {
                    self.collapse_sel_to_focus();
                }
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Home),
                shift,
                ..
            } => {
                self.edit.move_line_start(*shift);
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::End),
                shift,
                ..
            } => {
                self.edit.move_line_end(*shift);
                self.ensure_caret_visible();
                self.reset_cursor_blink();
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        let (start, end) = self.ordered_sel();
        let (value, sel) = if start.block == end.block {
            let text = self
                .doc
                .get_inline(start.block)
                .map(|i| i.text.clone())
                .unwrap_or_default();
            let a = start.offset.min(end.offset);
            let b = start.offset.max(end.offset);
            (text, a..b)
        } else {
            let a = self.doc.plain_byte_offset(start.block, start.offset);
            let b = self.doc.plain_byte_offset(end.block, end.offset);
            (self.doc.plain_text(), a.min(b)..a.max(b))
        };
        A11yProps {
            role: A11yRole::TextInput,
            label: Some("RichEdit".into()),
            value: Some(value),
            text_selection: Some(sel),
            focusable: true,
            actions: vec![A11yAction::Focus, A11yAction::SetValue],
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn hit_test_prefer_self(&self, point: Point) -> bool {
        VScrollbarState::hit(self.bounds, self.scroll_y, self.content_height(), point)
            || self.scrollbar.is_dragging()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synui_core::{Block, RichDoc};

    #[test]
    fn single_click_places_caret_without_word_select() {
        use synui_core::{Event, EventCtx, Point, PointerButton, Widget};

        let mut edit = RichEdit::with_text("Hello RichEdit world");
        edit.set_bounds(Rect::new(0.0, 0.0, 400.0, 200.0));
        edit.focused = true;
        let path = [edit.id()];
        let mut ctx = EventCtx::new(edit.id(), &path, Default::default());
        let pos = Point::new(24.0, 20.0);

        assert!(
            edit.event(
                &mut ctx,
                &Event::PointerDown {
                    id: 0,
                    position: pos,
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                },
            )
            .handled
        );
        assert!(
            !edit.edit.has_selection(),
            "single click must place a caret, got {:?}",
            edit.edit.sel_range()
        );
        assert!(edit.dragging_select);

        // A later click after the double-click window must also stay a caret,
        // even if no frame ticks advanced (the old dt-based timer bug).
        edit.last_click_at =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(800));
        let _ = edit.event(
            &mut ctx,
            &Event::PointerUp {
                id: 0,
                position: pos,
                button: PointerButton::Primary,
            },
        );
        assert!(
            edit.event(
                &mut ctx,
                &Event::PointerDown {
                    id: 0,
                    position: pos,
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                },
            )
            .handled
        );
        assert!(
            !edit.edit.has_selection(),
            "click after timeout must not word-select, got {:?}",
            edit.edit.sel_range()
        );
        assert_eq!(edit.click_count, 1);
    }

    #[test]
    fn double_click_word_selection_survives_tiny_pointer_move() {
        use synui_core::{Event, EventCtx, Point, PointerButton, Widget};

        let mut edit = RichEdit::with_text("Hello RichEdit world");
        edit.set_bounds(Rect::new(0.0, 0.0, 400.0, 200.0));
        edit.focused = true;
        let path = [edit.id()];
        let mut ctx = EventCtx::new(edit.id(), &path, Default::default());

        let pos = Point::new(24.0, 20.0);
        assert!(
            edit.event(
                &mut ctx,
                &Event::PointerDown {
                    id: 0,
                    position: pos,
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                },
            )
            .handled
        );
        let _ = edit.event(
            &mut ctx,
            &Event::PointerUp {
                id: 0,
                position: pos,
                button: PointerButton::Primary,
            },
        );

        // Second press inside the double-click window (wall clock).
        assert!(
            edit.event(
                &mut ctx,
                &Event::PointerDown {
                    id: 0,
                    position: pos,
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                },
            )
            .handled
        );
        assert!(
            edit.edit.has_selection(),
            "double-click should select a word, got {:?}",
            edit.edit.sel_range()
        );
        let selected = edit.edit.sel_range();

        let _ = edit.event(
            &mut ctx,
            &Event::PointerMove {
                id: 0,
                position: Point::new(pos.x + 1.0, pos.y + 1.0),
            },
        );
        assert_eq!(
            edit.edit.sel_range(),
            selected,
            "tiny move after double-click must keep word selection"
        );
        assert!(!edit.dragging_select);
    }

    #[test]
    fn cross_block_selection_delete_and_copy() {
        let mut edit = RichEdit::with_doc(BlockDoc::from_blocks(vec![
            Block::paragraph("hello"),
            Block::paragraph("world"),
        ]));
        edit.sel_anchor = DocPos::new(0, 2); // "llo"
        edit.set_focus_block(1, 3, true); // through "wor"
        assert!(edit.has_cross_block_sel());
        assert_eq!(edit.selected_plain(), "llo\nwor");
        edit.delete_selection_rich();
        assert_eq!(edit.doc.blocks.len(), 1);
        assert!(matches!(&edit.doc.blocks[0], Block::Paragraph(p) if p.text == "held"));
        assert!(!edit.has_doc_selection());
    }

    #[test]
    fn shift_adjacent_extends_selection() {
        let mut edit = RichEdit::with_doc(BlockDoc::from_blocks(vec![
            Block::paragraph("aa"),
            Block::paragraph("bb"),
        ]));
        edit.activate_block(0, 2);
        edit.move_to_adjacent_block(1, true);
        assert!(edit.has_cross_block_sel());
        assert_eq!(edit.selected_plain(), "\n");
        // Focus at start of block 1; anchor at end of block 0 → empty middle newline only.
        // Extend to end of block 1:
        edit.set_focus_block(1, 2, true);
        assert_eq!(edit.selected_plain(), "\nbb");
    }

    #[test]
    fn rich_clipboard_preserves_font_size_and_bold() {
        let mut src = RichDoc::from_plain("Big");
        src.apply_font_size(0..3, Some(240));
        src.toggle_bold(0..3);
        let mut edit = RichEdit::with_doc(BlockDoc::from_blocks(vec![Block::Paragraph(src)]));
        edit.edit.selection = 0..3;
        edit.sel_anchor = DocPos::new(0, 0);
        edit.copy_selection_to_clipboard();
        let mut dest = RichEdit::with_text("");
        if let Some(json) = crate::clipboard_get_rich() {
            let doc = BlockDoc::from_clipboard_json(&json).unwrap();
            dest.insert_doc(doc);
        }
        let inl = dest.doc.get_inline(0).unwrap();
        assert_eq!(inl.text, "Big");
        assert!(inl.spans.iter().any(|s| s.bold && s.font_size == Some(240)));
    }

    #[test]
    fn paste_heading_into_paragraph_keeps_large_size() {
        let mut edit = RichEdit::with_doc(BlockDoc::from_blocks(vec![
            Block::heading(1, "Heading"),
            Block::paragraph("body"),
        ]));
        edit.sel_anchor = DocPos::new(0, 0);
        edit.set_focus_block(0, 7, true);
        edit.copy_selection_to_clipboard();
        edit.activate_block(1, 4);
        if let Some(json) = crate::clipboard_get_rich() {
            let doc = BlockDoc::from_clipboard_json(&json).unwrap();
            edit.insert_doc(doc);
        }
        let tenths = (Block::heading(1, "").font_size() * 10.0).round() as u32;
        let inl = edit.doc.get_inline(1).unwrap();
        assert!(inl.text.contains("Heading"));
        assert!(
            inl.spans.iter().any(|s| s.font_size == Some(tenths)),
            "expected baked heading size {tenths}, spans={:?}",
            inl.spans
        );
    }

    #[test]
    fn typing_coalesce_one_undo() {
        let mut edit = RichEdit::with_text("");
        for ch in ["a", "b", "c"] {
            edit.history.begin_coalesce();
            edit.push_undo();
            edit.insert_text(ch);
        }
        assert_eq!(edit.doc.plain_text(), "abc");
        edit.flush_edit_to_doc();
        if let Some(prev) = edit.history.undo(edit.doc.clone()) {
            edit.set_doc(prev);
        }
        assert_eq!(edit.doc.plain_text(), "");
    }
}
