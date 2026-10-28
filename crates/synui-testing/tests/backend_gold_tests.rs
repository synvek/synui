//! Req 2.9 backend pixel parity (Phase 11–12): Gpu vs Cpu headless paths.
//!
//! Strict ≤1/256 applies to solid fills without text AA. Text/AA widgets use a
//! wider tolerance (see DEVELOPMENT.md). Hybrid: kind construction + scene-bridge
//! replay (windowless); full Hybrid wgpu raster remains surface-bound.

use synui_core::{
    alloc_widget_id, A11yProps, Constraints, Event, EventCtx, EventResult, LayoutStyle, MeasureCtx,
    PaintCtx, Rect, Size, Style, Widget, WidgetId, WidgetStore,
};
use synui_render::{replay_vello_scene, HybridSceneSink, RenderBackend, RenderBackendKind};
use synui_testing::{
    assert_rgba_within_tolerance, snapshot_rgba_cpu, snapshot_rgba_gpu, HeadlessRenderer,
};
use synui_theme::{DesignSystem, Theme};
use synui_widgets::{Button, Label, StyledBox, Switch};
use vello::Scene;
use vello_hybrid::{Resources, Scene as HybridScene};

const W: u32 = 64;
const H: u32 = 32;
/// Req 2.9 strict tolerance for solid (non-AA text) fills.
const TOL_STRICT: u8 = 1;
/// Practical tolerance for text / AA edges between Vello GPU-compute and CPU paths.
const TOL_AA: u8 = 64;

fn theme_default() -> Theme {
    Theme::default()
}

fn theme_cupertino() -> Theme {
    Theme::new(
        DesignSystem::Cupertino,
        vello::peniko::Color::from_rgb8(0, 122, 255),
        false,
    )
}

fn theme_fluent() -> Theme {
    Theme::new(
        DesignSystem::Fluent,
        vello::peniko::Color::from_rgb8(0, 120, 212),
        false,
    )
}

/// Solid rectangle — no text, for strict Req 2.9.
struct SolidRect {
    id: WidgetId,
    bounds: Rect,
    color: vello::peniko::Color,
}

impl SolidRect {
    fn new(color: vello::peniko::Color) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            color,
        }
    }
}

impl Widget for SolidRect {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;

        static S: OnceLock<LayoutStyle> = OnceLock::new();

        S.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: synui_core::FlexDirection::Row,
            justify_content: synui_core::JustifyContent::FlexStart,
            align_items: synui_core::AlignItems::Stretch,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            width: synui_core::Dimension::Fill,
            height: synui_core::Dimension::Fill,
            padding: [0.0; 4],
            gap: 0.0,
            ..LayoutStyle::base()
        })
    }
    fn measure(&mut self, _ctx: &mut MeasureCtx, c: Constraints) -> Size {
        Size::new(c.max_width as f64, c.max_height as f64)
    }
    fn paint(&self, _ctx: &mut PaintCtx, scene: &mut Scene) {
        synui_render::paint_simple_rect(scene, self.bounds, self.color, 0.0);
    }
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event) -> EventResult {
        EventResult::IGNORED
    }
    fn a11y(&self) -> A11yProps {
        A11yProps::default()
    }
}

#[cfg_attr(
    target_os = "macos",
    ignore = "Vello GPU-compute headless parity is validated on Linux CI"
)]
#[test]
fn backend_gpu_cpu_solid_fill_strict() {
    let mut store = WidgetStore::new();
    let id = store.insert(SolidRect::new(vello::peniko::Color::from_rgb8(200, 40, 40)));
    store.set_root(id);
    let theme = theme_default();
    let gpu = snapshot_rgba_gpu(&mut store, &theme, W, H);
    let cpu = snapshot_rgba_cpu(&mut store, &theme, W, H);
    assert_rgba_within_tolerance(&gpu, &cpu, W, H, TOL_STRICT).expect("Gpu↔Cpu solid fill");
}

