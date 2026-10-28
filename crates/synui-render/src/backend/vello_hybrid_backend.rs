//! Vello Hybrid (CPU strip setup + GPU raster) render backend.

use crate::backend::RenderError;
use crate::scene_bridge::{replay_vello_scene, HybridSceneSink, SparseSceneSink};
use peniko::Color;
use std::sync::Arc;
use vello::Scene;
use vello_hybrid::{
    RenderSize, RenderTargetConfig, Renderer, Resources, Scene as HybridScene, TextureBindings,
};
use wgpu29 as wgpu;
use winit::window::Window;

struct HybridSurface {
    #[allow(dead_code)]
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    hybrid_scene: HybridScene,
    resources: Resources,
}

/// Hybrid renderer using the `vello_hybrid` crate (wgpu 29).
pub struct VelloHybridBackend {
    surface: Option<HybridSurface>,
}

impl VelloHybridBackend {
    pub fn new() -> Self {
        Self { surface: None }
    }

    pub async fn create_surface(
        &mut self,
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .map_err(|e| RenderError::Surface(e.to_string()))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| RenderError::Surface(e.to_string()))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("synui_vello_hybrid"),
                required_features: wgpu::Features::empty(),
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::Surface(e.to_string()))?;

        let cap = surface.get_capabilities(&adapter);
        let format = cap
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(cap.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: cap.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let w = u16::try_from(width.max(1)).unwrap_or(u16::MAX);
        let h = u16::try_from(height.max(1)).unwrap_or(u16::MAX);
        let renderer = Renderer::new(
            &device,
            &RenderTargetConfig {
                format,
                width: width.max(1),
                height: height.max(1),
            },
        );
        let hybrid_scene = HybridScene::new(w, h);

        self.surface = Some(HybridSurface {
            instance,
            surface,
            device,
            queue,
            config,
            renderer,
            hybrid_scene,
            resources: Resources::new(),
        });
        Ok(())
    }

    pub fn resize_surface(&mut self, width: u32, height: u32) {
        let Some(hs) = self.surface.as_mut() else {
            return;
        };
        let width = width.max(1);
        let height = height.max(1);
        hs.config.width = width;
        hs.config.height = height;
        hs.surface.configure(&hs.device, &hs.config);
        hs.renderer = Renderer::new(
            &hs.device,
            &RenderTargetConfig {
                format: hs.config.format,
                width,
                height,
            },
        );
        let w = u16::try_from(width).unwrap_or(u16::MAX);
        let h = u16::try_from(height).unwrap_or(u16::MAX);
        hs.hybrid_scene = HybridScene::new(w, h);
    }

    pub fn render_scene(
        &mut self,
        scene: &Scene,
        base_color: Color,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        let hs = self
            .surface
            .as_mut()
            .expect("create_surface must be called before render_scene");

        {
            let mut sink = HybridSceneSink {
                scene: &mut hs.hybrid_scene,
                resources: &mut hs.resources,
            };
            sink.clear(base_color);
            replay_vello_scene(scene, &mut sink)?;
        }

        let frame = match hs.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(tex)
            | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => tex,
            _ => return Ok(()),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = hs
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("synui_vello_hybrid"),
            });

        hs.renderer.render(
            &hs.hybrid_scene,
            &mut hs.resources,
            &hs.device,
            &hs.queue,
            &mut encoder,
            &RenderSize {
                width: width.max(1),
                height: height.max(1),
            },
            &view,
            &TextureBindings::new(),
        )?;

        hs.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

impl Default for VelloHybridBackend {
    fn default() -> Self {
        Self::new()
    }
}
