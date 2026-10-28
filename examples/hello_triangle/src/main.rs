//! Phase 0: Vello + Winit rendering verification.

use std::sync::Arc;
use synui_render::{RenderBackend, RenderBackendKind};
use vello::kurbo::{Affine, Rect};
use vello::peniko::color::palette;
use vello::peniko::{Color, Fill};
use vello::Scene;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::Window;

struct App {
    backend: RenderBackend,
    window: Option<Arc<Window>>,
    scene: Scene,
    width: u32,
    height: u32,
}

impl Default for App {
    fn default() -> Self {
        Self {
            backend: RenderBackend::new(RenderBackendKind::Gpu),
            window: None,
            scene: Scene::new(),
            width: 1,
            height: 1,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("SynUI hello_triangle"))
                .unwrap(),
        );
        let size = window.inner_size();
        self.width = size.width;
        self.height = size.height;
        pollster::block_on(
            self.backend
                .create_surface(window.clone(), size.width, size.height),
        )
        .unwrap();
        self.window = Some(window);
        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.width = size.width;
                self.height = size.height;
                self.backend.resize_surface(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                let window = self.window.as_ref().unwrap();
                self.scene.reset();
                let rect = Rect::new(100.0, 100.0, 300.0, 300.0);
                self.scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgb8(0, 200, 128),
                    None,
                    &rect,
                );
                let _ = self.backend.render_scene(
                    &self.scene,
                    palette::css::BLACK,
                    self.width,
                    self.height,
                );
                window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
