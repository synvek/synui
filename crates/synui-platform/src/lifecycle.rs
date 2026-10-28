//! Cross-platform application lifecycle events.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// App became active (desktop: no-op; mobile: resumed).
    Resumed,
    /// App paused (mobile background).
    Suspended,
    /// App is terminating.
    Terminating,
    /// Low memory warning (mobile).
    LowMemory,
}

/// Platform-specific lifecycle handler.
pub trait AppLifecycle: Send {
    fn on_lifecycle(&mut self, event: LifecycleEvent);
    /// Safe-area insets [top, right, bottom, left] in physical pixels.
    fn safe_area_insets(&self) -> [f32; 4] {
        [0.0; 4]
    }
    /// Soft keyboard visible (mobile IME).
    fn soft_keyboard_visible(&self) -> bool {
        false
    }
    fn set_soft_keyboard_visible(&mut self, _visible: bool) {}
    /// Update safe-area insets from platform WindowInsets (mobile hosts).
    fn set_safe_area_insets(&mut self, _insets: [f32; 4]) {}
}

/// Desktop no-op lifecycle.
#[derive(Debug, Default)]
pub struct DesktopLifecycle;

impl AppLifecycle for DesktopLifecycle {
    fn on_lifecycle(&mut self, _event: LifecycleEvent) {}
}

#[cfg(feature = "android")]
pub mod android {
    use super::{AppLifecycle, LifecycleEvent};

    /// Android NDK lifecycle integration (winit NDK backend).
    #[derive(Debug, Default)]
    pub struct AndroidLifecycle {
        pub resumed: bool,
        pub soft_keyboard_visible: bool,
        /// Filled from WindowInsets when NDK hook is wired.
        pub safe_area: [f32; 4],
    }

    impl AppLifecycle for AndroidLifecycle {
        fn on_lifecycle(&mut self, event: LifecycleEvent) {
            match event {
                LifecycleEvent::Resumed => {
                    self.resumed = true;
                    tracing::info!("android: resumed");
                }
                LifecycleEvent::Suspended => {
                    self.resumed = false;
                }
                LifecycleEvent::LowMemory => {
                    tracing::warn!("android: low memory — trim caches");
                }
                LifecycleEvent::Terminating => {}
            }
        }

        fn safe_area_insets(&self) -> [f32; 4] {
            self.safe_area
        }

        fn soft_keyboard_visible(&self) -> bool {
            self.soft_keyboard_visible
        }

        fn set_soft_keyboard_visible(&mut self, visible: bool) {
            self.soft_keyboard_visible = visible;
        }

        fn set_safe_area_insets(&mut self, insets: [f32; 4]) {
            self.safe_area = insets;
        }
    }
}

#[cfg(feature = "ios")]
pub mod ios {
    use super::{AppLifecycle, LifecycleEvent};

    /// iOS lifecycle integration (UIKit / winit bridge).
    #[derive(Debug, Default)]
    pub struct IosLifecycle {
        pub active: bool,
        pub safe_area: [f32; 4],
        pub soft_keyboard_visible: bool,
    }

    impl AppLifecycle for IosLifecycle {
        fn on_lifecycle(&mut self, event: LifecycleEvent) {
            match event {
                LifecycleEvent::Resumed => self.active = true,
                LifecycleEvent::Suspended => self.active = false,
                LifecycleEvent::LowMemory => tracing::warn!("ios: memory warning"),
                LifecycleEvent::Terminating => {}
            }
        }

        fn safe_area_insets(&self) -> [f32; 4] {
            self.safe_area
        }

        fn soft_keyboard_visible(&self) -> bool {
            self.soft_keyboard_visible
        }

        fn set_soft_keyboard_visible(&mut self, visible: bool) {
            self.soft_keyboard_visible = visible;
        }

        fn set_safe_area_insets(&mut self, insets: [f32; 4]) {
            self.safe_area = insets;
        }
    }
}

#[cfg(feature = "web")]
pub mod web {
    use super::{AppLifecycle, LifecycleEvent};

    /// Web platform lifecycle (visibility API).
    #[derive(Debug, Default)]
    pub struct WebLifecycle {
        pub visible: bool,
    }

    impl AppLifecycle for WebLifecycle {
        fn on_lifecycle(&mut self, event: LifecycleEvent) {
            match event {
                LifecycleEvent::Resumed => self.visible = true,
                LifecycleEvent::Suspended => self.visible = false,
                _ => {}
            }
        }
    }

