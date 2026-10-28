//! Synchronize overlay stack with widget open/expanded state.

use synui_core::{OverlayConfig, OverlayPlacement, Rect, Size, Widget, WidgetId, WidgetStore};
use synui_layout::PopupAnchor;

use crate::containers::{BottomSheet, Dialog, ModalHost, PopupHost, SnackBar};
use crate::input::{Dropdown, DropdownMenu};
use crate::menu::{ContextMenu, MenuBar, MenuPanel};
use crate::navigation::NavigationDrawer;
use crate::tooltip::{estimate_tooltip_size, TooltipBubble};

fn walk_widgets(store: &WidgetStore, root: WidgetId, out: &mut Vec<WidgetId>) {
    out.push(root);
    for &child in store.children_of(root) {
        walk_widgets(store, child, out);
    }
}

fn all_widget_ids(store: &WidgetStore) -> Vec<WidgetId> {
    let mut ids = Vec::new();
    if let Some(root) = store.root() {
        walk_widgets(store, root, &mut ids);
    }
    for owner in store.overlay_owners() {
        if !ids.contains(&owner) {
            ids.push(owner);
        }
    }
    ids
}

/// Apply pending overlay dismiss notifications to owner widgets.
pub fn apply_overlay_dismiss(store: &mut WidgetStore) {
    let owners = store.take_pending_overlay_dismiss();
    for owner in owners {
        notify_overlay_dismissed(store, owner);
    }
}

fn notify_overlay_dismissed(store: &mut WidgetStore, owner: WidgetId) {
    enum DismissKind {
        MenuBar,
        Other,
        Unknown,
    }
    let kind = if let Some(w) = store.get_mut(owner) {
        if let Some(dd) = w.as_any_mut().downcast_mut::<Dropdown>() {
            dd.expanded = false;
            dd.overlay_popup_bounds = None;
            DismissKind::Other
        } else if let Some(dlg) = w.as_any_mut().downcast_mut::<Dialog>() {
            dlg.open = false;
            if let Some(f) = dlg.on_dismiss.as_mut() {
                f();
            }
            DismissKind::Other
        } else if let Some(sheet) = w.as_any_mut().downcast_mut::<BottomSheet>() {
            sheet.open = false;
            sheet.drag_offset = 0.0;
            DismissKind::Other
        } else if let Some(bar) = w.as_any_mut().downcast_mut::<SnackBar>() {
            bar.visible = false;
            DismissKind::Other
        } else if let Some(mb) = w.as_any_mut().downcast_mut::<MenuBar>() {
            // Backdrop click previously only cleared open_index — File stayed hot
            // via focused + focus_title. Fully leave menu mode.
            mb.exit_menu_mode();
            DismissKind::MenuBar
        } else if let Some(cm) = w.as_any_mut().downcast_mut::<ContextMenu>() {
            cm.open = false;
            DismissKind::Other
        } else if let Some(drawer) = w.as_any_mut().downcast_mut::<NavigationDrawer>() {
            drawer.open = false;
            DismissKind::Other
        } else {
            DismissKind::Unknown
        }
    } else {
        DismissKind::Unknown
    };
    if matches!(kind, DismissKind::Unknown) {
        return;
    }
    store.mark_paint_dirty_widget(owner);
    if matches!(kind, DismissKind::MenuBar) && store.focus_manager().focused() == Some(owner) {
        store.focus_manager_mut().clear_focus();
    }
}

/// Sync overlay stack with Dropdown / Dialog / BottomSheet / SnackBar / Menu / Tooltip state.
pub fn sync_overlays(store: &mut WidgetStore) {
    apply_overlay_dismiss(store);
    DropdownMenu::apply_pending_selections(store);
    MenuPanel::apply_pending(store);

    let ids = all_widget_ids(store);
    for id in ids {
        if store
            .get(id)
            .map(|w| w.as_any().downcast_ref::<Dropdown>().is_some())
            == Some(true)
        {
            sync_dropdown(store, id);
        } else if store
            .get(id)
            .map(|w| w.as_any().downcast_ref::<Dialog>().is_some())
            == Some(true)
        {
            sync_dialog(store, id);
        } else if store
            .get(id)
            .map(|w| w.as_any().downcast_ref::<BottomSheet>().is_some())
            == Some(true)
        {
            sync_bottom_sheet(store, id);
        } else if store
            .get(id)
            .map(|w| w.as_any().downcast_ref::<SnackBar>().is_some())
            == Some(true)
        {
            sync_snackbar(store, id);
        } else if store
            .get(id)
            .map(|w| w.as_any().downcast_ref::<MenuBar>().is_some())
            == Some(true)
        {
            sync_menu_bar(store, id);
        } else if store
            .get(id)
            .map(|w| w.as_any().downcast_ref::<ContextMenu>().is_some())
            == Some(true)
        {
            sync_context_menu(store, id);
        } else if store
            .get(id)
            .map(|w| w.as_any().downcast_ref::<NavigationDrawer>().is_some())
            == Some(true)
        {
            sync_navigation_drawer(store, id);
        }
    }
    sync_tooltip(store);
}

