//! Render backend selection (GPU / CPU / hybrid).

mod vello_cpu_backend;
mod vello_gpu_backend;
#[cfg(not(target_arch = "wasm32"))]
mod vello_hybrid_backend;

pub use vello_cpu_backend::VelloCpuBackend;
pub use vello_gpu_backend::VelloGpuBackend;
#[cfg(not(target_arch = "wasm32"))]
pub use vello_hybrid_backend::VelloHybridBackend;

use peniko::Color;
use std::sync::Arc;
use thiserror::Error;
use vello::Scene;
use winit::window::Window;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("surface error: {0}")]
    Surface(String),
    #[error("vello gpu error: {0}")]
    Gpu(#[from] vello::Error),
    #[cfg(not(target_arch = "wasm32"))]
    #[error("vello hybrid error: {0}")]
    Hybrid(#[from] vello_hybrid::RenderError),
    #[cfg(target_arch = "wasm32")]
    #[error("vello hybrid unavailable on wasm32")]
    HybridUnavailable,
    #[error("scene bridge error: {0}")]
    SceneBridge(String),
}

/// Which Vello implementation drives presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderBackendKind {
    /// Full GPU compute pipeline (`vello` crate).
    #[default]
    Gpu,
    /// Software sparse-strips renderer (`vello_cpu` crate).
    Cpu,
    /// CPU path setup + GPU rasterization (`vello_hybrid` crate).
    /// On `wasm32` this falls back to [`Self::Cpu`] (hybrid links a second wgpu).
    Hybrid,
}

/// Unified entry point for SynUI window rendering.
pub enum RenderBackend {
    Gpu(VelloGpuBackend),
    Cpu(VelloCpuBackend),
    #[cfg(not(target_arch = "wasm32"))]
    Hybrid(VelloHybridBackend),
}

impl Default for RenderBackend {
    fn default() -> Self {
        Self::Gpu(VelloGpuBackend::new())
    }
}

impl RenderBackend {
    pub fn new(kind: RenderBackendKind) -> Self {
        match kind {
            RenderBackendKind::Gpu => Self::Gpu(VelloGpuBackend::new()),
            RenderBackendKind::Cpu => Self::Cpu(VelloCpuBackend::new()),
            #[cfg(not(target_arch = "wasm32"))]
            RenderBackendKind::Hybrid => Self::Hybrid(VelloHybridBackend::new()),
            #[cfg(target_arch = "wasm32")]
            RenderBackendKind::Hybrid => Self::Cpu(VelloCpuBackend::new()),
        }
    }

    pub fn kind(&self) -> RenderBackendKind {
        match self {
            Self::Gpu(_) => RenderBackendKind::Gpu,
            Self::Cpu(_) => RenderBackendKind::Cpu,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Hybrid(_) => RenderBackendKind::Hybrid,
        }
    }

    pub async fn create_surface(
        &mut self,
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        match self {
            Self::Gpu(b) => b.create_surface(window, width, height).await,
            Self::Cpu(b) => b.create_surface(window, width, height).await,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Hybrid(b) => b.create_surface(window, width, height).await,
        }
    }

    pub fn resize_surface(&mut self, width: u32, height: u32) {
        match self {
            Self::Gpu(b) => b.resize_surface(width, height),
            Self::Cpu(b) => b.resize_surface(width, height),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Hybrid(b) => b.resize_surface(width, height),
        }
    }

    pub fn render_scene(
        &mut self,
        scene: &Scene,
        base_color: Color,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        match self {
            Self::Gpu(b) => b.render_scene(scene, base_color, width, height),
            Self::Cpu(b) => b.render_scene(scene, base_color, width, height),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Hybrid(b) => b.render_scene(scene, base_color, width, height),
        }
    }
}

/// Legacy alias: default GPU backend type.
pub type VelloBackend = RenderBackend;
