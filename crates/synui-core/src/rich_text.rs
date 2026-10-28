//! Rich text document model (Phase 14): plain string + non-overlapping style spans.

use crate::TextStyleRun;
use std::ops::Range;

/// Inline style applied over a UTF-8 byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub range: Range<usize>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub font_size: Option<u32>, // stored as fixed-point tenths (e.g. 140 = 14.0)
    pub color: Option<[u8; 4]>,
}

impl TextSpan {
    pub fn plain(range: Range<usize>) -> Self {
        Self {
            range,
            bold: false,
            italic: false,
            underline: false,
            font_size: None,
            color: None,
        }
    }

    fn font_size_f32(&self) -> Option<f32> {
        self.font_size.map(|t| t as f32 / 10.0)
    }

    fn is_default(&self) -> bool {
        !self.bold
            && !self.italic
            && !self.underline
            && self.font_size.is_none()
            && self.color.is_none()
    }

    fn to_style_run(&self) -> TextStyleRun {
        TextStyleRun {
            range: self.range.clone(),
            font_size: self.font_size_f32(),
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            color: self.color,
        }
    }
}

/// Rich document: UTF-8 text + style spans (non-overlapping, sorted by start).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichDoc {
    pub text: String,
    pub spans: Vec<TextSpan>,
}

