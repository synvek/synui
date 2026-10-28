# Widget Guide

Widgets implement the `Widget` trait from `synui-core`. Paint resources (`TextRenderer`, `IconCache`, `ImageCache`) are injected via `PaintCtx` from the application — never allocate them per widget per frame.

## Basic widgets

- `Label` — static text
- `Button` — click handler with MD3 ripple
- `Icon` — curated Material Symbols via `IconCache` (short names or `material-symbols:*`; see [Icons](icons.md))
- `Image` — raster or SVG via shared `ImageCache` (`Window::with_image`); SVG stays crisp at any size and tints `currentColor` with theme `on_surface`
- `TextField` — single-line input with IME support

## Input widgets

- `Checkbox`, `Radio`, `Switch`, `Slider`
- `Dropdown` — expandable option list with keyboard navigation

## Navigation

- `AppBar`, `TabBar`, `BottomNavigation`, `NavigationDrawer`, `ListItem`

## Containers

- `Flex`, `Card`, `Dialog`, `BottomSheet`, `SnackBar`, `ModalHost`
- `SplitPane` — horizontal/vertical drag splitter; nestable fixed layout (`View::split`)

## Data views

- `ListView`, `TreeView`, `DataTable` — virtualized via `VirtualMountHost`; DataTable is `A11yRole::Grid`; merge spans via `with_spans`; see [Declarative](declarative.md)
- `ScrollView` — `with_transparent_virt` / `virtual_scroll`; nested scroll chaining at edges; `sync_virtual_mounts`
- **Facade (Phase 43):** `synui::prelude::{Scroll, VirtualList, Styled}` mount the same hosts through `EngineAdapter` (demos: `list_demo`, `todo_list`, `style_sticky_demo`)
- **Facade (Phase 44):** `synui::prelude::{DataTable, VirtualTree, SplitPane, TabBar}` (demos: `table_demo`, `tree_list_demo`, `data_browser_demo`)
- **Facade (Phase 45):** `RichDocument`, `AppBar`/`NavigationDrawer`/`BottomNavigation`, `Dialog`/`BottomSheet`/`SnackBar`, `Icon`/`Image` (demos: `rich_*`, `navigation_demo`, `show_image`)
- **Facade (Phase 46):** in-app `MenuBar`/`ContextMenu`/`Menu`/`MenuItem` (demos: `list_demo`, `tree_list_demo`, `widget_gallery_facade`); `DesktopMenu`/muda remains platform shell
- Demo: `cargo run -p data_browser_demo` / `cargo run -p widget_gallery_facade` / `cargo run -p list_demo`

## Examples

```bash
cargo run -p counter
cargo run -p todo_list
cargo run -p widget_gallery_facade
```

Source: [`crates/synui-widgets/src/`](../../../crates/synui-widgets/src/)
