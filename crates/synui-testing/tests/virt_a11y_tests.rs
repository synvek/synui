//! Phase 24: virt a11y metadata + mount-bounded children.

use synui_core::{A11yRole, Size, Widget, WidgetStore};
use synui_layout::LayoutEngine;
use synui_render::TextRenderer;
use synui_testing::HeadlessRenderer;
use synui_theme::Theme;
use synui_widgets::{sync_post_layout, sync_pre_paint, ColumnDef, DataTable, Label, ListView};

#[test]
fn list_a11y_exposes_row_count_without_o_n_children() {
    let mut store = WidgetStore::new();
    let mut list = ListView::new(10_000, 28.0);
    list.set_item_builder(|i| Box::new(Label::new(format!("item {i}"))));
    list.set_bounds(synui_core::Rect::new(0.0, 0.0, 400.0, 280.0));
    let id = store.insert(list);
    store.set_root(id);
    store.set_viewport(Size::new(400.0, 280.0));

    let mut layout = LayoutEngine::new();
    let mut text = TextRenderer::new();
    layout.compute(&mut store, &mut text, id, Size::new(400.0, 280.0));
    // Phase 33: same host sync as winit/headless (no ad-hoc apply_scroll + sync_virtual).
    sync_post_layout(&mut store, &mut text);
    sync_pre_paint(&mut store, &mut text);

    let mounted = store.children_of(id).len();
    assert!(mounted <= 40, "mounted={mounted}");

    let props = store.get(id).unwrap().a11y();
    assert_eq!(props.role, A11yRole::List);
    assert_eq!(props.row_count, Some(10_000));
    // Children in the store (a11y walk source) stay O(mount), not O(rows).
    assert!(mounted < 200);
}

/// Phase 33: HeadlessRenderer::render_store runs virt sync without manual calls.
#[test]
fn headless_render_store_mounts_list_virt() {
    let mut store = WidgetStore::new();
    let mut list = ListView::new(5_000, 28.0);
    list.set_item_builder(|i| Box::new(Label::new(format!("item {i}"))));
    let id = store.insert(list);
    store.set_root(id);
    store.mark_layout_dirty();
    store.mark_paint_dirty();

    let mut headless = HeadlessRenderer::new(400, 280);
    let _ = headless.render_store(&mut store, &Theme::default());

    let mounted = store.children_of(id).len();
    assert!(
        mounted > 0 && mounted <= 40,
        "headless host parity should mount virt rows, mounted={mounted}"
    );
}

#[test]
fn data_table_exposes_grid_role() {
    let mut store = WidgetStore::new();
    let cols = vec![
        ColumnDef::new("a", "A", 80.0),
        ColumnDef::new("b", "B", 80.0),
    ];
    let table = DataTable::new(cols, 500);
    let id = store.insert(table);
    store.set_root(id);

    let props = store.get(id).unwrap().a11y();
    assert_eq!(props.role, A11yRole::Grid);
    assert_eq!(props.row_count, Some(500));
    assert_eq!(props.col_count, Some(2));
}
