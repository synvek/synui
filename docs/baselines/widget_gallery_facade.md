# `widget_gallery_facade` behavior baseline

## Stable owner

The facade gallery owns the stable gallery smoke subset, organized by **AppBar + NavigationDrawer category switching** (Basic / Inputs / Progress / Surfaces / Navigation / Data / Rich / Media / Animation / Style): App-scoped state, typed actions, controlled inputs (including Switch / Radio / TextArea), `LinearProgress` / `CircularProgress` (determinate arc), `Card` / `ListItem`, `BusinessKey` identity, MenuBar/ContextMenu, TabBar, VirtualList, VirtualTree, SplitPane, DataTable, RichDocument, Icon, Image (`Window::with_image`), Label, AppBar, NavigationDrawer, BottomNavigation, Dialog, BottomSheet, SnackBar, Scroll body for the active category with `Styled::sticky_top` + fixed badge smoke, prelude `Lottie` / `MorphingShape` smoke (`Window::with_lottie` + `from_asset`, `playing(true)` while the Animation category is mounted), Dialog/BottomSheet action buttons centered, and non-panicking style diagnostics.

See [`docs/gallery_facade_vs_engine.md`](../gallery_facade_vs_engine.md).

## Explicitly excluded behavior

Legacy `View`/`Signal` scheduling, Engine render hooks, plugins, and JSON View creation are not part of this facade baseline. The Engine gold archive was deleted in Phase 52. Full sticky/z-index overlap suite and StyleCache re-check remain owned by `style_sticky_demo`. Lottie/Morph are prelude (`use synui::prelude::*`).

## Commands

```bash
cargo check -p widget_gallery_facade
cargo test -p synui --lib facade_menu_bar_and_context_menu_mount_and_dispatch_actions
cargo test -p synui --lib facade_label_mounts_native_label_with_a11y_name
cargo test -p synui --lib facade_switch_radio_and_linear_progress_mount_native_hosts
cargo test -p synui --lib facade_card_list_item_and_circular_progress_mount_native_hosts
cargo test -p synui --lib facade_text_area_mounts_and_polls_controlled_changes
cargo test -p synui --lib facade_lottie_and_morphing_shape_mount_native_hosts
cargo test -p synui --lib facade_text_input_and_area_focus_protects_controlled_edit
cargo test -p synui --lib facade_number_input_drag_grabs_pointer
python3 scripts/migration_gate.py --facade-regression
python3 scripts/migration_manifest.py --check
```
