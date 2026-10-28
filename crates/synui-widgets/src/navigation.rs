//! Navigation widgets: app bar, tabs, drawer, bottom nav.

use synui_core::alloc_widget_id;
use synui_core::{
    A11yProps, A11yRole, AlignItems, Constraints, Dimension, Display, Event, EventCtx, EventResult,
    FlexDirection, JustifyContent, LayoutStyle, MeasureCtx, PaintCtx, Rect, Size, TextAlign,
    Widget, WidgetId,
};
use synui_render::paint_simple_rect;
use synui_theme::RippleState;
use vello::kurbo::{Affine, RoundedRect};
use vello::peniko::{Color, Fill};
use vello::Scene;

pub struct AppBar {
    id: WidgetId,
    bounds: Rect,
    pub title: String,
    child_ids: Vec<WidgetId>,
    /// Optional bar fill; `None` uses theme primary (Material) or surface (Cupertino).
    pub background: Option<Color>,
}

impl AppBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            title: title.into(),
            child_ids: Vec::new(),
            background: None,
        }
    }

    pub fn with_actions(mut self, actions: Vec<WidgetId>) -> Self {
        self.child_ids = actions;
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }

    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.child_ids = children;
    }
}

impl Widget for AppBar {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let cupertino = ctx.theme.cupertino;
        let bg = if let Some(color) = self.background {
            color
        } else if cupertino {
            ctx.theme.surface
        } else {
            ctx.theme.primary
        };
        let fg = if cupertino {
            ctx.theme.on_surface
        } else {
            ctx.theme.on_primary
        };
        paint_simple_rect(scene, self.bounds, bg, 0.0);
        if cupertino {
            // Hairline bottom border (iOS NavigationBar).
            let line = Rect::new(
                self.bounds.x0,
                self.bounds.y1 - 1.0,
                self.bounds.x1,
                self.bounds.y1,
            );
            paint_simple_rect(scene, line, ctx.theme.outline.multiply_alpha(0.35), 0.0);
        }
        if let Some(text) = ctx.text.as_deref_mut() {
            let title_bounds = Rect::new(
                self.bounds.x0 + 16.0,
                self.bounds.y0,
                self.bounds.x1 - 16.0,
                self.bounds.y1,
            );
            let align = if cupertino {
                TextAlign::Center
            } else {
                TextAlign::Start
            };
            text.paint_text(
                scene,
                &self.title,
                if cupertino { 17.0 } else { 20.0 },
                fg,
                title_bounds,
                Some(title_bounds.width() as f32),
                align,
            );
        }
    }

    fn measure(&mut self, ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        let height: f64 = 56.0;
        let width = if let Some(m) = ctx.text_measurer.as_deref_mut() {
            m.measure_text(&self.title, 20.0, None).width
        } else {
            self.title.len() as f64 * 10.0
        };
        Size::new(
            width.max(200.0).min(constraints.max_width as f64),
            height.min(constraints.max_height as f64),
        )
    }

    fn layout_style(&self) -> &synui_core::LayoutStyle {
        use std::sync::OnceLock;

        static STYLE: OnceLock<synui_core::LayoutStyle> = OnceLock::new();

        STYLE.get_or_init(|| synui_core::LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: synui_core::FlexDirection::Row,
            justify_content: synui_core::JustifyContent::FlexEnd,
            align_items: synui_core::AlignItems::Center,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            width: synui_core::Dimension::Fill,
            height: synui_core::Dimension::Length(56.0),
            padding: [0.0; 4],
            gap: 0.0,
            ..synui_core::LayoutStyle::base()
        })
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Generic,
            label: Some(self.title.clone()),
            ..Default::default()
        }
    }
}

pub struct BottomNavigation {
    id: WidgetId,
    bounds: Rect,
    pub labels: Vec<String>,
    pub icons: Vec<String>,
    pub selected: usize,
    /// Optional bar fill; `None` uses theme surface.
    pub background: Option<Color>,
}

impl BottomNavigation {
    pub fn new(labels: Vec<String>) -> Self {
        let n = labels.len();
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            labels,
            icons: vec![String::new(); n],
            selected: 0,
            background: None,
        }
    }

    pub fn with_icons(mut self, icons: Vec<String>) -> Self {
        self.icons = icons;
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }
}

