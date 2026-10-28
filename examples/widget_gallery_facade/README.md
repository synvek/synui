# Stable facade widget gallery

`widget_gallery_facade` is the **facade-only** gallery for SynUI v0.2. Open AppBar **Sections** (NavigationDrawer) to switch categories (Basic / Inputs / Progress / …) — same chrome pattern as the old Engine gallery. It demonstrates documented `synui::prelude::*` contracts:

- App-owned `State` and typed `Action` updates;
- controlled `TextInput`, `TextArea`, `Checkbox`, `Switch`, `Radio`, `NumberInput`, and `Select` inputs;
- `LinearProgress` / `CircularProgress` display smoke (determinate arc; avoid indeterminate in this dense tree);
- `Card` / `ListItem` surface smoke (Phase 54);
- MenuBar / ContextMenu, TabBar, VirtualList smoke;
- DataTable / RichDocument / Icon / AppBar / Dialog smoke;
- VirtualTree / SplitPane / Image (`Window::with_image`) / Label smoke (Phase 48);
- body content in prelude `Scroll` (MenuBar/AppBar/overlays stay outside);
- `Styled::sticky_top` + fixed badge smoke (Phase 55); full sticky suite remains `style_sticky_demo`;
- prelude `Lottie` / `MorphingShape` smoke via `Window::with_lottie` + `Lottie::from_asset` with `playing(true)` while Animation is mounted (Phase 57/59);
- Dialog / BottomSheet Close actions horizontally centered (Phase 59);
- non-panicking `StyleCache`/`StyleInput` validation and `StyleDiagnostic` retention.

Capability matrix vs Engine gallery: [`docs/gallery_facade_vs_engine.md`](../../docs/gallery_facade_vs_engine.md).

It deliberately does not import or enable `legacy`, Engine crates, plugins, JSON View creation, global registries, or numeric identity/action scheduling. Full interactive surfaces remain in dedicated demos (`table_demo`, `tree_list_demo`, `data_browser_demo`, `rich_*`, `navigation_demo`, `show_image`, `style_sticky_demo`, …).

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
cargo test -p synui --test counter_prelude_fixture
cargo test -p synui --lib style::tests
python3 scripts/migration_manifest.py --check
```
