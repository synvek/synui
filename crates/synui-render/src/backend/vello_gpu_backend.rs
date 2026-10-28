//! Vello GPU (wgpu compute) render backend.

use crate::backend::RenderError;
use peniko::Color;
use std::sync::Arc;
use vello::util::RenderSurface;
use vello::wgpu;
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};
use winit::window::Window;

/// Full GPU renderer using the `vello` crate.
pub struct VelloGpuBackend {
    pub context: vello::util::RenderContext,
    renderers: Vec<Option<Renderer>>,
    surface: Option<RenderSurface<'static>>,
}

impl VelloGpuBackend {
    pub fn new() -> Self {
        Self {
            context: vello::util::RenderContext::new(),
            renderers: Vec::new(),
            surface: None,
        }
    }

    pub async fn create_surface(
        &mut self,
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        let surface = self
            .context
            .create_surface(window, width, height, wgpu::PresentMode::AutoNoVsync)
            .await
            .map_err(|e| RenderError::Surface(e.to_string()))?;
        self.surface = Some(surface);
        Ok(())
    }

    pub fn resize_surface(&mut self, width: u32, height: u32) {
        if let Some(surface) = self.surface.as_mut() {
            self.context.resize_surface(surface, width, height);
        }
    }

    pub fn render_scene(
        &mut self,
        scene: &Scene,
        base_color: Color,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        let surface = self.surface.as_mut().ok_or_else(|| {
            RenderError::Surface("create_surface must be called before render_scene".into())
        })?;

        let dev_id = surface.dev_id;
        let device = &self.context.devices[dev_id].device;
        let queue = &self.context.devices[dev_id].queue;

        while self.renderers.len() <= dev_id {
            self.renderers.push(None);
        }
        if self.renderers[dev_id].is_none() {
            self.renderers[dev_id] = Some(
                Renderer::new(
                    device,
                    RendererOptions {
                        use_cpu: false,
                        antialiasing_support: vello::AaSupport::all(),
                        num_init_threads: None,
                        pipeline_cache: None,
                    },
                )
                .map_err(RenderError::Gpu)?,
            );
        }
        let renderer = self.renderers[dev_id].as_mut().unwrap();

        renderer.render_to_texture(
            device,
            queue,
            scene,
            &surface.target_view,
            &RenderParams {
                base_color,
                width,
                height,
                // Area AA: far cheaper than MSAA8 on Windows; full-viewport MSAA
                // present-per-keystroke was the dominant TextField input lag source.
                antialiasing_method: AaConfig::Area,
            },
        )?;

        let surface_texture = surface
            .surface
            .get_current_texture()
            .map_err(|e| RenderError::Surface(e.to_string()))?;
        let window_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("synui_blit"),
        });
        surface
            .blitter
            .copy(device, &mut encoder, &surface.target_view, &window_view);
        queue.submit(Some(encoder.finish()));
        surface_texture.present();
        Ok(())
    }
}

impl Default for VelloGpuBackend {
    fn default() -> Self {
        Self::new()
    }
}
