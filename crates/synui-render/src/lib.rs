//! Vello-based rendering pipeline for SynUI.

mod backend;
mod bidi;
mod icon;
mod image;
mod lottie;
mod scene_bridge;
mod svg;
mod text;

#[cfg(not(target_arch = "wasm32"))]
pub use backend::VelloHybridBackend;
pub use backend::{
    RenderBackend, RenderBackendKind, RenderError, VelloBackend, VelloCpuBackend, VelloGpuBackend,
};
pub use bidi::{
    arrow_left_moves_to_start, logical_index_for_visual_x, paragraph_direction, visual_caret_x,
};
pub use icon::*;
pub use image::*;
pub use lottie::*;
pub use svg::*;
pub use text::*;

#[cfg(not(target_arch = "wasm32"))]
pub use scene_bridge::HybridSceneSink;
pub use scene_bridge::{replay_vello_scene, SparseSceneSink};
pub use vello::{kurbo, peniko, Scene};
