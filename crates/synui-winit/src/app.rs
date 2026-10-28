//! Application event loop and frame rendering.

use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use synui_a11y::A11yBridge;
use synui_core::{
    A11yAction, Event, Key, NamedKey, PaintCtx, PaintMode, PointerButton, PointerCursor, Rect, Size,
    WidgetStore,
};
use synui_i18n::L10n;
use synui_layout::LayoutEngine;
use synui_platform::{AppLifecycle, DesktopLifecycle, LifecycleEvent};
use synui_render::{
    IconCache, ImageCache, LottieCache, RenderBackend, RenderBackendKind, TextRenderer,
};
use synui_theme::{DesignSystem, Theme};
use vello::kurbo::{Affine, Point};
use vello::Scene;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};
use winit::window::{CursorIcon, WindowAttributes, WindowId};

use crate::window::WindowState;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering as AtomicOrdering};

/// When true, theme flips always `scene.reset()` (debug escape hatch).
static FORCE_THEME_FULL_RESET: AtomicBool = AtomicBool::new(false);
static SCENE_RESET_COUNT: AtomicU32 = AtomicU32::new(0);

/// Phase 40: force Full scene reset on every theme change (default false).
pub fn force_theme_full_reset(enabled: bool) {
    FORCE_THEME_FULL_RESET.store(enabled, AtomicOrdering::Relaxed);
}

pub fn force_theme_full_reset_enabled() -> bool {
    FORCE_THEME_FULL_RESET.load(AtomicOrdering::Relaxed)
        || std::env::var_os("SYNUI_THEME_FULL_RESET").is_some()
}

/// Number of retained-scene `reset()` calls since last take (tests / metrics).
pub fn take_scene_reset_count() -> u32 {
    SCENE_RESET_COUNT.swap(0, AtomicOrdering::Relaxed)
}

fn note_scene_reset() {
    SCENE_RESET_COUNT.fetch_add(1, AtomicOrdering::Relaxed);
}

/// When true, overlay open/close/move always Full-paint (debug escape hatch).
static FORCE_OVERLAY_FULL_PAINT: AtomicBool = AtomicBool::new(false);

pub fn force_overlay_full_paint(enabled: bool) {
    FORCE_OVERLAY_FULL_PAINT.store(enabled, AtomicOrdering::Relaxed);
}

/// Kept for debug/env compatibility; overlay structure paint is always Full now.
#[allow(dead_code)]
pub fn force_overlay_full_paint_enabled() -> bool {
    FORCE_OVERLAY_FULL_PAINT.load(AtomicOrdering::Relaxed)
        || std::env::var_os("SYNUI_OVERLAY_FULL_PAINT").is_some()
}

/// Phase 40/41: classify theme mutation paint policy (testable without a window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePaintPolicy {
    /// Light/dark token flip — Damaged paint, no `scene.reset`.
    DamagedNoReset,
    /// Design/seed / forced — Full `scene.reset`.
    FullReset,
}

pub fn theme_change_paint_policy(
    dark_changed: bool,
    design_changed: bool,
    seed_changed: bool,
    force_full: bool,
) -> Option<ThemePaintPolicy> {
    if !(dark_changed || design_changed || seed_changed) {
        return None;
    }
    let light_dark_only = dark_changed && !design_changed && !seed_changed;
    if light_dark_only && !force_full {
        Some(ThemePaintPolicy::DamagedNoReset)
    } else {
        Some(ThemePaintPolicy::FullReset)
    }
}

fn collect_overlay_bounds(store: &WidgetStore) -> Vec<Rect> {
    store
        .overlay_stack()
        .iter()
        .filter_map(|e| store.get(e.root).map(|w| w.bounds()))
        .collect()
}

/// Phase 41: overlay open/close/move must Full-paint. Damaged dirty-rect erase
/// alone left MenuBar popups blank and wiped nav chrome into empty columns until
/// a later Full compact — especially with closed Dialog/Drawer in the tree.
fn mark_overlay_structure_paint(store: &mut WidgetStore, _before: &[Rect]) {
    let _ = _before;
    store.mark_paint_dirty();
}

/// User events for AccessKit, DomA11y, and desktop menu integration.
#[derive(Debug)]
pub enum UserEvent {
    AccessKit(accesskit_winit::Event),
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    Menu(muda::MenuEvent),
    /// DomA11y button click from the browser ARIA host (widget id key).
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    DomA11yClick(u64),
    /// A cross-thread UI post was queued; process it on the next redraw.
    UiPost,
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::AccessKit(event)
    }
}

/// Application driver trait for custom UI setup.
pub trait AppDriver: 'static {
    fn build_ui(&mut self, store: &mut WidgetStore) -> synui_core::WidgetId;
    fn on_frame(&mut self, _store: &mut WidgetStore) {}
    /// Called once the native event loop proxy is available, allowing drivers to
    /// connect cross-thread schedulers to the host wakeup mechanism.
    fn on_event_loop_ready(&mut self, _proxy: EventLoopProxy<UserEvent>) {}
    /// Return a theme to apply each frame (e.g. light/dark toggle).
    fn theme(&mut self) -> Option<Theme> {
        None
    }
    /// Phase 40: pointer hover target changed — refresh `:hover` styles without `view_fn`.
    fn on_hover_changed(&mut self, _store: &mut WidgetStore, _hover: Option<synui_core::WidgetId>) {
    }
    /// Phase 41: keyboard / pointer focus changed — refresh `:focus` styles without `view_fn`.
    fn on_focus_changed(&mut self, _store: &mut WidgetStore, _focus: Option<synui_core::WidgetId>) {
    }
    /// Handle a desktop menu item activation (id from [`crate::menu_id`] or custom).
    fn on_menu(&mut self, _id: &str, _store: &mut WidgetStore) {}
    /// System back / Escape when no overlay consumed it. Return true if handled.
    fn on_back(&mut self, _store: &mut WidgetStore) -> bool {
        false
    }
    /// Platform feature unavailable (e.g. Linux menu without GTK). Req 1.4.
    fn on_platform_fallback(&mut self, _feature: &str, _store: &mut WidgetStore) {}
}

/// SynUI application host.
pub struct Application<D: AppDriver> {
    backend_kind: RenderBackendKind,
    window_state: Option<WindowState>,
    scene: Scene,
    store: WidgetStore,
    layout: LayoutEngine,
    text: TextRenderer,
    icons: IconCache,
    images: ImageCache,
    lotties: LottieCache,
    theme: Theme,
    design_system: DesignSystem,
    l10n: L10n,
    #[allow(dead_code)] // AccessKit path on native; DomA11y on wasm
    a11y: A11yBridge,
    lifecycle: Box<dyn AppLifecycle>,
    driver: D,
    root_built: bool,
    #[cfg(not(target_arch = "wasm32"))]
    accesskit_adapter: Option<accesskit_winit::Adapter>,
    event_loop_proxy: Option<EventLoopProxy<UserEvent>>,
    modifiers: ModifiersState,
    needs_redraw: bool,
    last_frame: Instant,
    /// When set, `WaitUntil` was armed for caret blink; used to credit wall-clock dt.
    cursor_blink_wake_at: Option<Instant>,
    ime_allowed: bool,
    ime_cursor_area: Option<(f64, f64, f64, f64)>,
    last_pointer_cursor: PointerCursor,
    last_structure_version: u64,
    last_content_epoch: u64,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    desktop_menu: Option<crate::DesktopMenu>,
    /// Extra windows with independent surfaces (Phase 10).
    extra_windows: std::collections::HashMap<WindowId, WindowState>,
    window_title: String,
    /// Open a second window on resume (demo / tests).
    second_window_title: Option<String>,
    /// Menu/tray requested another window (consumed on next AboutToWait).
    pending_new_window: Option<String>,
}

