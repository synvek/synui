//! Prelude-only consumer and compile-fail boundary evidence.
//!
//! The `compile_fail` examples in `synui/src/lib.rs` are rustdoc fixtures. This
//! integration target checks that those fixtures, the public denylist, and the
//! documented API reference stay synchronized.

use std::collections::HashSet;

use synui::prelude::*;
use synui::{app, model, platform, style, view, widgets};

const FACADE_SOURCE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
const API_REFERENCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/api-reference-v0.2.md"
));

const REQUIRED_DENYLIST: &[&str] = &[
    "WidgetStore",
    "ViewEngine",
    "Engine layout/render types",
    "numeric action identifiers",
    "global key/memo/section scheduling",
    "plugin registry/loading symbols",
    "JSON View creation symbols",
];

#[test]
fn ordinary_consumer_can_use_prelude_and_documented_stable_modules() {
    let app = app::App::new(app::AppConfig::default()).unwrap();
    let context = app.context();
    let state = context.state(0_i32).unwrap();
    let action = context
        .action({
            let state = state.clone();
            move |_, (): ()| state.set(state.get().unwrap() + 1).unwrap()
        })
        .unwrap();
    let button = widgets::Button::new("increment").on_press(action);
    let root = view::Node::container([button.node(), view::Node::text("ready")]);
    assert_eq!(root.describe(&mut view::BuildCx::new()).children().len(), 2);

    let _model_error: Option<model::VirtualModelError> = None;
    let _window = platform::Window::new("stable facade");
    let mut cache = style::StyleCache::default();
    assert!(cache.apply(style::StyleInput::text("padding:8")).applied);
}

#[test]
fn denylist_is_complete_and_absent_from_curated_exports() {
    let symbols = synui::FACADE_DENYLIST
        .iter()
        .map(|entry| entry.symbol)
        .collect::<HashSet<_>>();
    for required in REQUIRED_DENYLIST {
        assert!(
            symbols.contains(required),
            "missing denylist entry: {required}"
        );
    }
    assert_eq!(symbols.len(), synui::FACADE_DENYLIST.len());

    for export in synui::CURATED_EXPORT_INDEX {
        for forbidden in REQUIRED_DENYLIST {
            assert!(
                !export.canonical_path.contains(forbidden),
                "forbidden symbol in {}",
                export.canonical_path
            );
            assert!(
                !export.type_name.contains(forbidden),
                "forbidden type in {}",
                export.type_name
            );
        }
    }
}

#[test]
fn denylist_entries_have_non_ordinary_boundary_and_replacement_evidence() {
    for entry in synui::FACADE_DENYLIST {
        assert!(!entry.boundary.trim().is_empty());
        assert!(!entry.replacement.trim().is_empty());
        assert!(!entry.reason.trim().is_empty());
        let documented_anchor = match entry.symbol {
            "global key/memo/section scheduling" => "global scheduling registries",
            "plugin registry/loading symbols" => "Experimental Plugin ABI",
            "JSON View creation symbols" => "JSON View Creation",
            symbol => symbol,
        };
        assert!(
            API_REFERENCE.contains(documented_anchor),
            "API reference omits denylist symbol {}",
            entry.symbol
        );
        assert!(
            API_REFERENCE.contains("Stable replacement or explicit deferral"),
            "API reference omits the replacement column for {}",
            entry.symbol
        );
    }
}

#[test]
fn rustdoc_compile_fail_fixtures_cover_each_forbidden_ordinary_import() {
    assert!(FACADE_SOURCE.contains("```compile_fail"));
    for forbidden in [
        "synui::key_scope",
        "synui::section",
        "synui::section_fill",
        "synui::memo::schedule",
        "synui::ActionId",
        "synui::plugins::Plugin",
        "synui::view_from_json",
        "let _: WidgetStore",
    ] {
        assert!(
            FACADE_SOURCE.contains(forbidden),
            "missing compile-fail fixture for {forbidden}"
        );
    }
}