impl Widget for BottomNavigation {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let bg = self.background.unwrap_or(ctx.theme.surface);
        paint_simple_rect(scene, self.bounds, bg, 0.0);
        let n = self.labels.len().max(1);
        let item_w = self.bounds.width() / n as f64;
        for (i, label) in self.labels.iter().enumerate() {
            let item = Rect::new(
                self.bounds.x0 + i as f64 * item_w,
                self.bounds.y0,
                self.bounds.x0 + (i + 1) as f64 * item_w,
                self.bounds.y1,
            );
            let color = if i == self.selected {
                ctx.theme.primary
            } else {
                ctx.theme.on_surface_variant
            };
            if let Some(icon_name) = self.icons.get(i).filter(|s| !s.is_empty()) {
                if let Some(icons) = ctx.icons.as_deref_mut() {
                    let icon_size = 24.0;
                    let icon_bounds = Rect::new(
                        item.x0 + (item_w - icon_size) / 2.0,
                        item.y0 + 8.0,
                        item.x0 + (item_w + icon_size) / 2.0,
                        item.y0 + 8.0 + icon_size,
                    );
                    let _ = icons.paint_icon(scene, icon_name, icon_bounds, color);
                }
            }
            if let Some(text) = ctx.text.as_deref_mut() {
                let label_bounds = Rect::new(item.x0, item.y1 - 20.0, item.x1, item.y1 - 4.0);
                text.paint_text(
                    scene,
                    label,
                    12.0,
                    color,
                    label_bounds,
                    None,
                    TextAlign::Center,
                );
            }
        }
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        if let Event::PointerUp { position, .. } = event {
            if self.bounds.contains(*position) {
                let n = self.labels.len().max(1);
                let item_w = self.bounds.width() / n as f64;
                let idx = ((position.x - self.bounds.x0) / item_w.max(1.0)) as usize;
                self.selected = idx.min(n.saturating_sub(1));
                return EventResult::HANDLED;
            }
        }
        EventResult::IGNORED
    }
}

pub struct NavigationDrawer {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    pub open: bool,
    pub width: f64,
    /// Optional rail fill; `None` uses theme surface_container.
    pub background: Option<Color>,
}

impl NavigationDrawer {
    pub fn new() -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            open: false,
            width: 280.0,
            background: None,
        }
    }

    pub fn with_children(mut self, children: Vec<WidgetId>) -> Self {
        self.child_ids = children;
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }

    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.child_ids = children;
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

impl Widget for NavigationDrawer {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        // Always expose children so overlay layout can mount nav buttons when open.
        // Closed drawers use Display::None and are skipped by the layout engine.
        &self.child_ids
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static OPEN: OnceLock<LayoutStyle> = OnceLock::new();
        static CLOSED: OnceLock<LayoutStyle> = OnceLock::new();
        if self.open {
            // Sized for the modal overlay left rail (see sync_navigation_drawer).
            OPEN.get_or_init(|| LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Stretch,
                flex_grow: 0.0,
                flex_shrink: 0.0,
                width: Dimension::Fill,
                height: Dimension::Fill,
                padding: [8.0, 8.0, 8.0, 8.0],
                gap: 4.0,
                ..LayoutStyle::base()
            })
        } else {
            CLOSED.get_or_init(|| LayoutStyle {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Stretch,
                flex_grow: 0.0,
                flex_shrink: 0.0,
                width: Dimension::Length(0.0),
                height: Dimension::Length(0.0),
                padding: [0.0; 4],
                gap: 0.0,
                ..LayoutStyle::base()
            })
        }
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        if !self.open {
            return Size::ZERO;
        }
        Size::new(
            self.width.min(constraints.max_width as f64),
            constraints.max_height as f64,
        )
    }

    fn bounds(&self) -> Rect {
        if self.open {
            self.bounds
        } else {
            Rect::ZERO
        }
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn hit_test(&self, point: synui_core::Point) -> bool {
        self.open && self.bounds.contains(point)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        if !self.open {
            return;
        }
        // Scrim is painted by WidgetStore for modal overlays; only the rail here.
        let bg = self.background.unwrap_or(ctx.theme.surface_container);
        paint_simple_rect(scene, self.bounds, bg, 0.0);
    }
}

pub struct TabBar {
    id: WidgetId,
    bounds: Rect,
    pub tabs: Vec<String>,
    pub selected: usize,
    /// Optional bar fill; `None` uses theme surface.
    pub background: Option<Color>,
}

