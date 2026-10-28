//! In-app MenuBar and ContextMenu (Phase 15–16) — overlay panels + keyboard nav.

use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Constraints, Dimension, Event, EventCtx, EventResult, Key,
    LayoutStyle, MeasureCtx, NamedKey, PaintCtx, Point, PointerButton, Rect, Size, TextAlign,
    Widget, WidgetId,
};
use synui_render::paint_simple_rect;
use vello::Scene;

/// One menu action.
#[derive(Clone)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

impl MenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }
}

/// Top-level menu with a title and items.
#[derive(Clone)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

/// Overlay panel owned by a [`MenuBar`] or [`ContextMenu`].
pub struct MenuPanel {
    id: WidgetId,
    bounds: Rect,
    pub owner: WidgetId,
    pub items: Vec<MenuItem>,
    pub highlighted: usize,
    item_height: f64,
    pub pending_action: Option<String>,
    pub pending_close: bool,
}

impl MenuPanel {
    pub fn new(owner: WidgetId, items: Vec<MenuItem>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            owner,
            items,
            highlighted: 0,
            item_height: 28.0,
            pending_action: None,
            pending_close: false,
        }
    }

    pub fn with_highlighted(mut self, h: usize) -> Self {
        self.highlighted = h.min(self.items.len().saturating_sub(1));
        self
    }

    fn activate_highlighted(&mut self) {
        if let Some(item) = self.items.get(self.highlighted) {
            if item.enabled {
                self.pending_action = Some(item.id.clone());
            }
        }
        self.pending_close = true;
    }

    pub fn apply_pending(store: &mut synui_core::WidgetStore) {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        let mut panel_ids = Vec::new();
        let mut push = |id: WidgetId| {
            if seen.insert(id) {
                panel_ids.push(id);
            }
        };
        // Menu panels live on the overlay stack, not the main tree.
        for owner in store.overlay_owners().collect::<Vec<_>>() {
            if let Some(host) = store.overlay_for_owner(owner) {
                fn walk(
                    store: &synui_core::WidgetStore,
                    id: WidgetId,
                    push: &mut dyn FnMut(WidgetId),
                ) {
                    push(id);
                    for &c in store.children_of(id) {
                        walk(store, c, push);
                    }
                }
                walk(store, host, &mut push);
            }
        }
        if let Some(root) = store.root() {
            fn walk(store: &synui_core::WidgetStore, id: WidgetId, push: &mut dyn FnMut(WidgetId)) {
                push(id);
                for &c in store.children_of(id) {
                    walk(store, c, push);
                }
            }
            walk(store, root, &mut push);
        }

        for id in panel_ids {
            let pending = store.get(id).and_then(|w| {
                w.as_any().downcast_ref::<MenuPanel>().map(|p| {
                    (
                        p.owner,
                        p.pending_action.clone(),
                        p.pending_close,
                        p.highlighted,
                    )
                })
            });
            let Some((owner, action, close, highlighted)) = pending else {
                continue;
            };
            if action.is_none() && !close {
                if let Some(w) = store.get_mut(owner) {
                    if let Some(mb) = w.as_any_mut().downcast_mut::<MenuBar>() {
                        mb.highlighted_item = Some(highlighted);
                    }
                    if let Some(cm) = w.as_any_mut().downcast_mut::<ContextMenu>() {
                        cm.highlighted = highlighted;
                    }
                }
                continue;
            }
            if let Some(w) = store.get_mut(id) {
                if let Some(p) = w.as_any_mut().downcast_mut::<MenuPanel>() {
                    p.pending_action = None;
                    p.pending_close = false;
                }
            }
            if let Some(w) = store.get_mut(owner) {
                if let Some(mb) = w.as_any_mut().downcast_mut::<MenuBar>() {
                    if let Some(a) = action.clone() {
                        mb.last_action = Some(a);
                    }
                    if close {
                        // Selecting an item (or Esc) must leave menu mode — otherwise
                        // paint keeps highlighting File while focused && closed.
                        mb.exit_menu_mode();
                    }
                }
                if let Some(cm) = w.as_any_mut().downcast_mut::<ContextMenu>() {
                    if let Some(a) = action {
                        cm.last_action = Some(a);
                    }
                    if close {
                        cm.open = false;
                    }
                }
            }
            if close {
                // Drop keyboard focus so the bar does not stay in menu-mode chrome.
                if store.focus_manager().focused() == Some(owner) {
                    store.focus_manager_mut().clear_focus();
                }
            }
        }
    }
}

