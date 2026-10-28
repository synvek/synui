//! Block-level rich document model (Phase 20).
//!
//! Flat list of paragraphs, headings, list items, and images — not a nested tree.

use crate::rich_text::RichDoc;
use crate::ImageFit;
use std::fmt::Write as _;

/// Inline text + spans within a single block (reuse flat [`RichDoc`] storage).
pub type InlineBlock = RichDoc;

/// One document block in a [`BlockDoc`].
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(InlineBlock),
    Heading {
        /// 1..=3
        level: u8,
        inline: InlineBlock,
    },
    BulletItem {
        indent: u8,
        inline: InlineBlock,
    },
    OrderedItem {
        indent: u8,
        number: u32,
        inline: InlineBlock,
    },
    Image {
        id: String,
        width: Option<f64>,
        height: Option<f64>,
        fit: ImageFit,
    },
}

impl Block {
    pub fn paragraph(text: impl Into<String>) -> Self {
        Block::Paragraph(RichDoc::from_plain(text))
    }

    pub fn heading(level: u8, text: impl Into<String>) -> Self {
        Block::Heading {
            level: level.clamp(1, 3),
            inline: RichDoc::from_plain(text),
        }
    }

    pub fn bullet(indent: u8, text: impl Into<String>) -> Self {
        Block::BulletItem {
            indent,
            inline: RichDoc::from_plain(text),
        }
    }

    pub fn ordered(indent: u8, number: u32, text: impl Into<String>) -> Self {
        Block::OrderedItem {
            indent,
            number,
            inline: RichDoc::from_plain(text),
        }
    }

    pub fn image(id: impl Into<String>) -> Self {
        Block::Image {
            id: id.into(),
            width: None,
            height: Some(120.0),
            fit: ImageFit::Contain,
        }
    }

    pub fn is_text(&self) -> bool {
        !matches!(self, Block::Image { .. })
    }

    pub fn inline(&self) -> Option<&InlineBlock> {
        match self {
            Block::Paragraph(i)
            | Block::Heading { inline: i, .. }
            | Block::BulletItem { inline: i, .. }
            | Block::OrderedItem { inline: i, .. } => Some(i),
            Block::Image { .. } => None,
        }
    }

    pub fn inline_mut(&mut self) -> Option<&mut InlineBlock> {
        match self {
            Block::Paragraph(i)
            | Block::Heading { inline: i, .. }
            | Block::BulletItem { inline: i, .. }
            | Block::OrderedItem { inline: i, .. } => Some(i),
            Block::Image { .. } => None,
        }
    }

    pub fn indent(&self) -> u8 {
        match self {
            Block::BulletItem { indent, .. } | Block::OrderedItem { indent, .. } => *indent,
            _ => 0,
        }
    }

    pub fn set_indent(&mut self, indent: u8) {
        match self {
            Block::BulletItem { indent: i, .. } | Block::OrderedItem { indent: i, .. } => {
                *i = indent.min(8);
            }
            _ => {}
        }
    }

    /// Paint-time list marker (not part of the text buffer).
    pub fn marker(&self) -> Option<String> {
        match self {
            Block::BulletItem { .. } => Some("• ".into()),
            Block::OrderedItem { number, .. } => Some(format!("{number}. ")),
            _ => None,
        }
    }

    pub fn font_size(&self) -> f32 {
        match self {
            Block::Heading { level: 1, .. } => 24.0,
            Block::Heading { level: 2, .. } => 20.0,
            Block::Heading { level: 3, .. } => 17.0,
            _ => 15.0,
        }
    }

    /// Empty paragraph/heading/list with same kind (for Enter split).
    pub fn empty_same_kind(&self) -> Block {
        match self {
            Block::Paragraph(_) => Block::paragraph(""),
            Block::Heading { level, .. } => Block::heading(*level, ""),
            Block::BulletItem { indent, .. } => Block::bullet(*indent, ""),
            Block::OrderedItem { indent, number, .. } => {
                Block::ordered(*indent, number.saturating_add(1), "")
            }
            Block::Image { .. } => Block::paragraph(""),
        }
    }
}

/// Flat block document used by [`RichEdit`](phase 20).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDoc {
    pub blocks: Vec<Block>,
}

impl BlockDoc {
    pub fn new() -> Self {
        Self {
            blocks: vec![Block::paragraph("")],
        }
    }

    pub fn from_blocks(blocks: Vec<Block>) -> Self {
        if blocks.is_empty() {
            Self::new()
        } else {
            Self { blocks }
        }
    }

