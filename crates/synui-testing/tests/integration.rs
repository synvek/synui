use synui_core::WidgetStore;
use synui_layout::LayoutEngine;
use synui_render::TextRenderer;
use synui_testing::{assert_layout_valid, snapshot_png, snapshot_store};
use synui_theme::Theme;
use synui_widgets::{Flex, Label};

#[test]
fn layout_produces_valid_bounds() {
    let mut store = WidgetStore::new();
    let label = store.insert(Label::new("test"));
    let root = store.insert(Flex::new().column().with_children(vec![label]));
    store.set_children(root, vec![label]);
    store.set_root(root);

    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(
        &mut store,
        &mut text,
        root,
        synui_core::Size::new(400.0, 300.0),
    );
    assert!(assert_layout_valid(&store));
}

#[test]
fn headless_png_roundtrip() {
    let mut store = WidgetStore::new();
    let label = store.insert(Label::new("Hi"));
    store.set_root(label);
    let png = snapshot_png(&mut store, &Theme::default(), 64, 32).expect("png");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(png.len() > 100);
}

#[test]
fn headless_scene_builds() {
    let mut store = WidgetStore::new();
    let label = store.insert(Label::new("snapshot"));
    store.set_root(label);
    let scene = snapshot_store(&mut store, &Theme::default(), 64, 64);
    let _ = scene;
}