impl TabBar {
    pub fn new(tabs: Vec<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            tabs,
            selected: 0,
            background: None,
        }
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }
}

const TAB_BAR_HEIGHT: f64 = 40.0;

impl Widget for TabBar {
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
        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();
        STYLE.get_or_init(|| LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Center,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            width: Dimension::Fill,
            height: Dimension::Length(TAB_BAR_HEIGHT as f32),
            min_height: Dimension::Length(TAB_BAR_HEIGHT as f32),
            ..LayoutStyle::base()
        })
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(
            constraints.max_width.min(f32::MAX) as f64,
            TAB_BAR_HEIGHT.min(constraints.max_height as f64),
        )
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let cupertino = ctx.theme.cupertino;
        let n = self.tabs.len().max(1);
        let tab_w = self.bounds.width() / n as f64;
        if let Some(bg) = self.background {
            paint_simple_rect(scene, self.bounds, bg, 0.0);
        }
        if cupertino {
            // SegmentedControl: rounded capsule with selected fill.
            let pad = 2.0;
            let outer = Rect::new(
                self.bounds.x0 + pad,
                self.bounds.y0 + pad,
                self.bounds.x1 - pad,
                self.bounds.y1 - pad,
            );
            paint_simple_rect(scene, outer, ctx.theme.outline.multiply_alpha(0.2), 8.0);
            let sel = Rect::new(
                outer.x0 + self.selected as f64 * (outer.width() / n as f64),
                outer.y0,
                outer.x0 + (self.selected as f64 + 1.0) * (outer.width() / n as f64),
                outer.y1,
            );
            paint_simple_rect(scene, sel, ctx.theme.surface, 6.0);
            for (i, tab) in self.tabs.iter().enumerate() {
                let tab_bounds = Rect::new(
                    outer.x0 + i as f64 * (outer.width() / n as f64),
                    outer.y0,
                    outer.x0 + (i as f64 + 1.0) * (outer.width() / n as f64),
                    outer.y1,
                );
                let color = if i == self.selected {
                    ctx.theme.on_surface
                } else {
                    ctx.theme.on_surface_variant
                };
                if let Some(text) = ctx.text.as_deref_mut() {
                    text.paint_text(scene, tab, 13.0, color, tab_bounds, None, TextAlign::Center);
                }
            }
            return;
        }
        if self.background.is_none() {
            paint_simple_rect(scene, self.bounds, ctx.theme.surface, 0.0);
        }
        // Keep label band above the 3px indicator so underline never sits on glyphs.
        let label_band = Rect::new(
            self.bounds.x0,
            self.bounds.y0,
            self.bounds.x1,
            (self.bounds.y1 - 4.0).max(self.bounds.y0),
        );
        for (i, tab) in self.tabs.iter().enumerate() {
            let tab_bounds = Rect::new(
                self.bounds.x0 + i as f64 * tab_w,
                label_band.y0,
                self.bounds.x0 + (i + 1) as f64 * tab_w,
                label_band.y1,
            );
            let color = if i == self.selected {
                ctx.theme.primary
            } else {
                ctx.theme.on_surface_variant
            };
            if let Some(text) = ctx.text.as_deref_mut() {
                text.paint_text(scene, tab, 14.0, color, tab_bounds, None, TextAlign::Center);
            }
        }
        let ind_x = self.bounds.x0 + self.selected as f64 * tab_w;
        let indicator = Rect::new(ind_x, self.bounds.y1 - 3.0, ind_x + tab_w, self.bounds.y1);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ctx.theme.primary,
            None,
            &RoundedRect::from_rect(indicator, 1.5),
        );
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        if let Event::PointerUp { position, .. } = event {
            if self.bounds.contains(*position) {
                let tab_w = self.bounds.width() / self.tabs.len().max(1) as f64;
                let idx = ((position.x - self.bounds.x0) / tab_w.max(1.0)) as usize;
                self.selected = idx.min(self.tabs.len().saturating_sub(1));
                return EventResult::HANDLED;
            }
        }
        EventResult::IGNORED
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Tab,
            value: self.tabs.get(self.selected).cloned(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tab_bar_tests {
    use super::*;
    use synui_core::Widget;

    #[test]
    fn tab_bar_reserves_fixed_height_and_does_not_shrink() {
        let bar = TabBar::new(vec!["Table".into(), "Preview".into()]);
        let style = bar.layout_style();
        assert_eq!(style.height, Dimension::Length(TAB_BAR_HEIGHT as f32));
        assert_eq!(style.min_height, Dimension::Length(TAB_BAR_HEIGHT as f32));
        assert_eq!(style.flex_shrink, 0.0);
        let mut bar = bar;
        let size = bar.measure(
            &mut MeasureCtx {
                max_width: None,
                text_measurer: None,
            },
            Constraints::UNBOUNDED,
        );
        assert_eq!(size.height, TAB_BAR_HEIGHT);
    }

    #[test]
    fn open_drawer_overlay_shows_children_and_left_rail() {
        use crate::overlay_sync::{layout_overlays, sync_overlays};
        use synui_core::{Event, Point, PointerButton, Size, WidgetStore};
        use synui_layout::LayoutEngine;
        use synui_render::TextRenderer;

        let mut store = WidgetStore::new();
        let mut text = TextRenderer::new();
        let root = crate::containers::Flex::new()
            .with_justify(JustifyContent::FlexStart)
            .with_align(AlignItems::Center)
            .with_width(Dimension::Fill)
            .with_height(Dimension::Fill)
            .column();
        let root_id = store.insert(root);
        let mut drawer = NavigationDrawer::new();
        drawer.open = true;
        let list_btn = store.insert(crate::basic::Button::new("List"));
        let settings_btn = store.insert(crate::basic::Button::new("Settings"));
        drawer.replace_children(vec![list_btn, settings_btn]);
        let drawer_id = store.insert(drawer);
        let body = store.insert(crate::basic::Label::new("body content"));
        store.set_children(root_id, vec![drawer_id, body]);
        store.set_children(drawer_id, vec![list_btn, settings_btn]);
        store.set_root(root_id);
        store.set_viewport(Size::new(900.0, 600.0));

        sync_overlays(&mut store);
        let mut layout = LayoutEngine::new();
        layout.compute(&mut store, &mut text, root_id, Size::new(900.0, 600.0));
        layout_overlays(&mut store, &mut text);

        assert!(
            store.overlay_for_owner(drawer_id).is_some(),
            "open drawer must live on the modal overlay stack"
        );
        let db = store.get(drawer_id).unwrap().bounds();
        assert!(db.x0 < 8.0 && db.width() <= 280.5, "left rail, got {db:?}");
        assert!(
            db.height() > 400.0,
            "rail should be full height, got {db:?}"
        );

        let list_b = store.get(list_btn).unwrap().bounds();
        assert!(
            list_b.height() > 8.0 && list_b.width() > 8.0,
            "List button must be laid out inside the rail, got {list_b:?}"
        );
        assert!(
            list_b.x0 >= db.x0 - 1.0 && list_b.x1 <= db.x1 + 1.0,
            "List button must sit in the rail: btn={list_b:?} rail={db:?}"
        );

        // Scrim click (middle of window) dismisses via backdrop.
        let mid = Point::new(450.0, 300.0);
        assert!(
            store
                .dispatch_event(&Event::PointerDown {
                    id: 0,
                    position: mid,
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                })
                .handled
        );
        sync_overlays(&mut store);
        let open = store
            .get(drawer_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NavigationDrawer>()
            .unwrap()
            .open;
        assert!(!open, "scrim click must close the drawer");
        assert!(store.overlay_for_owner(drawer_id).is_none());
    }
}

pub struct ListItem {
    id: WidgetId,
    bounds: Rect,
    pub title: String,
    pub subtitle: Option<String>,
    child_ids: Vec<WidgetId>,
    ripple: Option<RippleState>,
    pub on_press: Option<Box<dyn FnMut()>>,
    pub action: Option<synui_core::ActionId>,
    /// Optional row fill; `None` uses theme surface.
    pub background: Option<Color>,
}

impl ListItem {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            title: title.into(),
            subtitle: None,
            child_ids: Vec::new(),
            ripple: None,
            on_press: None,
            action: None,
            background: None,
        }
    }

    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn with_trailing(mut self, child: WidgetId) -> Self {
        self.child_ids = vec![child];
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }

    pub fn replace_children(&mut self, children: Vec<WidgetId>) {
        self.child_ids = children;
    }

    pub fn on_press(mut self, f: impl FnMut() + 'static) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }

    pub fn with_action(mut self, id: synui_core::ActionId) -> Self {
        self.action = Some(id);
        self
    }

    pub fn set_action(&mut self, id: Option<synui_core::ActionId>) {
        self.action = id;
    }

    pub fn advance_ripple(&mut self, dt_ms: f32) -> bool {
        if let Some(ripple) = &mut self.ripple {
            let active = ripple.advance(dt_ms);
            if !active {
                self.ripple = None;
            }
            active
        } else {
            false
        }
    }
}

impl Widget for ListItem {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        let height = if self.subtitle.is_some() { 72.0 } else { 56.0 };
        Size::new(constraints.max_width.min(400.0) as f64, height)
    }

    fn layout_style(&self) -> &synui_core::LayoutStyle {
        use std::sync::OnceLock;

        static STYLE: OnceLock<synui_core::LayoutStyle> = OnceLock::new();

        STYLE.get_or_init(|| synui_core::LayoutStyle {
            display: synui_core::Display::Flex,
            flex_direction: synui_core::FlexDirection::Row,
            justify_content: synui_core::JustifyContent::SpaceBetween,
            align_items: synui_core::AlignItems::Center,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            width: synui_core::Dimension::Fill,
            height: synui_core::Dimension::Auto,
            padding: [0.0; 4],
            gap: 0.0,
            ..synui_core::LayoutStyle::base()
        })
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let bg = self.background.unwrap_or(ctx.theme.surface);
        paint_simple_rect(scene, self.bounds, bg, 0.0);
        if let Some(ripple) = &self.ripple {
            if ctx.theme.metrics.ripple_enabled {
                let origin = synui_core::Point::new(
                    self.bounds.x0 + ripple.origin.x,
                    self.bounds.y0 + ripple.origin.y,
                );
                let max_r = synui_theme::max_ripple_radius(ripple.origin, self.bounds);
                let r = ripple.current_radius(max_r);
                let mut ripple_color = ctx.theme.primary;
                ripple_color.components[3] = ripple.current_opacity();
                let circle = synui_core::Circle::new(origin, r);
                // Clip so a wide row cannot paint a viewport-sized halo outside bounds.
                let clip = RoundedRect::from_rect(self.bounds, 0.0);
                scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &clip);
                scene.fill(Fill::NonZero, Affine::IDENTITY, ripple_color, None, &circle);
                scene.pop_layer();
            }
        }
        let title_bounds = Rect::new(
            self.bounds.x0 + 16.0,
            self.bounds.y0 + 8.0,
            self.bounds.x1 - 48.0,
            self.bounds.y0 + 28.0,
        );
        if let Some(text) = ctx.text.as_deref_mut() {
            text.paint_text(
                scene,
                &self.title,
                16.0,
                ctx.theme.on_surface,
                title_bounds,
                None,
                TextAlign::Start,
            );
            if let Some(sub) = &self.subtitle {
                let sub_bounds = Rect::new(
                    self.bounds.x0 + 16.0,
                    self.bounds.y0 + 28.0,
                    self.bounds.x1 - 48.0,
                    self.bounds.y1 - 8.0,
                );
                text.paint_text(
                    scene,
                    sub,
                    12.0,
                    ctx.theme.on_surface_variant,
                    sub_bounds,
                    None,
                    TextAlign::Start,
                );
            }
        }
        let divider = Rect::new(
            self.bounds.x0 + 16.0,
            self.bounds.y1 - 1.0,
            self.bounds.x1,
            self.bounds.y1,
        );
        let mut divider_color = ctx.theme.outline;
        divider_color.components[3] = 0.3;
        paint_simple_rect(scene, divider, divider_color, 0.0);
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        match event {
            Event::PointerDown { position, .. } if self.bounds.contains(*position) => {
                if self.action.is_some() || self.on_press.is_some() {
                    let local = synui_core::Point::new(
                        position.x - self.bounds.x0,
                        position.y - self.bounds.y0,
                    );
                    self.ripple = Some(RippleState::new(
                        local,
                        vello::peniko::Color::from_rgb8(103, 80, 164),
                    ));
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            Event::PointerUp { position, .. } if self.bounds.contains(*position) => {
                if let Some(aid) = self.action {
                    synui_core::dispatch_action(aid);
                    EventResult::HANDLED
                } else if let Some(f) = self.on_press.as_mut() {
                    f();
                    EventResult::HANDLED
                } else {
                    EventResult::IGNORED
                }
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::ListItem,
            label: Some(self.title.clone()),
            ..Default::default()
        }
    }
}
