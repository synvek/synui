//! Phase 3 integration tests: overlay, dropdown, layout, snapshots.

use synui_core::{
    A11yRole, Event, Key, LayoutDirection, NamedKey, OverlayConfig, OverlayPlacement, Size,
    WidgetStore,
};
use synui_layout::{LayoutEngine, PopupAnchor};
use synui_render::TextRenderer;
use synui_testing::{assert_layout_valid, find_by_role, simulate_key, simulate_tap, snapshot_png};
use synui_theme::Theme;
use synui_widgets::{
    Button, Dialog, Dropdown, Flex, Label, ListItem, ModalHost, Slider, Switch, TabBar,
};

#[test]
fn overlay_push_pop() {
    let mut store = WidgetStore::new();
    let overlay = store.insert(ModalHost::new());
    store.push_overlay(overlay, OverlayConfig::modal(), vec![]);
    assert_eq!(store.overlay_stack().len(), 1);
    store.pop_overlay();
    assert!(store.overlay_stack().is_empty());
}

#[test]
fn widget_store_remove_clears_children() {
    let mut store = WidgetStore::new();
    let child = store.insert(Label::new("child"));
    let parent = store.insert(Flex::new().with_children(vec![child]));
    store.set_children(parent, vec![child]);
    assert!(store.remove(child));
    assert!(store.get(child).is_none());
}

#[test]
fn popup_anchor_below() {
    let anchor = synui_core::Rect::new(10.0, 10.0, 100.0, 40.0);
    let pos = PopupAnchor::position(
        anchor,
        Size::new(80.0, 120.0),
        OverlayPlacement::Below,
        Size::new(400.0, 300.0),
        LayoutDirection::Ltr,
    );
    assert!((pos.y - 40.0).abs() < 0.01);
}

#[test]
fn dropdown_layout_valid() {
    let mut store = WidgetStore::new();
    let dd = store.insert(Dropdown::new(vec!["A".into(), "B".into(), "C".into()]));
    store.set_root(dd);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, dd, Size::new(200.0, 200.0));
    assert!(assert_layout_valid(&store));
}

#[test]
fn dialog_has_a11y_role() {
    let mut store = WidgetStore::new();
    let dlg = store.insert(Dialog::new("Test"));
    store.set_root(dlg);
    let found = find_by_role(&store, A11yRole::Dialog);
    assert_eq!(found.len(), 1);
}

#[test]
fn button_snapshot_png() {
    let mut store = WidgetStore::new();
    let btn = store.insert(Button::new("OK"));
    store.set_root(btn);
    let png = snapshot_png(&mut store, &Theme::default(), 64, 64).expect("png");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn label_snapshot_png() {
    let mut store = WidgetStore::new();
    let label = store.insert(Label::new("Hello"));
    store.set_root(label);
    let png = snapshot_png(&mut store, &Theme::default(), 128, 32).expect("png");
    assert!(png.len() > 100);
}

#[test]
fn flex_layout_children() {
    let mut store = WidgetStore::new();
    let a = store.insert(Label::new("A"));
    let b = store.insert(Label::new("B"));
    let flex = store.insert(Flex::new().column().with_children(vec![a, b]));
    store.set_children(flex, vec![a, b]);
    store.set_root(flex);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, flex, Size::new(300.0, 200.0));
    assert!(assert_layout_valid(&store));
}

#[test]
fn list_item_clickable() {
    let mut store = WidgetStore::new();
    let item = store.insert(ListItem::new("Task").on_press(|| {}));
    store.set_root(item);
    simulate_tap(&mut store, 50.0, 20.0);
}

#[test]
fn slider_keyboard_step() {
    let mut store = WidgetStore::new();
    let slider = store.insert(Slider::new(0.0, 1.0));
    store.set_root(slider);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, slider, Size::new(200.0, 40.0));
    store.focus_manager_mut().focus(slider);
    simulate_key(&mut store, Key::Named(NamedKey::ArrowRight));
    let w = store.get(slider).unwrap();
    let s = w.as_any().downcast_ref::<Slider>().unwrap();
    assert!(s.value > 0.0);
}

