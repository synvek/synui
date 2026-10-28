//! Integration checks for the documented Stable Component contract.
//!
//! The manifest and curated export index are read-only sources of truth here.
//! These tests deliberately consume only the ordinary facade modules.

use std::collections::HashSet;

use synui::prelude::*;

const MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/component-api-example-migration.toml"
));
const API_REFERENCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/api-reference-v0.2.md"
));

const STABLE_COMPONENTS: &[&str] = &[
    "text",
    "container-row-column",
    "button",
    "text-input",
    "text-area",
    "checkbox",
    "switch",
    "radio",
    "linear-progress",
    "circular-progress",
    "lottie",
    "morphing-shape",
    "card",
    "list-item",
    "number-input",
    "select",
    "list-virtual-list",
    "tree",
    "data-table",
    "split-pane",
    "tab-bar",
    "rich-text",
    "app-bar",
    "navigation-drawer",
    "bottom-navigation",
    "dialog",
    "bottom-sheet",
    "snack-bar",
    "icon",
    "label",
    "image",
    "menu-bar",
    "context-menu",
    "scroll",
    "style",
    "platform-window",
];

const CONTRACT_FIELDS: &[&str] = &[
    "canonical_path",
    "type_name",
    "prelude",
    "stability",
    "source_paths",
    "ownership",
    "thread_rule",
    "lifecycle",
    "error_contract",
    "identity_policy",
    "event_model",
    "a11y_metadata",
    "replacement",
    "fixture_commands",
    "validation_workloads",
    "boundary_note",
];

fn component_blocks() -> impl Iterator<Item = &'static str> {
    MANIFEST
        .split("[[components]]")
        .skip(1)
        .map(|block| block.split("[[examples]]").next().unwrap_or(block))
}

fn field_value<'a>(block: &'a str, field: &str) -> Option<&'a str> {
    block.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == field).then_some(value.trim())
    })
}

fn required_value<'a>(block: &'a str, field: &str) -> &'a str {
    let value =
        field_value(block, field).unwrap_or_else(|| panic!("component record is missing {field}"));
    assert!(
        value != "\"\"" && value != "[]",
        "component field {field} is empty"
    );
    value
}

#[test]
fn stable_component_records_are_complete_and_canonical() {
    let records = component_blocks().collect::<Vec<_>>();
    let ids = records
        .iter()
        .filter_map(|block| field_value(block, "id"))
        .collect::<Vec<_>>();
    let stable = records
        .iter()
        .filter(|block| field_value(block, "stability") == Some("\"stable\""))
        .collect::<Vec<_>>();

    assert_eq!(stable.len(), STABLE_COMPONENTS.len());
    assert_eq!(ids.len(), records.len());
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());

    let mut canonical_paths = HashSet::new();
    let mut canonical_pairs = HashSet::new();
    for block in stable {
        let id = required_value(block, "id").trim_matches('"');
        assert!(
            STABLE_COMPONENTS.contains(&id),
            "unexpected stable component {id}"
        );
        let canonical = required_value(block, "canonical_path");
        let type_name = required_value(block, "type_name");
        assert!(
            canonical_paths.insert(canonical),
            "duplicate canonical path {canonical}"
        );
        assert!(
            canonical_pairs.insert((canonical, type_name)),
            "duplicate canonical path/type pair"
        );
        for field in CONTRACT_FIELDS {
            required_value(block, field);
        }
        assert_eq!(
            field_value(block, "prelude"),
            Some("true"),
            "stable component {id} must be in Prelude"
        );
        assert!(
            API_REFERENCE.contains(canonical.trim_matches('"')),
            "API reference omits {canonical}"
        );
    }
}

#[test]
fn facade_export_index_has_unique_stable_paths_and_types() {
    let mut paths = HashSet::new();
    let mut pairs = HashSet::new();
    for export in synui::CURATED_EXPORT_INDEX {
        assert!(export.prelude);
        assert_eq!(export.stability, "stable");
        assert!(export.canonical_path.starts_with("synui::"));
        assert!(export
            .type_name
            .starts_with(|character: char| character.is_ascii_alphabetic()));
        assert!(!export.advanced_replacement.trim().is_empty());
        assert!(
            paths.insert(export.canonical_path),
            "duplicate export path {}",
            export.canonical_path
        );
        assert!(pairs.insert((export.canonical_path, export.type_name)));
    }
}

