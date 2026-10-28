//! Overlay stack for popups, dialogs, and dropdown menus.

use crate::widget::WidgetId;

/// Configuration for an overlay layer.
#[derive(Debug, Clone, Copy)]
pub struct OverlayConfig {
    /// Blocks interaction with widgets below.
    pub modal: bool,
    /// Clicking the backdrop dismisses this overlay.
    pub backdrop_dismiss: bool,
}

impl OverlayConfig {
    pub fn modal() -> Self {
        Self {
            modal: true,
            backdrop_dismiss: true,
        }
    }

    pub fn popup() -> Self {
        Self {
            modal: false,
            backdrop_dismiss: true,
        }
    }

    /// Non-interactive hover tip: does not block hits or dismiss on backdrop.
    pub fn tooltip() -> Self {
        Self {
            modal: false,
            backdrop_dismiss: false,
        }
    }

    pub fn persistent() -> Self {
        Self {
            modal: true,
            backdrop_dismiss: false,
        }
    }
}

/// A single overlay entry (root widget + behavior).
#[derive(Debug, Clone)]
pub struct OverlayEntry {
    pub root: WidgetId,
    pub config: OverlayConfig,
    /// Focus trap widget ids (Tab cycles within this list).
    pub focus_trap: Vec<WidgetId>,
}

/// Z-ordered stack of overlay layers (bottom to top).
#[derive(Debug, Default)]
pub struct OverlayStack {
    entries: Vec<OverlayEntry>,
}

impl OverlayStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: OverlayEntry) {
        self.entries.push(entry);
    }

    pub fn pop(&mut self) -> Option<OverlayEntry> {
        self.entries.pop()
    }

    pub fn top(&self) -> Option<&OverlayEntry> {
        self.entries.last()
    }

    pub fn top_mut(&mut self) -> Option<&mut OverlayEntry> {
        self.entries.last_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, OverlayEntry> {
        self.entries.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, OverlayEntry> {
        self.entries.iter_mut()
    }

    pub fn remove_widget(&mut self, id: WidgetId) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.root != id);
        self.entries.len() < before
    }
}
