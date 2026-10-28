//! Golden PNG and interaction regression tests.

use synui_core::{Key, NamedKey, Signal, WidgetStore};
use synui_layout::LayoutEngine;
use synui_render::TextRenderer;
use synui_testing::{simulate_key, simulate_tap, snapshot_png};
use synui_theme::Theme;
use synui_widgets::{Button, Dialog, Dropdown, Flex, Label, TabBar};

#[test]
fn golden_button_png() {
    let mut store = WidgetStore::new();
    let btn = store.insert(Button::new("OK"));
    store.set_root(btn);
    let png = snapshot_png(&mut store, &Theme::default(), 80, 40).expect("png");
    insta::assert_binary_snapshot!("button_default.png", png);
}

#[test]
fn golden_label_light_dark() {
    let mut store = WidgetStore::new();
    let label = store.insert(Label::new("SynUI"));
    store.set_root(label);
    let light = snapshot_png(&mut store, &Theme::default(), 100, 32).expect("png");
    insta::assert_binary_snapshot!("label_light.png", light);
    let dark = Theme::from_seed(vello::peniko::Color::from_rgb8(103, 80, 164), true);
    let dark_png = snapshot_png(&mut store, &dark, 100, 32).expect("png");
    insta::assert_binary_snapshot!("label_dark.png", dark_png);
}

#[test]
fn golden_dropdown_expanded() {
    let mut store = WidgetStore::new();
    let mut dd = Dropdown::new(vec!["A".into(), "B".into()]);
    dd.expanded = true;
    let id = store.insert(dd);
    store.set_root(id);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(
        &mut store,
        &mut text,
        id,
        synui_core::Size::new(120.0, 120.0),
    );
    synui_widgets::sync_overlays(&mut store);
    synui_widgets::layout_overlays(&mut store, &mut text);
    let png = snapshot_png(&mut store, &Theme::default(), 120, 120).expect("png");
    insta::assert_binary_snapshot!("dropdown_expanded.png", png);
}

#[test]
fn golden_dialog_open_scrim() {
    let mut store = WidgetStore::new();
    let ok = store.insert(Button::new("OK"));
    let mut dlg = Dialog::new("Title");
    dlg.open = true;
    let dlg_id = store.insert(dlg.with_children(vec![ok]));
    store.set_children(dlg_id, vec![ok]);
    store.set_root(dlg_id);
    synui_widgets::sync_overlays(&mut store);
    let mut text = TextRenderer::new();
    synui_widgets::layout_overlays(&mut store, &mut text);
    let png = snapshot_png(&mut store, &Theme::default(), 320, 240).expect("png");
    insta::assert_binary_snapshot!("dialog_open.png", png);
}

#[test]
fn golden_tabbar_selected_png() {
    let mut store = WidgetStore::new();
    let mut tabs = TabBar::new(vec!["Home".into(), "Settings".into()]);
    tabs.selected = 1;
    let id = store.insert(tabs);
    store.set_root(id);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(
        &mut store,
        &mut text,
        id,
        synui_core::Size::new(200.0, 48.0),
    );
    let png = snapshot_png(&mut store, &Theme::default(), 200, 48).expect("png");
    insta::assert_binary_snapshot!("tabbar_selected.png", png);
}

#[test]
fn interaction_counter_increment() {
    let count = Signal::new(0i32);
    let mut store = WidgetStore::new();
    let label = store.insert(Label::new(format!("Count: {}", count.get())));
    let sig = count.clone();
    let btn = store.insert(Button::new("+").on_press(move || sig.update(|c| *c += 1)));
    let flex = store.insert(Flex::new().column().with_children(vec![label, btn]));
    store.set_children(flex, vec![label, btn]);
    store.set_root(flex);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(
        &mut store,
        &mut text,
        flex,
        synui_core::Size::new(200.0, 100.0),
    );
    let btn_bounds = store.get(btn).unwrap().bounds();
    simulate_tap(&mut store, btn_bounds.center().x, btn_bounds.center().y);
    assert_eq!(count.get(), 1);
}

#[test]
fn interaction_dropdown_select_option() {
    let mut store = WidgetStore::new();
    let dd = Dropdown::new(vec!["One".into(), "Two".into()]);
    let id = store.insert(dd);
    store.set_root(id);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(
        &mut store,
        &mut text,
        id,
        synui_core::Size::new(160.0, 120.0),
    );
    let field = store.get(id).unwrap().bounds();
    simulate_tap(&mut store, field.center().x, field.y0 + 20.0);
    synui_widgets::sync_overlays(&mut store);
    store.focus_manager_mut().focus(id);
    simulate_key(&mut store, Key::Named(NamedKey::ArrowDown));
    simulate_key(&mut store, Key::Named(NamedKey::Enter));
    synui_widgets::sync_overlays(&mut store);
    let w = store.get(id).unwrap();
    let d = w.as_any().downcast_ref::<Dropdown>().unwrap();
    assert_eq!(d.selected, 1);
    assert!(!d.expanded);
}

#[test]
fn interaction_dialog_escape_dismiss() {
    let mut store = WidgetStore::new();
    let mut dlg = Dialog::new("Dismiss");
    dlg.open = true;
    let dlg_id = store.insert(dlg);
    store.set_root(dlg_id);
    synui_widgets::sync_overlays(&mut store);
    assert_eq!(store.overlay_stack().len(), 1);
    simulate_key(&mut store, Key::Named(NamedKey::Escape));
    synui_widgets::apply_overlay_dismiss(&mut store);
    assert!(store.overlay_stack().is_empty());
    let w = store.get(dlg_id).unwrap();
    let d = w.as_any().downcast_ref::<Dialog>().unwrap();
    assert!(!d.open);
}

#[test]
fn dialog_focus_trap_tab_cycles() {
    let mut store = WidgetStore::new();
    let a = store.insert(Button::new("A"));
    let b = store.insert(Button::new("B"));
    let mut dlg = Dialog::new("Trap");
    dlg.open = true;
    let dlg_id = store.insert(dlg.with_children(vec![a, b]));
    store.set_children(dlg_id, vec![a, b]);
    store.set_root(dlg_id);
    synui_widgets::sync_overlays(&mut store);
    let trap_ids: Vec<_> = store.focus_manager().focus_trap().expect("trap").to_vec();
    assert!(trap_ids.len() >= 2);
    simulate_key(&mut store, Key::Named(NamedKey::Tab));
    let focused = store.focus_manager().focused();
    assert!(focused.is_some());
    assert!(trap_ids.contains(&focused.unwrap()));
}