#[test]
fn every_stable_facade_component_constructs_through_documented_modules() {
    let app = App::new(AppConfig::default()).unwrap();
    let context = app.context();
    let state = context.state(String::from("value")).unwrap();
    let text_change = context.action(|_, _: String| {}).unwrap();
    let checked = context.state(false).unwrap();
    let bool_change = context.action(|_, _: bool| {}).unwrap();
    let number = context.state(1.0_f64).unwrap();
    let number_change = context.action(|_, _: f64| {}).unwrap();
    let choice = context.state(ChoiceId::new(0)).unwrap();
    let choice_change = context.action(|_, _: ChoiceId| {}).unwrap();

    let root = Node::container([
        Node::text("text"),
        Button::new("button")
            .on_press(context.action(|_, _: ()| {}).unwrap())
            .node(),
        TextInput::new("placeholder")
            .controlled(state, text_change)
            .node(),
        TextArea::new("notes")
            .controlled(
                context.state(String::from("area")).unwrap(),
                context.action(|_, _: String| {}).unwrap(),
            )
            .node(),
        Checkbox::new(false).controlled(checked, bool_change).node(),
        Switch::new(false).node(),
        Radio::new("A", true).node(),
        LinearProgress::new(0.5).node(),
        CircularProgress::new(0.25).node(),
        Lottie::from_asset("pulse").playing(false).node(),
        MorphingShape::new().playing(false).node(),
        Card::new([Node::text("card body")]).node(),
        ListItem::new("item").node(),
        NumberInput::new(1.0)
            .range(0.0, 10.0)
            .controlled(number, number_change)
            .node(),
        Select::new(["one", "two"], 0)
            .controlled(choice, choice_change)
            .node(),
        DataTable::new()
            .logical_dimensions(100_000, 8)
            .mounted_position(500, 0)
            .node(),
    ]);
    let spec = root.describe(&mut BuildCx::new());
    assert_eq!(spec.children().len(), 16);

    let mut styles = StyleCache::default();
    assert!(
        styles
            .apply(StyleInput::Rust(Style::new().gap(2.0)))
            .applied
    );
    assert!(
        styles
            .apply(StyleInput::Macro(style!(padding: 8; gap: 4)))
            .applied
    );

    let _window = Window::new("contract");
    let _run_loop = RunLoop::new();
}

#[test]
fn stable_elements_expose_identity_events_and_lifecycle_error_types() {
    fn assert_element_contract<E: Element>() {}
    assert_element_contract::<Node>();
    assert_element_contract::<Button>();
    assert_element_contract::<TextInput>();
    assert_element_contract::<TextArea>();
    assert_element_contract::<Checkbox>();
    assert_element_contract::<Switch>();
    assert_element_contract::<Radio>();
    assert_element_contract::<LinearProgress>();
    assert_element_contract::<CircularProgress>();
    assert_element_contract::<Lottie>();
    assert_element_contract::<MorphingShape>();
    assert_element_contract::<Card>();
    assert_element_contract::<ListItem>();
    assert_element_contract::<NumberInput>();
    assert_element_contract::<Select>();
    assert_element_contract::<DataTable>();
    assert_element_contract::<VirtualList>();
    assert_element_contract::<VirtualTree>();
    assert_element_contract::<SplitPane>();
    assert_element_contract::<TabBar>();
    assert_element_contract::<RichDocument>();
    assert_element_contract::<AppBar>();
    assert_element_contract::<NavigationDrawer>();
    assert_element_contract::<BottomNavigation>();
    assert_element_contract::<Dialog>();
    assert_element_contract::<BottomSheet>();
    assert_element_contract::<SnackBar>();
    assert_element_contract::<Icon>();
    assert_element_contract::<Label>();
    assert_element_contract::<Image>();
    assert_element_contract::<Scroll>();
    assert_element_contract::<MenuBar>();
    assert_element_contract::<ContextMenu>();

    let keyed = Node::text("item").key(BusinessKey::new("item-id"));
    let spec = keyed.describe(&mut BuildCx::new());
    assert_eq!(spec.key_value().map(BusinessKey::as_str), Some("item-id"));
    assert!(ElementError::IdentityUnavailable
        .to_string()
        .contains("identity"));
    assert!(InputConfigError::InvalidChoice {
        choice: ChoiceId::new(4)
    }
    .to_string()
    .contains("choice"));
    assert!(matches!(
        VirtualModelError::InvalidRange,
        VirtualModelError::InvalidRange
    ));
    assert!(matches!(
        PlatformError::App(AppError::ContextDestroyed),
        PlatformError::App(_)
    ));
}

#[test]
fn non_stable_component_records_have_explicit_replacement_boundaries() {
    for block in component_blocks() {
        if field_value(block, "stability") == Some("\"stable\"") {
            continue;
        }
        let stability = required_value(block, "stability");
        assert!(
            [
                "\"external_element\"",
                "\"advanced_integration\"",
                "\"deferred\""
            ]
            .contains(&stability),
            "unknown component stability {stability}"
        );
        assert_eq!(field_value(block, "prelude"), Some("false"));
        for field in [
            "canonical_path",
            "type_name",
            "ownership",
            "thread_rule",
            "lifecycle",
            "error_contract",
            "identity_policy",
            "event_model",
            "a11y_metadata",
            "replacement",
            "boundary_note",
        ] {
            required_value(block, field);
        }
        let replacement = required_value(block, "replacement");
        let boundary = required_value(block, "boundary_note");
        assert!(
            replacement.contains("Element")
                || replacement.contains("external")
                || replacement.contains("Engine")
                || replacement.contains("BlockDoc")
        );
        assert!(
            boundary.contains("external_element")
                || boundary.contains("advanced_integration")
                || boundary.contains("deferred")
                || boundary.contains("boundary")
        );
    }
}
