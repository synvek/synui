//! Vello CPU (sparse strips software) render backend.

use crate::backend::RenderError;
use crate::scene_bridge::{replay_vello_scene, CpuSceneSink, SparseSceneSink};
use peniko::Color;
use std::sync::Arc;
use vello::util::RenderSurface;
use vello::wgpu;
use vello::Scene;
use vello_cpu::{Pixmap, RenderContext, Resources};
use winit::window::Window;

const COPY_BYTES_PER_ROW_ALIGNMENT: u32 = 256;

/// Software renderer using the `vello_cpu` crate, presented via wgpu texture upload.
pub struct VelloCpuBackend {
    pub context: vello::util::RenderContext,
    surface: Option<RenderSurface<'static>>,
    cpu_resources: Resources,
    /// Intermediate texture with `COPY_DST`; vello's `target_texture` is storage-only.
    upload_texture: Option<wgpu::Texture>,
    upload_view: Option<wgpu::TextureView>,
    upload_size: (u32, u32),
    /// Blitter with premultiplied-alpha blending for pixmap presentation.
    presenter: Option<wgpu::util::TextureBlitter>,
    presenter_format: Option<wgpu::TextureFormat>,
}

impl VelloCpuBackend {
    pub fn new() -> Self {
        Self {
            context: vello::util::RenderContext::new(),
            surface: None,
            cpu_resources: Resources::new(),
            upload_texture: None,
            upload_view: None,
            upload_size: (0, 0),
            presenter: None,
            presenter_format: None,
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
            .create_surface(window, width, height, wgpu::PresentMode::AutoVsync)
            .await
            .map_err(|e| RenderError::Surface(e.to_string()))?;
        self.surface = Some(surface);
        let (dev_id, format, w, h) = {
            let s = self.surface.as_ref().unwrap();
            (
                s.dev_id,
                s.format,
                s.config.width.max(1),
                s.config.height.max(1),
            )
        };
        self.ensure_gpu_resources(dev_id, format, w, h);
        Ok(())
    }

    pub fn resize_surface(&mut self, width: u32, height: u32) {
        let info = self.surface.as_ref().map(|s| (s.dev_id, s.format));
        if let Some(surface) = self.surface.as_mut() {
            self.context.resize_surface(surface, width, height);
        }
        if let Some((dev_id, format)) = info {
            let w = width.max(1);
            let h = height.max(1);
            self.ensure_gpu_resources(dev_id, format, w, h);
        }
    }

    fn ensure_gpu_resources(
        &mut self,
        dev_id: usize,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) {
        if self.presenter_format != Some(format) {
            let device = &self.context.devices[dev_id].device;
            self.presenter = Some(
                wgpu::util::TextureBlitterBuilder::new(device, format)
                    .blend_state(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
                    .build(),
            );
            self.presenter_format = Some(format);
        }
        if self.upload_size == (width, height) && self.upload_texture.is_some() {
            return;
        }
        let device = &self.context.devices[dev_id].device;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("synui_cpu_upload"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.upload_texture = Some(texture);
        self.upload_view = Some(view);
        self.upload_size = (width, height);
    }

    pub fn render_scene(
        &mut self,
        scene: &Scene,
        base_color: Color,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| RenderError::Surface("surface not created".into()))?;
        let width = width.max(1).min(surface.config.width.max(1));
        let height = height.max(1).min(surface.config.height.max(1));
        let dev_id = surface.dev_id;
        let format = surface.format;

        let w = u16::try_from(width).unwrap_or(u16::MAX);
        let h = u16::try_from(height).unwrap_or(u16::MAX);
        let mut cpu_ctx = RenderContext::new(w, h);
        {
            let mut sink = CpuSceneSink {
                ctx: &mut cpu_ctx,
                resources: &mut self.cpu_resources,
            };
            sink.clear(base_color);
            if let Err(e) = replay_vello_scene(scene, &mut sink) {
                eprintln!("synui: CPU scene replay failed: {e}");
            }
        }
        cpu_ctx.flush();
        let mut pixmap = Pixmap::new(w, h);
        cpu_ctx.render_to_pixmap(&mut self.cpu_resources, &mut pixmap);

        self.ensure_gpu_resources(dev_id, format, width, height);
        let upload_texture = self
            .upload_texture
            .as_ref()
            .expect("upload texture initialized");
        let upload_view = self.upload_view.as_ref().expect("upload view initialized");
        let presenter = self.presenter.as_ref().expect("presenter initialized");

        let unpadded_bytes_per_row = 4 * width;
        let padded_bytes_per_row =
            unpadded_bytes_per_row.next_multiple_of(COPY_BYTES_PER_ROW_ALIGNMENT);
        let src = pixmap.data_as_u8_slice();
        let upload_data = pad_pixmap_rows(src, width, height, padded_bytes_per_row);

        let device = &self.context.devices[dev_id].device;
        let queue = &self.context.devices[dev_id].queue;

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("synui_cpu_staging"),
            size: upload_data.len() as u64,
            usage: wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        });
        staging
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(&upload_data);
        staging.unmap();

        let surface = self.surface.as_mut().ok_or_else(|| {
            RenderError::Surface("create_surface must be called before render_scene".into())
        })?;
        let surface_texture = surface
            .surface
            .get_current_texture()
            .map_err(|e| RenderError::Surface(e.to_string()))?;
        let window_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("synui_cpu_present"),
        });
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: upload_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        {
            let clear = wgpu_clear_color(base_color);
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("synui_cpu_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &window_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        presenter.copy(device, &mut encoder, upload_view, &window_view);
        queue.submit(Some(encoder.finish()));
        surface_texture.present();
        Ok(())
    }
}

fn pad_pixmap_rows(src: &[u8], width: u32, height: u32, padded_bytes_per_row: u32) -> Vec<u8> {
    let unpadded_bytes_per_row = 4 * width;
    if padded_bytes_per_row == unpadded_bytes_per_row {
        return src.to_vec();
    }
    let mut padded = vec![0u8; (padded_bytes_per_row * height) as usize];
    for y in 0..height {
        let src_start = (y * unpadded_bytes_per_row) as usize;
        let dst_start = (y * padded_bytes_per_row) as usize;
        padded[dst_start..dst_start + unpadded_bytes_per_row as usize]
            .copy_from_slice(&src[src_start..src_start + unpadded_bytes_per_row as usize]);
    }
    padded
}

fn wgpu_clear_color(color: Color) -> wgpu::Color {
    let c = color.to_rgba8();
    wgpu::Color {
        r: c.r as f64 / 255.0,
        g: c.g as f64 / 255.0,
        b: c.b as f64 / 255.0,
        a: 1.0,
    }
}

impl Default for VelloCpuBackend {
    fn default() -> Self {
        Self::new()
    }
}