    /// Migrate a flat [`RichDoc`] by splitting on `\n` into paragraphs (spans per line only
    /// when they fall entirely within that line).
    pub fn from_rich_doc(doc: &RichDoc) -> Self {
        if doc.text.is_empty() {
            return Self::new();
        }
        let mut blocks = Vec::new();
        let mut line_start = 0usize;
        for (i, ch) in doc.text.char_indices() {
            if ch == '\n' {
                let slice = &doc.text[line_start..i];
                let mut inline = RichDoc::from_plain(slice);
                for span in &doc.spans {
                    if span.range.start >= line_start && span.range.end <= i {
                        let mut s = span.clone();
                        s.range = (span.range.start - line_start)..(span.range.end - line_start);
                        inline.spans.push(s);
                    }
                }
                inline.normalize();
                blocks.push(Block::Paragraph(inline));
                line_start = i + ch.len_utf8();
            }
        }
        let slice = &doc.text[line_start..];
        let mut inline = RichDoc::from_plain(slice);
        for span in &doc.spans {
            if span.range.start >= line_start && span.range.end <= doc.text.len() {
                let mut s = span.clone();
                s.range = (span.range.start - line_start)..(span.range.end - line_start);
                inline.spans.push(s);
            }
        }
        inline.normalize();
        blocks.push(Block::Paragraph(inline));
        Self::from_blocks(blocks)
    }

    pub fn from_plain(text: impl Into<String>) -> Self {
        Self::from_rich_doc(&RichDoc::from_plain(text))
    }

