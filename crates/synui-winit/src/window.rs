//! Window state and render surface management.

use std::sync::Arc;
use synui_render::{RenderBackend, RenderBackendKind};
use vello::kurbo::Point;
use winit::window::Window;

/// Per-window rendering state (independent surface / backend).
pub struct WindowState {
    pub window: Arc<Window>,
    /// Physical surface dimensions used by wgpu/Vello.
    pub width: u32,
    pub height: u32,
    /// Logical UI dimensions used by layout, painting, and hit testing.
    logical_width: f64,
    logical_height: f64,
    scale_factor: f64,
    cursor: Point,
    pub backend: RenderBackend,
}

impl WindowState {
    pub fn new(window: Arc<Window>, kind: RenderBackendKind) -> Self {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let scale_factor = normalized_scale_factor(window.scale_factor());
        let (logical_width, logical_height) = logical_dimensions(width, height, scale_factor);
        Self {
            window,
            width,
            height,
            logical_width,
            logical_height,
            scale_factor,
            cursor: Point::ZERO,
            backend: RenderBackend::new(kind),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        (self.logical_width, self.logical_height) =
            logical_dimensions(self.width, self.height, self.scale_factor);
        self.backend.resize_surface(self.width, self.height);
    }

    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        self.scale_factor = normalized_scale_factor(scale_factor);
        (self.logical_width, self.logical_height) =
            logical_dimensions(self.width, self.height, self.scale_factor);
    }

    pub fn logical_size(&self) -> (f64, f64) {
        (self.logical_width, self.logical_height)
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    pub fn cursor(&self) -> Point {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: Point) {
        self.cursor = cursor;
    }
}

fn normalized_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

fn logical_dimensions(width: u32, height: u32, scale_factor: f64) -> (f64, f64) {
    let scale_factor = normalized_scale_factor(scale_factor);
    (
        (width as f64 / scale_factor).max(1.0),
        (height as f64 / scale_factor).max(1.0),
    )
}