    /// Parallel DOM/ARIA tree for web accessibility (AccessKit has no web adapter).
    #[derive(Debug, Default)]
    pub struct DomA11yTree {
        pub elements: Vec<DomA11yNode>,
        /// Host element id for the ARIA sibling container (set by wasm glue).
        pub host_element_id: Option<String>,
    }

    #[derive(Debug, Clone)]
    pub struct DomA11yNode {
        pub role: String,
        pub label: String,
        pub value: Option<String>,
        pub widget_key: u64,
    }

    /// Click handler for DomA11y buttons (widget key → host event loop).
    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    static DOM_A11Y_CLICK: std::sync::Mutex<Option<Box<dyn Fn(u64) + Send + Sync>>> =
        std::sync::Mutex::new(None);

    /// Register a handler invoked when a DomA11y button is clicked in the browser.
    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    pub fn set_dom_a11y_click_handler(f: impl Fn(u64) + Send + Sync + 'static) {
        if let Ok(mut g) = DOM_A11Y_CLICK.lock() {
            *g = Some(Box::new(f));
        }
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "web")))]
    pub fn set_dom_a11y_click_handler(_f: impl Fn(u64) + Send + Sync + 'static) {}

    impl DomA11yTree {
        pub fn from_store(store: &synui_core::WidgetStore) -> Self {
            let mut tree = Self::default();
            if let Some(root) = store.root() {
                collect_a11y(store, root, &mut tree.elements);
            }
            tree
        }

        /// Sync ARIA siblings next to the canvas.
        ///
        /// On `wasm32` with `web-sys`, updates (or creates) elements under `#synui-a11y-host`.
        /// On native builds this is a documented no-op that still refreshes `elements`.
        pub fn sync_to_dom(&self) {
            #[cfg(all(target_arch = "wasm32", feature = "web"))]
            {
                wasm_sync(self);
            }
            #[cfg(not(all(target_arch = "wasm32", feature = "web")))]
            {
                let _ = &self.elements;
                tracing::trace!(
                    count = self.elements.len(),
                    "DomA11yTree::sync_to_dom (native stub)"
                );
            }
        }
    }

    fn collect_a11y(
        store: &synui_core::WidgetStore,
        id: synui_core::WidgetId,
        out: &mut Vec<DomA11yNode>,
    ) {
        if let Some(props) = store.effective_a11y(id) {
            out.push(DomA11yNode {
                role: format!("{:?}", props.role),
                label: props.label.unwrap_or_default(),
                value: props.value,
                widget_key: id.0,
            });
        }
        for &c in store.children_of(id) {
            collect_a11y(store, c, out);
        }
    }

    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    fn wasm_sync(tree: &DomA11yTree) {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        let window = web_sys::window().expect("window");
        let document = window.document().expect("document");
        let host_id = tree.host_element_id.as_deref().unwrap_or("synui-a11y-host");
        let host = document.get_element_by_id(host_id).or_else(|| {
            let div = document.create_element("div").ok()?;
            div.set_id(host_id);
            div.set_attribute("aria-live", "polite").ok()?;
            document.body()?.append_child(&div).ok()?;
            Some(div)
        });
        let Some(host) = host else {
            return;
        };
        while let Some(child) = host.first_child() {
            let _ = host.remove_child(&child);
        }
        for node in &tree.elements {
            let role_l = node.role.to_lowercase();
            let is_button = role_l.contains("button");
            let Ok(el) = document.create_element(if is_button { "button" } else { "div" }) else {
                continue;
            };
            let _ = el.set_attribute("role", &role_l);
            if !node.label.is_empty() {
                let _ = el.set_attribute("aria-label", &node.label);
                let _ = el.set_attribute("data-synui-label", &node.label);
            }
            let text = node
                .value
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(node.label.as_str());
            if !text.is_empty() {
                el.set_text_content(Some(text));
            }
            if node.label.starts_with("Count:") || text.starts_with("Count:") {
                let _ = el.set_attribute("data-testid", "synui-count");
            } else if is_button
                && (node.label == "+1" || node.label.contains("+1") || text.contains("+1"))
            {
                let _ = el.set_attribute("data-testid", "synui-inc");
            }
            let _ = el.set_attribute("data-synui-role", &role_l);
            let _ = host.set_attribute("data-synui-ready", "1");
            if is_button {
                let key = node.widget_key;
                let closure = Closure::wrap(Box::new(move |_ev: web_sys::MouseEvent| {
                    if let Ok(g) = DOM_A11Y_CLICK.lock() {
                        if let Some(cb) = g.as_ref() {
                            cb(key);
                        }
                    }
                }) as Box<dyn FnMut(_)>);
                let _ =
                    el.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
                closure.forget();
            }
            let _ = host.append_child(&el);
        }
    }
}