#[cfg_attr(
    target_os = "macos",
    ignore = "Vello GPU-compute headless parity is validated on Linux CI"
)]
#[test]
fn backend_gpu_cpu_styled_box_background() {
    let mut store = WidgetStore::new();
    let style = Style::from_css("width:fill;height:fill;background:#c82828;border-radius:4")
        .expect("style");
    let id = store.insert(StyledBox::new(style));
    store.set_root(id);
    let theme = theme_default();
    let gpu = snapshot_rgba_gpu(&mut store, &theme, W, H);
    let cpu = snapshot_rgba_cpu(&mut store, &theme, W, H);
    assert_rgba_within_tolerance(&gpu, &cpu, W, H, TOL_STRICT).expect("Gpu↔Cpu StyledBox");
}

#[cfg_attr(
    target_os = "macos",
    ignore = "Vello GPU-compute headless parity is validated on Linux CI"
)]
#[test]
fn backend_gpu_cpu_label_aa() {
    let mut store = WidgetStore::new();
    let id = store.insert(Label::new("Hello"));
    store.set_root(id);
    let theme = theme_default();
    let gpu = snapshot_rgba_gpu(&mut store, &theme, 120, 48);
    let cpu = snapshot_rgba_cpu(&mut store, &theme, 120, 48);
    assert_rgba_within_tolerance(&gpu, &cpu, 120, 48, TOL_AA).expect("Gpu↔Cpu label AA");
}

#[cfg_attr(
    target_os = "macos",
    ignore = "Vello GPU-compute headless parity is validated on Linux CI"
)]
#[test]
fn backend_gpu_cpu_cupertino_switch_aa() {
    let mut store = WidgetStore::new();
    let mut sw = Switch::new();
    sw.on = true;
    let id = store.insert(sw);
    store.set_root(id);
    let theme = theme_cupertino();
    let gpu = snapshot_rgba_gpu(&mut store, &theme, 120, 48);
    let cpu = snapshot_rgba_cpu(&mut store, &theme, 120, 48);
    assert_rgba_within_tolerance(&gpu, &cpu, 120, 48, TOL_AA).expect("Gpu↔Cpu Switch AA");
}

#[cfg_attr(
    target_os = "macos",
    ignore = "Vello GPU-compute headless parity is validated on Linux CI"
)]
#[test]
fn backend_gpu_cpu_fluent_button_aa() {
    let mut store = WidgetStore::new();
    let id = store.insert(Button::new("Fluent"));
    store.set_root(id);
    let theme = theme_fluent();
    let gpu = snapshot_rgba_gpu(&mut store, &theme, 120, 48);
    let cpu = snapshot_rgba_cpu(&mut store, &theme, 120, 48);
    assert_rgba_within_tolerance(&gpu, &cpu, 120, 48, TOL_AA).expect("Gpu↔Cpu Fluent AA");
}

#[test]
fn backend_hybrid_kind_and_scene_bridge_replay() {
    let backend = RenderBackend::new(RenderBackendKind::Hybrid);
    assert_eq!(backend.kind(), RenderBackendKind::Hybrid);

    let mut store = WidgetStore::new();
    let id = store.insert(SolidRect::new(vello::peniko::Color::from_rgb8(200, 40, 40)));
    store.set_root(id);
    let theme = theme_default();
    let mut headless = HeadlessRenderer::with_cpu(W, H, false);
    let scene = headless.render_store(&mut store, &theme);
    assert!(
        !scene.encoding().is_empty(),
        "vello scene must contain draws"
    );

    let mut hybrid = HybridScene::new(W as u16, H as u16);
    let mut resources = Resources::new();
    let mut sink = HybridSceneSink {
        scene: &mut hybrid,
        resources: &mut resources,
    };
    replay_vello_scene(&scene, &mut sink).expect("Hybrid scene bridge replay");
    assert_eq!(hybrid.width(), W as u16);
    assert_eq!(hybrid.height(), H as u16);
}