impl RichDoc {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            spans: Vec::new(),
        }
    }

    pub fn style_runs(&self) -> Vec<TextStyleRun> {
        self.spans
            .iter()
            .filter(|s| !s.is_default())
            .map(TextSpan::to_style_run)
            .collect()
    }

    /// Insert `s` at `index`, shifting spans.
    pub fn insert(&mut self, index: usize, s: &str) {
        let index = floor_boundary(&self.text, index.min(self.text.len()));
        self.text.insert_str(index, s);
        let len = s.len();
        for span in &mut self.spans {
            if span.range.start >= index {
                span.range.start += len;
                span.range.end += len;
            } else if span.range.end > index {
                span.range.end += len;
            }
        }
    }

    /// Delete byte range `[start, end)`, adjusting spans.
    pub fn delete_range(&mut self, start: usize, end: usize) {
        let start = floor_boundary(&self.text, start.min(self.text.len()));
        let end = floor_boundary(&self.text, end.min(self.text.len()).max(start));
        if start == end {
            return;
        }
        let len = end - start;
        self.text.replace_range(start..end, "");
        let mut out = Vec::new();
        for mut span in self.spans.drain(..) {
            if span.range.end <= start {
                out.push(span);
            } else if span.range.start >= end {
                span.range.start -= len;
                span.range.end -= len;
                out.push(span);
            } else {
                // Overlap: shrink / split
                let new_start = if span.range.start < start {
                    span.range.start
                } else {
                    start
                };
                let new_end = if span.range.end > end {
                    span.range.end - len
                } else {
                    start
                };
                if new_end > new_start {
                    span.range = new_start..new_end;
                    out.push(span);
                }
            }
        }
        self.spans = out;
        self.normalize();
    }

    /// Extract a sub-document `[start, end)` with remapped spans (bold/italic/font_size/…).
    pub fn slice(&self, start: usize, end: usize) -> RichDoc {
        let a = floor_boundary(&self.text, start.min(self.text.len()));
        let b = floor_boundary(&self.text, end.min(self.text.len()).max(a));
        let text = self.text[a..b].to_string();
        let mut spans = Vec::new();
        for span in &self.spans {
            if span.range.end <= a || span.range.start >= b {
                continue;
            }
            let mut s = span.clone();
            s.range = (span.range.start.max(a) - a)..(span.range.end.min(b) - a);
            if s.range.start < s.range.end {
                spans.push(s);
            }
        }
        let mut doc = RichDoc { text, spans };
        doc.normalize();
        doc
    }

    /// Insert another rich fragment at `index` without inheriting surrounding span styles.
    pub fn insert_rich(&mut self, index: usize, other: &RichDoc) {
        if other.text.is_empty() {
            return;
        }
        let index = floor_boundary(&self.text, index.min(self.text.len()));
        self.split_at(index);
        self.text.insert_str(index, &other.text);
        let len = other.text.len();
        for span in &mut self.spans {
            if span.range.start >= index {
                span.range.start += len;
                span.range.end += len;
            }
        }
        for span in &other.spans {
            let mut s = span.clone();
            s.range = (span.range.start + index)..(span.range.end + index);
            self.spans.push(s);
        }
        self.normalize();
    }

    /// Set font size (tenths of a point, e.g. 180 = 18.0) over `range`.
    pub fn apply_font_size(&mut self, range: Range<usize>, size_tenths: Option<u32>) {
        self.apply_flag(range, |s| s.font_size = size_tenths);
    }

    /// Toggle bold/italic/underline on `range`. Empty range is a no-op.
    pub fn toggle_bold(&mut self, range: Range<usize>) {
        let on = !self.range_all_flag(range.clone(), |s| s.bold);
        self.apply_flag(range, |s| s.bold = on);
    }

    pub fn toggle_italic(&mut self, range: Range<usize>) {
        let on = !self.range_all_flag(range.clone(), |s| s.italic);
        self.apply_flag(range, |s| s.italic = on);
    }

    pub fn toggle_underline(&mut self, range: Range<usize>) {
        let on = !self.range_all_flag(range.clone(), |s| s.underline);
        self.apply_flag(range, |s| s.underline = on);
    }

    fn range_all_flag(&self, range: Range<usize>, f: impl Fn(&TextSpan) -> bool) -> bool {
        let a = floor_boundary(&self.text, range.start.min(self.text.len()));
        let b = floor_boundary(&self.text, range.end.min(self.text.len()).max(a));
        if a == b {
            return false;
        }
        let mut covered = a;
        for span in &self.spans {
            if span.range.end <= a || span.range.start >= b {
                continue;
            }
            let start = span.range.start.max(a);
            if start > covered {
                return false; // gap = not styled
            }
            if !f(span) {
                return false;
            }
            covered = span.range.end.min(b).max(covered);
        }
        covered >= b
    }

    fn apply_flag(&mut self, range: Range<usize>, f: impl Fn(&mut TextSpan)) {
        let a = floor_boundary(&self.text, range.start.min(self.text.len()));
        let b = floor_boundary(&self.text, range.end.min(self.text.len()).max(a));
        if a == b {
            return;
        }
        self.split_at(a);
        self.split_at(b);
        // Collect uncovered gaps inside [a,b)
        let mut cursor = a;
        let mut extras = Vec::new();
        let existing: Vec<Range<usize>> = self
            .spans
            .iter()
            .filter(|s| s.range.start < b && s.range.end > a)
            .map(|s| s.range.start.max(a)..s.range.end.min(b))
            .collect();
        for r in existing {
            if r.start > cursor {
                extras.push(TextSpan::plain(cursor..r.start));
            }
            cursor = r.end.max(cursor);
        }
        if cursor < b {
            extras.push(TextSpan::plain(cursor..b));
        }
        self.spans.extend(extras);
        for span in &mut self.spans {
            if span.range.start >= a && span.range.end <= b {
                f(span);
            }
        }
        self.normalize();
    }

    fn split_at(&mut self, index: usize) {
        let index = floor_boundary(&self.text, index.min(self.text.len()));
        let mut out = Vec::new();
        for span in self.spans.drain(..) {
            if span.range.start < index && span.range.end > index {
                let mut left = span.clone();
                left.range = span.range.start..index;
                let mut right = span;
                right.range = index..right.range.end;
                out.push(left);
                out.push(right);
            } else {
                out.push(span);
            }
        }
        self.spans = out;
    }

    /// Merge adjacent identical spans and drop empties / defaults.
    pub fn normalize(&mut self) {
        self.spans
            .retain(|s| s.range.start < s.range.end && !s.is_default());
        self.spans.sort_by_key(|s| s.range.start);
        let mut i = 0;
        while i + 1 < self.spans.len() {
            let can_merge = {
                let a = &self.spans[i];
                let b = &self.spans[i + 1];
                a.range.end == b.range.start
                    && a.bold == b.bold
                    && a.italic == b.italic
                    && a.underline == b.underline
                    && a.font_size == b.font_size
                    && a.color == b.color
            };
            if can_merge {
                let end = self.spans[i + 1].range.end;
                self.spans[i].range.end = end;
                self.spans.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    /// Minimal JSON: `{"text":"...","spans":[{"start":0,"end":2,"bold":true},...]}`
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\"text\":");
        out.push_str(&json_escape(&self.text));
        out.push_str(",\"spans\":[");
        for (i, s) in self.spans.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('{');
            out.push_str(&format!(
                "\"start\":{},\"end\":{}",
                s.range.start, s.range.end
            ));
            if s.bold {
                out.push_str(",\"bold\":true");
            }
            if s.italic {
                out.push_str(",\"italic\":true");
            }
            if s.underline {
                out.push_str(",\"underline\":true");
            }
            if let Some(sz) = s.font_size {
                out.push_str(&format!(",\"font_size\":{sz}"));
            }
            if let Some(c) = s.color {
                out.push_str(&format!(",\"color\":[{},{},{},{}]", c[0], c[1], c[2], c[3]));
            }
            out.push('}');
        }
        out.push_str("]}");
        out
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if !s.starts_with('{') {
            return Err("expected object".into());
        }
        // Tiny hand parser for our schema only.
        let text = extract_string_field(s, "text").unwrap_or_default();
        let mut spans = Vec::new();
        if let Some(arr_start) = s.find("\"spans\"") {
            let rest = &s[arr_start..];
            if let Some(bracket) = rest.find('[') {
                let mut i = bracket + 1;
                let bytes = rest.as_bytes();
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
                    let end = rest[i..].find('}').map(|e| i + e + 1).unwrap_or(rest.len());
                    let obj = &rest[i..end];
                    let start = extract_usize(obj, "start").unwrap_or(0);
                    let end_i = extract_usize(obj, "end").unwrap_or(start);
                    let bold = obj.contains("\"bold\":true");
                    let italic = obj.contains("\"italic\":true");
                    let underline = obj.contains("\"underline\":true");
                    let font_size = extract_usize(obj, "font_size").map(|v| v as u32);
                    spans.push(TextSpan {
                        range: start..end_i,
                        bold,
                        italic,
                        underline,
                        font_size,
                        color: None,
                    });
                    i = end;
                }
            }
        }
        let mut doc = Self { text, spans };
        doc.normalize();
        Ok(doc)
    }
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn json_escape(s: &str) -> String {
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

fn extract_string_field(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let idx = s.find(&pat)?;
    let after = &s[idx + pat.len()..];
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
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => out.push(other),
                None => break,
            },
            c => out.push(c),
        }
    }
    Some(out)
}