/// Reposition / create the hover tip overlay from the current tip session.
///
/// Call after [`crate::apply_scroll_views`] / sticky so the tip tracks scrolled anchors.
pub fn sync_tooltip(store: &mut WidgetStore) {
    let has_modal = store.overlay_stack().iter().any(|e| e.config.modal);
    let (want_anchor, want_text) = if has_modal {
        (None, None)
    } else {
        let session = store.tooltip_session();
        match session.candidate {
            Some(anchor)
                if session.dwell_ms >= session.show_delay_ms
                    || session.shown_anchor == Some(anchor) =>
            {
                let text = store
                    .resolve_tooltip(anchor)
                    .map(|(_, t)| t)
                    .filter(|t| !t.is_empty());
                (Some(anchor), text)
            }
            _ => (None, None),
        }
    };

    let shown = store.tooltip_session().shown_anchor;
    if let Some(anchor) = shown {
        if want_anchor != Some(anchor) || want_text.is_none() {
            store.remove_overlay_for_owner(anchor);
            store.tooltip_session_mut().shown_anchor = None;
        }
    }

    let Some(anchor) = want_anchor else {
        return;
    };
    let Some(text) = want_text else {
        return;
    };

    let anchor_bounds = match store.get(anchor) {
        Some(w) => w.bounds(),
        None => {
            store.remove_overlay_for_owner(anchor);
            store.tooltip_session_mut().shown_anchor = None;
            return;
        }
    };

    let popup_size = estimate_tooltip_size(&text);
    let pos = PopupAnchor::position(
        anchor_bounds,
        popup_size,
        OverlayPlacement::Above,
        store.viewport(),
        store.layout_direction(),
    );
    // If Above was clamped to y=0 and still covers the anchor, try Below.
    let mut pos = pos;
    if pos.y + popup_size.height > anchor_bounds.y0 && pos.y < anchor_bounds.y1 {
        pos = PopupAnchor::position(
            anchor_bounds,
            popup_size,
            OverlayPlacement::Below,
            store.viewport(),
            store.layout_direction(),
        );
    }
    let bounds = Rect::new(
        pos.x,
        pos.y,
        pos.x + popup_size.width,
        pos.y + popup_size.height,
    );

    if let Some(host) = store.overlay_for_owner(anchor) {
        if let Some(w) = store.get_mut(host) {
            w.set_bounds(bounds);
            if let Some(tip) = w.as_any_mut().downcast_mut::<TooltipBubble>() {
                tip.text = text;
            }
        }
        store.tooltip_session_mut().shown_anchor = Some(anchor);
        return;
    }

    let bubble = store.insert(TooltipBubble::new(text));
    store.insert_overlay(bubble, bounds, OverlayConfig::tooltip(), vec![], anchor);
    store.tooltip_session_mut().shown_anchor = Some(anchor);
}

