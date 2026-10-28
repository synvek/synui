use criterion::{black_box, criterion_group, criterion_main, Criterion};
use synui_core::{Size, WidgetStore};
use synui_layout::LayoutEngine;
use synui_render::TextRenderer;
use synui_theme::Theme;
use synui_widgets::{Flex, Label};
use vello::Scene;

fn build_label_tree(store: &mut WidgetStore, count: usize) -> synui_core::WidgetId {
    let labels: Vec<_> = (0..count)
        .map(|i| store.insert(Label::new(format!("Item {i}"))))
        .collect();
    let flex = store.insert(Flex::new().column().with_children(labels.clone()));
    store.set_children(flex, labels);
    flex
}

fn bench_layout_10k(c: &mut Criterion) {
    c.bench_function("layout_1000_labels", |b| {
        b.iter(|| {
            let mut store = WidgetStore::new();
            let root = build_label_tree(&mut store, 1_000);
            store.set_root(root);
            let mut layout = LayoutEngine::new();
            let mut text = TextRenderer::new();
            layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
            black_box(root);
        });
    });
    c.bench_function("layout_10k_labels", |b| {
        b.iter(|| {
            let mut store = WidgetStore::new();
            let root = build_label_tree(&mut store, 10_000);
            store.set_root(root);
            let mut layout = LayoutEngine::new();
            let mut text = TextRenderer::new();
            layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
            black_box(root);
        });
    });
    // Pure layout after tree already exists (excludes store.insert cost).
    c.bench_function("layout_10k_labels_pure", |b| {
        let mut store = WidgetStore::new();
        let root = build_label_tree(&mut store, 10_000);
        store.set_root(root);
        b.iter(|| {
            let mut layout = LayoutEngine::new();
            let mut text = TextRenderer::new();
            layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
            black_box(root);
        });
    });
    // Structure-dirty rebuild on the same engine (Phase 9 node pool path).
    c.bench_function("layout_10k_pooled_rebuild", |b| {
        let mut store = WidgetStore::new();
        let root = build_label_tree(&mut store, 10_000);
        store.set_root(root);
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
        b.iter(|| {
            layout.mark_structure_changed();
            layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
            black_box(root);
        });
    });
    // Warm incremental path: structure fixed, remeasure dirty leaves only.
    c.bench_function("layout_10k_incremental_relayout", |b| {
        let mut store = WidgetStore::new();
        let root = build_label_tree(&mut store, 10_000);
        store.set_root(root);
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
        b.iter(|| {
            layout.mark_dirty(root, &store);
            layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
            black_box(root);
        });
    });
}