impl Widget for MenuPanel {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();
        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            width: Dimension::Fill,
            height: Dimension::Fill,
            ..LayoutStyle::base()
        })
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(
            constraints.max_width as f64,
            self.items.len() as f64 * self.item_height,
        )
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        paint_simple_rect(scene, self.bounds, ctx.theme.surface, 4.0);
        if let Some(text) = ctx.text.as_deref_mut() {
            for (j, item) in self.items.iter().enumerate() {
                let y0 = self.bounds.y0 + j as f64 * self.item_height;
                let row = Rect::new(self.bounds.x0, y0, self.bounds.x1, y0 + self.item_height);
                if j == self.highlighted {
                    let mut hl = ctx.theme.primary;
                    hl.components[3] = 0.2;
                    paint_simple_rect(scene, row, hl, 0.0);
                }
                let color = if item.enabled {
                    ctx.theme.on_surface
                } else {
                    ctx.theme.outline
                };
                text.paint_text(
                    scene,
                    &item.label,
                    13.0,
                    color,
                    Rect::new(row.x0 + 10.0, row.y0, row.x1, row.y1),
                    Some((row.width() - 10.0) as f32),
                    TextAlign::Start,
                );
            }
        }
    }

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                ..
            } if self.bounds.contains(*position) => {
                let idx = ((position.y - self.bounds.y0) / self.item_height).floor() as usize;
                if idx < self.items.len() {
                    self.highlighted = idx;
                    self.activate_highlighted();
                }
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } if self.bounds.contains(*position) => {
                let idx = ((position.y - self.bounds.y0) / self.item_height).floor() as usize;
                if idx < self.items.len() {
                    self.highlighted = idx;
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowDown),
                ..
            } => {
                if !self.items.is_empty() {
                    self.highlighted = (self.highlighted + 1) % self.items.len();
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowUp),
                ..
            } => {
                if !self.items.is_empty() {
                    self.highlighted = if self.highlighted == 0 {
                        self.items.len() - 1
                    } else {
                        self.highlighted - 1
                    };
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            }
            | Event::KeyDown {
                key: Key::Named(NamedKey::Space),
                ..
            } => {
                self.activate_highlighted();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Escape),
                ..
            } => {
                self.pending_close = true;
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Generic,
            label: Some("Menu".into()),
            focusable: true,
            actions: vec![A11yAction::Focus],
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }
}

/// Horizontal menu bar; click / F10 / Alt to open a dropdown panel (overlay).
pub struct MenuBar {
    id: WidgetId,
    bounds: Rect,
    pub menus: Vec<Menu>,
    pub open_index: Option<usize>,
    pub highlighted_item: Option<usize>,
    pub last_action: Option<String>,
    pub item_width: f64,
    pub item_height: f64,
    focused: bool,
    /// Title index shown when the bar has focus but no dropdown is open.
    focus_title: Option<usize>,
}

impl MenuBar {
    pub fn new(menus: Vec<Menu>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            menus,
            open_index: None,
            highlighted_item: None,
            last_action: None,
            item_width: 160.0,
            item_height: 28.0,
            focused: false,
            focus_title: None,
        }
    }

    pub fn title_width(&self) -> f64 {
        72.0
    }

    pub fn title_anchor(&self, menu_idx: usize) -> Rect {
        let x0 = self.bounds.x0 + menu_idx as f64 * self.title_width();
        Rect::new(x0, self.bounds.y0, x0 + self.title_width(), self.bounds.y1)
    }

    fn hit_title(&self, pos: Point) -> Option<usize> {
        if !self.bounds.contains(pos) {
            return None;
        }
        let rel = ((pos.x - self.bounds.x0) / self.title_width()).floor() as isize;
        if rel >= 0 && (rel as usize) < self.menus.len() {
            Some(rel as usize)
        } else {
            None
        }
    }

    fn open_menu(&mut self, idx: usize) {
        self.open_index = Some(idx);
        self.focus_title = Some(idx);
        self.highlighted_item = Some(0);
        self.focused = true;
    }

    /// Leave open/focus chrome after an item is chosen or the menu is dismissed.
    pub fn exit_menu_mode(&mut self) {
        self.open_index = None;
        self.highlighted_item = None;
        self.focused = false;
        self.focus_title = None;
    }

    fn title_is_hot(&self, index: usize) -> bool {
        self.open_index == Some(index)
            || (self.focused && self.open_index.is_none() && self.focus_title == Some(index))
    }
}