fn sync_dropdown(store: &mut WidgetStore, id: WidgetId) {
    let snapshot = store.get(id).and_then(|w| {
        w.as_any().downcast_ref::<Dropdown>().map(|dd| {
            (
                dd.expanded,
                dd.options.clone(),
                dd.selected,
                dd.highlighted,
                dd.field_bounds(),
                dd.item_height,
            )
        })
    });
    if snapshot.is_none() {
        return;
    }
    let (expanded, options, selected, highlighted, anchor, item_height) = snapshot.unwrap();
    let has_overlay = store.overlay_for_owner(id).is_some();
    if expanded && !has_overlay {
        let option_count = options.len();
        let menu = store.insert(
            DropdownMenu::new(id)
                .with_options(options)
                .with_selected(selected)
                .with_highlighted(highlighted),
        );
        let popup_host = store.insert(PopupHost::new().with_child(menu));
        store.set_children(popup_host, vec![menu]);

        let popup_size = Size::new(anchor.width(), item_height * option_count as f64);
        let pos = PopupAnchor::position(
            anchor,
            popup_size,
            OverlayPlacement::Below,
            store.viewport(),
            store.layout_direction(),
        );
        let bounds = Rect::new(
            pos.x,
            pos.y,
            pos.x + popup_size.width,
            pos.y + popup_size.height,
        );
        store.insert_overlay(popup_host, bounds, OverlayConfig::popup(), vec![], id);
        if let Some(w) = store.get_mut(id) {
            if let Some(dd_mut) = w.as_any_mut().downcast_mut::<Dropdown>() {
                dd_mut.overlay_popup_bounds = Some(bounds);
            }
        }
    } else if !expanded && has_overlay {
        store.remove_overlay_for_owner(id);
        if let Some(w) = store.get_mut(id) {
            if let Some(dd_mut) = w.as_any_mut().downcast_mut::<Dropdown>() {
                dd_mut.overlay_popup_bounds = None;
            }
        }
    } else if expanded && has_overlay {
        let option_count = store
            .get(id)
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<Dropdown>()
                    .map(|d| d.options.len())
            })
            .unwrap_or(0);
        let popup_size = Size::new(anchor.width(), item_height * option_count as f64);
        let pos = PopupAnchor::position(
            anchor,
            popup_size,
            OverlayPlacement::Below,
            store.viewport(),
            store.layout_direction(),
        );
        let bounds = Rect::new(
            pos.x,
            pos.y,
            pos.x + popup_size.width,
            pos.y + popup_size.height,
        );
        if let Some(w) = store.get_mut(id) {
            if let Some(dd_mut) = w.as_any_mut().downcast_mut::<Dropdown>() {
                dd_mut.overlay_popup_bounds = Some(bounds);
            }
        }
        if let Some(host) = store.overlay_for_owner(id) {
            if let Some(host_w) = store.get_mut(host) {
                host_w.set_bounds(bounds);
            }
            sync_dropdown_menu_state(store, host, selected, highlighted);
        }
    }
}

fn sync_dropdown_menu_state(
    store: &mut WidgetStore,
    host: WidgetId,
    selected: usize,
    highlighted: usize,
) {
    let children: Vec<WidgetId> = store.children_of(host).to_vec();
    for child in children {
        if let Some(w) = store.get_mut(child) {
            if let Some(menu) = w.as_any_mut().downcast_mut::<DropdownMenu>() {
                menu.selected = selected;
                menu.highlighted = highlighted;
            }
        }
    }
}

fn sync_dialog(store: &mut WidgetStore, id: WidgetId) {
    let open = store
        .get(id)
        .and_then(|w| w.as_any().downcast_ref::<Dialog>().map(|dlg| dlg.open));
    if open.is_none() {
        return;
    }
    let open = open.unwrap();
    let has_overlay = store.overlay_for_owner(id).is_some();
    if open && !has_overlay {
        let host = store.insert(ModalHost::new());
        store.reparent_into_overlay(id, host);
        let trap = store.collect_focusable(id);
        let vp = store.viewport();
        let host_bounds = Rect::new(0.0, 0.0, vp.width, vp.height);
        store.insert_overlay(host, host_bounds, OverlayConfig::modal(), trap, id);
        let dlg_w = (vp.width * 0.8).clamp(280.0, 560.0);
        let dlg_h = (vp.height * 0.35).clamp(180.0, 420.0);
        let origin = PopupAnchor::position(
            Rect::ZERO,
            Size::new(dlg_w, dlg_h),
            OverlayPlacement::Center,
            vp,
            store.layout_direction(),
        );
        let dialog_bounds = Rect::new(origin.x, origin.y, origin.x + dlg_w, origin.y + dlg_h);
        if let Some(w) = store.get_mut(id) {
            w.set_bounds(dialog_bounds);
        }
    } else if !open && has_overlay {
        store.remove_overlay_for_owner(id);
    }
}

