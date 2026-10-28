//! Input events and dispatch results.

use crate::geometry::Point;
use crate::WidgetId;

/// Normalized pointer identifier (0–9 per requirements).
pub type PointerId = u8;

/// Unified input event types.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PointerDown {
        id: PointerId,
        position: Point,
        button: PointerButton,
        /// Shift held (multi-select range).
        shift: bool,
        /// Ctrl (Windows/Linux) or Command (macOS) — multi-select toggle.
        ctrl: bool,
    },
    PointerUp {
        id: PointerId,
        position: Point,
        button: PointerButton,
    },
    PointerMove {
        id: PointerId,
        position: Point,
    },
    PointerCancel {
        id: PointerId,
    },
    KeyDown {
        key: Key,
        repeat: bool,
        shift: bool,
        /// Ctrl (Windows/Linux) or Command (macOS).
        ctrl: bool,
        /// Alt / Option.
        alt: bool,
    },
    KeyUp {
        key: Key,
    },
    Scroll {
        position: Point,
        delta_x: f32,
        delta_y: f32,
    },
    FocusGained,
    FocusLost,
    Gesture(crate::event::Gesture),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Named(NamedKey),
    Character(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    F10,
}

/// Recognized gestures.
#[derive(Debug, Clone, PartialEq)]
pub enum Gesture {
    Tap {
        position: Point,
    },
    Drag {
        start: Point,
        current: Point,
        delta: crate::Vec2,
    },
    Scroll {
        position: Point,
        delta_x: f32,
        delta_y: f32,
    },
    /// Two-finger pinch; `scale` is relative to gesture start (1.0 = unchanged).
    Pinch {
        scale: f32,
        center: Point,
    },
}

/// Result of event handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventResult {
    pub handled: bool,
    pub stop_propagation: bool,
}

impl EventResult {
    pub const HANDLED: Self = Self {
        handled: true,
        stop_propagation: true,
    };

    pub const IGNORED: Self = Self {
        handled: false,
        stop_propagation: false,
    };
}

/// Context passed during event dispatch.
pub struct EventCtx<'a> {
    pub target: WidgetId,
    pub propagation_stopped: bool,
    pub path: &'a [WidgetId],
    pub layout_direction: crate::widget::LayoutDirection,
    /// Request exclusive pointer delivery until [`Self::release_pointer`].
    pub(crate) want_pointer_grab: bool,
    pub(crate) want_pointer_release: bool,
    /// Phase 37: explicit paint request (same path as HANDLED → host paint dirty).
    pub(crate) want_paint: bool,
}

impl<'a> EventCtx<'a> {
    pub fn new(
        target: WidgetId,
        path: &'a [WidgetId],
        layout_direction: crate::widget::LayoutDirection,
    ) -> Self {
        Self {
            target,
            propagation_stopped: false,
            path,
            layout_direction,
            want_pointer_grab: false,
            want_pointer_release: false,
            want_paint: false,
        }
    }

    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }

    /// Capture subsequent pointer move/up to this widget (scrollbar / slider drag).
    pub fn grab_pointer(&mut self) {
        self.want_pointer_grab = true;
    }

    /// Release a previous [`Self::grab_pointer`].
    pub fn release_pointer(&mut self) {
        self.want_pointer_release = true;
    }

    /// Request paint for ephemeral widget visuals (selection, caret, hover).
    ///
    /// Does **not** schedule a declarative rebuild. Prefer this (or returning
    /// `HANDLED`) over `Signal::set` for local interaction chrome. Host marks
    /// paint dirty for `target`; also records [`crate::schedule_paint`].
    pub fn request_paint(&mut self) {
        self.want_paint = true;
        crate::runtime::schedule_paint();
    }
}