    pub fn ensure_nonempty(&mut self) {
        if self.blocks.is_empty() {
            self.blocks.push(Block::paragraph(""));
        }
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            match b {
                Block::Image { id, .. } => {
                    let _ = write!(out, "[image:{id}]");
                }
                other => {
                    if let Some(inline) = other.inline() {
                        out.push_str(&inline.text);
                    }
                }
            }
        }
        out
    }

    /// UTF-8 byte offset of `(block, offset)` inside [`Self::plain_text`].
    pub fn plain_byte_offset(&self, block: usize, offset: usize) -> usize {
        let mut off = 0usize;
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                off += 1; // newline separator
            }
            let piece_len = match b {
                Block::Image { id, .. } => format!("[image:{id}]").len(),
                other => other.inline().map(|inl| inl.text.len()).unwrap_or(0),
            };
            if i == block {
                return off + offset.min(piece_len);
            }
            off += piece_len;
        }
        off
    }

    pub fn get_inline(&self, index: usize) -> Option<&InlineBlock> {
        self.blocks.get(index).and_then(Block::inline)
    }

    pub fn get_inline_mut(&mut self, index: usize) -> Option<&mut InlineBlock> {
        self.blocks.get_mut(index).and_then(Block::inline_mut)
    }

    /// Split text block at byte offset: left keeps prefix, right gets suffix (same kind).
    pub fn split_at(&mut self, index: usize, offset: usize) {
        self.ensure_nonempty();
        if index >= self.blocks.len() {
            return;
        }
        let Some(inline) = self.blocks[index].inline().cloned() else {
            // Image: insert empty paragraph after
            self.blocks.insert(index + 1, Block::paragraph(""));
            return;
        };
        let offset = offset.min(inline.text.len());
        let left_text = inline.text[..offset].to_string();
        let right_text = inline.text[offset..].to_string();
        let mut left = RichDoc {
            text: left_text,
            spans: Vec::new(),
        };
        let mut right = RichDoc {
            text: right_text,
            spans: Vec::new(),
        };
        for span in &inline.spans {
            if span.range.end <= offset {
                left.spans.push(span.clone());
            } else if span.range.start >= offset {
                let mut s = span.clone();
                s.range = (span.range.start - offset)..(span.range.end - offset);
                right.spans.push(s);
            } else {
                let mut l = span.clone();
                l.range = span.range.start..offset;
                left.spans.push(l);
                let mut r = span.clone();
                r.range = 0..(span.range.end - offset);
                right.spans.push(r);
            }
        }
        left.normalize();
        right.normalize();

        let kind = self.blocks[index].clone();
        let left_block = replace_inline(&kind, left);
        let right_block = match &kind {
            Block::OrderedItem { indent, number, .. } => Block::OrderedItem {
                indent: *indent,
                number: number.saturating_add(1),
                inline: right,
            },
            other => replace_inline(other, right),
        };
        self.blocks[index] = left_block;
        self.blocks.insert(index + 1, right_block);
    }

    /// Merge `index` into previous text block (or delete empty list / outdent).
    pub fn merge_with_previous(&mut self, index: usize) -> Option<usize> {
        if index == 0 || index >= self.blocks.len() {
            return None;
        }
        // Outdent list first when indent > 0
        if self.blocks[index].indent() > 0 {
            let ind = self.blocks[index].indent() - 1;
            self.blocks[index].set_indent(ind);
            return Some(0);
        }
        // Convert list/heading to paragraph when at indent 0 and empty? Prefer merge.
        let prev = index - 1;
        if !self.blocks[prev].is_text() || !self.blocks[index].is_text() {
            // Delete current if empty image neighbor handling: remove block
            if self
                .blocks
                .get(index)
                .and_then(|b| b.inline())
                .map(|i| i.text.is_empty())
                .unwrap_or(false)
            {
                self.blocks.remove(index);
                self.ensure_nonempty();
                return Some(
                    self.blocks
                        .get(prev)
                        .and_then(|b| b.inline())
                        .map(|i| i.text.len())
                        .unwrap_or(0),
                );
            }
            return None;
        }
        let right = self.blocks[index]
            .inline()
            .cloned()
            .unwrap_or_else(RichDoc::new);
        let caret = self.blocks[prev]
            .inline()
            .map(|i| i.text.len())
            .unwrap_or(0);
        if let Some(left) = self.blocks[prev].inline_mut() {
            let base = left.text.len();
            left.text.push_str(&right.text);
            for mut span in right.spans {
                span.range.start += base;
                span.range.end += base;
                left.spans.push(span);
            }
            left.normalize();
        }
        self.blocks.remove(index);
        self.ensure_nonempty();
        Some(caret)
    }

    /// Document position for cross-block selection (Phase 25).
    pub fn plain_slice(
        &self,
        start_block: usize,
        start_off: usize,
        end_block: usize,
        end_off: usize,
    ) -> String {
        if self.blocks.is_empty() {
            return String::new();
        }
        let (sb, so, eb, eo) = if (start_block, start_off) <= (end_block, end_off) {
            (start_block, start_off, end_block, end_off)
        } else {
            (end_block, end_off, start_block, start_off)
        };
        let sb = sb.min(self.blocks.len().saturating_sub(1));
        let eb = eb.min(self.blocks.len().saturating_sub(1));
        if sb == eb {
            let text = self.get_inline(sb).map(|i| i.text.as_str()).unwrap_or("");
            let a = so.min(text.len());
            let b = eo.min(text.len());
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            return text[a..b].to_string();
        }
        let mut out = String::new();
        for bi in sb..=eb {
            let text = self
                .blocks
                .get(bi)
                .and_then(|b| b.inline())
                .map(|i| i.text.as_str())
                .unwrap_or("");
            if bi > sb {
                out.push('\n');
            }
            if !self.blocks.get(bi).map(|b| b.is_text()).unwrap_or(false) {
                // Image / non-text: skip content (atomic; not partially selectable).
                continue;
            }
            let piece = if bi == sb {
                &text[so.min(text.len())..]
            } else if bi == eb {
                &text[..eo.min(text.len())]
            } else {
                text
            };
            out.push_str(piece);
        }
        out
    }

    /// Extract a sub-document with remapped inline spans (styles + font_size).
    pub fn slice_doc(
        &self,
        start_block: usize,
        start_off: usize,
        end_block: usize,
        end_off: usize,
    ) -> BlockDoc {
        if self.blocks.is_empty() {
            return BlockDoc::from_plain("");
        }
        let (sb, so, eb, eo) = if (start_block, start_off) <= (end_block, end_off) {
            (start_block, start_off, end_block, end_off)
        } else {
            (end_block, end_off, start_block, start_off)
        };
        let sb = sb.min(self.blocks.len().saturating_sub(1));
        let eb = eb.min(self.blocks.len().saturating_sub(1));
        let mut blocks = Vec::new();
        for bi in sb..=eb {
            let Some(src) = self.blocks.get(bi) else {
                continue;
            };
            match src {
                Block::Image {
                    id,
                    width,
                    height,
                    fit,
                } if bi > sb && bi < eb => {
                    blocks.push(Block::Image {
                        id: id.clone(),
                        width: *width,
                        height: *height,
                        fit: *fit,
                    });
                }
                Block::Image { .. } => {}
                other => {
                    let Some(inline) = other.inline() else {
                        continue;
                    };
                    let (a, b) = if bi == sb && bi == eb {
                        (so.min(inline.text.len()), eo.min(inline.text.len()))
                    } else if bi == sb {
                        (so.min(inline.text.len()), inline.text.len())
                    } else if bi == eb {
                        (0, eo.min(inline.text.len()))
                    } else {
                        (0, inline.text.len())
                    };
                    let (a, b) = if a <= b { (a, b) } else { (b, a) };
                    let mut sliced = inline.slice(a, b);
                    // Headings store size on the block, not spans. Bake it into the
                    // clipboard payload so paste into a paragraph keeps the large size.
                    if matches!(other, Block::Heading { .. })
                        && !sliced.text.is_empty()
                        && !sliced.spans.iter().any(|span| span.font_size.is_some())
                    {
                        let tenths = (other.font_size() * 10.0).round() as u32;
                        sliced.apply_font_size(0..sliced.text.len(), Some(tenths));
                    }
                    let block = match other {
                        Block::Paragraph(_) => Block::Paragraph(sliced),
                        Block::Heading { level, .. } => Block::Heading {
                            level: *level,
                            inline: sliced,
                        },
                        Block::BulletItem { indent, .. } => Block::BulletItem {
                            indent: *indent,
                            inline: sliced,
                        },
                        Block::OrderedItem { indent, number, .. } => Block::OrderedItem {
                            indent: *indent,
                            number: *number,
                            inline: sliced,
                        },
                        Block::Image { .. } => unreachable!(),
                    };
                    blocks.push(block);
                }
            }
        }
        if blocks.is_empty() {
            BlockDoc::from_plain("")
        } else {
            BlockDoc { blocks }
        }
    }

    /// Serialize for in-app rich clipboard (preserves spans including font_size).
    pub fn to_clipboard_json(&self) -> String {
        let mut out = String::from("{\"v\":1,\"blocks\":[");
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            match b {
                Block::Paragraph(inline) => {
                    out.push_str("{\"t\":\"p\",\"inline\":");
                    out.push_str(&inline.to_json());
                    out.push('}');
                }
                Block::Heading { level, inline } => {
                    out.push_str(&format!("{{\"t\":\"h\",\"level\":{level},\"inline\":"));
                    out.push_str(&inline.to_json());
                    out.push('}');
                }
                Block::BulletItem { indent, inline } => {
                    out.push_str(&format!("{{\"t\":\"ul\",\"indent\":{indent},\"inline\":"));
                    out.push_str(&inline.to_json());
                    out.push('}');
                }
                Block::OrderedItem {
                    indent,
                    number,
                    inline,
                } => {
                    out.push_str(&format!(
                        "{{\"t\":\"ol\",\"indent\":{indent},\"n\":{number},\"inline\":"
                    ));
                    out.push_str(&inline.to_json());
                    out.push('}');
                }
                Block::Image {
                    id,
                    width,
                    height,
                    fit,
                } => {
                    let w = width
                        .map(|v| format!("{v}"))
                        .unwrap_or_else(|| "null".into());
                    let h = height
                        .map(|v| format!("{v}"))
                        .unwrap_or_else(|| "null".into());
                    let fit_s = match fit {
                        ImageFit::Contain => "contain",
                        ImageFit::Cover => "cover",
                        ImageFit::Fill => "fill",
                    };
                    out.push_str(&format!(
                        "{{\"t\":\"img\",\"id\":{},\"w\":{w},\"h\":{h},\"fit\":\"{fit_s}\"}}",
                        clip_json_escape(id)
                    ));
                }
            }
        }
        out.push_str("]}");
        out
    }

    /// Parse [`Self::to_clipboard_json`] output.
    pub fn from_clipboard_json(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if !s.contains("\"v\":1") || !s.contains("\"blocks\"") {
            return Err("not a synui rich clipboard payload".into());
        }
        let Some(arr_start) = s.find("\"blocks\"") else {
            return Err("missing blocks".into());
        };
        let rest = &s[arr_start..];
        let Some(bracket) = rest.find('[') else {
            return Err("missing array".into());
        };
        let mut i = bracket + 1;
        let bytes = rest.as_bytes();
        let mut blocks = Vec::new();
        while i < rest.len() {
            while i < rest.len() && matches!(bytes[i], b' ' | b'\n' | b',' | b'\t') {
                i += 1;
            }
            if i < rest.len() && bytes[i] == b']' {
                break;
            }
            if i >= rest.len() || bytes[i] != b'{' {
                break;
            }
            let end = find_matching_brace(rest, i).ok_or("unbalanced block object")?;
            let obj = &rest[i..=end];
            let t = clip_extract_string(obj, "t").unwrap_or_default();
            let block = match t.as_str() {
                "p" => {
                    let inline = clip_extract_inline_json(obj)?;
                    Block::Paragraph(RichDoc::from_json(&inline)?)
                }
                "h" => {
                    let level = clip_extract_usize(obj, "level").unwrap_or(1) as u8;
                    let inline = clip_extract_inline_json(obj)?;
                    Block::Heading {
                        level: level.clamp(1, 3),
                        inline: RichDoc::from_json(&inline)?,
                    }
                }
                "ul" => {
                    let indent = clip_extract_usize(obj, "indent").unwrap_or(0) as u8;
                    let inline = clip_extract_inline_json(obj)?;
                    Block::BulletItem {
                        indent,
                        inline: RichDoc::from_json(&inline)?,
                    }
                }
                "ol" => {
                    let indent = clip_extract_usize(obj, "indent").unwrap_or(0) as u8;
                    let number = clip_extract_usize(obj, "n").unwrap_or(1) as u32;
                    let inline = clip_extract_inline_json(obj)?;
                    Block::OrderedItem {
                        indent,
                        number,
                        inline: RichDoc::from_json(&inline)?,
                    }
                }
                "img" => {
                    let id = clip_extract_string(obj, "id").unwrap_or_default();
                    Block::Image {
                        id,
                        width: None,
                        height: Some(120.0),
                        fit: ImageFit::Contain,
                    }
                }
                other => return Err(format!("unknown block type {other}")),
            };
            blocks.push(block);
            i = end + 1;
        }
        if blocks.is_empty() {
            Ok(BlockDoc::from_plain(""))
        } else {
            Ok(BlockDoc { blocks })
        }
    }

    /// Delete a half-open document range `[start, end)` across blocks; returns caret
    /// `(block, offset)` after deletion.
    pub fn delete_range(
        &mut self,
        start_block: usize,
        start_off: usize,
        end_block: usize,
        end_off: usize,
    ) -> (usize, usize) {
        self.ensure_nonempty();
        let (mut sb, mut so, mut eb, eo) = if (start_block, start_off) <= (end_block, end_off) {
            (start_block, start_off, end_block, end_off)
        } else {
            (end_block, end_off, start_block, start_off)
        };
        sb = sb.min(self.blocks.len().saturating_sub(1));
        eb = eb.min(self.blocks.len().saturating_sub(1));
        if sb == eb {
            if let Some(inline) = self.get_inline_mut(sb) {
                let a = so.min(inline.text.len());
                let b = eo.min(inline.text.len());
                if a < b {
                    inline.delete_range(a, b);
                }
                return (sb, a);
            }
            return (sb, so);
        }
        // Suffix of end block after eo.
        let end_suffix = self
            .get_inline(eb)
            .map(|i| {
                let o = eo.min(i.text.len());
                let mut tail = RichDoc {
                    text: i.text[o..].to_string(),
                    spans: Vec::new(),
                };
                for span in &i.spans {
                    if span.range.start >= o {
                        let mut s = span.clone();
                        s.range = (span.range.start - o)..(span.range.end - o);
                        tail.spans.push(s);
                    } else if span.range.end > o {
                        let mut s = span.clone();
                        s.range = 0..(span.range.end - o);
                        tail.spans.push(s);
                    }
                }
                tail.normalize();
                tail
            })
            .unwrap_or_else(RichDoc::new);

        // Truncate start block to so, then append end_suffix.
        if let Some(inline) = self.get_inline_mut(sb) {
            let a = so.min(inline.text.len());
            if a < inline.text.len() {
                inline.delete_range(a, inline.text.len());
            }
            let base = inline.text.len();
            inline.text.push_str(&end_suffix.text);
            for mut span in end_suffix.spans {
                span.range.start += base;
                span.range.end += base;
                inline.spans.push(span);
            }
            inline.normalize();
            so = a;
        }

        // Remove middle + end blocks (sb+1 ..= eb).
        for _ in sb + 1..=eb {
            if sb + 1 < self.blocks.len() {
                self.blocks.remove(sb + 1);
            }
        }
        self.ensure_nonempty();
        (sb.min(self.blocks.len().saturating_sub(1)), so)
    }

    pub fn indent_list(&mut self, index: usize) {
        if let Some(b) = self.blocks.get_mut(index) {
            if matches!(b, Block::BulletItem { .. } | Block::OrderedItem { .. }) {
                let i = b.indent().saturating_add(1).min(8);
                b.set_indent(i);
            } else if let Some(inline) = b.inline().cloned() {
                // Promote paragraph to bullet
                *b = Block::BulletItem { indent: 0, inline };
            }
        }
    }

    pub fn outdent_list(&mut self, index: usize) {
        if let Some(b) = self.blocks.get_mut(index) {
            if b.indent() > 0 {
                let i = b.indent() - 1;
                b.set_indent(i);
            } else if matches!(b, Block::BulletItem { .. } | Block::OrderedItem { .. }) {
                if let Some(inline) = b.inline().cloned() {
                    *b = Block::Paragraph(inline);
                }
            }
        }
    }

    /// Export Markdown subset (# / ## / ###, lists, **bold** / *italic*, images).
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            match b {
                Block::Paragraph(inline) => {
                    out.push_str(&inline_to_md(inline));
                }
                Block::Heading { level, inline } => {
                    for _ in 0..*level {
                        out.push('#');
                    }
                    out.push(' ');
                    out.push_str(&inline_to_md(inline));
                }
                Block::BulletItem { indent, inline } => {
                    for _ in 0..*indent {
                        out.push_str("  ");
                    }
                    out.push_str("- ");
                    out.push_str(&inline_to_md(inline));
                }
                Block::OrderedItem {
                    indent,
                    number,
                    inline,
                } => {
                    for _ in 0..*indent {
                        out.push_str("  ");
                    }
                    let _ = write!(out, "{number}. ");
                    out.push_str(&inline_to_md(inline));
                }
                Block::Image { id, .. } => {
                    let _ = write!(out, "![]({id})");
                }
            }
        }
        out
    }

    /// Import Markdown subset; unknown syntax falls into plain paragraphs.
    pub fn from_markdown(md: &str) -> Self {
        let mut blocks = Vec::new();
        for line in md.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("![") {
                if let Some(close) = rest.find("](") {
                    let after = &rest[close + 2..];
                    if let Some(end) = after.find(')') {
                        let id = &after[..end];
                        blocks.push(Block::image(id));
                        continue;
                    }
                }
            }
            let mut indent = 0u8;
            let mut rest = trimmed;
            while rest.starts_with("  ") || rest.starts_with('\t') {
                if rest.starts_with('\t') {
                    rest = &rest[1..];
                } else {
                    rest = &rest[2..];
                }
                indent = indent.saturating_add(1);
            }
            if let Some(t) = rest.strip_prefix("# ") {
                blocks.push(Block::Heading {
                    level: 1,
                    inline: md_inline_to_doc(t),
                });
            } else if let Some(t) = rest.strip_prefix("## ") {
                blocks.push(Block::Heading {
                    level: 2,
                    inline: md_inline_to_doc(t),
                });
            } else if let Some(t) = rest.strip_prefix("### ") {
                blocks.push(Block::Heading {
                    level: 3,
                    inline: md_inline_to_doc(t),
                });
            } else if let Some(t) = rest.strip_prefix("- ") {
                blocks.push(Block::BulletItem {
                    indent,
                    inline: md_inline_to_doc(t),
                });
            } else if let Some((num, t)) = parse_ordered_prefix(rest) {
                blocks.push(Block::OrderedItem {
                    indent,
                    number: num,
                    inline: md_inline_to_doc(t),
                });
            } else {
                blocks.push(Block::Paragraph(md_inline_to_doc(trimmed)));
            }
        }
        Self::from_blocks(blocks)
    }
}