/// Side drawer as a left rail on a modal overlay (scrim + backdrop dismiss).
fn sync_navigation_drawer(store: &mut WidgetStore, id: WidgetId) {
    let snapshot = store.get(id).and_then(|w| {
        w.as_any()
            .downcast_ref::<NavigationDrawer>()
            .map(|d| (d.open, d.width))
    });
    let Some((open, width)) = snapshot else {
        return;
    };
    let has_overlay = store.overlay_for_owner(id).is_some();
    let vp = store.viewport();
    if open && !has_overlay {
        let host = store.insert(ModalHost::new());
        store.reparent_into_overlay(id, host);
        let trap = store.collect_focusable(id);
        let host_bounds = Rect::new(0.0, 0.0, vp.width, vp.height);
        store.insert_overlay(host, host_bounds, OverlayConfig::modal(), trap, id);
        place_drawer_rail(store, id, width, vp);
    } else if open && has_overlay {
        if let Some(host) = store.overlay_for_owner(id) {
            if let Some(w) = store.get_mut(host) {
                w.set_bounds(Rect::new(0.0, 0.0, vp.width, vp.height));
            }
        }
        place_drawer_rail(store, id, width, vp);
    } else if !open && has_overlay {
        store.remove_overlay_for_owner(id);
    }
}

fn place_drawer_rail(store: &mut WidgetStore, id: WidgetId, width: f64, vp: Size) {
    let rail_w = width.min(vp.width).max(1.0);
    let bounds = Rect::new(0.0, 0.0, rail_w, vp.height.max(1.0));
    if let Some(w) = store.get_mut(id) {
        w.set_bounds(bounds);
    }
}

fn sync_bottom_sheet(store: &mut WidgetStore, id: WidgetId) {
    let open = store
        .get(id)
        .and_then(|w| w.as_any().downcast_ref::<BottomSheet>().map(|s| s.open));
    if open.is_none() {
        return;
    }
    let open = open.unwrap();
    let has_overlay = store.overlay_for_owner(id).is_some();
    if open && !has_overlay {
        let host = store.insert(ModalHost::new());
        store.reparent_into_overlay(id, host);
        let vp = store.viewport();
        let host_bounds = Rect::new(0.0, 0.0, vp.width, vp.height);
        let trap = store.collect_focusable(id);
        store.insert_overlay(host, host_bounds, OverlayConfig::modal(), trap, id);
        let sheet_height = (vp.height * 0.4).clamp(200.0, vp.height * 0.85);
        let origin = PopupAnchor::position(
            Rect::ZERO,
            Size::new(vp.width, sheet_height),
            OverlayPlacement::BottomSheet,
            vp,
            store.layout_direction(),
        );
        let sheet_bounds = Rect::new(
            origin.x,
            origin.y,
            origin.x + vp.width,
            origin.y + sheet_height,
        );
        if let Some(w) = store.get_mut(id) {
            w.set_bounds(sheet_bounds);
        }
    } else if !open && has_overlay {
        store.remove_overlay_for_owner(id);
    }
}

fn sync_snackbar(store: &mut WidgetStore, id: WidgetId) {
    let visible = store
        .get(id)
        .and_then(|w| w.as_any().downcast_ref::<SnackBar>().map(|b| b.visible));
    if visible.is_none() {
        return;
    }
    let visible = visible.unwrap();
    let has_overlay = store.overlay_for_owner(id).is_some();
    if visible && !has_overlay {
        let host = store.insert(ModalHost::new());
        store.reparent_into_overlay(id, host);
        let vp = store.viewport();
        let host_bounds = Rect::new(0.0, 0.0, vp.width, vp.height);
        store.insert_overlay(host, host_bounds, OverlayConfig::persistent(), vec![], id);
        let bar_height = 48.0;
        let margin = 16.0;
        let bar_bounds = Rect::new(
            margin,
            vp.height - bar_height - margin,
            vp.width - margin,
            vp.height - margin,
        );
        if let Some(w) = store.get_mut(id) {
            w.set_bounds(bar_bounds);
        }
    } else if !visible && has_overlay {
        store.remove_overlay_for_owner(id);
    }
}

