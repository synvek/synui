//! Phase 48 polish: MenuBar + Tree | Split(Tab(Table|Preview) | Detail).
//! Preview uses prelude RichDocument. Floating dock remains hard-locked.
//! Split ratios are App-owned State so rebuilds do not reset a dragged divider.

use synui::prelude::*;

const ROW_COUNT: usize = 200;

fn sample_rows() -> Vec<Vec<String>> {
    (0..ROW_COUNT)
        .map(|i| {
            vec![
                format!("item-{i:04}"),
                if i % 2 == 0 { "file" } else { "folder" }.into(),
                format!("{}", (i * 13) % 100),
                if i % 3 == 0 { "ok" } else { "warn" }.into(),
            ]
        })
        .collect()
}

fn folder_tree() -> Vec<TreeNode> {
    vec![TreeNode::new("root", "workspace").with_children([
        TreeNode::new("src", "src").with_children([
            TreeNode::new("src/main", "main.rs"),
            TreeNode::new("src/lib", "lib.rs"),
        ]),
        TreeNode::new("docs", "docs").with_children([TreeNode::new("docs/readme", "README.md")]),
        TreeNode::new("examples", "examples"),
    ])]
}

fn preview_text(rows: &[Vec<String>], selected: &[usize], filter: &str) -> String {
    let q = filter.trim().to_ascii_lowercase();
    let visible: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            q.is_empty()
                || row
                    .iter()
                    .any(|cell| cell.to_ascii_lowercase().contains(&q))
        })
        .map(|(index, _)| index)
        .collect();
    if selected.is_empty() {
        return "No row selected.\n\nSelect a row in Table, then open Preview.".into();
    }
    let mut out = String::from("# Selection\n\n");
    for &visual in selected.iter().take(8) {
        let data_idx = visible.get(visual).copied().unwrap_or(visual);
        if let Some(row) = rows.get(data_idx) {
            out.push_str(&format!(
                "- **{}** · {} · score {} · {}\n",
                row.first().map(String::as_str).unwrap_or("?"),
                row.get(1).map(String::as_str).unwrap_or("?"),
                row.get(2).map(String::as_str).unwrap_or("?"),
                row.get(3).map(String::as_str).unwrap_or("?"),
            ));
        }
    }
    if selected.len() > 8 {
        out.push_str(&format!("\n…and {} more\n", selected.len() - 8));
    }
    out
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let status = context.state(String::from("Ready — tree · filter · table/preview tabs"))?;
    let folder = context.state(String::from("workspace"))?;
    let tab = context.state(0_usize)?;
    let filter = context.state(String::new())?;
    let selected_rows = context.state(Vec::<usize>::new())?;
    let rows = context.state(sample_rows())?;
    let outer_ratio = context.state(0.28_f64)?;
    let inner_ratio = context.state(0.72_f64)?;
    let tree_roots = folder_tree();

    let on_menu = context.action({
        let status = status.clone();
        let filter = filter.clone();
        let tab = tab.clone();
        move |_, id: String| match id.as_str() {
            "clear-filter" => {
                filter.set(String::new()).expect("filter state is active");
                status
                    .set(String::from("Menu: clear-filter"))
                    .expect("status state is active");
            }
            "show-table" => {
                tab.set(0).expect("tab state is active");
                status
                    .set(String::from("Menu: show-table"))
                    .expect("status state is active");
            }
            "show-preview" => {
                tab.set(1).expect("tab state is active");
                status
                    .set(String::from("Menu: show-preview"))
                    .expect("status state is active");
            }
            other => {
                status
                    .set(format!("Menu: {other}"))
                    .expect("status state is active");
            }
        }
    })?;

    let on_tree_select = context.action({
        let folder = folder.clone();
        let status = status.clone();
        move |_, id: String| {
            folder.set(id.clone()).expect("folder state is active");
            status
                .set(format!("Folder = {id}"))
                .expect("status state is active");
        }
    })?;
    let on_tab = context.action({
        let tab = tab.clone();
        move |_, index: usize| tab.set(index).expect("tab state is active")
    })?;
    let on_filter = context.action({
        let filter = filter.clone();
        let status = status.clone();
        move |_, value: String| {
            filter.set(value.clone()).expect("filter state is active");
            status
                .set(format!("Filter = {value}"))
                .expect("status state is active");
        }
    })?;
    let on_select = context.action({
        let selected_rows = selected_rows.clone();
        let status = status.clone();
        move |_, selection: SelectionChange| {
            let idxs: Vec<usize> = selection.ranges.iter().map(|r| r.row_start).collect();
            selected_rows
                .set(idxs.clone())
                .expect("selection state is active");
            status
                .set(format!("Selected rows {idxs:?}"))
                .expect("status state is active");
        }
    })?;
    let on_sort = context.action({
        let status = status.clone();
        move |_, descriptors: Vec<SortDescriptor>| {
            let message = descriptors
                .first()
                .map(|d| format!("Sorted column {}", d.column))
                .unwrap_or_else(|| "Sort cleared".into());
            status.set(message).expect("status state is active");
        }
    })?;
    let on_edit = context.action({
        let rows = rows.clone();
        let status = status.clone();
        move |_, (row, column, value): (usize, usize, String)| {
            let mut current = rows.get().expect("rows state is active");
            if let Some(cell) = current.get_mut(row).and_then(|r| r.get_mut(column)) {
                *cell = value.clone();
            }
            rows.set(current).expect("rows state is active");
            status
                .set(format!("Edited ({row},{column}) = {value}"))
                .expect("status state is active");
        }
    })?;
    let on_outer_ratio = context.action({
        let outer_ratio = outer_ratio.clone();
        move |_, ratio: f64| {
            outer_ratio
                .set(ratio.clamp(0.05, 0.95))
                .expect("outer ratio state is active");
        }
    })?;
    let on_inner_ratio = context.action({
        let inner_ratio = inner_ratio.clone();
        move |_, ratio: f64| {
            inner_ratio
                .set(ratio.clamp(0.05, 0.95))
                .expect("inner ratio state is active");
        }
    })?;

    app.run(Window::new("data_browser_demo"), move |_| {
        let current_tab = tab.get()?;
        let query = filter.get()?;
        let current_rows = rows.get()?;
        let selection = selected_rows.get()?;
        let current_folder = folder.get()?;
        let current_outer = outer_ratio.get()? as f32;
        let current_inner = inner_ratio.get()? as f32;
        let preview = preview_text(&current_rows, &selection, &query);

        let table_pane = DataTable::new()
            .columns([
                TableColumn::new("name", "Name", 160.0),
                TableColumn::new("kind", "Kind", 80.0),
                TableColumn::new("score", "Score", 80.0),
                TableColumn::new("flag", "Flag", 90.0),
            ])
            .rows(current_rows.len())
            .freeze_leading(1)
            .filter(query.clone())
            .cell({
                let current_rows = current_rows.clone();
                move |row, column| {
                    current_rows
                        .get(row)
                        .and_then(|r| r.get(column).cloned())
                        .unwrap_or_default()
                }
            })
            .on_select(on_select.clone())
            .on_sort(on_sort.clone())
            .on_edit(on_edit.clone())
            .node()
            .key(BusinessKey::new("table"));

        let preview_pane = RichDocument::new(BlockDoc::from_plain(&preview))
            .node()
            .key(BusinessKey::new("preview"));

        let main_content = if current_tab == 0 {
            table_pane
        } else {
            preview_pane
        };

        let detail = Styled::new(
            "padding:10;gap:4;width:fill;height:fill;background:surface-container",
            [
                Label::new(format!("Detail — folder `{current_folder}`"))
                    .node()
                    .key(BusinessKey::new("detail-folder")),
                Label::new(format!(
                    "tab={} · filter matches on client substring",
                    if current_tab == 0 { "Table" } else { "Preview" }
                ))
                .node()
                .key(BusinessKey::new("detail-meta")),
            ],
        )
        .node()
        .key(BusinessKey::new("detail"));

        let right = SplitPane::vertical(
            Node::column([
                TabBar::new(["Table", "Preview"])
                    .selected(current_tab)
                    .on_change(on_tab.clone())
                    .node()
                    .key(BusinessKey::new("tabs")),
                TextInput::new("Filter rows…")
                    .controlled(filter.clone(), on_filter.clone())
                    .node()
                    .key(BusinessKey::new("filter")),
                main_content,
            ])
            .key(BusinessKey::new("main")),
            detail,
        )
        .ratio(current_inner)
        .on_ratio(on_inner_ratio.clone())
        .node()
        .key(BusinessKey::new("inner-split"));

        Ok(Node::column([
            MenuBar::new([
                Menu::new(
                    "File",
                    [
                        MenuItem::new("refresh", "Refresh"),
                        MenuItem::new("quit", "Quit"),
                    ],
                ),
                Menu::new(
                    "View",
                    [
                        MenuItem::new("show-table", "Show Table"),
                        MenuItem::new("show-preview", "Show Preview"),
                        MenuItem::new("clear-filter", "Clear Filter"),
                    ],
                ),
            ])
            .on_action(on_menu.clone())
            .node()
            .key(BusinessKey::new("menu-bar")),
            Label::new(status.get()?)
                .node()
                .key(BusinessKey::new("status")),
            Label::new("Preview is prelude RichDocument · floating dock hard-locked")
                .node()
                .key(BusinessKey::new("tips")),
            SplitPane::horizontal(
                Node::column([
                    Label::new("Folders").node(),
                    VirtualTree::new(tree_roots.clone())
                        .on_select(on_tree_select.clone())
                        .node()
                        .key(BusinessKey::new("tree")),
                ])
                .key(BusinessKey::new("tree-pane")),
                right,
            )
            .ratio(current_outer)
            .on_ratio(on_outer_ratio.clone())
            .node()
            .key(BusinessKey::new("outer-split")),
        ]))
    })
}