fn replace_inline(kind: &Block, inline: InlineBlock) -> Block {
    match kind {
        Block::Paragraph(_) => Block::Paragraph(inline),
        Block::Heading { level, .. } => Block::Heading {
            level: *level,
            inline,
        },
        Block::BulletItem { indent, .. } => Block::BulletItem {
            indent: *indent,
            inline,
        },
        Block::OrderedItem { indent, number, .. } => Block::OrderedItem {
            indent: *indent,
            number: *number,
            inline,
        },
        Block::Image { .. } => Block::Paragraph(inline),
    }
}

fn parse_ordered_prefix(s: &str) -> Option<(u32, &str)> {
    let mut digits = 0usize;
    for c in s.chars() {
        if c.is_ascii_digit() {
            digits += 1;
        } else {
            break;
        }
    }
    if digits == 0 {
        return None;
    }
    let (num_s, rest) = s.split_at(digits);
    let rest = rest.strip_prefix(". ")?;
    let num: u32 = num_s.parse().ok()?;
    Some((num, rest))
}

fn inline_to_md(inline: &InlineBlock) -> String {
    if inline.spans.is_empty() {
        return inline.text.clone();
    }
    // Emit per-character with active bold/italic (underline ignored in MD subset).
    let chars: Vec<(usize, char)> = inline.text.char_indices().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let (byte_i, _) = chars[i];
        let (bold, italic) = flags_at(inline, byte_i);
        let mut j = i + 1;
        while j < chars.len() {
            let (bj, _) = chars[j];
            let (b2, i2) = flags_at(inline, bj);
            if b2 != bold || i2 != italic {
                break;
            }
            j += 1;
        }
        let end_byte = if j < chars.len() {
            chars[j].0
        } else {
            inline.text.len()
        };
        let slice = &inline.text[byte_i..end_byte];
        match (bold, italic) {
            (true, true) => {
                out.push_str("***");
                out.push_str(slice);
                out.push_str("***");
            }
            (true, false) => {
                out.push_str("**");
                out.push_str(slice);
                out.push_str("**");
            }
            (false, true) => {
                out.push('*');
                out.push_str(slice);
                out.push('*');
            }
            (false, false) => out.push_str(slice),
        }
        i = j;
    }
    out
}

