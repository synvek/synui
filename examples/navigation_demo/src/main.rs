//! Phase 47: AppBar + Drawer + BottomNav + Dialog + in-app MenuBar.
//! Platform history sync remains an explicit host boundary. Floating dock hard-locked.

use synui::prelude::*;

#[derive(Clone, Debug, PartialEq)]
enum DemoRoute {
    List,
    Detail(u32),
    Settings,
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let route = context.state(DemoRoute::List)?;
    let drawer_open = context.state(false)?;
    let dialog_open = context.state(false)?;
    let nav_index = context.state(0_usize)?;
    let status = context.state(String::from(
        "Ready — File/Help = MenuBar dropdown; AppBar Drawer = side rail",
    ))?;

    let navigate = context.action({
        let route = route.clone();
        move |_, next: DemoRoute| route.set(next).expect("route state is active")
    })?;
    let back = context.action({
        let route = route.clone();
        move |_, (): ()| route.set(DemoRoute::List).expect("route state is active")
    })?;
    let toggle_drawer = context.action({
        let drawer_open = drawer_open.clone();
        move |cx, (): ()| {
            let open = drawer_open.get_in(cx).unwrap_or(false);
            let _ = drawer_open.set_in(cx, !open);
        }
    })?;
    let on_drawer = context.action({
        let drawer_open = drawer_open.clone();
        move |cx, open: bool| {
            let _ = drawer_open.set_in(cx, open);
        }
    })?;
    let open_dialog = context.action({
        let dialog_open = dialog_open.clone();
        move |cx, (): ()| {
            let _ = dialog_open.set_in(cx, true);
        }
    })?;
    let dismiss_dialog = context.action({
        let dialog_open = dialog_open.clone();
        move |cx, (): ()| {
            let _ = dialog_open.set_in(cx, false);
        }
    })?;
    let on_nav = context.action({
        let nav_index = nav_index.clone();
        let route = route.clone();
        move |cx, index: usize| {
            let _ = nav_index.set_in(cx, index);
            let next = match index {
                1 => DemoRoute::Settings,
                _ => DemoRoute::List,
            };
            let _ = route.set_in(cx, next);
        }
    })?;
    let on_menu = context.action({
        let status = status.clone();
        let open_dialog = open_dialog.clone();
        let navigate = navigate.clone();
        move |cx, id: String| match id.as_str() {
            "about" => {
                let _ = open_dialog.invoke(cx, ());
                let _ = status.set_in(cx, String::from("Menu: about"));
            }
            "home" => {
                let _ = navigate.invoke(cx, DemoRoute::List);
                let _ = status.set_in(cx, String::from("Menu: home"));
            }
            "settings" => {
                let _ = navigate.invoke(cx, DemoRoute::Settings);
                let _ = status.set_in(cx, String::from("Menu: settings"));
            }
            other => {
                let _ = status.set_in(cx, format!("Menu: {other}"));
            }
        }
    })?;

    app.run(Window::new("navigation_demo"), move |cx| {
        let current = route.get_in(cx)?;
        let open = drawer_open.get_in(cx)?;
        let show_dialog = dialog_open.get_in(cx)?;
        let selected = nav_index.get_in(cx)?;

        let body = match current {
            DemoRoute::List => Node::column([
                Node::text("Items"),
                Button::new("Item 1")
                    .on_click({
                        let navigate = navigate.clone();
                        move |event: EventContext| {
                            let _ = navigate.invoke(event.app(), DemoRoute::Detail(1));
                        }
                    })
                    .node()
                    .key(BusinessKey::new("item-1")),
                Button::new("Item 2")
                    .on_click({
                        let navigate = navigate.clone();
                        move |event: EventContext| {
                            let _ = navigate.invoke(event.app(), DemoRoute::Detail(2));
                        }
                    })
                    .node()
                    .key(BusinessKey::new("item-2")),
                Button::new("About dialog")
                    .on_press(open_dialog.clone())
                    .node()
                    .key(BusinessKey::new("about")),
            ]),
            DemoRoute::Detail(id) => Node::column([
                Node::text(format!("Detail {id}")).key(BusinessKey::new("detail")),
                Button::new("Back").on_press(back.clone()).node(),
            ]),
            DemoRoute::Settings => Node::column([
                Node::text("Settings").key(BusinessKey::new("settings")),
                Button::new("Back").on_press(back.clone()).node(),
            ]),
        };

        Ok(Node::column([
            MenuBar::new([
                Menu::new(
                    "File",
                    [
                        MenuItem::new("home", "Home"),
                        MenuItem::new("settings", "Settings"),
                        MenuItem::new("quit", "Quit"),
                    ],
                ),
                Menu::new("Help", [MenuItem::new("about", "About")]),
            ])
            .on_action(on_menu.clone())
            .node()
            .key(BusinessKey::new("menu-bar")),
            Node::text(status.get_in(cx)?).key(BusinessKey::new("status")),
            AppBar::new("SynUI Navigation")
                .trailing([Button::new("Drawer").on_press(toggle_drawer.clone()).node()])
                .node()
                .key(BusinessKey::new("app-bar")),
            NavigationDrawer::new([
                Button::new("List")
                    .on_click({
                        let navigate = navigate.clone();
                        let on_drawer = on_drawer.clone();
                        move |event: EventContext| {
                            let _ = navigate.invoke(event.app(), DemoRoute::List);
                            let _ = on_drawer.invoke(event.app(), false);
                        }
                    })
                    .node(),
                Button::new("Settings")
                    .on_click({
                        let navigate = navigate.clone();
                        let on_drawer = on_drawer.clone();
                        move |event: EventContext| {
                            let _ = navigate.invoke(event.app(), DemoRoute::Settings);
                            let _ = on_drawer.invoke(event.app(), false);
                        }
                    })
                    .node(),
            ])
            .open(open)
            .on_open_change(on_drawer.clone())
            .node()
            .key(BusinessKey::new("drawer")),
            body.key(BusinessKey::new("body")),
            BottomNavigation::new(["Home", "Settings"])
                .selected(selected.min(1))
                .on_change(on_nav.clone())
                .node()
                .key(BusinessKey::new("bottom-nav")),
            Dialog::new(
                "About",
                [
                    Node::text("Facade navigation chrome + in-app MenuBar (Phase 47)."),
                    Button::new("Close").on_press(dismiss_dialog.clone()).node(),
                ],
            )
            .open(show_dialog)
            .on_dismiss(dismiss_dialog.clone())
            .node()
            .key(BusinessKey::new("dialog")),
            Node::text("history-sync=platform-host-boundary"),
        ]))
    })
}