fn offset_subtree_bounds(store: &mut WidgetStore, root: WidgetId, dx: f64, dy: f64) {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(w) = store.get_mut(id) {
            let b = w.bounds();
            w.set_bounds(Rect::new(b.x0 + dx, b.y0 + dy, b.x1 + dx, b.y1 + dy));
        }
        for &child in store.children_of(id) {
            stack.push(child);
        }
    }
}

/// Layout overlay widget subtrees after the main layout pass.
pub fn layout_overlays(store: &mut WidgetStore, text: &mut dyn synui_core::TextMeasurer) {
    use synui_layout::LayoutEngine;
    let mut layout = LayoutEngine::new();
    let vp = store.viewport();
    let roots: Vec<WidgetId> = store.overlay_stack().iter().map(|e| e.root).collect();
    for root in roots {
        // Absolute tips are positioned by sync_tooltip; taffy would pin them to (0,0).
        if store
            .get(root)
            .map(|w| w.as_any().is::<TooltipBubble>())
            .unwrap_or(false)
        {
            continue;
        }
        let host_bounds = store
            .get(root)
            .map(|w| w.bounds())
            .unwrap_or_else(|| Rect::new(0.0, 0.0, vp.width, vp.height));
        let is_popup = store
            .get(root)
            .map(|w| w.as_any().is::<PopupHost>())
            .unwrap_or(false);
        if is_popup {
            layout.compute(
                store,
                text,
                root,
                Size::new(host_bounds.width(), host_bounds.height()),
            );
            offset_subtree_bounds(store, root, host_bounds.x0, host_bounds.y0);
        } else {
            layout.compute(store, text, root, Size::new(vp.width, vp.height));
        }
    }
    // ModalHost centers children; pin open drawers to the left rail and lay out
    // their nav buttons so the panel is not an empty grey strip.
    finish_navigation_drawer_overlays(store, text);
}

fn finish_navigation_drawer_overlays(
    store: &mut WidgetStore,
    text: &mut dyn synui_core::TextMeasurer,
) {
    use synui_layout::LayoutEngine;
    let vp = store.viewport();
    let owners: Vec<WidgetId> = store.overlay_owners().collect();
    for owner in owners {
        let width = match store
            .get(owner)
            .and_then(|w| w.as_any().downcast_ref::<NavigationDrawer>())
        {
            Some(d) if d.open => d.width,
            _ => continue,
        };
        let rail_w = width.min(vp.width).max(1.0);
        let mut layout = LayoutEngine::new();
        layout.compute(store, text, owner, Size::new(rail_w, vp.height.max(1.0)));
        if let Some(host) = store.overlay_for_owner(owner) {
            if let Some(w) = store.get_mut(host) {
                w.set_bounds(Rect::new(0.0, 0.0, vp.width, vp.height));
            }
        }
    }
}

