//! Phase 44: facade DataTable with sort / select / edit / filter.

use synui::prelude::*;

const ROW_COUNT: usize = 500;

fn sample_cell(row: usize, column: usize) -> String {
    match column {
        0 => format!("item-{row:04}"),
        1 => format!("{}", (row * 17) % 100),
        2 => {
            if row % 3 == 0 {
                "active".into()
            } else {
                "idle".into()
            }
        }
        _ => format!("{:.1}", row as f32 * 0.37),
    }
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let status = context.state(String::from(
        "Ready — sort headers, select rows, edit cells",
    ))?;
    let filter = context.state(String::new())?;
    let rows = context.state(
        (0..ROW_COUNT)
            .map(|row| {
                (0..4)
                    .map(|column| sample_cell(row, column))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<Vec<String>>>(),
    )?;

    let select = context.action({
        let status = status.clone();
        move |_, selection: SelectionChange| {
            let summary = if selection.ranges.is_empty() {
                "no row selected".into()
            } else {
                let rows: Vec<_> = selection
                    .ranges
                    .iter()
                    .map(|range| range.row_start)
                    .collect();
                format!("selected {rows:?}")
            };
            status
                .set(format!("Selection — {summary}"))
                .expect("status state is active");
        }
    })?;
    let sort = context.action({
        let status = status.clone();
        move |_, descriptors: Vec<SortDescriptor>| {
            let message = descriptors
                .first()
                .map(|descriptor| {
                    format!(
                        "Sorted column {} ({:?})",
                        descriptor.column, descriptor.direction
                    )
                })
                .unwrap_or_else(|| "Sort cleared".into());
            status.set(message).expect("status state is active");
        }
    })?;
    let edit = context.action({
        let status = status.clone();
        let rows = rows.clone();
        move |_, (row, column, value): (usize, usize, String)| {
            let mut current = rows.get().expect("rows state is active");
            if let Some(cell) = current.get_mut(row).and_then(|r| r.get_mut(column)) {
                *cell = value.clone();
            }
            rows.set(current).expect("rows state is active");
            status
                .set(format!("Edited cell ({row},{column}) = {value}"))
                .expect("status state is active");
        }
    })?;
    let update_filter = context.action({
        let filter = filter.clone();
        let status = status.clone();
        move |_, value: String| {
            filter.set(value.clone()).expect("filter state is active");
            status
                .set(format!("Filter = {value}"))
                .expect("status state is active");
        }
    })?;

    app.run(Window::new("table_demo"), move |_| {
        let current_rows = rows.get()?;
        let query = filter.get()?;
        Ok(Node::column([
            Node::text(status.get()?).key(BusinessKey::new("status")),
            Node::text("DataTable — sort · edit · freeze col0 · filter (client substring)"),
            TextInput::new("Filter rows…")
                .controlled(filter.clone(), update_filter.clone())
                .node()
                .key(BusinessKey::new("filter")),
            DataTable::new()
                .columns([
                    TableColumn::new("name", "Name", 160.0),
                    TableColumn::new("score", "Score", 80.0),
                    TableColumn::new("state", "State", 100.0),
                    TableColumn::new("val", "Value", 90.0),
                ])
                .rows(current_rows.len())
                .freeze_leading(1)
                .filter(query)
                .cell({
                    let current_rows = current_rows.clone();
                    move |row, column| {
                        current_rows
                            .get(row)
                            .and_then(|r| r.get(column).cloned())
                            .unwrap_or_default()
                    }
                })
                .on_select(select.clone())
                .on_sort(sort.clone())
                .on_edit(edit.clone())
                .node()
                .key(BusinessKey::new("table")),
            Node::text("Merged cells (data coordinates; static demo)"),
            DataTable::new()
                .columns([
                    TableColumn::new("region", "Region", 100.0),
                    TableColumn::new("code", "Code", 80.0),
                    TableColumn::new("st", "Status", 80.0),
                ])
                .rows(3)
                .cell(|row, col| match (row, col) {
                    (0, 0) => "North".into(),
                    (1, 0) => "South".into(),
                    (2, 0) => "East".into(),
                    (_, 1) => format!("{}1", ['A', 'B', 'C'][row.min(2)]),
                    (_, 2) => {
                        if row % 2 == 0 {
                            "ok".into()
                        } else {
                            "warn".into()
                        }
                    }
                    _ => String::new(),
                })
                .span(0, 0, 2, 1)
                .node()
                .key(BusinessKey::new("merge-table")),
            Node::text("VirtualList with sticky-style header label"),
            VirtualList::new(2_000)
                .item_extent(28.0)
                .item(|index| format!("Item {index}"))
                .node()
                .key(BusinessKey::new("list")),
        ]))
    })
}
