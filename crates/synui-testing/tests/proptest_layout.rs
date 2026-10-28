//! Property-based layout smoke tests (Phase 6 Sprint X).

use proptest::prelude::*;
use synui_core::{Size, WidgetStore};
use synui_layout::LayoutEngine;
use synui_render::TextRenderer;
use synui_testing::assert_layout_valid;
use synui_widgets::{Flex, Label};

proptest! {
    #[test]
    fn random_shallow_column_is_layout_valid(n in 1usize..12, seed in 0u64..10_000u64) {
        let mut store = WidgetStore::new();
        let mut kids = Vec::with_capacity(n);
        for i in 0..n {
            let text = format!("L{}-{}", seed, i);
            kids.push(store.insert(Label::new(text)));
        }
        let root = store.insert(Flex::new().column().with_children(kids.clone()));
        store.set_children(root, kids);
        store.set_root(root);
        store.set_viewport(Size::new(400.0, 600.0));
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(&mut store, &mut text, root, Size::new(400.0, 600.0));
        assert_layout_valid(&store);
    }
}