impl<D: AppDriver> Application<D> {
    pub fn new(driver: D) -> Self {
        let mut l10n = L10n::new("en-US");
        let _ = l10n.add_bundle(
            "en-US",
            "hello = Hello, SynUI!\ncounter = Count: { $count }",
        );
        Self {
            backend_kind: {
                #[cfg(target_arch = "wasm32")]
                {
                    // Avoid hanging on WebGPU adapter request in headless / no-GPU browsers.
                    // DomA11y and input still work; Gpu can be selected via with_render_backend.
                    RenderBackendKind::Cpu
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    RenderBackendKind::default()
                }
            },
            window_state: None,
            scene: Scene::new(),
            store: WidgetStore::new(),
            layout: LayoutEngine::new(),
            text: TextRenderer::new(),
            icons: IconCache::new(),
            images: ImageCache::new(),
            lotties: LottieCache::new(),
            theme: Theme::default(),
            design_system: DesignSystem::default(),
            l10n,
            a11y: A11yBridge::new(),
            lifecycle: platform_lifecycle(),
            driver,
            root_built: false,
            #[cfg(not(target_arch = "wasm32"))]
            accesskit_adapter: None,
            event_loop_proxy: None,
            modifiers: ModifiersState::empty(),
            needs_redraw: true,
            last_frame: Instant::now(),
            cursor_blink_wake_at: None,
            ime_allowed: false,
            ime_cursor_area: None,
            last_pointer_cursor: PointerCursor::Default,
            last_structure_version: u64::MAX,
            last_content_epoch: u64::MAX,
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            desktop_menu: None,
            extra_windows: std::collections::HashMap::new(),
            window_title: "SynUI".into(),
            second_window_title: None,
            pending_new_window: None,
        }
    }