#[test]
fn switch_toggle_space() {
    let mut store = WidgetStore::new();
    let sw = store.insert(Switch::new());
    store.set_root(sw);
    store.focus_manager_mut().focus(sw);
    simulate_key(&mut store, Key::Named(NamedKey::Space));
    let w = store.get(sw).unwrap();
    let s = w.as_any().downcast_ref::<Switch>().unwrap();
    assert!(s.on);
}

#[test]
fn tab_bar_selection() {
    let mut store = WidgetStore::new();
    let tabs = store.insert(TabBar::new(vec!["One".into(), "Two".into()]));
    store.set_root(tabs);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, tabs, Size::new(200.0, 48.0));
    let bounds = store.get(tabs).unwrap().bounds();
    let tab_w = bounds.width() / 2.0;
    simulate_tap(
        &mut store,
        bounds.x0 + tab_w * 1.5,
        bounds.y0 + bounds.height() / 2.0,
    );
    let w = store.get(tabs).unwrap();
    let t = w.as_any().downcast_ref::<TabBar>().unwrap();
    assert_eq!(t.selected, 1);
}

#[test]
fn signal_notify() {
    let sig = synui_core::Signal::new(1);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(0));
    let s = seen.clone();
    sig.subscribe(move |v| *s.borrow_mut() = *v);
    sig.set(5);
    assert_eq!(*seen.borrow(), 5);
    assert!(sig.take_dirty());
}

#[test]
fn dropdown_expand_hit_test() {
    let mut store = WidgetStore::new();
    let mut dd = Dropdown::new(vec!["X".into(), "Y".into()]);
    dd.expanded = true;
    let id = store.insert(dd);
    store.set_root(id);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, id, Size::new(200.0, 200.0));
    let w = store.get(id).unwrap();
    let d = w.as_any().downcast_ref::<Dropdown>().unwrap();
    let popup = d.popup_bounds();
    assert!(popup.height() > 0.0);
}

#[test]
fn theme_ref_extended_colors() {
    let theme = Theme::from_seed(vello::peniko::Color::from_rgb8(103, 80, 164), true);
    let tr = theme.to_theme_ref();
    assert!(tr.is_dark);
    assert_eq!(tr.secondary.components.len(), 4);
}

#[test]
fn dialog_focus_trap_tab_cycles() {
    let mut store = WidgetStore::new();
    let ok = store.insert(Button::new("OK"));
    let cancel = store.insert(Button::new("Cancel"));
    let mut dlg = Dialog::new("Confirm");
    dlg.open = true;
    let dlg_id = store.insert(dlg.with_children(vec![ok, cancel]));
    store.set_children(dlg_id, vec![ok, cancel]);
    store.set_root(dlg_id);
    synui_widgets::sync_overlays(&mut store);
    assert!(store.overlay_stack().len() == 1);
    let trap = store.focus_manager().focus_trap();
    assert!(trap.is_some());
    assert!(trap.unwrap().len() >= 2);
}

#[test]
fn dropdown_overlay_on_expand() {
    let mut store = WidgetStore::new();
    let mut dd = Dropdown::new(vec!["A".into(), "B".into()]);
    dd.expanded = true;
    let id = store.insert(dd);
    store.set_root(id);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, id, Size::new(200.0, 200.0));
    synui_widgets::sync_overlays(&mut store);
    assert!(store.overlay_for_owner(id).is_some());
}

