//! Shared plain-text editing core (Phase 14).
//!
//! Selection indices are UTF-8 byte offsets; moves snap to char boundaries.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::Mutex;

/// Plain-text buffer + selection + IME composition.
#[derive(Debug, Clone, Default)]
pub struct EditCore {
    pub text: String,
    /// UTF-8 byte range. Caret is at `selection.end` when extending.
    pub selection: Range<usize>,
    pub ime_composing: String,
}

impl EditCore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.len();
        Self {
            text,
            selection: len..len,
            ime_composing: String::new(),
        }
    }

    pub fn clamp_selection(&mut self) {
        let len = self.text.len();
        self.selection.start = floor_char_boundary(&self.text, self.selection.start.min(len));
        self.selection.end = floor_char_boundary(&self.text, self.selection.end.min(len));
    }

    pub fn set_caret(&mut self, index: usize) {
        let i = floor_char_boundary(&self.text, index.min(self.text.len()));
        self.selection = i..i;
    }

    pub fn has_selection(&self) -> bool {
        self.selection.start != self.selection.end
    }

    pub fn sel_range(&self) -> Range<usize> {
        let a = self.selection.start.min(self.selection.end);
        let b = self.selection.start.max(self.selection.end);
        a..b
    }

    pub fn select_all(&mut self) {
        self.selection = 0..self.text.len();
    }

    pub fn delete_selection(&mut self) {
        if !self.has_selection() {
            return;
        }
        let range = self.sel_range();
        self.text.replace_range(range.clone(), "");
        self.selection = range.start..range.start;
    }

    pub fn insert_text(&mut self, s: &str) {
        self.delete_selection();
        let i = self.selection.start.min(self.text.len());
        self.text.insert_str(i, s);
        let end = i + s.len();
        self.selection = end..end;
    }

    pub fn backspace(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.selection.start > 0 {
            let start = prev_char_boundary(&self.text, self.selection.start);
            self.text.replace_range(start..self.selection.start, "");
            self.set_caret(start);
        }
    }

    pub fn delete_forward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.selection.end < self.text.len() {
            let end = next_char_boundary(&self.text, self.selection.end);
            self.text.replace_range(self.selection.end..end, "");
        }
    }

    pub fn move_left(&mut self, extend: bool) {
        let caret = self.selection.end;
        let next = prev_char_boundary(&self.text, caret);
        if extend {
            self.selection.end = next;
        } else if self.has_selection() {
            let i = self.sel_range().start;
            self.selection = i..i;
        } else {
            self.set_caret(next);
        }
    }

    pub fn move_right(&mut self, extend: bool) {
        let caret = self.selection.end;
        let next = next_char_boundary(&self.text, caret);
        if extend {
            self.selection.end = next;
        } else if self.has_selection() {
            let i = self.sel_range().end;
            self.selection = i..i;
        } else {
            self.set_caret(next);
        }
    }

    /// Move by whitespace/punctuation word boundaries (MVP; not full UAX#29).
    pub fn move_word_left(&mut self, extend: bool) {
        let caret = self.selection.end.min(self.text.len());
        let next = prev_word_boundary(&self.text, caret);
        if extend {
            self.selection.end = next;
        } else {
            self.set_caret(next);
        }
    }

    pub fn move_word_right(&mut self, extend: bool) {
        let caret = self.selection.end.min(self.text.len());
        let next = next_word_boundary(&self.text, caret);
        if extend {
            self.selection.end = next;
        } else {
            self.set_caret(next);
        }
    }

    /// Select the word containing `index` (or adjacent word if on whitespace).
    pub fn select_word_at(&mut self, index: usize) {
        let i = floor_char_boundary(&self.text, index.min(self.text.len()));
        let (a, b) = word_range_at(&self.text, i);
        self.selection = a..b;
    }

    /// Move caret to start of previous/current line (hard newlines only).
    pub fn move_line_start(&mut self, extend: bool) {
        let caret = self.selection.end.min(self.text.len());
        let start = self.text[..caret].rfind('\n').map(|i| i + 1).unwrap_or(0);
        if extend {
            self.selection.end = start;
        } else {
            self.set_caret(start);
        }
    }

    /// Move caret to end of current line (hard newlines only).
    pub fn move_line_end(&mut self, extend: bool) {
        let caret = self.selection.end.min(self.text.len());
        let end = self.text[caret..]
            .find('\n')
            .map(|i| caret + i)
            .unwrap_or(self.text.len());
        if extend {
            self.selection.end = end;
        } else {
            self.set_caret(end);
        }
    }

    /// Vertical move by hard-newline lines, preserving column byte offset when possible.
    pub fn move_vertical(&mut self, lines_delta: i32, extend: bool) {
        if lines_delta == 0 {
            return;
        }
        let caret = self.selection.end.min(self.text.len());
        let line_starts = line_start_offsets(&self.text);
        let (line_idx, col) = line_and_col(&self.text, &line_starts, caret);
        let target_line = if lines_delta < 0 {
            line_idx.saturating_sub((-lines_delta) as usize)
        } else {
            (line_idx + lines_delta as usize).min(line_starts.len().saturating_sub(1))
        };
        let line_start = line_starts[target_line];
        let line_end = if target_line + 1 < line_starts.len() {
            line_starts[target_line + 1].saturating_sub(1) // exclude '\n'
        } else {
            self.text.len()
        };
        let line = &self.text[line_start..line_end];
        let within = floor_char_boundary(line, col.min(line.len()));
        let idx = line_start + within;
        if extend {
            self.selection.end = idx;
        } else {
            self.set_caret(idx);
        }
    }

    pub fn composition_range(&self) -> Option<Range<usize>> {
        if self.ime_composing.is_empty() {
            return None;
        }
        let start = self.selection.start.min(self.text.len());
        Some(start..start + self.ime_composing.len())
    }

    /// Display string with IME preedit overlaid (replacing selection when present).
    pub fn display_with_ime(&self) -> String {
        if self.ime_composing.is_empty() {
            return self.text.clone();
        }
        let start = self.selection.start.min(self.text.len());
        let end = self.selection.end.min(self.text.len());
        let (a, b) = (start.min(end), start.max(end));
        let mut s = String::new();
        s.push_str(&self.text[..a]);
        s.push_str(&self.ime_composing);
        s.push_str(&self.text[b..]);
        s
    }

    /// Commit IME text (replace selection / insert at caret).
    pub fn commit_ime(&mut self, text: &str) {
        self.insert_text(text);
        self.ime_composing.clear();
    }

    pub fn selected_text(&self) -> &str {
        let r = self.sel_range();
        &self.text[r]
    }
}