    /// Attach a native File menu (Open… / Quit). No-op when menus are unavailable.
    #[allow(unused_mut)]
    pub fn with_menu(mut self, menu: crate::DesktopMenu) -> Self {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            self.desktop_menu = Some(menu);
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = menu;
        }
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.window_title = title.into();
        self
    }

    /// Build the widget tree once before the first paint / input (Phase 25).
    ///
    /// CursorMoved can arrive before the first `RedrawRequested`; without this,
    /// `dispatch_event` panics on a missing root.
    fn ensure_ui_built(&mut self) {
        if self.root_built {
            return;
        }
        let root = self.driver.build_ui(&mut self.store);
        self.store.set_root(root);
        self.root_built = true;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let focus = self.store.focus_manager().focused();
            if let Some(adapter) = self.accesskit_adapter.as_mut() {
                let a11y = &mut self.a11y;
                let store = &self.store;
                adapter.update_if_active(|| a11y.build_tree_update(store, focus));
            }
        }
        #[cfg(all(feature = "web", target_arch = "wasm32"))]
        {
            let mut tree = synui_platform::web::DomA11yTree::from_store(&self.store);
            tree.host_element_id = Some("synui-a11y-host".into());
            tree.sync_to_dom();
        }
    }

    /// Request a second window to be created when the app resumes.
    pub fn with_second_window(mut self, title: impl Into<String>) -> Self {
        self.second_window_title = Some(title.into());
        self
    }

    pub fn with_render_backend(mut self, kind: RenderBackendKind) -> Self {
        self.backend_kind = kind;
        self
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.design_system = theme.design_system;
        self.theme = theme;
        self
    }

    /// Set visual design language (MD3, Fluent, Cupertino). Overrides colors, corners, and motion.
    pub fn with_design_system(mut self, design_system: DesignSystem) -> Self {
        self.design_system = design_system;
        self.theme = self.theme.clone().with_design_system(design_system);
        self.store.mark_dirty();
        self
    }

    pub fn design_system(&self) -> DesignSystem {
        self.design_system
    }

    pub fn with_l10n(mut self, l10n: L10n) -> Self {
        self.l10n = l10n;
        self
    }

    pub fn store_mut(&mut self) -> &mut WidgetStore {
        &mut self.store
    }

    pub fn l10n(&self) -> &L10n {
        &self.l10n
    }

    pub fn icons_mut(&mut self) -> &mut IconCache {
        &mut self.icons
    }

    pub fn images_mut(&mut self) -> &mut ImageCache {
        &mut self.images
    }

    pub fn lotties_mut(&mut self) -> &mut LottieCache {
        &mut self.lotties
    }

    /// Update platform safe-area insets on the lifecycle + store (mobile hosts).
    pub fn set_safe_area_insets(&mut self, insets: [f32; 4]) {
        self.lifecycle.set_safe_area_insets(insets);
        self.store.set_safe_area(insets);
    }

    pub fn run(
        mut self,
        event_loop: EventLoop<UserEvent>,
    ) -> Result<(), winit::error::EventLoopError> {
        let proxy = event_loop.create_proxy();
        self.event_loop_proxy = Some(proxy.clone());
        self.driver.on_event_loop_ready(proxy);
        #[cfg(all(feature = "web", target_arch = "wasm32"))]
        {
            let proxy = self.event_loop_proxy.clone().expect("proxy");
            synui_platform::set_dom_a11y_click_handler(move |key| {
                let _ = proxy.send_event(UserEvent::DomA11yClick(key));
            });
        }
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        if self.desktop_menu.is_some() {
            let proxy = self.event_loop_proxy.clone().expect("proxy");
            muda::MenuEvent::set_event_handler(Some(move |event| {
                let _ = proxy.send_event(UserEvent::Menu(event));
            }));
        }
        event_loop.run_app(&mut self)
    }

    /// Open an additional window with its own surface. Shares the same driver/store.
    pub fn open_extra_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        title: &str,
    ) -> Option<WindowId> {
        let attrs = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(LogicalSize::new(480.0, 360.0));
        let window = Arc::new(event_loop.create_window(attrs).ok()?);
        let id = window.id();
        let mut ws = WindowState::new(window, self.backend_kind);
        if let Err(e) = pollster::block_on(ws.backend.create_surface(
            ws.window.clone(),
            ws.width,
            ws.height,
        )) {
            tracing::error!("synui: extra window create_surface failed: {e}");
            return None;
        }
        ws.window.request_redraw();
        self.extra_windows.insert(id, ws);
        self.request_redraw();
        Some(id)
    }

    pub fn extra_window_count(&self) -> usize {
        self.extra_windows.len()
    }

    /// Queue a new window (e.g. from menu) when the event loop is available.
    pub fn request_new_window(&mut self, title: impl Into<String>) {
        self.pending_new_window = Some(title.into());
    }

    fn window_mut(&mut self, id: WindowId) -> Option<&mut WindowState> {
        if self
            .window_state
            .as_ref()
            .map(|w| w.window.id() == id)
            .unwrap_or(false)
        {
            self.window_state.as_mut()
        } else {
            self.extra_windows.get_mut(&id)
        }
    }

    fn is_primary(&self, id: WindowId) -> bool {
        self.window_state
            .as_ref()
            .map(|w| w.window.id() == id)
            .unwrap_or(false)
    }

    fn window_scale_factor(&self, id: WindowId) -> f64 {
        if self
            .window_state
            .as_ref()
            .map(|w| w.window.id() == id)
            .unwrap_or(false)
        {
            self.window_state
                .as_ref()
                .map(WindowState::scale_factor)
                .unwrap_or(1.0)
        } else {
            self.extra_windows
                .get(&id)
                .map(WindowState::scale_factor)
                .unwrap_or(1.0)
        }
    }

    fn logical_point(&self, id: WindowId, x: f64, y: f64) -> Point {
        let scale_factor = self.window_scale_factor(id);
        Point::new(x / scale_factor, y / scale_factor)
    }

    fn window_cursor(&self, id: WindowId) -> Point {
        if self
            .window_state
            .as_ref()
            .map(|w| w.window.id() == id)
            .unwrap_or(false)
        {
            self.window_state
                .as_ref()
                .map(WindowState::cursor)
                .unwrap_or(Point::ZERO)
        } else {
            self.extra_windows
                .get(&id)
                .map(WindowState::cursor)
                .unwrap_or(Point::ZERO)
        }
    }

    fn sync_pointer_cursor(&mut self, id: WindowId, pos: Point) {
        let cursor = self.store.pointer_cursor_at(pos);
        if cursor == self.last_pointer_cursor {
            return;
        }
        self.last_pointer_cursor = cursor;
        let icon = pointer_cursor_to_winit(cursor);
        if let Some(ws) = self.window_mut(id) {
            ws.window.set_cursor(icon);
        }
    }

    fn request_redraw(&mut self) {
        self.needs_redraw = true;
        if let Some(ws) = &self.window_state {
            ws.window.request_redraw();
        }
        for ws in self.extra_windows.values() {
            ws.window.request_redraw();
        }
    }

    fn sync_ime_state(&mut self) {
        let Some(ws) = &self.window_state else {
            return;
        };
        let window = &ws.window;
        if let Some(id) = self.store.focus_manager().focused() {
            if let Some(w) = self.store.get(id) {
                if let Some(tf) = w.as_any().downcast_ref::<synui_widgets::TextField>() {
                    if !self.ime_allowed {
                        window.set_ime_allowed(true);
                        self.ime_allowed = true;
                        self.ime_cursor_area = None;
                    }
                    let area = tf.ime_cursor_area();
                    let width = area.width().max(1.0);
                    let height = area.height().max(1.0);
                    let key = (area.x0, area.y0, width, height);
                    if self.ime_cursor_area != Some(key) {
                        window.set_ime_cursor_area(
                            LogicalPosition::new(area.x0, area.y0),
                            LogicalSize::new(width, height),
                        );
                        self.ime_cursor_area = Some(key);
                    }
                    self.lifecycle.set_soft_keyboard_visible(true);
                    return;
                }
                if let Some(ta) = w.as_any().downcast_ref::<synui_widgets::TextArea>() {
                    if !self.ime_allowed {
                        window.set_ime_allowed(true);
                        self.ime_allowed = true;
                        self.ime_cursor_area = None;
                    }
                    let area = ta.ime_cursor_area();
                    let width = area.width().max(1.0);
                    let height = area.height().max(1.0);
                    let key = (area.x0, area.y0, width, height);
                    if self.ime_cursor_area != Some(key) {
                        window.set_ime_cursor_area(
                            LogicalPosition::new(area.x0, area.y0),
                            LogicalSize::new(width, height),
                        );
                        self.ime_cursor_area = Some(key);
                    }
                    self.lifecycle.set_soft_keyboard_visible(true);
                    return;
                }
                if let Some(re) = w.as_any().downcast_ref::<synui_widgets::RichEdit>() {
                    if !self.ime_allowed {
                        window.set_ime_allowed(true);
                        self.ime_allowed = true;
                        self.ime_cursor_area = None;
                    }
                    let area = re.ime_cursor_area();
                    let width = area.width().max(1.0);
                    let height = area.height().max(1.0);
                    let key = (area.x0, area.y0, width, height);
                    if self.ime_cursor_area != Some(key) {
                        window.set_ime_cursor_area(
                            LogicalPosition::new(area.x0, area.y0),
                            LogicalSize::new(width, height),
                        );
                        self.ime_cursor_area = Some(key);
                    }
                    self.lifecycle.set_soft_keyboard_visible(true);
                    return;
                }
            }
        }
        if self.ime_allowed {
            window.set_ime_allowed(false);
            self.ime_allowed = false;
            self.ime_cursor_area = None;
        }
        self.lifecycle.set_soft_keyboard_visible(false);
    }
}