impl Widget for MenuBar {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();
        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            width: Dimension::Fill,
            height: Dimension::Length(32.0),
            flex_grow: 0.0,
            flex_shrink: 0.0,
            ..LayoutStyle::base()
        })
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(constraints.max_width as f64, 32.0)
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        paint_simple_rect(scene, self.bounds, ctx.theme.surface_container, 0.0);
        if let Some(text) = ctx.text.as_deref_mut() {
            for (i, menu) in self.menus.iter().enumerate() {
                let cell = self.title_anchor(i);
                if self.title_is_hot(i) {
                    let mut hl = ctx.theme.primary;
                    hl.components[3] = 0.2;
                    paint_simple_rect(scene, cell, hl, 0.0);
                }
                text.paint_text(
                    scene,
                    &menu.title,
                    13.0,
                    ctx.theme.on_surface,
                    cell,
                    Some(cell.width() as f32),
                    TextAlign::Center,
                );
            }
        }
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        match event {
            Event::FocusGained => {
                self.focused = true;
                if self.focus_title.is_none() {
                    self.focus_title = Some(0);
                }
                ctx.request_paint();
                EventResult::HANDLED
            }
            Event::FocusLost => {
                self.exit_menu_mode();
                ctx.request_paint();
                EventResult::IGNORED
            }
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                ..
            } => {
                if let Some(ti) = self.hit_title(*position) {
                    if self.open_index == Some(ti) {
                        // Toggle closed — fully leave menu mode (do not leave File hot).
                        self.exit_menu_mode();
                    } else {
                        self.open_menu(ti);
                    }
                    ctx.request_paint();
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::PointerMove { position, .. } if self.open_index.is_some() => {
                if let Some(ti) = self.hit_title(*position) {
                    if self.open_index != Some(ti) {
                        self.open_menu(ti);
                        // Overlay damage alone does not repaint title chrome — File
                        // would stay hot while View's panel is already open.
                        ctx.request_paint();
                    }
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::F10),
                ..
            }
            | Event::KeyDown {
                key: Key::Named(NamedKey::Space),
                alt: true,
                ..
            } => {
                if self.menus.is_empty() {
                    return EventResult::IGNORED;
                }
                if self.open_index.is_some() {
                    self.exit_menu_mode();
                } else {
                    self.open_menu(0);
                }
                ctx.request_paint();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Escape),
                ..
            } if self.open_index.is_some() || self.focused => {
                self.exit_menu_mode();
                ctx.request_paint();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowLeft),
                ..
            } if self.open_index.is_some() || self.focused => {
                if self.menus.is_empty() {
                    return EventResult::HANDLED;
                }
                let cur = self.open_index.unwrap_or(0);
                let next = if cur == 0 {
                    self.menus.len() - 1
                } else {
                    cur - 1
                };
                self.open_menu(next);
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowRight),
                ..
            } if self.open_index.is_some() || self.focused => {
                if self.menus.is_empty() {
                    return EventResult::HANDLED;
                }
                let cur = self.open_index.unwrap_or(0);
                let next = (cur + 1) % self.menus.len();
                self.open_menu(next);
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowDown),
                ..
            } if self.open_index.is_none() && self.focused => {
                if !self.menus.is_empty() {
                    self.open_menu(0);
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            } if self.open_index.is_none() && self.focused => {
                if !self.menus.is_empty() {
                    self.open_menu(0);
                }
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Generic,
            label: Some("MenuBar".into()),
            focusable: true,
            actions: vec![A11yAction::Focus],
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }
}

/// Right-click / programmatic context menu popup (panel via overlay).
pub struct ContextMenu {
    id: WidgetId,
    bounds: Rect,
    pub items: Vec<MenuItem>,
    pub open: bool,
    pub anchor: Point,
    pub highlighted: usize,
    pub last_action: Option<String>,
    pub item_width: f64,
    pub item_height: f64,
}

impl ContextMenu {
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            items,
            open: false,
            anchor: Point::new(0.0, 0.0),
            highlighted: 0,
            last_action: None,
            item_width: 180.0,
            item_height: 28.0,
        }
    }

    pub fn show_at(&mut self, pos: Point) {
        self.anchor = pos;
        self.open = true;
        self.highlighted = 0;
    }

    pub fn hide(&mut self) {
        self.open = false;
    }
}