fn line_start_offsets(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            starts.push(i + ch.len_utf8());
        }
    }
    starts
}

fn line_and_col(text: &str, line_starts: &[usize], caret: usize) -> (usize, usize) {
    let caret = caret.min(text.len());
    let mut line_idx = 0usize;
    for (i, &s) in line_starts.iter().enumerate() {
        if s <= caret {
            line_idx = i;
        } else {
            break;
        }
    }
    let col = caret - line_starts[line_idx];
    (line_idx, col)
}

pub fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn prev_char_boundary(s: &str, i: usize) -> usize {
    if i == 0 {
        return 0;
    }
    let mut i = i.min(s.len());
    i -= 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn next_char_boundary(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    let mut i = i;
    i += 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn prev_word_boundary(s: &str, i: usize) -> usize {
    let i = floor_char_boundary(s, i.min(s.len()));
    if i == 0 {
        return 0;
    }
    let bytes = s.as_bytes();
    let mut i = i;
    // Skip trailing whitespace
    while i > 0 {
        let p = prev_char_boundary(s, i);
        let ch = s[p..i].chars().next().unwrap_or(' ');
        if !ch.is_whitespace() {
            break;
        }
        i = p;
    }
    if i == 0 {
        return 0;
    }
    let p = prev_char_boundary(s, i);
    let ch = s[p..i].chars().next().unwrap_or(' ');
    if is_word_char(ch) {
        while i > 0 {
            let p = prev_char_boundary(s, i);
            let ch = s[p..i].chars().next().unwrap_or(' ');
            if !is_word_char(ch) {
                break;
            }
            i = p;
        }
    } else {
        // punctuation / other: move one char
        let _ = bytes;
        i = p;
    }
    i
}

pub fn next_word_boundary(s: &str, i: usize) -> usize {
    let mut i = floor_char_boundary(s, i.min(s.len()));
    if i >= s.len() {
        return s.len();
    }
    let ch = s[i..].chars().next().unwrap_or(' ');
    if is_word_char(ch) {
        while i < s.len() {
            let n = next_char_boundary(s, i);
            let ch = s[i..n].chars().next().unwrap_or(' ');
            if !is_word_char(ch) {
                break;
            }
            i = n;
        }
    } else if !ch.is_whitespace() {
        i = next_char_boundary(s, i);
    }
    while i < s.len() {
        let n = next_char_boundary(s, i);
        let ch = s[i..n].chars().next().unwrap_or(' ');
        if !ch.is_whitespace() {
            break;
        }
        i = n;
    }
    i
}

pub fn word_range_at(s: &str, index: usize) -> (usize, usize) {
    let i = floor_char_boundary(s, index.min(s.len()));
    if s.is_empty() {
        return (0, 0);
    }
    let at = if i >= s.len() {
        prev_char_boundary(s, s.len())
    } else {
        i
    };
    let ch = s[at..].chars().next().unwrap_or(' ');
    if !is_word_char(ch) && i > 0 {
        let p = prev_char_boundary(s, i);
        let pch = s[p..i].chars().next().unwrap_or(' ');
        if is_word_char(pch) {
            let start = prev_word_boundary(s, i);
            let end = next_word_boundary(s, start);
            // trim trailing whitespace from end
            let mut e = end;
            while e > start {
                let p = prev_char_boundary(s, e);
                if s[p..e]
                    .chars()
                    .next()
                    .map(|c| c.is_whitespace())
                    .unwrap_or(false)
                {
                    e = p;
                } else {
                    break;
                }
            }
            return (start, e);
        }
    }
    if is_word_char(ch) {
        let start = {
            let mut j = at;
            while j > 0 {
                let p = prev_char_boundary(s, j);
                let c = s[p..j].chars().next().unwrap_or(' ');
                if !is_word_char(c) {
                    break;
                }
                j = p;
            }
            j
        };
        let end = {
            let mut j = at;
            while j < s.len() {
                let n = next_char_boundary(s, j);
                let c = s[j..n].chars().next().unwrap_or(' ');
                if !is_word_char(c) {
                    break;
                }
                j = n;
            }
            j
        };
        (start, end)
    } else {
        let end = next_char_boundary(s, at);
        (at, end)
    }
}

/// Snapshot undo/redo stack for plain text (or serialized rich docs).
#[derive(Debug, Clone)]
pub struct EditHistory<T: Clone> {
    undo: VecDeque<T>,
    redo: VecDeque<T>,
    max: usize,
    coalescing: bool,
    /// Whether the current coalesce group already stored its baseline snapshot.
    coalesce_has_snapshot: bool,
}

impl<T: Clone> EditHistory<T> {
    pub fn new(max: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            max: max.max(1),
            coalescing: false,
            coalesce_has_snapshot: false,
        }
    }

    /// Push a snapshot before a mutating edit. Clears redo.
    ///
    /// While coalescing, only the first snapshot in the group is kept (typing burst
    /// → one undo step). Call [`Self::begin_coalesce`] before the first `push` of
    /// a burst, then `push` on every keystroke.
    pub fn push(&mut self, snapshot: T) {
        self.redo.clear();
        if self.coalescing {
            if self.coalesce_has_snapshot {
                return;
            }
            self.coalesce_has_snapshot = true;
        }
        self.undo.push_back(snapshot);
        while self.undo.len() > self.max {
            self.undo.pop_front();
        }
    }

    /// Begin coalescing consecutive character inserts into one undo step.
    pub fn begin_coalesce(&mut self) {
        if !self.coalescing {
            self.coalescing = true;
            self.coalesce_has_snapshot = false;
        }
    }

    pub fn end_coalesce(&mut self) {
        self.coalescing = false;
        self.coalesce_has_snapshot = false;
    }

    pub fn undo(&mut self, current: T) -> Option<T> {
        self.end_coalesce();
        let prev = self.undo.pop_back()?;
        self.redo.push_back(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: T) -> Option<T> {
        self.end_coalesce();
        let next = self.redo.pop_back()?;
        self.undo.push_back(current);
        Some(next)
    }
}

#[derive(Default)]
struct ClipboardBuf {
    plain: String,
    /// In-process rich payload (BlockDoc JSON). Cleared by plain [`clipboard_set`].
    rich_json: Option<String>,
}

/// Process-local clipboard fallback (tests + wasm without permission).
static FALLBACK_CLIPBOARD: Mutex<ClipboardBuf> = Mutex::new(ClipboardBuf {
    plain: String::new(),
    rich_json: None,
});

#[cfg(test)]
static CLIPBOARD_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Read clipboard text. Desktop uses `arboard`; otherwise process-local buffer.
pub fn clipboard_get() -> String {
    // Unit tests share one process clipboard — prefer the in-process buffer so
    // parallel tests / host OS clipboard do not flake assertions.
    #[cfg(test)]
    {
        return FALLBACK_CLIPBOARD
            .lock()
            .map(|g| g.plain.clone())
            .unwrap_or_default();
    }
    #[cfg(not(test))]
    {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                if let Ok(t) = cb.get_text() {
                    return t;
                }
            }
        }
        FALLBACK_CLIPBOARD
            .lock()
            .map(|g| g.plain.clone())
            .unwrap_or_default()
    }
}

