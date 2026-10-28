//! Virtualized ListView demo (10k rows) with in-app MenuBar + ContextMenu (Phase 46).

use synui::prelude::*;

const ROW_COUNT: usize = 10_000;

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let status = context.state(String::from("Ready — use MenuBar or right-click a row"))?;

    let select = context.action({
        let status = status.clone();
        move |_, index: usize| {
            status
                .set(format!("Selected row {index}"))
                .expect("status state is active");
        }
    })?;
    let activate = context.action({
        let status = status.clone();
        move |_, index: usize| {
            status
                .set(format!("Activated row {index}"))
                .expect("status state is active");
        }
    })?;
    let on_context = context.action({
        let status = status.clone();
        move |_, (index, _x, _y): (usize, f64, f64)| {
            status
                .set(format!("Context on item {index}"))
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

    app.run(Window::new("list_demo"), move |_| {
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
                    "Edit",
                    [
                        MenuItem::new("copy", "Copy"),
                        MenuItem::new("paste", "Paste"),
                    ],
                ),
            ])
            .on_action(on_menu.clone())
            .node()
            .key(BusinessKey::new("menu-bar")),
            Node::text(status.get()?).key(BusinessKey::new("status")),
            Node::text("10k virtualized rows — scroll / ↑↓ / right-click · F10/Alt opens MenuBar"),
            VirtualList::new(ROW_COUNT)
                .item_extent(32.0)
                .item(|index| format!("Row {index}"))
                .on_select(select.clone())
                .on_activate(activate.clone())
                .on_context(on_context.clone())
                .node()
                .key(BusinessKey::new("virtual-list")),
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