impl<D: AppDriver> ApplicationHandler<UserEvent> for Application<D> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window_state.is_some() {
            return;
        }
        self.lifecycle.on_lifecycle(LifecycleEvent::Resumed);
        #[allow(unused_mut)]
        let mut attrs = WindowAttributes::default()
            .with_title(self.window_title.clone())
            .with_inner_size(LogicalSize::new(800.0, 600.0))
            .with_visible(false);
        #[cfg(all(feature = "web", target_arch = "wasm32"))]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            if let Some(canvas) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("synui-canvas"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            {
                attrs = attrs.with_canvas(Some(canvas));
            }
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let mut ws = WindowState::new(window.clone(), self.backend_kind);
        #[cfg(all(feature = "web", target_arch = "wasm32"))]
        let surface_ok = {
            // Headless browsers often hang forever in wgpu adapter requests.
            // Boot without a surface so DomA11y + input still run; try GPU later if needed.
            show_web_failure_page(
                "SynUI: deferred WebGPU surface (DomA11y-first boot). Canvas may stay blank.",
            );
            false
        };
        #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
        let surface_ok = match pollster::block_on(ws.backend.create_surface(
            ws.window.clone(),
            ws.width,
            ws.height,
        )) {
            Ok(()) => true,
            Err(e) => {
                tracing::error!("synui: create_surface failed: {e}");
                // Phase 10: try Cpu once when primary GPU/hybrid surface fails.
                if self.backend_kind != RenderBackendKind::Cpu {
                    tracing::warn!("synui: retrying surface with RenderBackendKind::Cpu");
                    self.backend_kind = RenderBackendKind::Cpu;
                    ws.backend = RenderBackend::new(RenderBackendKind::Cpu);
                    match pollster::block_on(ws.backend.create_surface(
                        ws.window.clone(),
                        ws.width,
                        ws.height,
                    )) {
                        Ok(()) => true,
                        Err(e2) => {
                            tracing::error!("synui: Cpu create_surface also failed: {e2}");
                            false
                        }
                    }
                } else {
                    false
                }
            }
        };
        let _ = surface_ok;
        let _ = &mut ws;

        #[cfg(not(target_arch = "wasm32"))]
        {
            let proxy = self.event_loop_proxy.clone().expect("event loop proxy");
            let adapter =
                accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &window, proxy);
            self.accesskit_adapter = Some(adapter);
        }
        window.set_visible(true);

        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        if let Some(menu) = &self.desktop_menu {
            match menu.attach(&window) {
                Ok(attached) => {
                    if !attached {
                        self.driver.on_platform_fallback("menu", &mut self.store);
                    }
                }
                Err(e) => {
                    tracing::warn!("synui: desktop menu attach failed: {e}");
                    self.driver.on_platform_fallback("menu", &mut self.store);
                }
            }
        }
        self.window_state = Some(ws);
        if let Some(title) = self.second_window_title.take() {
            let _ = self.open_extra_window(event_loop, &title);
        }
        #[cfg(all(feature = "web", target_arch = "wasm32"))]
        {
            // Ensure DomA11y mirrors even if the first RedrawRequested is delayed.
            self.ensure_ui_built();
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                if let Some(host) = document.get_element_by_id("synui-a11y-host") {
                    let tree = synui_platform::web::DomA11yTree::from_store(&self.store);
                    let _ = host
                        .set_attribute("data-synui-a11y-count", &tree.elements.len().to_string());
                    let _ = host.set_attribute("data-synui-resumed", "1");
                }
            }
        }
        self.request_redraw();
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // WaitUntil expiry must schedule a redraw; otherwise advance_cursor never
        // runs and about_to_wait would re-arm forever with a stale wake time.
        if matches!(cause, StartCause::ResumeTimeReached { .. })
            && (synui_widgets::next_cursor_blink_wake_ms(&self.store).is_some()
                || synui_widgets::next_tooltip_wake_ms(&self.store).is_some())
        {
            self.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(title) = self.pending_new_window.take() {
            let _ = self.open_extra_window(event_loop, &title);
        }
        // Do not re-arm WaitUntil while a redraw is pending — that deadlocks blink
        // (wake_ms stays ~530 until advance_cursor runs inside RedrawRequested).
        if self.needs_redraw {
            return;
        }
        if let Some(wake_ms) = [
            synui_widgets::next_cursor_blink_wake_ms(&self.store),
            synui_widgets::next_tooltip_wake_ms(&self.store),
        ]
        .into_iter()
        .flatten()
        .reduce(f32::min)
        {
            let wake = Instant::now() + std::time::Duration::from_secs_f32(wake_ms / 1000.0);
            self.cursor_blink_wake_at = Some(wake);
            event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.is_primary(id) {
            if let Some(adapter) = &mut self.accesskit_adapter {
                if let Some(ws) = &self.window_state {
                    adapter.process_event(&ws.window, &event);
                }
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                if self.is_primary(id) {
                    self.lifecycle.on_lifecycle(LifecycleEvent::Terminating);
                    event_loop.exit();
                } else {
                    self.extra_windows.remove(&id);
                }
            }
            WindowEvent::Resized(size) => {
                let primary = self.is_primary(id);
                let logical_size = self.window_mut(id).map(|ws| {
                    ws.resize(size.width, size.height);
                    ws.logical_size()
                });
                if primary {
                    if let Some((width, height)) = logical_size {
                        self.store.set_viewport(Size::new(width, height));
                    }
                }
                self.store.mark_dirty();
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let primary = self.is_primary(id);
                let logical_size = self.window_mut(id).map(|ws| {
                    let size = ws.window.inner_size();
                    ws.set_scale_factor(scale_factor);
                    // Scale changes can be delivered before the follow-up Resized
                    // event; keep the physical surface in sync immediately.
                    ws.resize(size.width, size.height);
                    ws.logical_size()
                });
                if primary {
                    if let Some((width, height)) = logical_size {
                        self.store.set_viewport(Size::new(width, height));
                    }
                }
                self.store.mark_dirty();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.ensure_ui_built();
                let now = Instant::now();
                let dt_raw = now.duration_since(self.last_frame).as_secs_f32() * 1000.0;
                // Ripple/lottie: cap so hitch recovery does not jump animations.
                let anim_dt = dt_raw.min(32.0);
                // Caret: credit wall time so a single post-WaitUntil frame can toggle.
                let cursor_dt = if self.cursor_blink_wake_at.map(|t| now >= t).unwrap_or(false) {
                    dt_raw
                } else {
                    anim_dt
                };
                self.cursor_blink_wake_at = None;
                self.last_frame = now;
                let dirty_anim =
                    synui_widgets::tick_animations_ex(&mut self.store, anim_dt, cursor_dt);
                // Tip dwell must use wall time: WaitUntil wakes ~show_delay apart, but
                // capped anim_dt would only credit 32ms per wake (~5s for 500ms delay).
                let tip_ready = synui_widgets::tooltip_tick(&mut self.store, dt_raw);
                if tip_ready {
                    let before_overlays = collect_overlay_bounds(&self.store);
                    synui_widgets::sync_overlays(&mut self.store);
                    mark_overlay_structure_paint(&mut self.store, &before_overlays);
                    self.store.mark_paint_dirty();
                }
                let image_anim = self.images.advance_animations(anim_dt);
                let animating = !dirty_anim.is_empty() || image_anim || tip_ready;
                if !dirty_anim.is_empty() {
                    // Phase 30: morph/lottie/caret — widget-scoped paint damage.
                    for id in dirty_anim {
                        self.store.mark_paint_dirty_widget(id);
                    }
                } else if image_anim {
                    self.store.mark_paint_dirty();
                }
                self.render_frame(id);
                self.needs_redraw = false;
                if animating || self.store.is_dirty() {
                    self.request_redraw();
                } else if let Some(wake_ms) = [
                    synui_widgets::next_cursor_blink_wake_ms(&self.store),
                    synui_widgets::next_tooltip_wake_ms(&self.store),
                ]
                .into_iter()
                .flatten()
                .reduce(f32::min)
                {
                    // Idle with focused editor or pending tip: sleep until next wake.
                    let wake = now + std::time::Duration::from_secs_f32(wake_ms / 1000.0);
                    self.cursor_blink_wake_at = Some(wake);
                    event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
                } else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.ensure_ui_built();
                let pos = self.window_cursor(id);
                let ev = match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) => Some(Event::PointerDown {
                        id: 0,
                        position: pos,
                        button: PointerButton::Primary,
                        shift: self.modifiers.shift_key(),
                        ctrl: self.modifiers.control_key() || self.modifiers.super_key(),
                    }),
                    (ElementState::Released, MouseButton::Left) => Some(Event::PointerUp {
                        id: 0,
                        position: pos,
                        button: PointerButton::Primary,
                    }),
                    _ => None,
                };
                if let Some(e) = ev {
                    if matches!(&e, Event::PointerDown { .. }) {
                        synui_widgets::tooltip_dismiss(&mut self.store);
                    }
                    let prev_focus = self.store.focus_manager().focused();
                    let result = self.store.dispatch_event(&e);
                    self.sync_pointer_cursor(id, pos);
                    let focus = self.store.focus_manager().focused();
                    if focus != prev_focus {
                        self.driver.on_focus_changed(&mut self.store, focus);
                    }
                    let before_overlays = collect_overlay_bounds(&self.store);
                    let had_overlays = !self.store.overlay_stack().is_empty();
                    synui_widgets::sync_overlays(&mut self.store);
                    let has_overlays = !self.store.overlay_stack().is_empty();
                    // Scrollbar thumb drag updates offset without a layout pass —
                    // re-apply scroll transforms so content moves with the thumb.
                    if result.handled {
                        synui_widgets::apply_scroll_views(&mut self.store);
                        synui_widgets::apply_sticky_and_fixed(&mut self.store);
                        synui_widgets::sync_tooltip(&mut self.store);
                    }
                    self.sync_ime_state();
                    // Phase 41: overlay open/close/move → Damaged dirty-rect erase (not Full).
                    if had_overlays || has_overlays {
                        mark_overlay_structure_paint(&mut self.store, &before_overlays);
                    } else {
                        let scroll_host = self.store.hit_test(pos).and_then(|id| {
                            if self
                                .store
                                .get(id)
                                .is_some_and(|w| w.as_any().is::<synui_widgets::ScrollView>())
                            {
                                Some(id)
                            } else {
                                synui_widgets::find_scroll_ancestor(&self.store, id)
                            }
                        });
                        if scroll_host.is_some() {
                            synui_widgets::mark_scroll_paint_dirty(&mut self.store, pos);
                        } else if let Some(id) = self
                            .store
                            .pointer_grab()
                            .or_else(|| self.store.hit_test(pos))
                        {
                            self.store.mark_paint_dirty_widget(id);
                        } else {
                            self.store.mark_paint_dirty();
                        }
                    }
                    self.store.mark_a11y_dirty();
                    self.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ensure_ui_built();
                let pos = self.logical_point(id, position.x, position.y);
                if let Some(ws) = self.window_mut(id) {
                    ws.set_cursor(pos);
                }
                let hit = self.store.hit_test(pos);
                if self.store.set_hovered(hit) {
                    self.driver.on_hover_changed(&mut self.store, hit);
                    self.request_redraw();
                }
                self.sync_pointer_cursor(id, pos);
                synui_widgets::tooltip_on_hover(&mut self.store, hit);
                let e = Event::PointerMove {
                    id: 0,
                    position: pos,
                };
                let result = self.store.dispatch_event(&e);
                if result.handled {
                    self.sync_pointer_cursor(id, pos);
                }
                // Idle mousemove must not force full-tree relayout (Sprint R).
                // Phase 41: scoped Damaged; overlay switch uses dirty-rect erase.
                if result.handled {
                    let before_overlays = collect_overlay_bounds(&self.store);
                    let had_overlays = !self.store.overlay_stack().is_empty();
                    synui_widgets::sync_overlays(&mut self.store);
                    let has_overlays = !self.store.overlay_stack().is_empty();
                    synui_widgets::apply_scroll_views(&mut self.store);
                    synui_widgets::apply_sticky_and_fixed(&mut self.store);
                    synui_widgets::sync_tooltip(&mut self.store);
                    if had_overlays || has_overlays {
                        mark_overlay_structure_paint(&mut self.store, &before_overlays);
                    } else if let Some(id) = self.store.pointer_grab().or(hit) {
                        self.store.mark_paint_dirty_widget(id);
                    } else {
                        self.store.mark_paint_dirty();
                    }
                    self.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.ensure_ui_built();
                let scale_factor = self.window_scale_factor(id);
                let pos = self.window_cursor(id);
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    MouseScrollDelta::PixelDelta(p) => {
                        ((p.x / scale_factor) as f32, (p.y / scale_factor) as f32)
                    }
                };
                let e = Event::Scroll {
                    position: pos,
                    delta_x: dx,
                    delta_y: dy,
                };
                if self.store.dispatch_event(&e).handled {
                    let before_overlays = collect_overlay_bounds(&self.store);
                    synui_widgets::apply_scroll_views(&mut self.store);
                    synui_widgets::apply_sticky_and_fixed(&mut self.store);
                    synui_widgets::sync_tooltip(&mut self.store);
                    // Phase 33: scroll → scoped Damaged (ScrollView viewport), not Full.
                    synui_widgets::mark_scroll_paint_dirty(&mut self.store, pos);
                    if self.store.tooltip_session().shown_anchor.is_some() {
                        mark_overlay_structure_paint(&mut self.store, &before_overlays);
                    }
                    self.request_redraw();
                }
            }
            WindowEvent::Touch(touch) => {
                self.ensure_ui_built();
                use winit::event::TouchPhase;
                let touch_id = (touch.id as u8).min(9);
                let pos = self.logical_point(id, touch.location.x, touch.location.y);
                let ev = match touch.phase {
                    TouchPhase::Started => Some(Event::PointerDown {
                        id: touch_id,
                        position: pos,
                        button: PointerButton::Primary,
                        shift: self.modifiers.shift_key(),
                        ctrl: self.modifiers.control_key() || self.modifiers.super_key(),
                    }),
                    TouchPhase::Moved => Some(Event::PointerMove {
                        id: touch_id,
                        position: pos,
                    }),
                    TouchPhase::Ended => Some(Event::PointerUp {
                        id: touch_id,
                        position: pos,
                        button: PointerButton::Primary,
                    }),
                    TouchPhase::Cancelled => Some(Event::PointerCancel { id: touch_id }),
                };
                if let Some(e) = ev {
                    let prev_focus = self.store.focus_manager().focused();
                    self.store.dispatch_event(&e);
                    let focus = self.store.focus_manager().focused();
                    if focus != prev_focus {
                        self.driver.on_focus_changed(&mut self.store, focus);
                    }
                    let before_overlays = collect_overlay_bounds(&self.store);
                    let had_overlays = !self.store.overlay_stack().is_empty();
                    synui_widgets::sync_overlays(&mut self.store);
                    let has_overlays = !self.store.overlay_stack().is_empty();
                    // Phase 41: touch move → scoped paint; overlays → Damaged rect erase.
                    if had_overlays || has_overlays {
                        mark_overlay_structure_paint(&mut self.store, &before_overlays);
                    } else if matches!(touch.phase, TouchPhase::Moved) {
                        if let Some(id) = self
                            .store
                            .pointer_grab()
                            .or_else(|| self.store.hit_test(pos))
                        {
                            self.store.mark_paint_dirty_widget(id);
                        } else {
                            self.store.mark_paint_dirty();
                        }
                    } else {
                        self.store.mark_paint_dirty();
                    }
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.ensure_ui_built();
                let shift = self.modifiers.shift_key();
                let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
                let alt = self.modifiers.alt_key();
                if let Some(key) = map_key(&event.logical_key) {
                    let ev = if event.state == ElementState::Pressed {
                        Event::KeyDown {
                            key: key.clone(),
                            repeat: event.repeat,
                            shift,
                            ctrl,
                            alt,
                        }
                    } else {
                        Event::KeyUp { key }
                    };
                    let prev_focus = self.store.focus_manager().focused();
                    self.store.dispatch_event(&ev);
                    let focus = self.store.focus_manager().focused();
                    if focus != prev_focus {
                        self.driver.on_focus_changed(&mut self.store, focus);
                    }
                    let before_overlays = collect_overlay_bounds(&self.store);
                    let had_overlays = !self.store.overlay_stack().is_empty();
                    // Typing hot path: skip full-tree overlay scan for plain characters.
                    let typing_char = matches!(
                        &ev,
                        Event::KeyDown {
                            key: Key::Character(_),
                            ctrl: false,
                            ..
                        } | Event::KeyDown {
                            key: Key::Named(
                                NamedKey::Space | NamedKey::Backspace | NamedKey::Delete
                            ),
                            ctrl: false,
                            ..
                        }
                    );
                    if had_overlays || !typing_char {
                        synui_widgets::sync_overlays(&mut self.store);
                    }
                    let has_overlays = !self.store.overlay_stack().is_empty();
                    let mut back_paint = false;
                    if event.state == ElementState::Pressed {
                        if let Event::KeyDown {
                            key: Key::Named(NamedKey::Escape),
                            ..
                        } = &ev
                        {
                            if self.store.overlay_stack().is_empty()
                                && self.driver.on_back(&mut self.store)
                            {
                                self.store.mark_layout_dirty();
                                back_paint = true;
                            }
                        }
                    }
                    self.sync_ime_state();
                    // Phase 41: caret Damaged; overlay open/close → dirty-rect Damaged.
                    if had_overlays || has_overlays {
                        mark_overlay_structure_paint(&mut self.store, &before_overlays);
                    } else if back_paint || self.store.needs_layout() {
                        self.store.mark_paint_dirty();
                    } else if let Some(id) = self.store.focus_manager().focused() {
                        self.store.mark_paint_dirty_widget(id);
                    } else {
                        self.store.mark_paint_dirty();
                    }
                    self.store.mark_a11y_dirty();
                    self.request_redraw();
                }
            }
            WindowEvent::Ime(ime) => {
                use winit::event::Ime;
                match ime {
                    Ime::Preedit(text, _) => {
                        if let Some(id) = self.store.focus_manager().focused() {
                            if let Some(w) = self.store.get_mut(id) {
                                if let Some(tf) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::TextField>()
                                {
                                    tf.ime_composing = text;
                                    tf.notify_text_changed();
                                    // Phase 33: preedit is paint-only on the focused field.
                                    self.store.mark_paint_dirty_widget(id);
                                    self.store.mark_a11y_dirty();
                                    self.request_redraw();
                                } else if let Some(ta) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::TextArea>()
                                {
                                    ta.ime_composing = text;
                                    ta.notify_text_changed();
                                    self.store.mark_paint_dirty_widget(id);
                                    self.store.mark_a11y_dirty();
                                    self.request_redraw();
                                } else if let Some(re) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::RichEdit>()
                                {
                                    re.ime_composing = text;
                                    re.notify_text_changed();
                                    self.store.mark_paint_dirty_widget(id);
                                    self.store.mark_a11y_dirty();
                                    self.request_redraw();
                                }
                            }
                        }
                    }
                    Ime::Enabled => {
                        self.sync_ime_state();
                    }
                    Ime::Disabled => {
                        if let Some(id) = self.store.focus_manager().focused() {
                            if let Some(w) = self.store.get_mut(id) {
                                if let Some(tf) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::TextField>()
                                {
                                    tf.ime_composing.clear();
                                } else if let Some(ta) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::TextArea>()
                                {
                                    ta.ime_composing.clear();
                                } else if let Some(re) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::RichEdit>()
                                {
                                    re.ime_composing.clear();
                                }
                            }
                        }
                        self.sync_ime_state();
                    }
                    Ime::Commit(text) => {
                        // Phase 38: IME Commit Damaged-only (aligned with preedit/caret).
                        if let Some(id) = self.store.focus_manager().focused() {
                            if let Some(w) = self.store.get_mut(id) {
                                if let Some(tf) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::TextField>()
                                {
                                    tf.commit_ime(&text);
                                    self.store.bump_measure_epoch_widget(id);
                                    self.store.mark_paint_dirty_widget(id);
                                    self.store.mark_a11y_dirty();
                                    self.request_redraw();
                                } else if let Some(ta) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::TextArea>()
                                {
                                    ta.commit_ime(&text);
                                    self.store.bump_measure_epoch_widget(id);
                                    self.store.mark_paint_dirty_widget(id);
                                    self.store.mark_a11y_dirty();
                                    self.request_redraw();
                                } else if let Some(re) =
                                    w.as_any_mut().downcast_mut::<synui_widgets::RichEdit>()
                                {
                                    re.commit_ime(&text);
                                    self.store.bump_measure_epoch_widget(id);
                                    self.store.mark_paint_dirty_widget(id);
                                    self.store.mark_a11y_dirty();
                                    self.request_redraw();
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::Occluded(occluded) => {
                if occluded {
                    self.lifecycle.on_lifecycle(LifecycleEvent::Suspended);
                } else {
                    self.lifecycle.on_lifecycle(LifecycleEvent::Resumed);
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::UiPost => {
                self.request_redraw();
            }
            #[cfg(not(target_arch = "wasm32"))]
            UserEvent::AccessKit(ak_event) => {
                if let Some(adapter) = &mut self.accesskit_adapter {
                    use accesskit_winit::WindowEvent as AkWindowEvent;
                    match ak_event.window_event {
                        AkWindowEvent::InitialTreeRequested => {
                            let update = self.a11y.build_tree_update(
                                &self.store,
                                self.store.focus_manager().focused(),
                            );
                            adapter.update_if_active(|| update);
                        }
                        AkWindowEvent::ActionRequested(req) => {
                            use accesskit::{Action, ActionData};
                            let action = match req.action {
                                Action::Click => Some(A11yAction::Click),
                                Action::Focus => Some(A11yAction::Focus),
                                Action::SetValue => Some(A11yAction::SetValue),
                                _ => None,
                            };
                            let value = match req.data {
                                Some(ActionData::Value(value)) => Some(value.into()),
                                _ => None,
                            };
                            if let (Some(action), Some(widget_id)) =
                                (action, self.a11y.widget_for_node(req.target_node))
                            {
                                self.store.dispatch_a11y_action(widget_id, action, value);
                                self.store.mark_dirty();
                                self.store.mark_a11y_dirty();
                                self.request_redraw();
                            }
                        }
                        AkWindowEvent::AccessibilityDeactivated => {}
                    }
                }
            }
            #[cfg(target_arch = "wasm32")]
            UserEvent::AccessKit(_) => {}
            #[cfg(all(feature = "web", target_arch = "wasm32"))]
            UserEvent::DomA11yClick(key) => {
                let widget_id = synui_core::WidgetId(key);
                self.store
                    .dispatch_a11y_action(widget_id, A11yAction::Click, None);
                self.store.mark_dirty();
                self.store.mark_a11y_dirty();
                self.request_redraw();
            }
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            UserEvent::Menu(menu_event) => {
                let id = menu_event.id.as_ref();
                if id == crate::menu_id::NEW_WINDOW {
                    self.request_new_window("SynUI Window");
                } else if id == crate::menu_id::TRAY_QUIT || id == crate::menu_id::QUIT {
                    _el.exit();
                    return;
                } else if id == crate::menu_id::TRAY_SHOW {
                    if let Some(ws) = &self.window_state {
                        ws.window.set_visible(true);
                        ws.window.focus_window();
                    }
                }
                self.driver.on_menu(id, &mut self.store);
                self.request_redraw();
            }
        }
    }
}

impl<D: AppDriver> Application<D> {
    fn render_frame(&mut self, window_id: WindowId) {
        let (width, height, logical_width, logical_height, scale_factor) = {
            let Some(ws) = self.window_mut(window_id) else {
                return;
            };
            let (logical_width, logical_height) = ws.logical_size();
            (
                ws.width,
                ws.height,
                logical_width,
                logical_height,
                ws.scale_factor(),
            )
        };
        // Shared UI work once per tick (on_frame / layout); present per window.
        let primary = self
            .window_state
            .as_ref()
            .map(|w| w.window.id() == window_id)
            .unwrap_or(true);
        if primary || self.needs_redraw {
            // Apply driver theme before on_frame so rebuild/paint share one scheme.
            // Phase 40: light/dark-only flips clear text caches + full paint dirty
            // without `scene.reset()` (token colors resolve at paint). Design/seed
            // geometry changes still Full-reset the retained scene.
            if let Some(driver_theme) = self.driver.theme() {
                let new_theme = Theme::new(
                    self.design_system,
                    driver_theme.seed(),
                    driver_theme.is_dark,
                );
                let dark_changed = new_theme.is_dark != self.theme.is_dark;
                let design_changed = new_theme.design_system != self.theme.design_system;
                let seed_changed = new_theme.seed() != self.theme.seed();
                self.theme = new_theme;
                if let Some(policy) = theme_change_paint_policy(
                    dark_changed,
                    design_changed,
                    seed_changed,
                    force_theme_full_reset_enabled(),
                ) {
                    match policy {
                        ThemePaintPolicy::DamagedNoReset => {
                            self.text.clear_layout_cache();
                            synui_render::with_text_renderer(|r| r.clear_layout_cache());
                            // Damaged path: dirty every widget, keep retained scene (no reset).
                            self.store.mark_all_widgets_paint_dirty();
                        }
                        ThemePaintPolicy::FullReset => {
                            self.store.mark_dirty();
                            self.scene.reset();
                            note_scene_reset();
                            self.text.clear_layout_cache();
                            synui_render::with_text_renderer(|r| r.clear_layout_cache());
                        }
                    }
                }
            }
            self.driver.on_frame(&mut self.store);
            let direction = if self.l10n.is_rtl() {
                synui_core::LayoutDirection::Rtl
            } else {
                synui_core::LayoutDirection::Ltr
            };
            self.layout.set_layout_direction(direction);
            self.store.set_layout_direction(direction);
            let mut insets = self.lifecycle.safe_area_insets();
            if self.lifecycle.soft_keyboard_visible() {
                // Soft keyboard: raise bottom inset so root layout clears the IME.
                insets[2] = insets[2].max(280.0);
            }
            self.store.set_safe_area(insets);
        }

        let mut relayout = false;
        if let Some(root) = self.store.root() {
            self.store
                .set_viewport(Size::new(logical_width, logical_height));
            // Paint-only + empty overlays: event paths already synced; skip O(n) walk.
            if self.store.needs_layout() || !self.store.overlay_stack().is_empty() {
                synui_widgets::sync_pre_layout(&mut self.store);
            }
            if self.store.needs_layout() {
                let ver = self.store.structure_version();
                let epoch = self.store.content_epoch();
                let structure_changed = self.last_structure_version != ver;
                if structure_changed {
                    self.layout.mark_structure_changed();
                    self.last_structure_version = ver;
                    self.last_content_epoch = epoch;
                    self.store.note_full_layout();
                } else if self.store.layout_is_full() {
                    if self.last_content_epoch != epoch {
                        // Prop-only measure changes: drop leaf sizes, keep structure.
                        self.layout.cache_mut().invalidate_leaf_sizes();
                        self.last_content_epoch = epoch;
                    }
                    self.store.note_full_layout();
                } else {
                    // Phase 30: structure-stable partial layout via dirty widget set.
                    let dirty = self.store.take_dirty_layout_widgets();
                    if dirty.is_empty() {
                        if self.last_content_epoch != epoch {
                            self.layout.cache_mut().invalidate_leaf_sizes();
                            self.last_content_epoch = epoch;
                        }
                        self.store.note_full_layout();
                    } else {
                        if self.last_content_epoch != epoch {
                            self.last_content_epoch = epoch;
                        }
                        for &id in &dirty {
                            self.layout.mark_dirty(id, &self.store);
                        }
                        self.store.note_partial_layout(dirty.len() as u32);
                    }
                }
                self.layout.compute(
                    &mut self.store,
                    &mut self.text as &mut dyn synui_core::TextMeasurer,
                    root,
                    Size::new(logical_width, logical_height),
                );
                synui_widgets::sync_post_layout(
                    &mut self.store,
                    &mut self.text as &mut dyn synui_core::TextMeasurer,
                );
                self.store.clear_layout_dirty();
                // Any layout can reposition siblings outside the dirty set. Scoped
                // Damaged erase would wipe them until Full compact — always full paint.
                self.store.mark_paint_dirty();
                relayout = true;
            }
            // After layout (or with prior bounds): mount/recycle virtualized rows (Phase 24 host).
            synui_widgets::sync_pre_paint(
                &mut self.store,
                &mut self.text as &mut dyn synui_core::TextMeasurer,
            );
            if !self.store.needs_paint() && !relayout {
                // Still present an up-to-date scene on secondary windows.
                if primary {
                    return;
                }
            }
        }
        let theme_ref = self.theme.to_theme_ref();
        let mut paint_ctx = PaintCtx {
            bounds: Rect::new(0.0, 0.0, logical_width, logical_height),
            theme: &theme_ref,
            text: Some(&mut self.text),
            icons: Some(&mut self.icons),
            images: Some(&mut self.images),
            lotties: Some(&self.lotties),
            text_color: None,
            font_size: None,
        };
        // Phase 31: retained-scene damage when paint-only + scoped dirty.
        // Phase 32: force Full compact after Damaged-frame budget.
        self.store.prepare_damage_compact();
        let paint_mode = self.store.paint_mode();
        match paint_mode {
            PaintMode::Full => {
                self.scene.reset();
                note_scene_reset();
                self.store.paint_all(&mut self.scene, &mut paint_ctx);
            }
            PaintMode::Damaged => {
                let _stats = self.store.paint_frame(&mut self.scene, &mut paint_ctx);
            }
        }
        self.store.note_paint_presented(paint_mode);
        self.store.clear_paint_dirty();
        let base = theme_ref.surface;
        // Temporarily take the backend so we can borrow `scene` immutably.
        let kind = self.backend_kind;
        let mut backend = match self.window_mut(window_id) {
            Some(ws) => std::mem::replace(&mut ws.backend, RenderBackend::new(kind)),
            None => return,
        };
        let render_result = if (scale_factor - 1.0).abs() < f64::EPSILON {
            backend.render_scene(&self.scene, base, width, height)
        } else {
            // Widgets are laid out in logical units. Scale only at the presentation
            // boundary so Retina surfaces receive the same UI at higher density.
            let mut scaled_scene = Scene::new();
            scaled_scene.append(&self.scene, Some(Affine::scale(scale_factor)));
            backend.render_scene(&scaled_scene, base, width, height)
        };
        if let Err(e) = render_result {
            eprintln!("synui: render failed: {e}");
        }
        if let Some(ws) = self.window_mut(window_id) {
            ws.backend = backend;
        }
        if relayout && primary {
            self.sync_ime_state();
        }
        if primary && self.store.take_a11y_dirty() {
            #[cfg(not(target_arch = "wasm32"))]
            {
                // Only build the AccessKit tree when a screen reader is active.
                // Eager build on every keystroke was a major Windows TextArea lag source.
                let focus = self.store.focus_manager().focused();
                if let Some(adapter) = self.accesskit_adapter.as_mut() {
                    let a11y = &mut self.a11y;
                    let store = &self.store;
                    adapter.update_if_active(|| a11y.build_tree_update(store, focus));
                }
            }
            #[cfg(all(feature = "web", target_arch = "wasm32"))]
            {
                let mut tree = synui_platform::web::DomA11yTree::from_store(&self.store);
                tree.host_element_id = Some("synui-a11y-host".into());
                tree.sync_to_dom();
            }
        }
    }
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn show_web_failure_page(message: &str) {
    use wasm_bindgen::JsCast;
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        web_sys::console::error_1(&message.into());
        return;
    };
    if let Some(banner) = document.get_element_by_id("synui-error") {
        banner.set_text_content(Some(message));
        if let Some(el) = banner.dyn_ref::<web_sys::HtmlElement>() {
            let _ = el.style().set_property("display", "block");
        }
    } else if let Some(body) = document.body() {
        let div = document.create_element("div").ok();
        if let Some(div) = div {
            div.set_id("synui-error");
            div.set_text_content(Some(message));
            let _ = body.append_child(&div);
        }
    }
    web_sys::console::error_1(&message.into());
}

fn platform_lifecycle() -> Box<dyn AppLifecycle> {
    #[cfg(all(feature = "mobile", target_os = "android"))]
    {
        return Box::new(synui_platform::AndroidLifecycle::default());
    }
    #[cfg(all(feature = "mobile", target_os = "ios"))]
    {
        return Box::new(synui_platform::IosLifecycle::default());
    }
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    {
        return Box::new(synui_platform::WebLifecycle::default());
    }
    #[allow(unreachable_code)]
    Box::new(DesktopLifecycle)
}

fn map_key(key: &WinitKey) -> Option<Key> {
    match key {
        WinitKey::Named(WinitNamedKey::Enter) => Some(Key::Named(NamedKey::Enter)),
        WinitKey::Named(WinitNamedKey::Tab) => Some(Key::Named(NamedKey::Tab)),
        WinitKey::Named(WinitNamedKey::Escape) => Some(Key::Named(NamedKey::Escape)),
        WinitKey::Named(WinitNamedKey::Backspace) => Some(Key::Named(NamedKey::Backspace)),
        WinitKey::Named(WinitNamedKey::Delete) => Some(Key::Named(NamedKey::Delete)),
        WinitKey::Named(WinitNamedKey::ArrowLeft) => Some(Key::Named(NamedKey::ArrowLeft)),
        WinitKey::Named(WinitNamedKey::ArrowRight) => Some(Key::Named(NamedKey::ArrowRight)),
        WinitKey::Named(WinitNamedKey::ArrowUp) => Some(Key::Named(NamedKey::ArrowUp)),
        WinitKey::Named(WinitNamedKey::ArrowDown) => Some(Key::Named(NamedKey::ArrowDown)),
        WinitKey::Named(WinitNamedKey::Home) => Some(Key::Named(NamedKey::Home)),
        WinitKey::Named(WinitNamedKey::End) => Some(Key::Named(NamedKey::End)),
        WinitKey::Named(WinitNamedKey::PageUp) => Some(Key::Named(NamedKey::PageUp)),
        WinitKey::Named(WinitNamedKey::PageDown) => Some(Key::Named(NamedKey::PageDown)),
        WinitKey::Named(WinitNamedKey::Space) => Some(Key::Named(NamedKey::Space)),
        WinitKey::Named(WinitNamedKey::F10) => Some(Key::Named(NamedKey::F10)),
        WinitKey::Character(c) => Some(Key::Character(c.to_string())),
        _ => None,
    }
}

fn pointer_cursor_to_winit(cursor: PointerCursor) -> CursorIcon {
    match cursor {
        PointerCursor::Default => CursorIcon::Default,
        PointerCursor::Text => CursorIcon::Text,
        PointerCursor::Pointer => CursorIcon::Pointer,
        PointerCursor::ColResize => CursorIcon::ColResize,
        PointerCursor::RowResize => CursorIcon::RowResize,
        PointerCursor::EwResize => CursorIcon::EwResize,
        PointerCursor::NsResize => CursorIcon::NsResize,
        PointerCursor::Grab => CursorIcon::Grab,
        PointerCursor::Grabbing => CursorIcon::Grabbing,
    }
}

/// Create an event loop with user events for AccessKit.
pub fn event_loop() -> EventLoop<UserEvent> {
    EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop")
}

/// Android entry: build an event loop bound to the `AndroidApp` from `android_main`.
#[cfg(target_os = "android")]
pub fn event_loop_android(
    app: winit::platform::android::activity::AndroidApp,
) -> EventLoop<UserEvent> {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    EventLoop::<UserEvent>::with_user_event()
        .with_android_app(app)
        .build()
        .expect("android event loop")
}

#[cfg(test)]
mod tests {
    use super::*;
    use synui_core::{OverlayConfig, PaintMode};

    #[test]
    fn light_dark_theme_policy_does_not_require_scene_reset() {
        let _ = take_scene_reset_count();
        let policy = theme_change_paint_policy(true, false, false, false);
        assert_eq!(policy, Some(ThemePaintPolicy::DamagedNoReset));
        // Host only calls note_scene_reset on FullReset.
        if matches!(policy, Some(ThemePaintPolicy::FullReset)) {
            note_scene_reset();
        }
        assert_eq!(
            take_scene_reset_count(),
            0,
            "Phase 41: light/dark flip must not increment scene reset count"
        );
    }

    #[test]
    fn design_seed_theme_policy_requires_full_reset() {
        assert_eq!(
            theme_change_paint_policy(false, true, false, false),
            Some(ThemePaintPolicy::FullReset)
        );
        assert_eq!(
            theme_change_paint_policy(true, false, false, true),
            Some(ThemePaintPolicy::FullReset)
        );
    }

    #[test]
    fn overlay_structure_change_forces_full_paint() {
        force_overlay_full_paint(false);
        let mut store = WidgetStore::new();
        let host = store.insert(synui_widgets::Label::new("popup"));
        store.insert_overlay(
            host,
            Rect::new(10.0, 10.0, 110.0, 60.0),
            OverlayConfig::popup(),
            vec![],
            host,
        );
        let before = collect_overlay_bounds(&store);
        store.remove_overlay_for_owner(host);
        // Structure ops escalate Full; clear so we assert mark_overlay_structure_paint.
        store.clear_paint_dirty();
        mark_overlay_structure_paint(&mut store, &before);
        assert!(
            store.damage_is_full(),
            "overlay open/close must Full-paint so MenuBar popups are not blank"
        );
        assert_eq!(store.paint_mode(), PaintMode::Full);
    }
}