/// Write plain clipboard text (clears any rich payload).
pub fn clipboard_set(text: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(text.to_string());
        }
    }
    if let Ok(mut g) = FALLBACK_CLIPBOARD.lock() {
        g.plain = text.to_string();
        g.rich_json = None;
    }
}

/// Write plain text to the OS clipboard and keep a rich JSON payload for in-app paste.
pub fn clipboard_set_rich(plain: &str, rich_json: impl Into<String>) {
    let rich_json = rich_json.into();
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(plain.to_string());
        }
    }
    if let Ok(mut g) = FALLBACK_CLIPBOARD.lock() {
        g.plain = plain.to_string();
        g.rich_json = Some(rich_json);
    }
}

/// Return rich clipboard JSON when the OS/plain clipboard still matches our last rich copy.
pub fn clipboard_get_rich() -> Option<String> {
    let plain_now = clipboard_get();
    let g = FALLBACK_CLIPBOARD.lock().ok()?;
    let rich = g.rich_json.as_ref()?;
    if plain_now == g.plain {
        Some(rich.clone())
    } else {
        None
    }
}

/// Handle common edit shortcuts: A/C/X/V/Z (and Shift+Z / Y redo).
/// Returns true if handled.
pub fn handle_edit_shortcut(
    edit: &mut EditCore,
    history: &mut EditHistory<String>,
    key: &str,
    ctrl: bool,
    shift: bool,
) -> bool {
    if !ctrl {
        return false;
    }
    match key {
        "a" | "A" => {
            history.end_coalesce();
            edit.select_all();
            true
        }
        "c" | "C" => {
            if edit.has_selection() {
                clipboard_set(edit.selected_text());
            }
            true
        }
        "x" | "X" => {
            if edit.has_selection() {
                history.push(edit.text.clone());
                history.end_coalesce();
                clipboard_set(edit.selected_text());
                edit.delete_selection();
            }
            true
        }
        "v" | "V" => {
            let paste = clipboard_get();
            if !paste.is_empty() {
                history.push(edit.text.clone());
                history.end_coalesce();
                edit.insert_text(&paste);
            }
            true
        }
        "z" | "Z" if shift => {
            if let Some(next) = history.redo(edit.text.clone()) {
                edit.text = next;
                edit.clamp_selection();
            }
            true
        }
        "y" | "Y" => {
            if let Some(next) = history.redo(edit.text.clone()) {
                edit.text = next;
                edit.clamp_selection();
            }
            true
        }
        "z" | "Z" => {
            if let Some(prev) = history.undo(edit.text.clone()) {
                edit.text = prev;
                edit.clamp_selection();
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_undo() {
        let mut edit = EditCore::new();
        let mut hist = EditHistory::new(32);
        hist.push(edit.text.clone());
        edit.insert_text("hi");
        assert_eq!(edit.text, "hi");
        if let Some(prev) = hist.undo(edit.text.clone()) {
            edit.text = prev;
        }
        assert!(edit.text.is_empty());
    }

    #[test]
    fn coalesce_typing_is_one_undo_step() {
        let mut edit = EditCore::new();
        let mut hist = EditHistory::new(32);
        for ch in ["a", "b", "c"] {
            hist.begin_coalesce();
            hist.push(edit.text.clone());
            edit.insert_text(ch);
        }
        assert_eq!(edit.text, "abc");
        let prev = hist.undo(edit.text.clone()).unwrap();
        assert!(prev.is_empty());
    }

    #[test]
    fn clipboard_roundtrip_fallback() {
        let _seq = CLIPBOARD_TEST_LOCK.lock().unwrap();
        clipboard_set("synui");
        assert_eq!(clipboard_get(), "synui");
        assert!(clipboard_get_rich().is_none());
    }

    #[test]
    fn clipboard_rich_roundtrip() {
        let _seq = CLIPBOARD_TEST_LOCK.lock().unwrap();
        clipboard_set_rich("hello", "{\"v\":1,\"blocks\":[]}");
        assert_eq!(clipboard_get(), "hello");
        assert_eq!(
            clipboard_get_rich().as_deref(),
            Some("{\"v\":1,\"blocks\":[]}")
        );
        clipboard_set("plain");
        assert!(clipboard_get_rich().is_none());
    }

    #[test]
    fn vertical_move() {
        let mut edit = EditCore::with_text("ab\ncd");
        edit.set_caret(1); // after 'a'
        edit.move_vertical(1, false);
        assert_eq!(edit.selection, 4..4); // after 'c'
    }
}
