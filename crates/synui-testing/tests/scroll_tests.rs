//! ScrollView interaction regression.

use synui_core::{Event, Point, Size, Style, WidgetStore};
use synui_layout::LayoutEngine;
use synui_render::TextRenderer;
use synui_widgets::{apply_scroll_views, Flex, Label, ScrollView, StyledBox};

#[test]
fn scroll_view_offsets_children_on_wheel() {
    let mut store = WidgetStore::new();
    let mut kids = Vec::new();
    for i in 0..8 {
        kids.push(store.insert(Label::new(format!("row {i}"))));
    }
    let sv = store.insert(ScrollView::new().with_children(kids.clone()));
    store.set_children(sv, kids.clone());
    store.set_root(sv);
    store.set_viewport(Size::new(200.0, 120.0));

    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, sv, Size::new(200.0, 120.0));
    apply_scroll_views(&mut store);

    let y0_before = store.get(kids[0]).unwrap().bounds().y0;
    let handled = store.dispatch_event(&Event::Scroll {
        position: Point::new(50.0, 50.0),
        delta_x: 0.0,
        delta_y: -40.0,
    });
    assert!(handled.handled);
    apply_scroll_views(&mut store);
    let y0_after = store.get(kids[0]).unwrap().bounds().y0;
    assert!(
        y0_after < y0_before,
        "expected child to move up after scroll down, before={y0_before} after={y0_after}"
    );
}

/// Phase 33: wheel scroll marks ScrollView Damaged (skip clean siblings), not Full.
#[test]
fn scroll_paint_damage_skips_sibling() {
    use synui_core::{PaintCtx, PaintMode, Rect};
    use synui_widgets::mark_scroll_paint_dirty;
    use vello::Scene;

    let mut store = WidgetStore::new();
    let mut kids = Vec::new();
    for i in 0..6 {
        kids.push(store.insert(Label::new(format!("row {i}"))));
    }
    let sv = store.insert(ScrollView::new().with_children(kids.clone()));
    store.set_children(sv, kids.clone());

    let sibling = store.insert(Label::new("outside scroll"));
    let root = store.insert(Flex::new().column().with_children(vec![sibling, sv]));
    store.set_children(root, vec![sibling, sv]);
    store.set_root(root);
    store.set_viewport(Size::new(240.0, 200.0));

    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, root, Size::new(240.0, 200.0));
    apply_scroll_views(&mut store);

    let theme = synui_core::ThemeRef::default();
    let mut scene = Scene::new();
    let mut paint_ctx = PaintCtx {
        bounds: Rect::new(0.0, 0.0, 240.0, 200.0),
        theme: &theme,
        text: Some(&mut text),
        icons: None,
        images: None,
        lotties: None,
        text_color: None,
        font_size: None,
    };
    store.mark_paint_dirty();
    store.paint_all(&mut scene, &mut paint_ctx);
    store.clear_paint_dirty();

    let handled = store.dispatch_event(&Event::Scroll {
        position: Point::new(40.0, 80.0),
        delta_x: 0.0,
        delta_y: -30.0,
    });
    assert!(handled.handled);
    apply_scroll_views(&mut store);
    mark_scroll_paint_dirty(&mut store, Point::new(40.0, 80.0));
    assert_eq!(store.paint_mode(), PaintMode::Damaged);
    let stats = store.paint_frame(&mut scene, &mut paint_ctx);
    assert!(
        stats.skipped_paint_nodes >= 1,
        "scroll Damaged should skip clean sibling, got {stats:?}"
    );
}

/// Flex parent + ScrollView must keep content taller than the viewport (not
/// crush it with flex-shrink), and the scroll host must shrink to remaining height.
#[test]
fn scroll_viewport_preserves_tall_content() {
    let mut store = WidgetStore::new();
    let mut rows = Vec::new();
    for i in 0..20 {
        rows.push(store.insert(Label::new(format!("row {i}"))));
    }
    let body = store.insert(
        StyledBox::new(Style::from_css("padding:8;gap:4;width:fill;flex-shrink:0").expect("css"))
            .with_children(rows.clone()),
    );
    store.set_children(body, rows.clone());

    let sv = store.insert(ScrollView::new().with_children(vec![body]));
    store.set_children(sv, vec![body]);

    let host = store.insert(
        StyledBox::new(
            Style::from_css("height:fill;width:fill;flex-grow:1;min-height:0").expect("css"),
        )
        .with_children(vec![sv]),
    );
    store.set_children(host, vec![sv]);

    let chrome = store.insert(Label::new("chrome"));
    let root = store.insert(
        Flex::new()
            .column()
            .with_gap(0.0)
            .with_padding([0.0; 4])
            .with_children(vec![chrome, host]),
    );
    store.set_children(root, vec![chrome, host]);
    store.set_root(root);

    let viewport = Size::new(320.0, 240.0);
    store.set_viewport(viewport);
    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, root, viewport);
    apply_scroll_views(&mut store);

    let sv_bounds = store.get(sv).unwrap().bounds();
    let body_h = store.get(body).unwrap().bounds().height();
    let content_h = store
        .get(sv)
        .unwrap()
        .as_any()
        .downcast_ref::<ScrollView>()
        .unwrap()
        .content_height;

    assert!(
        sv_bounds.height() < viewport.height - 10.0,
        "scroll viewport should fit under chrome, h={}",
        sv_bounds.height()
    );
    assert!(
        body_h > sv_bounds.height() + 20.0,
        "content must stay taller than viewport (not flex-shrunk): body={body_h} vp={}",
        sv_bounds.height()
    );
    assert!(
        content_h > sv_bounds.height() + 20.0,
        "content_height must exceed viewport for scrolling: content={content_h} vp={}",
        sv_bounds.height()
    );
}
