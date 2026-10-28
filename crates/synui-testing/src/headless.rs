//! Headless rendering without a window.

use synui_core::{PaintCtx, PaintMode, Rect, Size, WidgetStore};
use synui_layout::LayoutEngine;
use synui_render::{IconCache, ImageCache, TextRenderer};
use synui_theme::Theme;
use vello::wgpu;
use vello::{RenderParams, Renderer, RendererOptions, Scene};

pub struct HeadlessRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    text: TextRenderer,
    icons: IconCache,
    images: ImageCache,
    layout: LayoutEngine,
    /// Retained scene (Phase 31 — same PaintMode as winit).
    scene: Scene,
    width: u32,
    height: u32,
}

impl HeadlessRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_cpu(width, height, true)
    }

    /// Create a headless renderer. `use_cpu` selects Vello GPU-compute vs CPU path
    /// (Req 2.9 Gpu↔Cpu parity). Both share AA=`Area` for comparable output.
    pub fn with_cpu(width: u32, height: u32, use_cpu: bool) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("synui_headless"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .expect("device");
        let renderer = Renderer::new(
            &device,
            RendererOptions {
                use_cpu,
                antialiasing_support: vello::AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .expect("vello renderer");
        Self {
            device,
            queue,
            renderer,
            text: TextRenderer::new(),
            icons: IconCache::new(),
            images: ImageCache::new(),
            layout: LayoutEngine::new(),
            scene: Scene::new(),
            width,
            height,
        }
    }

    /// Layout + host sync + Phase 31/32 PaintMode present (winit-parity Phase 33).
    pub fn render_store(&mut self, store: &mut WidgetStore, theme: &Theme) -> Scene {
        if let Some(root) = store.root() {
            store.set_viewport(Size::new(self.width as f64, self.height as f64));
            synui_widgets::sync_pre_layout(store);
            if store.needs_layout() {
                let layout_full = store.layout_is_full();
                let mut partial_ids = Vec::new();
                let mut old_bounds = Vec::new();
                if layout_full {
                    self.layout.mark_structure_changed();
                    store.note_full_layout();
                } else {
                    let dirty = store.take_dirty_layout_widgets();
                    if dirty.is_empty() {
                        store.note_full_layout();
                    } else {
                        for &id in &dirty {
                            if let Some(w) = store.get(id) {
                                old_bounds.push(w.bounds());
                            }
                            self.layout.mark_dirty(id, store);
                        }
                        store.note_partial_layout(dirty.len() as u32);
                        partial_ids = dirty.into_iter().collect();
                    }
                }
                self.layout.compute(
                    store,
                    &mut self.text,
                    root,
                    Size::new(self.width as f64, self.height as f64),
                );
                synui_widgets::sync_post_layout(store, &mut self.text);
                store.clear_layout_dirty();
                if layout_full || partial_ids.is_empty() {
                    store.mark_paint_dirty();
                } else {
                    for rect in old_bounds {
                        store.mark_paint_dirty_rect(rect);
                    }
                    for id in partial_ids {
                        store.mark_paint_dirty_widget(id);
                    }
                }
            }
            synui_widgets::sync_pre_paint(store, &mut self.text);
        }
        if !store.needs_layout() && !store.needs_paint() && !self.scene.encoding().is_empty() {
            return self.scene.clone();
        }
        if !store.needs_paint() {
            // Layout already applied paint marks; if somehow missing, full paint.
            store.mark_paint_dirty();
        }
        let theme_ref = theme.to_theme_ref();
        let mut paint_ctx = PaintCtx {
            bounds: Rect::new(0.0, 0.0, self.width as f64, self.height as f64),
            theme: &theme_ref,
            text: Some(&mut self.text),
            icons: Some(&mut self.icons),
            images: Some(&mut self.images),
            lotties: None,
            text_color: None,
            font_size: None,
        };
        // Phase 32: force Full compact after Damaged-frame budget.
        store.prepare_damage_compact();
        let paint_mode = store.paint_mode();
        match paint_mode {
            PaintMode::Full => {
                self.scene.reset();
                store.paint_all(&mut self.scene, &mut paint_ctx);
            }
            PaintMode::Damaged => {
                let _ = store.paint_frame(&mut self.scene, &mut paint_ctx);
            }
        }
        store.note_paint_presented(paint_mode);
        store.clear_paint_dirty();
        self.scene.clone()
    }

    /// Render store to RGBA8 pixels via WGPU texture readback.
    pub fn render_to_rgba(&mut self, store: &mut WidgetStore, theme: &Theme) -> Vec<u8> {
        let scene = self.render_store(store, theme);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("synui_snapshot"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let params = RenderParams {
            base_color: vello::peniko::Color::TRANSPARENT,
            width: self.width,
            height: self.height,
            antialiasing_method: vello::AaConfig::Area,
        };
        self.renderer
            .render_to_texture(&self.device, &self.queue, &scene, &view, &params)
            .expect("render_to_texture");

        let bytes_per_row = self.width * 4;
        let padded_bytes_per_row = (bytes_per_row + 255) & !255;
        let buffer_size = (padded_bytes_per_row * self.height) as u64;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback_encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| ());
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in 0..self.height {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + bytes_per_row as usize;
            pixels.extend_from_slice(&data[start..end]);
        }
        drop(data);
        buffer.unmap();
        pixels
    }

    pub fn render_to_png(
        &mut self,
        store: &mut WidgetStore,
        theme: &Theme,
    ) -> Result<Vec<u8>, image::ImageError> {
        let pixels = self.render_to_rgba(store, theme);
        crate::snapshot::rgba_to_png(self.width, self.height, &pixels)
    }
}