impl Widget for ContextMenu {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();
        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            width: Dimension::Length(0.0),
            height: Dimension::Length(0.0),
            ..LayoutStyle::base()
        })
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, _c: Constraints) -> Size {
        Size::new(0.0, 0.0)
    }

    fn hit_test(&self, _point: Point) -> bool {
        false
    }

    fn paint(&self, _ctx: &mut PaintCtx, _scene: &mut Scene) {}

    fn event(&mut self, _ctx: &mut EventCtx, event: &Event) -> EventResult {
        match event {
            Event::PointerDown {
                button: PointerButton::Secondary,
                position,
                ..
            } => {
                self.show_at(*position);
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Escape),
                ..
            } if self.open => {
                self.hide();
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Generic,
            label: Some("ContextMenu".into()),
            focusable: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_down(key: Key) -> Event {
        Event::KeyDown {
            key,
            repeat: false,
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    #[test]
    fn menu_bar_f10_opens_and_arrows_cycle() {
        let mut bar = MenuBar::new(vec![
            Menu::new(
                "File",
                vec![MenuItem::new("n", "New"), MenuItem::new("o", "Open")],
            ),
            Menu::new("Edit", vec![MenuItem::new("c", "Copy")]),
        ]);
        let path = [bar.id()];
        let mut ctx = EventCtx::new(bar.id(), &path, Default::default());
        assert!(bar.open_index.is_none());
        let r = bar.event(&mut ctx, &key_down(Key::Named(NamedKey::F10)));
        assert!(r.handled);
        assert_eq!(bar.open_index, Some(0));

        let r = bar.event(&mut ctx, &key_down(Key::Named(NamedKey::ArrowRight)));
        assert!(r.handled);
        assert_eq!(bar.open_index, Some(1));
    }

    #[test]
    fn menu_panel_arrow_and_enter() {
        let owner = alloc_widget_id();
        let mut panel = MenuPanel::new(
            owner,
            vec![
                MenuItem::new("a", "Alpha"),
                MenuItem::new("b", "Beta"),
                MenuItem::new("c", "Gamma"),
            ],
        );
        let path = [panel.id()];
        let mut ctx = EventCtx::new(panel.id(), &path, Default::default());
        panel.event(&mut ctx, &key_down(Key::Named(NamedKey::ArrowDown)));
        assert_eq!(panel.highlighted, 1);
        panel.event(&mut ctx, &key_down(Key::Named(NamedKey::Enter)));
        assert_eq!(panel.pending_action.as_deref(), Some("b"));
        assert!(panel.pending_close);
    }

    #[test]
    fn menu_panel_hover_highlight_moves() {
        let owner = alloc_widget_id();
        let mut panel = MenuPanel::new(
            owner,
            vec![
                MenuItem::new("a", "Alpha"),
                MenuItem::new("b", "Beta"),
                MenuItem::new("c", "Gamma"),
            ],
        );
        panel.set_bounds(Rect::new(0.0, 0.0, 160.0, 84.0));
        let path = [panel.id()];
        let mut ctx = EventCtx::new(panel.id(), &path, Default::default());
        panel.event(
            &mut ctx,
            &Event::PointerMove {
                position: Point::new(20.0, 40.0),
                id: 0,
            },
        );
        assert_eq!(panel.highlighted, 1);
        panel.event(
            &mut ctx,
            &Event::PointerMove {
                position: Point::new(20.0, 70.0),
                id: 0,
            },
        );
        assert_eq!(panel.highlighted, 2);
    }

    #[test]
    fn menu_bar_pointer_moves_switches_open_menu() {
        let mut bar = MenuBar::new(vec![
            Menu::new("File", vec![MenuItem::new("n", "New")]),
            Menu::new("View", vec![MenuItem::new("z", "Zoom")]),
        ]);
        bar.set_bounds(Rect::new(0.0, 0.0, 200.0, 32.0));
        bar.open_menu(0);
        let path = [bar.id()];
        let mut ctx = EventCtx::new(bar.id(), &path, Default::default());
        bar.event(
            &mut ctx,
            &Event::PointerMove {
                position: Point::new(100.0, 16.0),
                id: 0,
            },
        );
        assert_eq!(bar.open_index, Some(1));
    }

    #[test]
    fn activating_menu_item_exits_menu_mode_so_file_is_not_left_hot() {
        use synui_core::{OverlayConfig, WidgetStore};

        let mut store = WidgetStore::new();
        let mut bar = MenuBar::new(vec![
            Menu::new("File", vec![MenuItem::new("n", "New")]),
            Menu::new("View", vec![MenuItem::new("show-preview", "Show Preview")]),
        ]);
        bar.open_menu(1);
        assert!(bar.title_is_hot(1));
        let bar_id = store.insert(bar);
        store.set_root(bar_id);
        store.focus_manager_mut().register(bar_id, true);
        store.focus_manager_mut().focus(bar_id);

        let panel = store.insert(MenuPanel::new(
            bar_id,
            vec![MenuItem::new("show-preview", "Show Preview")],
        ));
        if let Some(w) = store.get_mut(panel) {
            if let Some(p) = w.as_any_mut().downcast_mut::<MenuPanel>() {
                p.highlighted = 0;
                p.activate_highlighted();
            }
        }
        store.insert_overlay(
            panel,
            Rect::ZERO,
            OverlayConfig::popup(),
            Vec::new(),
            bar_id,
        );

        MenuPanel::apply_pending(&mut store);

        let bar = store
            .get(bar_id)
            .unwrap()
            .as_any()
            .downcast_ref::<MenuBar>()
            .unwrap();
        assert!(bar.open_index.is_none());
        assert!(!bar.title_is_hot(0), "File must not stay highlighted");
        assert!(!bar.title_is_hot(1), "View must not stay highlighted");
        assert_eq!(bar.last_action.as_deref(), Some("show-preview"));
        assert!(store.focus_manager().focused().is_none());
    }

    #[test]
    fn switching_open_menu_moves_title_hot_state() {
        let mut bar = MenuBar::new(vec![
            Menu::new("File", vec![MenuItem::new("n", "New")]),
            Menu::new("View", vec![MenuItem::new("z", "Zoom")]),
        ]);
        bar.set_bounds(Rect::new(0.0, 0.0, 200.0, 32.0));
        bar.open_menu(0);
        assert!(bar.title_is_hot(0));
        assert!(!bar.title_is_hot(1));
        let path = [bar.id()];
        let mut ctx = EventCtx::new(bar.id(), &path, Default::default());
        bar.event(
            &mut ctx,
            &Event::PointerMove {
                position: Point::new(100.0, 16.0),
                id: 0,
            },
        );
        assert_eq!(bar.open_index, Some(1));
        assert!(!bar.title_is_hot(0), "File must cool when View opens");
        assert!(bar.title_is_hot(1), "View must be hot with its panel");
    }

    #[test]
    fn backdrop_dismiss_exits_menu_mode() {
        use synui_core::{OverlayConfig, WidgetStore};

        let mut store = WidgetStore::new();
        let mut bar = MenuBar::new(vec![
            Menu::new("File", vec![MenuItem::new("n", "New")]),
            Menu::new("View", vec![MenuItem::new("z", "Zoom")]),
        ]);
        bar.open_menu(0);
        assert!(bar.title_is_hot(0));
        let bar_id = store.insert(bar);
        store.set_root(bar_id);
        store.set_viewport(Size::new(800.0, 600.0));
        store.focus_manager_mut().register(bar_id, true);
        store.focus_manager_mut().focus(bar_id);

        let panel = store.insert(MenuPanel::new(bar_id, vec![MenuItem::new("n", "New")]));
        store.insert_overlay(
            panel,
            Rect::new(0.0, 32.0, 160.0, 60.0),
            OverlayConfig::popup(),
            Vec::new(),
            bar_id,
        );

        let result = store.dispatch_event(&Event::PointerDown {
            button: PointerButton::Primary,
            position: Point::new(400.0, 400.0),
            id: 0,
            shift: false,
            ctrl: false,
        });
        assert!(result.handled, "backdrop click should dismiss popup");
        crate::sync_overlays(&mut store);

        let bar = store
            .get(bar_id)
            .unwrap()
            .as_any()
            .downcast_ref::<MenuBar>()
            .unwrap();
        assert!(bar.open_index.is_none());
        assert!(
            !bar.title_is_hot(0),
            "outside click must clear File hot state"
        );
        assert!(!bar.title_is_hot(1));
    }

    #[test]
    fn toggling_open_title_closed_exits_menu_mode() {
        let mut bar = MenuBar::new(vec![
            Menu::new("File", vec![MenuItem::new("n", "New")]),
            Menu::new("View", vec![MenuItem::new("z", "Zoom")]),
        ]);
        bar.set_bounds(Rect::new(0.0, 0.0, 200.0, 32.0));
        bar.open_menu(1);
        let path = [bar.id()];
        let mut ctx = EventCtx::new(bar.id(), &path, Default::default());
        bar.event(
            &mut ctx,
            &Event::PointerDown {
                button: PointerButton::Primary,
                position: Point::new(100.0, 16.0),
                id: 0,
                shift: false,
                ctrl: false,
            },
        );
        assert!(bar.open_index.is_none());
        assert!(!bar.title_is_hot(0));
        assert!(!bar.title_is_hot(1));
    }
}