fn bench_paint_full_tree(c: &mut Criterion) {
    c.bench_function("paint_1000_labels", |b| {
        let mut store = WidgetStore::new();
        let root = build_label_tree(&mut store, 1_000);
        store.set_root(root);
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
        let theme = Theme::default();
        let theme_ref = theme.to_theme_ref();
        b.iter(|| {
            let mut scene = Scene::new();
            let mut paint_ctx = synui_core::PaintCtx {
                bounds: synui_core::Rect::new(0.0, 0.0, 800.0, 600.0),
                theme: &theme_ref,
                text: Some(&mut text),
                icons: None,
                images: None,
                lotties: None,
                text_color: None,
                font_size: None,
            };
            store.paint_all(&mut scene, &mut paint_ctx);
            black_box(scene);
        });
    });
    // Phase 29/31: paint-only damage on a retained scene — one dirty leaf among 1k labels.
    c.bench_function("paint_1000_labels_damage_one", |b| {
        let mut store = WidgetStore::new();
        let root = build_label_tree(&mut store, 1_000);
        store.set_root(root);
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
        let leaf = store.children_of(root)[0];
        let theme = Theme::default();
        let theme_ref = theme.to_theme_ref();
        let mut scene = Scene::new();
        {
            let mut paint_ctx = synui_core::PaintCtx {
                bounds: synui_core::Rect::new(0.0, 0.0, 800.0, 600.0),
                theme: &theme_ref,
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
        }
        b.iter(|| {
            store.clear_paint_dirty();
            store.mark_paint_dirty_widget(leaf);
            let mut paint_ctx = synui_core::PaintCtx {
                bounds: synui_core::Rect::new(0.0, 0.0, 800.0, 600.0),
                theme: &theme_ref,
                text: Some(&mut text),
                icons: None,
                images: None,
                lotties: None,
                text_color: None,
                font_size: None,
            };
            let stats = store.paint_frame(&mut scene, &mut paint_ctx);
            assert!(stats.skipped_paint_nodes >= 1);
            black_box(stats.skipped_paint_nodes);
        });
    });
    // Phase 32: N Damaged frames with budget compact vs unbounded append (no Full).
    c.bench_function("paint_damage_bounded_128", |b| {
        let mut store = WidgetStore::new();
        let root = build_label_tree(&mut store, 1_000);
        store.set_root(root);
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
        let leaf = store.children_of(root)[0];
        let theme = Theme::default();
        let theme_ref = theme.to_theme_ref();
        synui_core::set_damaged_frame_budget(Some(32));
        b.iter(|| {
            let mut scene = Scene::new();
            {
                let mut paint_ctx = synui_core::PaintCtx {
                    bounds: synui_core::Rect::new(0.0, 0.0, 800.0, 600.0),
                    theme: &theme_ref,
                    text: Some(&mut text),
                    icons: None,
                    images: None,
                    lotties: None,
                    text_color: None,
                    font_size: None,
                };
                store.mark_paint_dirty();
                store.paint_all(&mut scene, &mut paint_ctx);
                store.note_paint_presented(synui_core::PaintMode::Full);
                store.clear_paint_dirty();
            }
            for _ in 0..128 {
                store.mark_paint_dirty_widget(leaf);
                store.prepare_damage_compact();
                let mode = store.paint_mode();
                let mut paint_ctx = synui_core::PaintCtx {
                    bounds: synui_core::Rect::new(0.0, 0.0, 800.0, 600.0),
                    theme: &theme_ref,
                    text: Some(&mut text),
                    icons: None,
                    images: None,
                    lotties: None,
                    text_color: None,
                    font_size: None,
                };
                match mode {
                    synui_core::PaintMode::Full => {
                        scene.reset();
                        store.paint_all(&mut scene, &mut paint_ctx);
                    }
                    synui_core::PaintMode::Damaged => {
                        let stats = store.paint_frame(&mut scene, &mut paint_ctx);
                        assert!(stats.skipped_paint_nodes >= 1);
                    }
                }
                store.note_paint_presented(mode);
                store.clear_paint_dirty();
            }
            black_box(store.damaged_frames_since_full());
        });
        synui_core::set_damaged_frame_budget(None);
    });
    c.bench_function("paint_damage_unbounded_128", |b| {
        let mut store = WidgetStore::new();
        let root = build_label_tree(&mut store, 1_000);
        store.set_root(root);
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
        let leaf = store.children_of(root)[0];
        let theme = Theme::default();
        let theme_ref = theme.to_theme_ref();
        b.iter(|| {
            let mut scene = Scene::new();
            {
                let mut paint_ctx = synui_core::PaintCtx {
                    bounds: synui_core::Rect::new(0.0, 0.0, 800.0, 600.0),
                    theme: &theme_ref,
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
            }
            // Never Full-compact: scene encoding grows with each Damaged append.
            for _ in 0..128 {
                store.clear_paint_dirty();
                store.mark_paint_dirty_widget(leaf);
                let mut paint_ctx = synui_core::PaintCtx {
                    bounds: synui_core::Rect::new(0.0, 0.0, 800.0, 600.0),
                    theme: &theme_ref,
                    text: Some(&mut text),
                    icons: None,
                    images: None,
                    lotties: None,
                    text_color: None,
                    font_size: None,
                };
                let stats = store.paint_frame(&mut scene, &mut paint_ctx);
                assert!(stats.skipped_paint_nodes >= 1);
                black_box(stats.painted_nodes);
            }
            black_box(scene);
        });
    });
}

fn bench_layout_partial(c: &mut Criterion) {
    // Phase 30: structure-stable mark_dirty on one leaf then compute.
    c.bench_function("layout_1000_partial_one_dirty", |b| {
        let mut store = WidgetStore::new();
        let root = build_label_tree(&mut store, 1_000);
        store.set_root(root);
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
        let leaf = store.children_of(root)[0];
        b.iter(|| {
            store.clear_layout_dirty();
            store.mark_layout_dirty_widget(leaf);
            let dirty = store.take_dirty_layout_widgets();
            for &id in &dirty {
                layout.mark_dirty(id, &store);
            }
            store.note_partial_layout(dirty.len() as u32);
            layout.compute(&mut store, &mut text, root, Size::new(800.0, 600.0));
            black_box(store.last_layout_stats().partial_layout_passes);
        });
    });
}

criterion_group!(
    benches,
    bench_layout_10k,
    bench_paint_full_tree,
    bench_layout_partial
);
criterion_main!(benches);
