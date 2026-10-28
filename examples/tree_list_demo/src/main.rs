//! Phase 46: facade VirtualTree + VirtualList with in-app MenuBar + ContextMenu.

use synui::prelude::*;

fn sample_tree() -> Vec<TreeNode> {
    vec![
        TreeNode::new("src", "src").with_children([
            TreeNode::new("src/main", "main.rs"),
            TreeNode::new("src/lib", "lib.rs"),
            TreeNode::new("src/widgets", "widgets").with_children([
                TreeNode::new("src/widgets/button", "button.rs"),
                TreeNode::new("src/widgets/input", "input.rs"),
            ]),
        ]),
        TreeNode::new("docs", "docs").with_children([
            TreeNode::new("docs/readme", "README.md"),
            TreeNode::new("docs/guide", "guide.md"),
        ]),
        TreeNode::new("cargo", "Cargo.toml"),
    ]
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let status = context.state(String::from("Ready — keyboard menus, tree, list selection"))?;
    let tree_sel = context.state::<Option<String>>(None)?;
    let list_sel = context.state::<Option<usize>>(None)?;

    let on_tree_select = context.action({
        let tree_sel = tree_sel.clone();
        let status = status.clone();
        move |_, id: String| {
            tree_sel
                .set(Some(id.clone()))
                .expect("tree selection state is active");
            status
                .set(format!("Tree selected: {id}"))
                .expect("status state is active");
        }
    })?;
    let on_tree_activate = context.action({
        let status = status.clone();
        move |_, id: String| {
            status
                .set(format!("Tree activated: {id}"))
                .expect("status state is active");
        }
    })?;
    let on_tree_expand = context.action({
        let status = status.clone();
        move |_, (id, expanded): (String, bool)| {
            status
                .set(format!(
                    "Tree {} {}",
                    id,
                    if expanded { "expanded" } else { "collapsed" }
                ))
                .expect("status state is active");
        }
    })?;
    let on_list_select = context.action({
        let list_sel = list_sel.clone();
        let status = status.clone();
        move |_, index: usize| {
            list_sel
                .set(Some(index))
                .expect("list selection state is active");
            status
                .set(format!("List selected: {index}"))
                .expect("status state is active");
        }
    })?;
    let on_list_context = context.action({
        let status = status.clone();
        move |_, (index, _x, _y): (usize, f64, f64)| {
            status
                .set(format!("Context on painted row {index}"))
                .expect("status state is active");
        }
    })?;
    let on_menu = context.action({
        let status = status.clone();
        move |_, id: String| {
            status
                .set(format!("Menu: {id}"))
                .expect("status state is active");
        }
    })?;
    let on_ctx_action = context.action({
        let status = status.clone();
        move |_, id: String| {
            status
                .set(format!("Context action: {id}"))
                .expect("status state is active");
        }
    })?;

    app.run(Window::new("tree_list_demo"), move |_| {
        let tree = tree_sel.get()?;
        let list = list_sel.get()?;
        Ok(Node::column([
            MenuBar::new([
                Menu::new(
                    "File",
                    [
                        MenuItem::new("new", "New"),
                        MenuItem::new("open", "Open"),
                        MenuItem::new("quit", "Quit"),
                    ],
                ),
                Menu::new(
                    "View",
                    [
                        MenuItem::new("expand", "Expand all"),
                        MenuItem::new("collapse", "Collapse"),
                    ],
                ),
            ])
            .on_action(on_menu.clone())
            .node()
            .key(BusinessKey::new("menu-bar")),
            Node::text(status.get()?).key(BusinessKey::new("status")),
            Node::text(format!(
                "Selection: tree={} | list={}",
                tree.as_deref().unwrap_or("none"),
                list.map(|index| index.to_string())
                    .unwrap_or_else(|| "none".into())
            )),
            Node::text(
                "F10 / Alt opens MenuBar · ←→ menus · ↑↓ items · Esc closes · right-click list",
            ),
            Node::row([
                Node::column([
                    Node::text("VirtualTree — ←→ expand/collapse"),
                    VirtualTree::new(sample_tree())
                        .on_select(on_tree_select.clone())
                        .on_activate(on_tree_activate.clone())
                        .on_expand(on_tree_expand.clone())
                        .node()
                        .key(BusinessKey::new("tree")),
                ])
                .key(BusinessKey::new("tree-col")),
                Node::column([
                    Node::text("VirtualList — scroll / select / right-click"),
                    VirtualList::new(2_000)
                        .item_extent(28.0)
                        .item(|index| format!("Item {index}"))
                        .on_select(on_list_select.clone())
                        .on_context(on_list_context.clone())
                        .node()
                        .key(BusinessKey::new("list")),
                    Node::text("VirtualList — denser window"),
                    VirtualList::new(5_000)
                        .item_extent(28.0)
                        .item(|index| format!("Mounted {index}"))
                        .node()
                        .key(BusinessKey::new("list-mount")),
                ])
                .key(BusinessKey::new("list-col")),
            ])
            .key(BusinessKey::new("body")),
            ContextMenu::new([
                MenuItem::new("open", "Open item"),
                MenuItem::new("delete", "Delete"),
                MenuItem::new("info", "Info"),
            ])
            .on_action(on_ctx_action.clone())
            .node()
            .key(BusinessKey::new("context-menu")),
        ]))
    })
}