fn sync_menu_bar(store: &mut WidgetStore, id: WidgetId) {
    let snapshot = store.get(id).and_then(|w| {
        w.as_any().downcast_ref::<MenuBar>().map(|mb| {
            (
                mb.open_index,
                mb.highlighted_item.unwrap_or(0),
                mb.item_width,
                mb.item_height,
                mb.open_index
                    .and_then(|i| mb.menus.get(i).map(|m| m.items.clone()))
                    .unwrap_or_default(),
                mb.open_index.map(|i| mb.title_anchor(i)),
            )
        })
    });
    let Some((open_index, highlighted, item_width, item_height, items, anchor)) = snapshot else {
        return;
    };
    let has_overlay = store.overlay_for_owner(id).is_some();
    if open_index.is_some() && !has_overlay && !items.is_empty() {
        let n = items.len();
        let panel = store.insert(MenuPanel::new(id, items).with_highlighted(highlighted));
        let popup_host = store.insert(PopupHost::new().with_child(panel));
        store.set_children(popup_host, vec![panel]);
        let anchor = anchor.unwrap_or(Rect::ZERO);
        let popup_size = Size::new(item_width, item_height * n as f64);
        let pos = PopupAnchor::position(
            anchor,
            popup_size,
            OverlayPlacement::Below,
            store.viewport(),
            store.layout_direction(),
        );
        let bounds = Rect::new(
            pos.x,
            pos.y,
            pos.x + popup_size.width,
            pos.y + popup_size.height,
        );
        let trap = store.collect_focusable(panel);
        store.insert_overlay(popup_host, bounds, OverlayConfig::popup(), trap, id);
        if let Some(w) = store.get_mut(panel) {
            w.set_bounds(bounds);
        }
    } else if open_index.is_none() && has_overlay {
        store.remove_overlay_for_owner(id);
    } else if open_index.is_some() && has_overlay {
        let n = items.len().max(1);
        let popup_size = Size::new(item_width, item_height * n as f64);
        let anchor = anchor.unwrap_or(Rect::ZERO);
        let pos = PopupAnchor::position(
            anchor,
            popup_size,
            OverlayPlacement::Below,
            store.viewport(),
            store.layout_direction(),
        );
        let bounds = Rect::new(
            pos.x,
            pos.y,
            pos.x + popup_size.width,
            pos.y + popup_size.height,
        );
        if let Some(host) = store.overlay_for_owner(id) {
            if let Some(host_w) = store.get_mut(host) {
                host_w.set_bounds(bounds);
            }
            let children: Vec<WidgetId> = store.children_of(host).to_vec();
            let mut panel_highlight = highlighted;
            for child in children {
                if let Some(w) = store.get_mut(child) {
                    if let Some(p) = w.as_any_mut().downcast_mut::<MenuPanel>() {
                        let switched = p.items.len() != items.len()
                            || p.items
                                .iter()
                                .zip(items.iter())
                                .any(|(a, b)| a.id != b.id || a.label != b.label);
                        if switched {
                            p.items = items.clone();
                            p.highlighted = 0;
                        }
                        // Keep panel-owned hover/keyboard highlight; do not reset from MenuBar.
                        panel_highlight = p.highlighted;
                        p.set_bounds(bounds);
                    }
                }
            }
            if let Some(w) = store.get_mut(id) {
                if let Some(mb) = w.as_any_mut().downcast_mut::<MenuBar>() {
                    mb.highlighted_item = Some(panel_highlight);
                }
            }
            // Title hot state follows open_index; ensure the bar is in the damage set
            // when the open menu switches (panel-only dirty rects skip the titles).
            store.mark_paint_dirty_widget(id);
        }
    }
}

fn sync_context_menu(store: &mut WidgetStore, id: WidgetId) {
    let snapshot = store.get(id).and_then(|w| {
        w.as_any().downcast_ref::<ContextMenu>().map(|cm| {
            (
                cm.open,
                cm.anchor,
                cm.items.clone(),
                cm.highlighted,
                cm.item_width,
                cm.item_height,
            )
        })
    });
    let Some((open, anchor, items, highlighted, item_width, item_height)) = snapshot else {
        return;
    };
    let has_overlay = store.overlay_for_owner(id).is_some();
    if open && !has_overlay && !items.is_empty() {
        let n = items.len();
        let panel = store.insert(MenuPanel::new(id, items).with_highlighted(highlighted));
        let popup_host = store.insert(PopupHost::new().with_child(panel));
        store.set_children(popup_host, vec![panel]);
        let popup_size = Size::new(item_width, item_height * n as f64);
        let bounds = Rect::new(
            anchor.x,
            anchor.y,
            anchor.x + popup_size.width,
            anchor.y + popup_size.height,
        );
        let trap = store.collect_focusable(panel);
        store.insert_overlay(popup_host, bounds, OverlayConfig::popup(), trap, id);
        if let Some(w) = store.get_mut(panel) {
            w.set_bounds(bounds);
        }
    } else if !open && has_overlay {
        store.remove_overlay_for_owner(id);
    } else if open && has_overlay {
        let n = items.len().max(1);
        let popup_size = Size::new(item_width, item_height * n as f64);
        let bounds = Rect::new(
            anchor.x,
            anchor.y,
            anchor.x + popup_size.width,
            anchor.y + popup_size.height,
        );
        if let Some(host) = store.overlay_for_owner(id) {
            if let Some(host_w) = store.get_mut(host) {
                host_w.set_bounds(bounds);
            }
            let children: Vec<WidgetId> = store.children_of(host).to_vec();
            for child in children {
                if let Some(w) = store.get_mut(child) {
                    if let Some(p) = w.as_any_mut().downcast_mut::<MenuPanel>() {
                        // Context menus don't switch content while open; preserve hover.
                        let _ = highlighted;
                        p.set_bounds(bounds);
                    }
                }
            }
        }
    }
}
