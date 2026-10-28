#[test]
fn layout_valid_empty_store() {
    let store = synui_core::WidgetStore::new();
    assert!(synui_testing::assert_layout_valid(&store));
}

#[test]
fn rgba_to_png_roundtrip() {
    let pixels = vec![255u8; 4 * 4 * 4];
    let png = synui_testing::rgba_to_png(4, 4, &pixels).expect("png");
    assert!(!png.is_empty());
}