#[test]
fn dropdown_dismisses_on_outside_click() {
    use synui_core::{Event, Point, PointerButton};
    let mut store = WidgetStore::new();
    let mut dd = Dropdown::new(vec!["A".into(), "B".into()]);
    dd.expanded = true;
    let id = store.insert(dd);
    store.set_root(id);
    store.set_viewport(Size::new(400.0, 300.0));
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, id, Size::new(400.0, 300.0));
    synui_widgets::sync_overlays(&mut store);
    synui_widgets::layout_overlays(&mut store, &mut text);
    let outside = Point::new(350.0, 250.0);
    store.dispatch_event(&Event::PointerDown {
        id: 0,
        position: outside,
        button: PointerButton::Primary,
        shift: false,
        ctrl: false,
    });
    synui_widgets::apply_overlay_dismiss(&mut store);
    let w = store.get(id).unwrap();
    let d = w.as_any().downcast_ref::<Dropdown>().unwrap();
    assert!(!d.expanded);
    assert!(store.overlay_for_owner(id).is_none());
}

#[test]
fn dialog_buttons_receive_clicks_in_overlay() {
    use std::cell::Cell;
    use std::rc::Rc;
    use synui_core::{Event, PointerButton};
    let mut store = WidgetStore::new();
    let fired = Rc::new(Cell::new(false));
    let fired_cb = fired.clone();
    let cancel = store.insert(Button::new("Cancel").on_press(move || fired_cb.set(true)));
    let ok = store.insert(Button::new("OK"));
    let mut dlg = Dialog::new("Confirm");
    dlg.open = true;
    let dlg_id = store.insert(dlg.with_children(vec![cancel, ok]));
    store.set_children(dlg_id, vec![cancel, ok]);
    store.set_root(dlg_id);
    store.set_viewport(Size::new(400.0, 300.0));
    synui_widgets::sync_overlays(&mut store);
    let mut text = TextRenderer::new();
    synui_widgets::layout_overlays(&mut store, &mut text);
    let cancel_bounds = store.get(cancel).unwrap().bounds();
    let pos = cancel_bounds.center();
    store.dispatch_event(&Event::PointerDown {
        id: 0,
        position: pos,
        button: PointerButton::Primary,
        shift: false,
        ctrl: false,
    });
    store.dispatch_event(&Event::PointerUp {
        id: 0,
        position: pos,
        button: PointerButton::Primary,
    });
    assert!(fired.get());
}

#[test]
fn focus_clears_on_blank_click() {
    use synui_core::{Event, Point, PointerButton};
    let mut store = WidgetStore::new();
    let tf = store.insert(synui_widgets::TextField::new("Test"));
    store.set_root(tf);
    store.set_viewport(Size::new(200.0, 200.0));
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, tf, Size::new(200.0, 200.0));
    store.focus_manager_mut().focus(tf);
    store.dispatch_event(&Event::PointerDown {
        id: 0,
        position: Point::new(190.0, 190.0),
        button: PointerButton::Primary,
        shift: false,
        ctrl: false,
    });
    assert!(store.focus_manager().focused().is_none());
}

#[test]
fn dialog_close_removes_overlay_without_deleting_dialog() {
    use synui_layout::LayoutEngine;
    let mut store = WidgetStore::new();
    let cancel = store.insert(Button::new("Cancel"));
    let ok = store.insert(Button::new("OK"));
    let column = store.insert(Flex::new().with_children(vec![]));
    let mut dlg = Dialog::new("Confirm");
    dlg.open = true;
    let dlg_id = store.insert(dlg.with_children(vec![cancel, ok]));
    store.set_children(dlg_id, vec![cancel, ok]);
    store.set_children(column, vec![dlg_id]);
    store.set_root(column);
    store.set_viewport(Size::new(400.0, 300.0));
    synui_widgets::sync_overlays(&mut store);
    assert!(store.overlay_for_owner(dlg_id).is_some());

    if let Some(w) = store.get_mut(dlg_id) {
        if let Some(d) = w.as_any_mut().downcast_mut::<Dialog>() {
            d.open = false;
        }
    }
    synui_widgets::sync_overlays(&mut store);

    assert!(store.get(dlg_id).is_some());
    assert!(store.overlay_for_owner(dlg_id).is_none());
    assert!(store.children_of(column).contains(&dlg_id));

    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, column, Size::new(400.0, 300.0));
}

