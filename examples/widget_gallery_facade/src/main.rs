use std::path::PathBuf;

use synui::prelude::*;

const LANGUAGES: [&str; 3] = ["Rust", "Python", "C++"];
const IMAGE_ID: &str = "gallery-cat";
const SVG_IMAGE_ID: &str = "gallery-home-svg";
const LOTTIE_ID: &str = "pulse";

/// Gallery sections (Engine-era category strip → facade drawer navigation).
const CATEGORIES: [&str; 10] = [
    "Basic",
    "Inputs",
    "Progress",
    "Surfaces",
    "Navigation",
    "Data",
    "Rich",
    "Media",
    "Animation",
    "Style",
];

fn asset_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets")
        .join(file)
}

/// Shared example fixtures live under `examples/assets/` (e.g. Lottie JSON).
fn example_asset_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("assets")
        .join(file)
}

fn sample_tree() -> Vec<TreeNode> {
    vec![
        TreeNode::new("src", "src").with_children([
            TreeNode::new("src/main", "main.rs"),
            TreeNode::new("src/lib", "lib.rs"),
        ]),
        TreeNode::new("docs", "docs").with_children([TreeNode::new("docs/readme", "README.md")]),
    ]
}

fn main() -> Result<(), PlatformError> {
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "widget_gallery_facade (facade-only stable gallery)\n\n\
             Check: cargo check -p widget_gallery_facade\n\
             Matrix: docs/gallery_facade_vs_engine.md\n\
             Open the AppBar Drawer to switch categories (Basic / Inputs / …).\n\
             Stable fixture: cargo test -p synui --test counter_prelude_fixture\n\
             This package uses only synui::prelude::*; retained Engine and\n\
             experimental construction paths are intentionally excluded."
        );
        return Ok(());
    }

    let app = App::new(AppConfig::default())?;
    let context = app.context();

    let category = context.state(0_usize)?;
    let count = context.state(0_i32)?;
    let name = context.state(String::from("SynUI developer"))?;
    let secret = context.state(String::from("hunter2"))?;
    let enabled = context.state(true)?;
    let toggle_on = context.state(false)?;
    let radio_a = context.state(true)?;
    let volume = context.state(40.0_f64)?;
    let progress = context.state(0.45_f32)?;
    let language = context.state(ChoiceId::new(0))?;
    let menu_status = context.state(String::from("Menu idle — open Drawer to switch sections"))?;
    let tab = context.state(0_usize)?;
    let list_sel = context.state::<Option<usize>>(None)?;
    let dialog_open = context.state(false)?;
    let drawer_open = context.state(false)?;
    let sheet_open = context.state(false)?;
    let snack_visible = context.state(false)?;
    let nav_index = context.state(0_usize)?;
    let rich_doc = context.state(BlockDoc::from_plain(
        "RichDocument smoke — edit here (full demos: rich_edit_demo).",
    ))?;
    let notes = context.state(String::from("TextArea smoke — multiline notes."))?;

    let mut style_cache = StyleCache::default();
    assert!(
        style_cache
            .apply(StyleInput::Rust(Style::new().padding_all(8.0).gap(4.0)))
            .applied
    );
    let invalid_style = style_cache.apply(StyleInput::text("unknown-property:1"));
    assert!(invalid_style.is_diagnostic_success());

    let increment = context.action({
        let count = count.clone();
        move |_, (): ()| {
            count
                .set(count.get().expect("count state") + 1)
                .expect("count state")
        }
    })?;
    let update_name = context.action({
        let name = name.clone();
        move |_, value: String| name.set(value).expect("name state")
    })?;
    let update_secret = context.action({
        let secret = secret.clone();
        move |_, value: String| secret.set(value).expect("secret state")
    })?;
    let update_notes = context.action({
        let notes = notes.clone();
        move |_, value: String| notes.set(value).expect("notes state")
    })?;
    let update_enabled = context.action({
        let enabled = enabled.clone();
        move |_, value: bool| enabled.set(value).expect("enabled state")
    })?;
    let update_toggle = context.action({
        let toggle_on = toggle_on.clone();
        move |_, value: bool| toggle_on.set(value).expect("toggle state")
    })?;
    let update_radio = context.action({
        let radio_a = radio_a.clone();
        move |_, value: bool| radio_a.set(value).expect("radio state")
    })?;
    let update_volume = context.action({
        let volume = volume.clone();
        move |_, value: f64| volume.set(value).expect("volume state")
    })?;
    let update_language = context.action({
        let language = language.clone();
        move |_, value: ChoiceId| language.set(value).expect("language state")
    })?;
    let on_list_item = context.action({
        let menu_status = menu_status.clone();
        move |_, (): ()| {
            menu_status
                .set("ListItem pressed".into())
                .expect("menu status")
        }
    })?;
    let on_menu = context.action({
        let menu_status = menu_status.clone();
        let dialog_open = dialog_open.clone();
        move |cx, id: String| {
            if id == "about" {
                let _ = dialog_open.set_in(cx, true);
            }
            let _ = menu_status.set_in(cx, format!("Menu: {id}"));
        }
    })?;
    let on_ctx = context.action({
        let menu_status = menu_status.clone();
        move |_, id: String| {
            menu_status
                .set(format!("Context: {id}"))
                .expect("menu status")
        }
    })?;
    let on_tab = context.action({
        let tab = tab.clone();
        move |_, index: usize| tab.set(index).expect("tab state")
    })?;
    let on_list = context.action({
        let list_sel = list_sel.clone();
        let menu_status = menu_status.clone();
        move |_, index: usize| {
            list_sel.set(Some(index)).expect("list selection");
            menu_status
                .set(format!("List row {index}"))
                .expect("menu status")
        }
    })?;
    let on_list_ctx = context.action({
        let menu_status = menu_status.clone();
        move |_, (index, _x, _y): (usize, f64, f64)| {
            menu_status
                .set(format!("List context row {index}"))
                .expect("menu status")
        }
    })?;
    let on_rich = context.action({
        let rich_doc = rich_doc.clone();
        move |_, next: BlockDoc| rich_doc.set(next).expect("rich doc")
    })?;
    let dismiss_dialog = context.action({
        let dialog_open = dialog_open.clone();
        move |cx, (): ()| {
            let _ = dialog_open.set_in(cx, false);
        }
    })?;
    let open_dialog = context.action({
        let dialog_open = dialog_open.clone();
        move |cx, (): ()| {
            let _ = dialog_open.set_in(cx, true);
        }
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
    let select_category = context.action({
        let category = category.clone();
        let drawer_open = drawer_open.clone();
        let menu_status = menu_status.clone();
        move |cx, index: usize| {
            let idx = index.min(CATEGORIES.len().saturating_sub(1));
            let _ = category.set_in(cx, idx);
            let _ = drawer_open.set_in(cx, false);
            let name = CATEGORIES.get(idx).copied().unwrap_or("?");
            let _ = menu_status.set_in(cx, format!("Section: {name}"));
        }
    })?;
    let toggle_sheet = context.action({
        let sheet_open = sheet_open.clone();
        move |cx, (): ()| {
            let open = sheet_open.get_in(cx).unwrap_or(false);
            let _ = sheet_open.set_in(cx, !open);
        }
    })?;
    let on_sheet = context.action({
        let sheet_open = sheet_open.clone();
        move |cx, (): ()| {
            let _ = sheet_open.set_in(cx, false);
        }
    })?;
    let show_snack_action = context.action({
        let snack_visible = snack_visible.clone();
        move |cx, (): ()| {
            let _ = snack_visible.set_in(cx, true);
        }
    })?;
    let hide_snack = context.action({
        let snack_visible = snack_visible.clone();
        move |cx, (): ()| {
            let _ = snack_visible.set_in(cx, false);
        }
    })?;
    let on_nav = context.action({
        let nav_index = nav_index.clone();
        let menu_status = menu_status.clone();
        move |cx, index: usize| {
            let _ = nav_index.set_in(cx, index);
            let _ = menu_status.set_in(cx, format!("BottomNav: {index}"));
        }
    })?;
    let on_tree = context.action({
        let menu_status = menu_status.clone();
        move |_, id: String| menu_status.set(format!("Tree: {id}")).expect("menu status")
    })?;

    let increment_btn = Button::new("Increment")
        .tooltip("Add one to the counter")
        .on_press(increment);
    let name_input = TextInput::new("Display name").controlled(name.clone(), update_name);
    let secret_input = TextInput::new("Password")
        .password(true)
        .controlled(secret.clone(), update_secret);
    let notes_input = TextArea::new("Notes").controlled(notes.clone(), update_notes);
    let enabled_input = Checkbox::new(true)
        .label("Enabled")
        .controlled(enabled.clone(), update_enabled);
    let switch_input = Switch::new(false).controlled(toggle_on.clone(), update_toggle);
    let radio_input = Radio::new("Option A", true).controlled(radio_a.clone(), update_radio);
    let volume_input = NumberInput::new(40.0)
        .range(0.0, 100.0)
        .controlled(volume.clone(), update_volume);
    let language_input = Select::new(LANGUAGES, 0).controlled(language.clone(), update_language);

    let window = Window::new("SynUI v0.2 Facade Gallery")
        .with_image(IMAGE_ID, asset_path("cat.png"))
        .with_image(SVG_IMAGE_ID, asset_path("home.svg"))
        .with_lottie(LOTTIE_ID, example_asset_path("lottie_pulse.json"));
    app.run(window, move |_| {
        let selected = LANGUAGES
            .get(language.get()?.index())
            .copied()
            .unwrap_or("Unknown");
        let cat = category.get()?.min(CATEGORIES.len() - 1);
        let cat_name = CATEGORIES[cat];
        let tab_idx = tab.get()?;
        let show_dialog = dialog_open.get()?;
        let show_drawer = drawer_open.get()?;
        let show_sheet = sheet_open.get()?;
        let show_snack = snack_visible.get()?;
        let nav_sel = nav_index.get()?;

        let section = match cat {
            0 => Node::column([
                Label::new("Basic")
                    .node()
                    .key(BusinessKey::new("basic-title")),
                Label::new(format!("Count: {}", count.get()?))
                    .node()
                    .key(BusinessKey::new("count")),
                increment_btn
                    .node()
                    .key(BusinessKey::new("increment")),
                Label::new(menu_status.get()?)
                    .node()
                    .key(BusinessKey::new("menu-status")),
            ]),
            1 => Styled::new(
                "width:fill;flex-direction:column;align-items:flex-start;gap:4",
                [
                    Label::new("Controlled inputs")
                        .node()
                        .key(BusinessKey::new("input-title")),
                    Label::new(format!("Name: {}", name.get()?))
                        .node()
                        .key(BusinessKey::new("name-value")),
                    name_input.node().key(BusinessKey::new("name-input")),
                    Label::new(format!(
                        "Password length: {} (glyphs masked)",
                        secret.get()?.chars().count()
                    ))
                    .node()
                    .key(BusinessKey::new("secret-value")),
                    secret_input
                        .node()
                        .key(BusinessKey::new("secret-input")),
                    Label::new(format!("Notes: {}", notes.get()?))
                        .node()
                        .key(BusinessKey::new("notes-value")),
                    notes_input.node().key(BusinessKey::new("text-area")),
                    Label::new(format!("Enabled: {}", enabled.get()?))
                        .node()
                        .key(BusinessKey::new("enabled-value")),
                    enabled_input
                        .node()
                        .key(BusinessKey::new("enabled-input")),
                    Label::new(format!("Switch: {}", toggle_on.get()?))
                        .node()
                        .key(BusinessKey::new("switch-value")),
                    switch_input
                        .node()
                        .key(BusinessKey::new("switch-input")),
                    Label::new(format!("Radio A: {}", radio_a.get()?))
                        .node()
                        .key(BusinessKey::new("radio-value")),
                    radio_input.node().key(BusinessKey::new("radio-input")),
                    Label::new(format!("Volume: {:.0}", volume.get()?))
                        .node()
                        .key(BusinessKey::new("volume-value")),
                    volume_input
                        .node()
                        .key(BusinessKey::new("volume-input")),
                    Label::new(format!("Language: {selected}"))
                        .node()
                        .key(BusinessKey::new("language-value")),
                    language_input
                        .node()
                        .key(BusinessKey::new("language-input")),
                ],
            )
            .node(),
            2 => Node::column([
                Label::new(format!("Progress: {:.0}%", progress.get()? * 100.0))
                    .node()
                    .key(BusinessKey::new("progress-value")),
                LinearProgress::new(progress.get()?)
                    .node()
                    .key(BusinessKey::new("linear-progress")),
                Label::new("CircularProgress (determinate; full demos elsewhere)")
                    .node()
                    .key(BusinessKey::new("circular-title")),
                CircularProgress::new(progress.get()?)
                    .indeterminate(false)
                    .node()
                    .key(BusinessKey::new("circular-progress")),
            ]),
            3 => Node::column([
                Label::new("Card / ListItem")
                    .node()
                    .key(BusinessKey::new("surface-title")),
                Card::new([
                    Label::new("Card body").node(),
                    ListItem::new("ListItem row")
                        .subtitle("Press updates menu status")
                        .on_press(on_list_item.clone())
                        .node()
                        .key(BusinessKey::new("list-item")),
                ])
                .elevation(2)
                .node()
                .key(BusinessKey::new("card")),
            ]),
            4 => Node::column([
                Label::new(format!("TabBar smoke (selected={tab_idx})"))
                    .node()
                    .key(BusinessKey::new("tab-title")),
                TabBar::new(["Inputs", "List", "About"])
                    .selected(tab_idx)
                    .on_change(on_tab.clone())
                    .node()
                    .key(BusinessKey::new("tab-bar")),
                Label::new("BottomNavigation smoke (full: navigation_demo)")
                    .node()
                    .key(BusinessKey::new("bottom-nav-title")),
                BottomNavigation::new(["Home", "Settings"])
                    .selected(nav_sel.min(1))
                    .on_change(on_nav.clone())
                    .node()
                    .key(BusinessKey::new("bottom-nav")),
                Label::new("AppBar Drawer is gallery chrome (categories), not this smoke.")
                    .node()
                    .key(BusinessKey::new("nav-note")),
            ]),
            5 => Node::column([
                Label::new(format!(
                    "VirtualList smoke (sel={:?}; right-click → ContextMenu)",
                    list_sel.get()?
                ))
                .node()
                .key(BusinessKey::new("list-title")),
                VirtualList::new(200)
                    .item_extent(28.0)
                    .item(|index| format!("Gallery row {index}"))
                    .on_select(on_list.clone())
                    .on_context(on_list_ctx.clone())
                    .node()
                    .key(BusinessKey::new("virtual-list")),
                ContextMenu::new([
                    MenuItem::new("open", "Open item"),
                    MenuItem::new("copy", "Copy"),
                    MenuItem::new("info", "Info"),
                ])
                .on_action(on_ctx.clone())
                .node()
                .key(BusinessKey::new("context-menu")),
                Label::new("SplitPane + VirtualTree (full: data_browser / tree_list)")
                    .node()
                    .key(BusinessKey::new("split-title")),
                SplitPane::horizontal(
                    Node::column([
                        Label::new("Tree").node(),
                        VirtualTree::new(sample_tree())
                            .on_select(on_tree.clone())
                            .node()
                            .key(BusinessKey::new("virtual-tree")),
                    ])
                    .key(BusinessKey::new("tree-pane")),
                    Node::column([
                        Label::new("Detail pane").node(),
                        Label::new("Full interaction demos remain dedicated packages.").node(),
                    ])
                    .key(BusinessKey::new("detail-pane")),
                )
                .ratio(0.4)
                .node()
                .key(BusinessKey::new("split-pane")),
                Label::new("DataTable smoke (full: table_demo)")
                    .node()
                    .key(BusinessKey::new("table-title")),
                DataTable::new()
                    .logical_dimensions(8, 3)
                    .cell(|row, col| format!("{row},{col}"))
                    .node()
                    .key(BusinessKey::new("data-table")),
            ]),
            6 => Node::column([
                Label::new("RichDocument smoke (full: rich_* demos)")
                    .node()
                    .key(BusinessKey::new("rich-title")),
                RichDocument::new(rich_doc.get()?)
                    .on_change(on_rich.clone())
                    .node()
                    .key(BusinessKey::new("rich-document")),
            ]),
            7 => Node::column([
                Label::new("Icon / Image smoke (curated Icon + SVG Image; full raster: show_image)")
                    .node()
                    .key(BusinessKey::new("media-title")),
                Icon::new("home")
                    .size(28.0)
                    .tooltip("Home (curated short name)")
                    .node()
                    .key(BusinessKey::new("icon-home")),
                Icon::new("material-symbols:search")
                    .size(28.0)
                    .tooltip("Search")
                    .node()
                    .key(BusinessKey::new("icon-search")),
                Icon::new("circle")
                    .size(28.0)
                    .tooltip("Circle glyph")
                    .node()
                    .key(BusinessKey::new("icon")),
                Image::new(IMAGE_ID)
                    .size(160.0, 108.0)
                    .node()
                    .key(BusinessKey::new("image")),
                Image::new(SVG_IMAGE_ID)
                    .size(48.0, 48.0)
                    .node()
                    .key(BusinessKey::new("image-svg")),
            ]),
            8 => Node::column([
                Label::new("Animation (prelude Lottie / MorphingShape)")
                    .node()
                    .key(BusinessKey::new("animation-title")),
                Lottie::from_asset(LOTTIE_ID)
                    .size(96.0, 96.0)
                    .playing(true)
                    .node()
                    .key(BusinessKey::new("lottie")),
                MorphingShape::new()
                    .size(96.0)
                    .playing(true)
                    .node()
                    .key(BusinessKey::new("morphing-shape")),
            ]),
            _ => Node::column([
                Label::new("Sticky / Scroll smoke (full: style_sticky_demo)")
                    .node()
                    .key(BusinessKey::new("sticky-title")),
                Styled::sticky_top(0.0, Label::new("Sticky bar").node())
                    .node()
                    .key(BusinessKey::new("sticky-header")),
                Label::new("Sticky filler 0")
                    .node()
                    .key(BusinessKey::new("sticky-row:0")),
                Label::new("Sticky filler 1")
                    .node()
                    .key(BusinessKey::new("sticky-row:1")),
                Label::new("Sticky filler 2")
                    .node()
                    .key(BusinessKey::new("sticky-row:2")),
                Label::new("Sticky filler 3")
                    .node()
                    .key(BusinessKey::new("sticky-row:3")),
                Label::new("Sticky filler 4")
                    .node()
                    .key(BusinessKey::new("sticky-row:4")),
                Label::new("Sticky filler 5")
                    .node()
                    .key(BusinessKey::new("sticky-row:5")),
                Label::new(format!(
                    "Style diagnostic kept last good style (ok={})",
                    invalid_style.is_diagnostic_success()
                ))
                .node()
                .key(BusinessKey::new("style-diagnostic")),
                Label::new("Fixed badge stays top-right (position:fixed).")
                    .node()
                    .key(BusinessKey::new("fixed-note")),
            ]),
        };

        let drawer_items: Vec<Node> = CATEGORIES
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let select_category = select_category.clone();
                let label = if i == cat {
                    format!("▸ {name}")
                } else {
                    (*name).to_string()
                };
                Button::new(label)
                    .on_click(move |event: EventContext| {
                        let _ = select_category.invoke(event.app(), i);
                    })
                    .node()
                    .key(BusinessKey::new(format!("cat:{i}")))
            })
            .collect();

        // MenuBar/AppBar fixed; Scroll shows one category. Overlays outside Scroll.
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
                Menu::new("Help", [MenuItem::new("about", "About SynUI")]),
            ])
            .on_action(on_menu.clone())
            .node()
            .key(BusinessKey::new("menu-bar")),
            AppBar::new(format!("Facade gallery — {cat_name}"))
                .trailing([
                    Button::new("Sections")
                        .on_press(toggle_drawer.clone())
                        .node(),
                    Button::new("Sheet").on_press(toggle_sheet.clone()).node(),
                    Button::new("Snack").on_press(show_snack_action.clone()).node(),
                    Button::new("About").on_press(open_dialog.clone()).node(),
                ])
                .node()
                .key(BusinessKey::new("app-bar")),
            Scroll::new([
                Label::new(format!("SynUI v0.2 · {cat_name}"))
                    .node()
                    .key(BusinessKey::new("heading")),
                section.key(BusinessKey::new(format!("section:{cat}"))),
            ])
            .node()
            .key(BusinessKey::new("gallery-scroll")),
            Styled::new(
                "position:fixed;top:16;right:16;z-index:20;padding:8;background:primary;border-radius:8",
                [Label::new("Fixed").node()],
            )
            .node()
            .key(BusinessKey::new("fixed-badge")),
            NavigationDrawer::new(drawer_items)
                .open(show_drawer)
                .on_open_change(on_drawer.clone())
                .node()
                .key(BusinessKey::new("drawer")),
            Dialog::new(
                "About facade gallery",
                [
                    Label::new(
                        "Open AppBar «Sections» (Drawer) to switch categories — same chrome pattern as the old Engine gallery.",
                    )
                    .wrap(true)
                    .node(),
                    Button::new("Close").on_press(dismiss_dialog.clone()).node(),
                ],
            )
            .open(show_dialog)
            .on_dismiss(dismiss_dialog.clone())
            .node()
            .key(BusinessKey::new("dialog")),
            BottomSheet::new([
                Label::new("BottomSheet smoke — dismiss via Close or scrim.").node(),
                Button::new("Close").on_press(on_sheet.clone()).node(),
            ])
            .open(show_sheet)
            .on_dismiss(on_sheet.clone())
            .node()
            .key(BusinessKey::new("bottom-sheet")),
            SnackBar::new("SnackBar smoke — dismiss via Action / overlay")
                .visible(show_snack)
                .on_dismiss(hide_snack.clone())
                .node()
                .key(BusinessKey::new("snack-bar")),
        ]))
    })
}