fn extract_usize(s: &str, key: &str) -> Option<usize> {
    let pat = format!("\"{key}\"");
    let idx = s.find(&pat)?;
    let after = &s[idx + pat.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_shifts_spans() {
        let mut doc = RichDoc::from_plain("abcd");
        doc.spans.push(TextSpan {
            range: 1..3,
            bold: true,
            italic: false,
            underline: false,
            font_size: None,
            color: None,
        });
        doc.insert(1, "X");
        assert_eq!(doc.text, "aXbcd");
        assert_eq!(doc.spans[0].range, 2..4);
    }

    #[test]
    fn json_roundtrip() {
        let mut doc = RichDoc::from_plain("Hi");
        doc.spans.push(TextSpan {
            range: 0..2,
            bold: true,
            italic: false,
            underline: true,
            font_size: None,
            color: None,
        });
        let json = doc.to_json();
        let back = RichDoc::from_json(&json).unwrap();
        assert_eq!(back.text, "Hi");
        assert!(back.spans[0].bold);
        assert!(back.spans[0].underline);
    }

    #[test]
    fn slice_and_insert_rich_preserves_font_size() {
        let mut doc = RichDoc::from_plain("HelloWorld");
        doc.apply_font_size(0..5, Some(180));
        doc.toggle_bold(5..10);
        let sliced = doc.slice(0, 5);
        assert_eq!(sliced.text, "Hello");
        assert_eq!(sliced.spans[0].font_size, Some(180));
        let mut target = RichDoc::from_plain("!");
        target.insert_rich(0, &sliced);
        assert_eq!(target.text, "Hello!");
        assert_eq!(target.spans[0].font_size, Some(180));
        assert_eq!(target.spans[0].range, 0..5);
    }

    #[test]
    fn toggle_bold() {
        let mut doc = RichDoc::from_plain("hello");
        doc.toggle_bold(0..5);
        assert!(doc.spans.iter().any(|s| s.bold && s.range == (0..5)));
        doc.toggle_bold(0..5);
        assert!(doc.spans.iter().all(|s| !s.bold));
    }
}