#[test]
fn dialog_buttons_laid_out_when_nested_in_main_tree() {
    let mut store = WidgetStore::new();
    let cancel = store.insert(Button::new("Cancel"));
    let ok = store.insert(Button::new("OK"));
    let column = store.insert(Flex::new().with_children(vec![]));
    let mut dlg = Dialog::new("Confirm");
    dlg.open = true;
    let dlg_id = store.insert(dlg.with_children(vec![cancel, ok]));
    store.set_children(dlg_id, vec![cancel, ok]);
    store.set_children(column, vec![dlg_id]);
    store.set_root(column);
    store.set_viewport(Size::new(400.0, 300.0));
    synui_widgets::sync_overlays(&mut store);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, column, Size::new(400.0, 300.0));
    synui_widgets::layout_overlays(&mut store, &mut text);
    let cancel_bounds = store.get(cancel).unwrap().bounds();
    assert!(cancel_bounds.width() > 1.0 && cancel_bounds.height() > 1.0);
    assert!(cancel_bounds.y0 > 50.0);
}

#[test]
fn dropdown_mouse_select_sticks() {
    use synui_core::{Event, Point, PointerButton};
    let mut store = WidgetStore::new();
    let dd = store.insert(Dropdown::new(vec![
        "Option A".into(),
        "Option B".into(),
        "Option C".into(),
    ]));
    store.set_root(dd);
    store.set_viewport(Size::new(400.0, 300.0));
    store.focus_manager_mut().focus(dd);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, dd, Size::new(400.0, 300.0));

    let field = store.get(dd).unwrap().bounds();
    let open_pos = Point::new(field.center().x, field.center().y);
    store.dispatch_event(&Event::PointerDown {
        id: 0,
        position: open_pos,
        button: PointerButton::Primary,
        shift: false,
        ctrl: false,
    });
    synui_widgets::sync_overlays(&mut store);
    store.dispatch_event(&Event::PointerUp {
        id: 0,
        position: open_pos,
        button: PointerButton::Primary,
    });
    synui_widgets::sync_overlays(&mut store);
    synui_widgets::layout_overlays(&mut store, &mut text);

    let menu_host = store.overlay_for_owner(dd).expect("popup host");
    let menu = store.children_of(menu_host)[0];
    let menu_bounds = store.get(menu).unwrap().bounds();
    let select_b = Point::new(menu_bounds.center().x, menu_bounds.y0 + 1.5 * 36.0);
    store.dispatch_event(&Event::PointerDown {
        id: 0,
        position: select_b,
        button: PointerButton::Primary,
        shift: false,
        ctrl: false,
    });
    synui_widgets::sync_overlays(&mut store);
    store.dispatch_event(&Event::PointerUp {
        id: 0,
        position: select_b,
        button: PointerButton::Primary,
    });
    synui_widgets::sync_overlays(&mut store);

    let selected = store
        .get(dd)
        .unwrap()
        .as_any()
        .downcast_ref::<Dropdown>()
        .unwrap()
        .selected;
    assert_eq!(selected, 1);
}

#[test]
fn text_field_selection_keyboard() {
    use synui_core::{Event, Key, NamedKey};
    let mut store = WidgetStore::new();
    let mut tf = synui_widgets::TextField::new("ph");
    tf.text = "hello".into();
    tf.selection = 5..5;
    let id = store.insert(tf);
    store.set_root(id);
    store.focus_manager_mut().focus(id);
    store.dispatch_event(&Event::FocusGained);
    // Shift+Left extends selection
    store.dispatch_event(&Event::KeyDown {
        key: Key::Named(NamedKey::ArrowLeft),
        repeat: false,
        shift: true,
        ctrl: false,
        alt: false,
    });
    {
        let tf = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::TextField>()
            .unwrap();
        assert_eq!(tf.selection.end, 4);
        assert_ne!(tf.selection.start, tf.selection.end);
    }
    // Select all
    store.dispatch_event(&Event::KeyDown {
        key: Key::Character("a".into()),
        repeat: false,
        shift: false,
        ctrl: true,
        alt: false,
    });
    {
        let tf = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::TextField>()
            .unwrap();
        assert_eq!(tf.selection, 0..5);
    }
    // Delete selection
    store.dispatch_event(&Event::KeyDown {
        key: Key::Named(NamedKey::Backspace),
        repeat: false,
        shift: false,
        ctrl: false,
        alt: false,
    });
    {
        let tf = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::TextField>()
            .unwrap();
        assert!(tf.text.is_empty());
        assert_eq!(tf.selection, 0..0);
    }
}