fn flags_at(inline: &InlineBlock, byte: usize) -> (bool, bool) {
    for s in &inline.spans {
        if s.range.start <= byte && byte < s.range.end {
            return (s.bold, s.italic);
        }
    }
    (false, false)
}

fn md_inline_to_doc(s: &str) -> InlineBlock {
    let mut text = String::new();
    let mut spans = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"***") {
            if let Some(end) = find_close(s, i + 3, "***") {
                let start = text.len();
                text.push_str(&s[i + 3..end]);
                let end_t = text.len();
                spans.push(crate::TextSpan {
                    range: start..end_t,
                    bold: true,
                    italic: true,
                    underline: false,
                    font_size: None,
                    color: None,
                });
                i = end + 3;
                continue;
            }
        }
        if bytes[i..].starts_with(b"**") {
            if let Some(end) = find_close(s, i + 2, "**") {
                let start = text.len();
                text.push_str(&s[i + 2..end]);
                let end_t = text.len();
                spans.push(crate::TextSpan {
                    range: start..end_t,
                    bold: true,
                    italic: false,
                    underline: false,
                    font_size: None,
                    color: None,
                });
                i = end + 2;
                continue;
            }
        }
        if bytes[i] == b'*' {
            if let Some(end) = find_close(s, i + 1, "*") {
                let start = text.len();
                text.push_str(&s[i + 1..end]);
                let end_t = text.len();
                spans.push(crate::TextSpan {
                    range: start..end_t,
                    bold: false,
                    italic: true,
                    underline: false,
                    font_size: None,
                    color: None,
                });
                i = end + 1;
                continue;
            }
        }
        text.push(s[i..].chars().next().unwrap());
        i += s[i..].chars().next().unwrap().len_utf8();
    }
    let mut doc = RichDoc { text, spans };
    doc.normalize();
    doc
}

