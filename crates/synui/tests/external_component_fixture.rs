//! Consumer fixture: third-party Elements compose through the stable facade only.
//!
//! The fixture intentionally exercises the open ElementSpec lifecycle shape
//! using only the documented facade and its application-level contracts.

use synui::prelude::*;

struct KeyedList {
    items: Vec<(BusinessKey, String)>,
}

impl Element for KeyedList {
    fn describe(&self, _cx: &mut BuildCx) -> ElementSpec {
        let children = self
            .items
            .iter()
            .map(|(key, label)| Node::text(label.clone()).key(key.clone()));
        ElementSpec::custom("list", children)
            .accessibility(Accessibility::new().role("list").name("fixture list"))
    }
}

struct RichTextElement {
    markdown: String,
}

impl Element for RichTextElement {
    fn describe(&self, _cx: &mut BuildCx) -> ElementSpec {
        ElementSpec::custom("rich-text", [Node::text(self.markdown.clone())])
            .property("format", "markdown")
            .accessibility(Accessibility::new().role("generic").name("document"))
    }
}

#[derive(Default)]
struct FixtureTableModel {
    requests: Vec<CellRequest>,
    selections: Vec<SelectionChange>,
    sorts: Vec<Vec<SortDescriptor>>,
    edits: Vec<(CellCoordinate, CellValue)>,
}

impl Model for FixtureTableModel {}

impl VirtualDataTableModel for FixtureTableModel {
    fn dimensions(&self) -> TableDimensions {
        TableDimensions::new(100_000, 8)
    }

    fn request_cells(&mut self, request: CellRequest) -> Result<(), VirtualModelError> {
        self.requests.push(request);
        Ok(())
    }

    fn apply_cell_update(
        &mut self,
        coordinate: CellCoordinate,
        value: CellValue,
    ) -> Result<(), VirtualModelError> {
        self.edits.push((coordinate, value));
        Ok(())
    }

    fn sort_changed(&mut self, sort: &[SortDescriptor]) -> Result<(), VirtualModelError> {
        self.sorts.push(sort.to_vec());
        Ok(())
    }

    fn selection_changed(&mut self, selection: &SelectionChange) -> Result<(), VirtualModelError> {
        self.selections.push(selection.clone());
        Ok(())
    }
}

#[test]
fn external_component_uses_the_open_element_contract() {
    let external: Box<dyn Element> = Box::new(KeyedList {
        items: vec![
            (BusinessKey::new("first"), "First".into()),
            (BusinessKey::new("second"), "Second".into()),
        ],
    });
    let root = Node::container([Node::text("Built-in"), Node::from(external)]);
    let spec = root.describe(&mut BuildCx::new());

    assert_eq!(spec.children().len(), 2);
    let list = spec.children()[1].describe(&mut BuildCx::new());
    assert_eq!(list.kind(), &ElementKind::Custom("list".into()));
    assert_eq!(list.accessibility_metadata().role_name(), Some("list"));
    assert_eq!(list.children().len(), 2);
    assert_eq!(
        list.children()[0]
            .describe(&mut BuildCx::new())
            .key_value()
            .map(BusinessKey::as_str),
        Some("first")
    );
    assert_eq!(
        list.children()[1]
            .describe(&mut BuildCx::new())
            .key_value()
            .map(BusinessKey::as_str),
        Some("second")
    );
}

#[test]
fn table_selection_and_bounded_data_supply_use_app_owned_model() {
    let app = App::new(AppConfig::default()).unwrap();
    let context = app.context();
    let model = context.model(FixtureTableModel::default()).unwrap();
    let mut adapter = VirtualDataTableAdapter::new(model.clone()).with_prefetch(PrefetchBand {
        rows_before: 1,
        rows_after: 2,
        columns_before: 1,
        columns_after: 1,
    });
    let viewport = CellRange::new(500, 502, 2, 4).unwrap();
    let request = adapter
        .request_visible(&context, viewport)
        .unwrap()
        .unwrap();
    assert_eq!(request.range, CellRange::new(499, 504, 1, 5).unwrap());
    assert_eq!(request.range.row_end - request.range.row_start, 5);
    assert_eq!(request.range.column_end - request.range.column_start, 4);

    adapter
        .notify_selection(&context, &SelectionChange::one(viewport))
        .unwrap();
    adapter
        .notify_sort(&context, &[SortDescriptor::ascending(1)])
        .unwrap();
    adapter
        .edit_cell(
            &context,
            CellCoordinate {
                row: 500,
                column: 2,
            },
            CellValue::Text("edited".into()),
        )
        .unwrap();

    let observed = model
        .with(&context, |table| {
            (
                table.requests.len(),
                table.selections.len(),
                table.sorts.len(),
                table.edits.len(),
            )
        })
        .unwrap();
    assert_eq!(observed, (1, 1, 1, 1));

    let table_spec = DataTable::new()
        .logical_dimensions(100_000, 8)
        .mounted_position(500, 2)
        .node()
        .describe(&mut BuildCx::new());
    let a11y = table_spec.accessibility_metadata();
    assert_eq!(a11y.role_name(), Some("grid"));
    assert_eq!(a11y.logical_row_count(), Some(100_000));
    assert_eq!(a11y.logical_column_count(), Some(8));
    assert_eq!(a11y.mounted_row_index(), Some(500));
    assert_eq!(a11y.mounted_column_index(), Some(2));
}

#[test]
fn rich_text_and_style_use_facade_descriptors_without_engine_types() {
    let external: Box<dyn Element> = Box::new(RichTextElement {
        markdown: "**fixture**".into(),
    });
    let rich_text = Node::from(external).describe(&mut BuildCx::new());
    assert_eq!(rich_text.kind(), &ElementKind::Custom("rich-text".into()));
    assert_eq!(
        rich_text.properties().get("format").map(String::as_str),
        Some("markdown")
    );
    assert_eq!(
        rich_text.children()[0]
            .describe(&mut BuildCx::new())
            .text_content(),
        Some("**fixture**")
    );

    let mut cache = StyleCache::default();
    assert!(cache.apply(StyleInput::text("padding: 4")).applied);
    let invalid = cache.apply(StyleInput::text("unknown-property: value"));
    assert!(invalid.is_diagnostic_success());
    assert_eq!(
        invalid
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.property_or_token.as_str()),
        Some("unknown-property")
    );
}

#[test]
fn external_fixture_has_a_platform_boundary_but_creates_no_window() {
    let window = Window::new("external facade fixture");
    assert_eq!(window.title(), "external facade fixture");
    let _run_loop = RunLoop::new();
}
