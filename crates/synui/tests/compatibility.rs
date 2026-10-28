//! Public facade export and denylist contract checks for Wave 0.

use std::collections::HashSet;

#[test]
fn canonical_exports_are_unique_and_prelude_scoped() {
    let paths = synui::CURATED_EXPORT_INDEX
        .iter()
        .map(|entry| entry.canonical_path)
        .collect::<HashSet<_>>();
    assert_eq!(paths.len(), synui::CURATED_EXPORT_INDEX.len());
    assert!(synui::CURATED_EXPORT_INDEX.iter().all(|entry| {
        entry.prelude && entry.stability == "stable" && !entry.advanced_replacement.is_empty()
    }));
}

#[test]
fn denylist_keeps_engine_and_experimental_symbols_out_of_ordinary_facade() {
    let symbols = synui::FACADE_DENYLIST
        .iter()
        .map(|entry| entry.symbol)
        .collect::<HashSet<_>>();
    for required in [
        "WidgetStore",
        "ViewEngine",
        "Engine layout/render types",
        "numeric action identifiers",
        "global key/memo/section scheduling",
        "plugin registry/loading symbols",
        "JSON View creation symbols",
    ] {
        assert!(
            symbols.contains(required),
            "missing denylist entry: {required}"
        );
    }
    assert!(synui::FACADE_DENYLIST
        .iter()
        .all(|entry| !entry.boundary.is_empty() && !entry.replacement.is_empty()));
}