fn find_close(s: &str, from: usize, delim: &str) -> Option<usize> {
    s[from..].find(delim).map(|o| from + o)
}

fn clip_json_escape(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if start >= bytes.len() || bytes[start] != b'{' {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn clip_extract_string(obj: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let idx = obj.find(&pat)?;
    let after = &obj[idx + pat.len()..];
    let colon = after.find(':')?;
    let mut rest = after[colon + 1..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    rest = &rest[1..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(o) => out.push(o),
                None => break,
            },
            '"' => break,
            c => out.push(c),
        }
    }
    Some(out)
}

fn clip_extract_usize(obj: &str, key: &str) -> Option<usize> {
    let pat = format!("\"{key}\"");
    let idx = obj.find(&pat)?;
    let after = &obj[idx + pat.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse().ok()
}

fn clip_extract_inline_json(obj: &str) -> Result<String, String> {
    let pat = "\"inline\"";
    let idx = obj.find(pat).ok_or_else(|| "missing inline".to_string())?;
    let after = &obj[idx + pat.len()..];
    let colon = after
        .find(':')
        .ok_or_else(|| "missing inline colon".to_string())?;
    let rest = after[colon + 1..].trim_start();
    if !rest.starts_with('{') {
        return Err("inline not object".into());
    }
    let end = find_matching_brace(rest, 0).ok_or("unbalanced inline")?;
    Ok(rest[..=end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextSpan;

    #[test]
    fn from_rich_splits_lines() {
        let mut doc = RichDoc::from_plain("a\nb");
        doc.spans.push(TextSpan {
            range: 0..1,
            bold: true,
            italic: false,
            underline: false,
            font_size: None,
            color: None,
        });
        let bd = BlockDoc::from_rich_doc(&doc);
        assert_eq!(bd.blocks.len(), 2);
        assert!(matches!(&bd.blocks[0], Block::Paragraph(p) if p.text == "a" && p.spans[0].bold));
        assert!(matches!(&bd.blocks[1], Block::Paragraph(p) if p.text == "b"));
    }

    #[test]
    fn markdown_roundtrip_subset() {
        let md = "# Title\nHello **bold** and *italic*\n- item\n1. one\n![](pic)";
        let doc = BlockDoc::from_markdown(md);
        assert!(matches!(&doc.blocks[0], Block::Heading { level: 1, .. }));
        assert!(matches!(&doc.blocks[2], Block::BulletItem { .. }));
        assert!(matches!(
            &doc.blocks[3],
            Block::OrderedItem { number: 1, .. }
        ));
        assert!(matches!(&doc.blocks[4], Block::Image { id, .. } if id == "pic"));
        let out = doc.to_markdown();
        let back = BlockDoc::from_markdown(&out);
        assert_eq!(back.blocks.len(), doc.blocks.len());
        assert_eq!(back.to_markdown(), out);
    }

    #[test]
    fn split_and_merge() {
        let mut doc = BlockDoc::from_plain("hello");
        doc.split_at(0, 2);
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Paragraph(p) if p.text == "he"));
        assert!(matches!(&doc.blocks[1], Block::Paragraph(p) if p.text == "llo"));
        let caret = doc.merge_with_previous(1).unwrap();
        assert_eq!(caret, 2);
        assert_eq!(doc.blocks.len(), 1);
        assert!(matches!(&doc.blocks[0], Block::Paragraph(p) if p.text == "hello"));
    }

    #[test]
    fn delete_range_cross_block() {
        let mut doc = BlockDoc::from_blocks(vec![
            Block::paragraph("abc"),
            Block::paragraph("def"),
            Block::paragraph("ghi"),
        ]);
        let (b, o) = doc.delete_range(0, 1, 2, 2);
        assert_eq!((b, o), (0, 1));
        assert_eq!(doc.blocks.len(), 1);
        assert!(matches!(&doc.blocks[0], Block::Paragraph(p) if p.text == "ai"));
    }

    #[test]
    fn plain_slice_cross_block() {
        let doc = BlockDoc::from_blocks(vec![Block::paragraph("ab"), Block::paragraph("cd")]);
        assert_eq!(doc.plain_slice(0, 1, 1, 1), "b\nc");
    }

    #[test]
    fn slice_doc_preserves_font_size_and_roundtrips_clipboard() {
        let mut inline = RichDoc::from_plain("Hello");
        inline.apply_font_size(0..5, Some(220));
        inline.toggle_bold(0..5);
        let doc = BlockDoc::from_blocks(vec![Block::Heading { level: 1, inline }]);
        let sliced = doc.slice_doc(0, 0, 0, 5);
        assert!(matches!(
            &sliced.blocks[0],
            Block::Heading { level: 1, inline } if inline.spans[0].font_size == Some(220)
                && inline.spans[0].bold
        ));
        let json = sliced.to_clipboard_json();
        let back = BlockDoc::from_clipboard_json(&json).unwrap();
        assert!(matches!(
            &back.blocks[0],
            Block::Heading { level: 1, inline } if inline.text == "Hello"
                && inline.spans[0].font_size == Some(220)
                && inline.spans[0].bold
        ));
    }

    #[test]
    fn slice_heading_bakes_block_font_size() {
        let doc = BlockDoc::from_blocks(vec![Block::heading(1, "Title")]);
        let sliced = doc.slice_doc(0, 0, 0, 5);
        let tenths = (Block::heading(1, "").font_size() * 10.0).round() as u32;
        assert!(matches!(
            &sliced.blocks[0],
            Block::Heading { level: 1, inline }
                if inline.spans.iter().any(|s| s.font_size == Some(tenths))
        ));
    }

    #[test]
    fn plain_byte_offset_matches_plain_text() {
        let doc = BlockDoc::from_markdown("# Heading\nParagraph — select\n- list");
        let plain = doc.plain_text();
        // Second text block starts after "Heading\n"
        let off = doc.plain_byte_offset(1, 0);
        assert_eq!(&plain[off..off + 9], "Paragraph");
        // Offset into em-dash must land on a char boundary in plain_text.
        let dash_in_block = "Paragraph ".len();
        let abs = doc.plain_byte_offset(1, dash_in_block);
        assert!(plain.is_char_boundary(abs));
        assert_eq!(&plain[abs..].chars().next().unwrap().to_string(), "—");
    }
}