#[test]
fn text_field_bidi_string_caret_bytes() {
    let mut tf = synui_widgets::TextField::new("");
    // Arabic letters are multi-byte; caret moves by char boundary.
    tf.text = "مرحبا".into();
    tf.selection = tf.text.len()..tf.text.len();
    let before = tf.selection.start;
    // simulate move left via event on a store
    let mut store = WidgetStore::new();
    let id = store.insert(tf);
    store.set_root(id);
    store.focus_manager_mut().focus(id);
    store.dispatch_event(&Event::FocusGained);
    store.dispatch_event(&Event::KeyDown {
        key: Key::Named(NamedKey::ArrowLeft),
        repeat: false,
        shift: false,
        ctrl: false,
        alt: false,
    });
    let tf = store
        .get(id)
        .unwrap()
        .as_any()
        .downcast_ref::<synui_widgets::TextField>()
        .unwrap();
    assert!(tf.selection.start < before);
    assert!(tf.text.is_char_boundary(tf.selection.start));
}

#[test]
fn text_field_a11y_exposes_selection() {
    let mut tf = synui_widgets::TextField::new("ph");
    tf.text = "hello".into();
    tf.selection = 1..4;
    let props = synui_core::Widget::a11y(&tf);
    assert_eq!(props.text_selection, Some(1..4));
    assert_eq!(props.value.as_deref(), Some("hello"));
}

#[test]
fn text_field_rtl_arrow_moves_visual() {
    use synui_core::{Event, EventCtx, Key, LayoutDirection, NamedKey, Widget};
    let mut tf = synui_widgets::TextField::new("");
    tf.text = "שלום".into(); // Hebrew
    tf.selection = 0..0;
    let path = [];
    let mut ctx = EventCtx::new(tf.id(), &path, LayoutDirection::Rtl);
    // In RTL, ArrowLeft moves toward end of logical string.
    tf.event(
        &mut ctx,
        &Event::KeyDown {
            key: Key::Named(NamedKey::ArrowLeft),
            repeat: false,
            shift: false,
            ctrl: false,
            alt: false,
        },
    );
    assert!(tf.selection.start > 0);
}

#[test]
fn text_area_enter_and_selection() {
    use synui_core::{Event, Key, NamedKey};
    let mut store = WidgetStore::new();
    let mut ta = synui_widgets::TextArea::new();
    ta.text = "hello".into();
    ta.selection = 5..5;
    let id = store.insert(ta);
    store.set_root(id);
    store.focus_manager_mut().focus(id);
    store.dispatch_event(&Event::FocusGained);
    store.dispatch_event(&Event::KeyDown {
        key: Key::Named(NamedKey::Enter),
        repeat: false,
        shift: false,
        ctrl: false,
        alt: false,
    });
    store.dispatch_event(&Event::KeyDown {
        key: Key::Character("w".into()),
        repeat: false,
        shift: false,
        ctrl: false,
        alt: false,
    });
    {
        let ta = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::TextArea>()
            .unwrap();
        assert_eq!(ta.text, "hello\nw");
        assert_eq!(ta.selection, 7..7);
    }
    store.dispatch_event(&Event::KeyDown {
        key: Key::Character("a".into()),
        repeat: false,
        shift: false,
        ctrl: true,
        alt: false,
    });
    {
        let ta = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::TextArea>()
            .unwrap();
        assert_eq!(ta.selection, 0..7);
    }
}

#[test]
fn text_area_a11y_exposes_selection() {
    let mut ta = synui_widgets::TextArea::new();
    ta.text = "ab\ncd".into();
    ta.selection = 1..4;
    let props = synui_core::Widget::a11y(&ta);
    assert_eq!(props.text_selection, Some(1..4));
    assert_eq!(props.value.as_deref(), Some("ab\ncd"));
}

#[test]
fn text_area_pointer_sets_caret() {
    use synui_core::{Event, Point, PointerButton, Widget};
    let mut store = WidgetStore::new();
    let mut ta = synui_widgets::TextArea::new();
    ta.text = "hello\nworld".into();
    ta.selection = 0..0;
    ta.set_bounds(synui_core::Rect::new(0.0, 0.0, 200.0, 100.0));
    let id = store.insert(ta);
    store.set_root(id);
    store.focus_manager_mut().focus(id);
    store.dispatch_event(&Event::FocusGained);
    // Click roughly on second line start (pad 8 + line_h 18).
    store.dispatch_event(&Event::PointerDown {
        id: 0,
        position: Point::new(10.0, 8.0 + 18.0 + 2.0),
        button: PointerButton::Primary,
        shift: false,
        ctrl: false,
    });
    let ta = store
        .get(id)
        .unwrap()
        .as_any()
        .downcast_ref::<synui_widgets::TextArea>()
        .unwrap();
    assert_eq!(ta.selection.start, 6); // after "hello\n"
}

#[test]
fn text_field_clipboard_and_undo() {
    use synui_core::{Event, Key};
    use synui_widgets::{clipboard_set, EditCore};
    clipboard_set("");
    let mut store = WidgetStore::new();
    let mut tf = synui_widgets::TextField::new("");
    tf.text = "hello".into();
    tf.selection = 0..5;
    let id = store.insert(tf);
    store.set_root(id);
    store.focus_manager_mut().focus(id);
    store.dispatch_event(&Event::FocusGained);
    store.dispatch_event(&Event::KeyDown {
        key: Key::Character("c".into()),
        repeat: false,
        shift: false,
        ctrl: true,
        alt: false,
    });
    assert_eq!(synui_widgets::clipboard_get(), "hello");
    store.dispatch_event(&Event::KeyDown {
        key: Key::Character("x".into()),
        repeat: false,
        shift: false,
        ctrl: true,
        alt: false,
    });
    {
        let tf = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::TextField>()
            .unwrap();
        assert!(tf.text.is_empty());
    }
    store.dispatch_event(&Event::KeyDown {
        key: Key::Character("z".into()),
        repeat: false,
        shift: false,
        ctrl: true,
        alt: false,
    });
    {
        let tf = store
            .get(id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::TextField>()
            .unwrap();
        assert_eq!(tf.text, "hello");
    }
    let _ = EditCore::new();
}

#[test]
fn rich_edit_toggle_bold() {
    use synui_core::{Event, Key, Widget};
    let mut store = WidgetStore::new();
    let mut re = synui_widgets::RichEdit::with_text("hello");
    re.selection = 0..5;
    re.set_bounds(synui_core::Rect::new(0.0, 0.0, 300.0, 160.0));
    let id = store.insert(re);
    store.set_root(id);
    store.focus_manager_mut().focus(id);
    store.dispatch_event(&Event::FocusGained);
    store.dispatch_event(&Event::KeyDown {
        key: Key::Character("b".into()),
        repeat: false,
        shift: false,
        ctrl: true,
        alt: false,
    });
    let re = store
        .get(id)
        .unwrap()
        .as_any()
        .downcast_ref::<synui_widgets::RichEdit>()
        .unwrap();
    assert!(re.doc.get_inline(0).unwrap().spans.iter().any(|s| s.bold));
}
