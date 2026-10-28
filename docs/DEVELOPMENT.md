# SynUI Development Guide

## Architecture

SynUI uses a **retained widget** core (`synui-core`) with an optional **declarative** layer (`synui::View`). The per-frame pipeline:

1. Winit events → `WidgetStore::dispatch_event` (overlay layers hit-tested first)
2. Taffy layout via `synui-layout::LayoutEngine`
3. Vello scene paint via `WidgetStore::paint_all` (main tree + overlay stack)
4. GPU present via `synui-render::VelloBackend`

Default application API is the **v0.2 facade** (`synui::prelude`). Ordinary example evidence is `examples/*` only (Phase 52 deleted the in-tree gold archive). See [examples_facade_gap_and_next_steps.md](examples_facade_gap_and_next_steps.md).

## Running examples

```bash
cargo run -p hello_triangle   # Phase 0: Vello + Winit smoke test
cargo run -p counter            # Declarative counter with Signal state
cargo run -p todo_list          # App + forms + File menu (Open…)
cargo run -p navigation_demo    # Typed Router: list → detail → settings
cargo run -p show_image         # Image widget demo
cargo run -p widget_gallery_facade  # Facade gallery smoke (prelude Elements)
cargo run -p mobile_hello       # Cross-platform hello (desktop today; mobile features optional)
cargo run -p web_hello          # Interactive web counter + DomA11y
```

### mobile_hello

Minimal declarative counter used to validate the Phase 7 mobile lifecycle wiring.

```bash
# Desktop (default path — same winit event loop as other examples)
cargo run -p mobile_hello

# Compile against the mobile feature matrix (needs Android NDK / iOS toolchain for full link):
cargo check -p mobile_hello --features mobile
# cargo check -p mobile_hello --target aarch64-linux-android --features mobile
# cargo check -p mobile_hello --target aarch64-apple-ios --features mobile
```

With `--features mobile`, `synui-winit` selects `AndroidLifecycle` / `IosLifecycle` on those targets (safe-area insets + soft-keyboard flags). On desktop hosts the binary still runs via `DesktopLifecycle`.

### web_hello

```bash
cargo run -p web_hello                 # native DomA11y smoke
./examples/web_hello/build-web.sh      # wasm-bindgen → www/pkg/
cd examples/web_hello/www && python3 -m http.server 8080
# open http://127.0.0.1:8080/ — click +1

# Playwright browser E2E (CI uses the same path after build-web.sh):
./examples/web_hello/run-e2e.sh
# or: cd examples/web_hello/e2e && npm install && npx playwright install chromium && npm test
```

`cargo check --target wasm32` alone does **not** produce a browser-ready package. See `examples/web_hello/README.md`.

### mobile targets

Ordinary mobile evidence is `mobile_hello` (see section above). Engine host templates were removed with the gold archive (Phase 52).

### App / Router / forms

```bash
cargo run -p navigation_demo
cargo run -p todo_list   # File → Open… loads tasks; empty Add shows validation error
```

See book: `router.md`, `forms.md`, `desktop_shell.md`.

## Overlay architecture

Modal dialogs, dropdowns, and bottom sheets use the overlay stack in `WidgetStore`:

- `push_overlay(root, OverlayConfig::modal(), focus_trap)` — pushes a layer with scrim and optional focus trap
- `pop_overlay()` — dismisses the top layer (also triggered by Escape / backdrop click)
- `FocusManager::set_focus_trap` — restricts Tab navigation to overlay widgets

`ModalHost` fills the viewport and centers dialog content. `PopupAnchor` in `synui-layout` positions popups relative to anchor bounds with viewport clamping.

## Signal state

Reactive state for declarative apps:

```rust
use synui::{Signal, View, run_app_signal};

let count = Signal::new(0);
run_app_signal(count.clone(), move |n| {
    View::column(vec![
        View::label(format!("{n}")).with_key(1),
        View::button("+1").with_key(2).on_press({
            let c = count.clone();
            move || c.update(|v| *v += 1)
        }),
    ])
})?;
```

`Signal::set` / `update` mark the signal dirty and schedule work: graph-registered
ids demote to `APPLY_KEYS` (Apply-only); unregistered ids (including the first
`set` before post-apply TLS sync) still schedule `REBUILD_ROOT`. Inspect with
`take_signal_schedule_stats()` / `take_frame_work()`.

## Declarative View guide

`View` variants include: `Label`, `Button`, `TextField`, `Icon`, `Checkbox`, `Switch`, `Slider`, `Image`, `Card`, `Dialog`, `ListItem`, `AppBar`, `Dropdown`, `TabBar`, `Radio`, `LinearProgress`, `BottomNavigation`, `NavigationDrawer`, `BottomSheet`, `SnackBar`, `TextArea`, `Flex`, `Column`, `Row`.

Use stable `with_key(id)` on each node so `ViewEngine` preserves widget instances across frames:

```rust
View::column(vec![
    View::label("Hello").with_key(1),
    View::button("Click").with_key(2).on_press(|| { /* ... */ }),
])
```

Structural changes (adding/removing children) trigger a minimal rebuild; prop-only changes patch existing widgets in place.

## Hot reload (desktop)

```bash
cargo build -p synui_hot_view --lib
SYNUI_HOT_RELOAD=1 cargo run -p counter --features hot_reload
```

Edit `crates/synui-hot-view/src/lib.rs` (`synui_hot_label` / `synui_hot_button`), save, and the
counter UI rebuilds from the reloaded dylib while keeping `Signal` state in-process.
Load failures keep the previous library.

## Platform features

Lifecycle in `synui-platform` (Phase 7):

- `android` → `AndroidLifecycle` (safe area + soft keyboard flags)
- `ios` → `IosLifecycle`
- `web` → `WebLifecycle` + `DomA11yTree::sync_to_dom` (wasm updates `#synui-a11y-host`)

```bash
cargo run -p mobile_hello
cargo run -p web_hello
# optional targets (need toolchains):
# cargo check -p mobile_hello --target aarch64-linux-android --features mobile
# cargo check -p web_hello --target wasm32-unknown-unknown --features web
```

`synui-winit` selects lifecycle via `cfg(target_os)` when built with `--features mobile` / `web`.

## Testing

```bash
cargo test --workspace
```

Golden PNG regression tests live in `crates/synui-testing/tests/golden_tests.rs` using `insta::assert_binary_snapshot`. CI sets `INSTA_UPDATE=no` to prevent silent snapshot drift. To refresh goldens locally after intentional visual changes: `INSTA_UPDATE=always cargo test -p synui-testing --test golden_tests`.

`synui-testing` also provides headless PNG rendering (`HeadlessRenderer::render_to_png`), layout validation, input simulation (`simulate_tap`, `simulate_key`), overlay integration tests, and `proptest_layout` random Flex trees.

## Validation Matrix

`scripts/validation_matrix.py` runs named workloads independently, continues after a failed item, writes a JSON report, and returns failure only after every selected item has been attributed. List or select workloads with:

```bash
python scripts/validation_matrix.py --list
python scripts/validation_matrix.py --only counter,a11y
python scripts/validation_matrix.py --only virtual-table,rich-text,headless-render
```

The full matrix includes facade Counter, the bounded 100,000-row table, retained rich text, headless rendering, native C/C++ Production ABI smoke, ABI-only Python fixtures, and the facade-to-AccessKit bridge. Python workloads require `python -m pip install ./sdk/python`, `cargo build -p synui-abi`, and may override `--abi-library`. CI runs Counter and a11y on Windows/macOS/Linux and uploads each independently attributed JSON report.

## Benchmarks

```bash
# Retained Engine layout/paint baselines
cargo bench -p synui-bench --bench layout_paint -- --sample-size 10
# Curated v0.2 facade workloads
cargo bench -p synui-bench --bench facade_api -- --sample-size 10
# Transitional v0.1 declarative diff baseline
cargo bench -p synui-bench --features legacy-bench --bench declarative_diff -- --sample-size 10
# Comparable v1 gate (use the manifest machine class only on matching hardware)
cargo run --release -p synui-bench --bin bench_gate -- \
  --machine-class github-ubuntu-x64-rust-1.88-release --binary target/release/counter
```

The strongly typed v1 scenario manifest is `synui_bench::BENCHMARK_MANIFEST_V1`. It records deterministic input, metric, baseline, machine class, and threshold for application startup, idle resources, State update latency, 100,000-row virtual-table scrolling, and Counter binary size. Measurements from other machine classes are reported but do not gate.

| Benchmark | Description |
|-----------|-------------|
| `facade_app_startup` | `App::new`/drop through the curated facade |
| `facade_idle_resources_32_state_8_action` | One idle Context with deterministic State/Action resources |
| `facade_state_update` | Typed `State<u64>` set/get latency |
| `facade_virtual_table_scroll_100k` | Bounded viewport/prefetch requests over 100k rows |
| `layout_1000_labels` | Taffy layout for 1k `Label` leaves |
| `layout_10k_labels` | Taffy layout for 10k labels (Req 4.1 baseline) |
| `layout_10k_incremental_relayout` | Warm incremental path (structure fixed) |
| `paint_1000_labels` | Vello scene build for 1k labels (post-layout) |
| `declarative_add_item_keyed_diff` | 50-step list growth with keyed diff |

### Phase 5 baseline (Apple Silicon, 2026-07-20)

| Benchmark | Mean (approx.) | Notes |
|-----------|----------------|-------|
| `layout_1000_labels` | **~23 ms** | |
| `layout_10k_labels` | **~64 ms** | Above Req 4.1 (≤8 ms); full Taffy rebuild + measure |
| `paint_1000_labels` | **~2.3 ms** | |

### Phase 6 layout notes

- `needs_layout` / `needs_paint` split: Morph/Lottie/caret ticks paint without relayout
- Leaf measure cache: clean leaves reuse `LayoutCache` sizes across computes
- Gate A target: `layout_10k` ≤ **20 ms**; Gate B: ≤ **8 ms** on steady/pooled rebuild (formal definition in Phase 10/11 — not cold first-build)
- Re-run benches after layout changes and paste means below:

| Benchmark | Mean (Phase 6) | Notes |
|-----------|----------------|-------|
| `layout_10k_labels` | **~42 ms** | Improved vs Phase 5 ~64 ms (~35%); Gate A (≤20 ms) not yet met — remaining cost is full Taffy tree rebuild per compute |

### Phase 7 layout notes

- Incremental Taffy: reuse nodes when `structure_version` unchanged; dirty leaves `set_style` + `mark_dirty`
- ASCII unwrapped measure fast-path in `TextRenderer` (paint still Parley)
- Feature `layout_parity` on `synui-layout`: debug assert incremental vs full rebuild

| Benchmark | Mean (Phase 7, Apple Silicon) | Notes |
|-----------|-------------------------------|-------|
| `layout_10k_labels` | **~24.4 ms** | vs Phase 6 ~42 ms; Gate A (≤20 ms) **not met** on cold full build — remaining cost is Taffy `new_leaf`/`compute_layout` + store insert |
| `layout_10k_incremental_relayout` | **~1.8 ms** | Warm path (structure fixed) well under Gate A; meets Gate B steady-rebuild target |

**Gate B (≤8 ms cold):** Not claimed. Flamegraph focus: Taffy tree construction dominates cold path; incremental relayout already ≤2 ms. Further cold wins need subtree Taffy reuse across structure-preserving rebuilds or fewer nodes.

### Phase 8 layout notes (Gate A)

- Cold path: `widget_to_node` HashMap reserve via `widget_count()`; child node `Vec` pre-capacity
- Split bench: `layout_10k_labels` (insert+layout) vs `layout_10k_labels_pure` (layout only on existing tree)

| Benchmark | Mean (Phase 8, Apple Silicon) | Notes |
|-----------|------------------------------|-------|
| `layout_10k_labels` | **~26.3 ms** | Cold insert+layout; Gate A ≤20 ms **not met** |
| `layout_10k_labels_pure` | **~25.6 ms** | Excludes `store.insert`; still Taffy-dominated |
| `layout_10k_incremental_relayout` | **~2.0 ms** | Warm path under Gate A; meets Gate B steady-rebuild target |

**Gate A verdict:** Cold floor ≈ **25–26 ms**. Bottleneck = **Taffy `new_leaf` ×10k + `compute_layout` + leaf measure** (store insert is only ~0.7 ms). HashMap reserve / child Vec capacity are minor. Reaching ≤20 ms needs pooled Taffy nodes or fewer widgets — not claimed for Phase 8.

### Phase 9 layout notes (node pool)

- `full_rebuild` reuses Taffy nodes when DFS topology (widget id order) matches (`try_pooled_rebuild`)
- Bench: `layout_10k_pooled_rebuild` — structure-dirty rebuild on same engine

| Benchmark | Mean (Phase 9, Apple Silicon) | Notes |
|-----------|------------------------------|-------|
| `layout_10k_labels_pure` | **~23.5 ms** | Cold first-build; still above ≤20 ms |
| `layout_10k_pooled_rebuild` | **~2.9 ms** | Topology-stable structure-dirty rebuild — **Gate A met** |
| `layout_10k_incremental_relayout` | **~1.8 ms** | Warm path |

**Gate A (Phase 9):** Achieved on **`layout_10k_pooled_rebuild` (~2.9 ms)**. Cold first-build floor ≈23.5 ms remains Taffy `new_leaf`×N; apps that keep engine + stable topology hit Gate A. Gate B ≤8ms cold first-build still not claimed.
Idle `CursorMoved` does not force layout. Use `mark_paint_dirty` for animation frames.

### Phase 10 notes (multi-window, insets, Gate B, wasm)

- **Multi-window:** Each `winit` window owns a Vello surface; `Application::open_extra_window` / File → **New Window** (`menu_id::NEW_WINDOW`) paints the same UI on every window. Tray **Show** / **Quit** via `AppTray::install_with_menu()`.
- **Insets → layout:** Safe-area insets from `Application::set_safe_area_insets` (Android `content_rect`, iOS UIKit hook) feed `WidgetStore::safe_area()` and layout padding — ordinary mobile path: `mobile_hello`.
- **Gate B (formal):** Req 4.1 **≤8 ms** applies to the **pooled / steady rebuild path** (`layout_10k_pooled_rebuild`, ~**2.9 ms** on Apple Silicon). The **cold first-build** benchmark `layout_10k_labels_pure` (~**23.5 ms**) is **not** claimed as ≤8 ms; it measures Taffy `new_leaf`×N on a fresh tree.
- **Web wasm size:** Run `./examples/web_hello/build-web.sh` and inspect `examples/web_hello/www/pkg/web_hello_bg.wasm`. Paste release size here after local builds:

| Artifact | Size (release, wasm32) | Notes |
|----------|------------------------|-------|
| `web_hello_bg.wasm` | **~12e6** bytes | CI gate `MAX=15000000` after `build-web.sh` (hybrid/wgpu29 off on wasm32) |

### Phase 11 notes (mobile hosts, Req 2.9 gold, plugins, History)

- **iOS / Android mobile hosts:** Historical Engine host packages were deleted in Phase 52. Ordinary mobile evidence is `mobile_hello` with `--features mobile` (safe-area insets + soft-keyboard flags via `synui-winit` lifecycle).
- **Req 2.9 backend gold:** `HeadlessRenderer::with_cpu(width, height, use_cpu)` — `false` = Gpu, `true` = Cpu. `backend_gold_tests.rs`: solid fills assert per-channel ≤ **1** (1/256); text/AA widgets ≤ **64** (Vello path AA divergence). Hybrid scene-bridge gold landed in Phase 12 (see below).
- **Plugins:** `cargo build -p synui_plugin_sample --lib` → cdylib with `synui_plugin_label`; `load_plugins_from_index` **hard-fails** on missing library. Use `load_plugins_from_index_opts(path, true)` only for docs/tests (`allow_stub`).
- **Hot reload:** `synui_create_view` descriptor (`kind=…;title=…;button=…`) drives `hot_counter_view` in `counter` — no manual `kind` branch. Kind 2 / JSON trees: see Phase 13.
- **History:** wasm `navigation_demo` enables `with_history_sync`; `popstate` queues path → `Router::sync_pending_history` / `take_pending_history_path` → `apply_path_segment`.
- **Linux menu:** `DesktopMenu::attach_gtk_hint` documents GTK `init_for_gtk_window` wiring; pure winit still uses `on_platform_fallback("menu")`.
- **Gate B (reconciled):** Req 4.1 **≤8 ms** applies only to the **pooled steady rebuild** path (`layout_10k_pooled_rebuild`, ~2.9 ms). Cold first-build (`layout_10k_labels_pure`, ~23.5 ms) is **not** claimed ≤8 ms — earlier Phase 6–8 one-liners that implied cold ≤8 ms are superseded by this definition.

### Phase 12 notes (Style / CSS subset, Hybrid gold, iOS UIKit)

- **Style:** `synui_core::Style` = `LayoutStyle` (Taffy) + `VisualStyle` (paint). `Style::from_css` parses a CSS **subset** (box/flex/visual); unknown properties **Err**. Declarative: `View::style` / `.style_css` wraps in `StyledBox`. Column/Row default padding is **0** (use style). See [book/style.md](book/src/style.md) and `examples/style_demo`.
- **Sizing priority:** explicit `width`/`height` in style wins; leaves still measure when `Auto`.
- **Hot descriptor:** `kind=…;title=…;button=…;style="padding:16;gap:8"` — style must be quoted; invalid CSS fails load.
- **Plugins:** optional `synui_plugin_style()`; `plugin_styled_panel` + gallery hard-load (`load_plugins_from_index`, stub fallback).
- **Req 2.9:** Gpu↔Cpu solid + **StyledBox** gold (tol≤1); Hybrid = `RenderBackendKind::Hybrid` + `replay_vello_scene` into `vello_hybrid::Scene` (windowless). Full Hybrid wgpu present remains surface-bound.
- **iOS:** `read_ios_safe_area` reads UIKit `safeAreaInsets` via objc2; fallback `[59,0,34,0]` only when UIApplication/windows unavailable.
- **Non-goals (historical):** see Phase 15+ / 16+ tables below.

### Phase 13 notes (Style scaffold close-out)

- **Grid:** `grid-template-columns/rows` with explicit `px`/`fr`/`auto`; `display:grid` without templates **Err**.
- **TextArea:** pointer select, IME, caret, Enter — aligned with TextField (winit IME path).
- **Nested create_view:** JSON when descriptor starts with `{`; `synui_plugin_create_view` + `register_view_factory` / `plugin_view_page`.
- **Web E2E:** `./examples/web_hello/run-e2e.sh` (Playwright DomA11y +1). CI after `build-web.sh`. Hybrid wgpu29 is **off** on wasm32 so wasm-bindgen succeeds.
- **Absolute:** `position:relative|absolute` + inset px → Taffy; dropdown/sheet/dialog use `OverlayPlacement`.

### Phase 14 notes (editing maturity + RichEdit)

- **EditCore:** shared plain-text buffer/selection/IME helpers for TextField, TextArea, RichEdit (`synui-widgets::edit_core`).
- **Clipboard / undo:** Cmd/Ctrl+C/X/V and Z/Y (desktop `arboard`; wasm process-local fallback). `EditHistory` snapshots (max 64).
- **Hit-test:** Parley `hit_test_xy` / `caret_rect` / `selection_rects` (no 8px grid). TextArea ArrowUp/Down + scroll.
- **RichDoc / RichEdit:** spans for bold/italic/underline; `to_json`/`from_json`; gallery Input + `examples/rich_edit_demo`.
- **View keys:** `alloc_view_key()` starts at 1_000_000; embed plugin trees with `rekey_from` / `plugin_view_page`.
- **create_view JSON:** also `text_field`, `text_area`, `rich_edit`.

### Phase 15 notes (text/layout harden + data views)

- **EditCore unification:** RichEdit composes EditCore + RichDoc; shared IME commit/undo; word nav; Parley `ime_cursor_area` on TF/TA/RichEdit.
- **Measure:** Taffy leaf measure passes available width → `MeasureCtx.max_width`; Label measure/paint share wrap width.
- **Flex:** transparent by default — use `background:surface` / Card / StyledBox for fills.
- **TextRenderer:** `with_text_renderer` shared path (no per-hit `TextRenderer::new()`).
- **Bindings:** focus-protected patch/push; `TextAreaBinding`; debug key→live widget assert.
- **ListView:** fixed-height virtualization + overscan; `View::list_view` / JSON; 10k painted-row bound test.
- **Menus:** in-app `MenuBar` / `ContextMenu` (overlay; muda may coexist). Gallery List section + `examples/list_demo`.
- **Non-goals (then):** CSS cascade / sticky / fixed / z-index; DataTable; HTML/Markdown full interchange; RichEdit block lists/tables/images; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms claim.

### Phase 16 notes (composable data views + menu usability)

- **TextLayoutCache:** keyed Parley layout reuse (`text_hash`, font size, max width, runs fingerprint); TF/TA/RichEdit/Label hot paths.
- **ListView v2:** viewport ± overscan mount via `item_builder`; recycle; fixed `item_extent` fast path; variable-height MVP (`extent_fn` + prefix sums). Sticky headers → Phase 17.
- **TreeView:** flatten visible nodes, expand/collapse, keyboard ←→; `View::tree_view` / JSON `tree_view`.
- **Menus:** MenuBar F10/Alt + ←→/↑↓/Enter/Esc; ContextMenu keyboard; panels on overlay stack (Dropdown-aligned).
- **Bindings:** `SelectionBinding` / `TreeSelectionBinding`; gallery Tree section + `examples/tree_list_demo`.
- **Non-goals (Phase 17+):** DataTable (columns/sort/cell edit/horizontal virt); CSS cascade / full sticky-fixed-z; list sticky headers; RichEdit blocks; GTK `init_for_gtk_window` menus; plugin CDN; cold `layout_10k` ≤8ms claim; ListView multi-select / drag-reorder.

### Phase 17 notes (tabular data views + sticky headers)

- **VirtualViewport:** shared fixed/variable-height scroll math + `layout_row_subtree` for mounted rows.
- **ListView:** sticky header (widget-level); measured variable height (`with_measure_heights`); store-aware `item_builder`; row Taffy after layout.
- **TreeView v2:** `item_builder` / mount via `sync_tree_views`; chevron column alignment retained.
- **DataTable:** columns, sticky header, click-sort, vertical virt + horizontal clip, cell edit MVP; `View::data_table` / JSON.
- **Demos:** `examples/table_demo`, gallery Table section.
- **Non-goals (Phase 18+):** CSS cascade / full sticky-fixed-z engine; RichEdit block lists/tables/images; List/Table multi-select & column drag-reorder; merge cells / freeze multi-column; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms; ScrollView full virtualization; SplitPane/dock.

### Phase 18 notes (desktop workspace shell)

- **SplitPane:** horizontal/vertical drag divider, min sizes, ←→/↑↓ nudge; `View::split` / JSON `split` (`text`: `h|0.35` or `v|0.4`).
- **Declarative hooks:** `ViewEngine::set_tree_item_builder`, `set_list_measure_heights`; `View::with_measure_heights`.
- **Stable key pool:** List/Tree/Table scroll-out recycles by index / node id / `(data_row,col)` instead of destroy (cap-bounded).
- **Gallery:** List Switch rows, Tree mounted labels; sticky **header** wording (not sticky columns).
- **Demo:** `examples/data_browser_demo` — Split(Tree, DataTable) master-detail + SelectionBinding.
- **Non-goals (Phase 19+):** CSS cascade / full sticky-fixed-z engine; RichEdit block lists/tables/images; List/Table multi-select & column drag-reorder; merge cells / freeze multi-column; ScrollView full virtualization; multi-window dock / floating panes; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 19 notes (CSS stacking + data grid MVP)

- **CSS subset:** `position:sticky|fixed`, `z-index` (finite stacking within parents); sticky clamps to nearest clip ancestor; fixed uses viewport insets; overlay stack still paints above CSS z.
- **DataTable:** `freeze_leading(n)` — left N columns ignore `scroll_x`; `View::with_freeze_leading`.
- **Multi-select:** ListView / DataTable `HashSet` selection; Shift range + Ctrl/Cmd toggle; `SelectionBinding` is `Signal<HashSet<usize>>` (breaking).
- **Demos:** `examples/style_sticky_demo`; browser/table freeze + multi-select; gallery Style sticky strip.
- **Non-goals (Phase 20+):** full cascade/selectors; horizontal sticky; RichEdit blocks; column drag-reorder / merge cells; ScrollView virtualization; multi-window dock; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 20 notes (RichEdit blocks + ScrollView virtual scroll MVP)

- **BlockDoc:** flat `Paragraph` / `Heading` / `BulletItem` / `OrderedItem` / `Image`; `InlineBlock` = `RichDoc` spans; migrate flat `RichDoc` via `\n` split.
- **RichEdit:** focus `(block_index, offset)`; Enter split / Backspace merge / Tab indent; Ctrl+B/I/U; `EditHistory<BlockDoc>`; `RichBinding` → `Signal<BlockDoc>` (breaking).
- **Markdown subset:** `#`/`##`/`###`, `- `/`1. `, `**bold**`/`*italic*`, `![](id)` import/export (not full CommonMark/HTML/tables).
- **virtual_scroll:** `View::virtual_scroll(slot_count, extent)` + `ViewEngine::set_scroll_slot_builder`; `sync_virtual_scroll_views` after sticky; fixed slot height; 10k → ≤~40 mounts; slot 0 always pinned (CSS sticky on other virt slots not guaranteed).
- **Demos:** `examples/rich_blocks_demo`; gallery Input blocks + Style virtual_scroll.
- **Non-goals (Phase 21+):** full CSS cascade/selectors/inheritance; column drag-reorder / merge cells; RichEdit nested editable tables / HTML / OT/CRDT; ScrollView transparent virt of arbitrary flex/grid; nested virt; variable-height scroll slots; multi-window dock; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 21 notes (DataTable columns + variable virtual_scroll)

- **DataTable:** header drag-reorder (freeze zone locked); column edge resize (min 40, double-click auto-fit); short-press still sorts; `data_col_of` keeps cell data indices stable; `ColumnLayout` / `ColumnLayoutBinding` / `ViewEngine::table_column_layout`.
- **virtual_scroll v2:** `set_slot_extent_fn` / `set_slot_heights` / `enable_measure_heights`; prefix-sum geometry via `VirtualViewport`; 10k mount ≤~40; slot 0 pinned.
- **Demos:** `data_browser_demo` column drag/resize; gallery Style variable virt + Table tips.
- **Non-goals (Phase 22+):** full CSS cascade/selectors/inheritance/`!important`; merge cells; ScrollView transparent virt of arbitrary flex/grid; nested virt; multi-window dock/floating panes; RichEdit nested tables/HTML/OT/CRDT; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 22 notes (workspace shell v2 + DataTable filter)

- **Nested SplitPane:** fixed layout tree (not dock); child may be another `View::split`; `SplitRatioBinding` / `ViewEngine::split_ratio` / `take_split_ratio_dirty`.
- **TabBar workspace:** `TabBinding` drives Table ↔ Preview content; keep keyed DataTable mounted across tab switches so column layout survives.
- **DataTable filter MVP:** global `filter_query` + per-column filters; case-insensitive substring on cell text; `filtered/total` counts; `TableFilterBinding` / `ViewEngine::table_filter_counts`.
- **Demo:** `data_browser_demo` v2 — `Split(H, Tree | Split(V, TabbedMain | Detail))` + filter field.
- **Non-goals (Phase 23+):** full CSS cascade/selectors/inheritance/`!important`; merge cells; ScrollView transparent virt of arbitrary flex/grid; nested virt; floating dock / draggable tabs / cross-window panes; RichEdit nested tables/HTML/OT/CRDT; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 23 notes (transparent virt + merge cells)

- **Transparent virt:** `ScrollView::enable_transparent_virt` / `View::scroll(...).with_transparent_virt()` — flat direct children as slots; mount window + recycle; widgets stay alive (no `slot_builder`); 10k → ≤~40 mounts.
- **DataTable merge:** `CellSpan` / `set_spans` / `with_spans`; paint/hit resolve to anchor; freeze-cross rejected; multi-row holes under filter pruned; spans cleared on sort.
- **Demos:** gallery Style transparent scroll + Table merge sample.
- **Non-goals (Phase 24+):** full CSS cascade/selectors/inheritance/`!important`; ScrollView arbitrary flex/grid tree virt; nested virt; floating dock / draggable tabs / cross-window panes; RichEdit nested tables/HTML/OT/CRDT; server filter / pivot / regex; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 24 notes (virt platform v2 + virt focus/a11y)

- **VirtualMountHost:** shared mount/recycle (`reconcile_mount_window`) for Scroll/List/Table/Tree; frame entry `sync_virtual_mounts`; pooled `layout_row_subtree` engine.
- **Nested virt:** scroll chaining at edges (`scroll_chaining`, default on); inner scrollables consume wheel first.
- **Row templates:** slot_builder / transparent flat children / list item builders share the same host path; Card rows in gallery Style.
- **A11y:** `A11yRole::Grid` / `Row` / `GridCell`; List/DataTable expose `row_count`/`col_count` without O(rows) a11y nodes; DataTable arrow keys resolve merge anchors + `ensure_col_visible`; focus restore after recycle.
- **Demos:** gallery Style Card transparent virt + nested list; Table Grid keyboard tip; `virt_a11y_tests`.
- **Non-goals (Phase 25+):** full CSS cascade/selectors/inheritance/`!important`; arbitrary deep flex/grid **auto-scan** transparent virt; floating dock / draggable tabs / cross-window panes; RichEdit nested tables/HTML/OT/CRDT; DataTable server filter / pivot / regex / span-aware sort analytics; horizontal sticky engine overhaul; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 25 notes (declarative runtime + cross-block selection)

- **Stable keys:** `reset_view_key_alloc` each `App` rebuild; `View::style` / `style_css` use `stable_styled_key(child, style)`; debug duplicate-key assert.
- **DiffStats dirty split:** `needs_layout` / `needs_paint`; `App::on_frame` skips full layout for text-only patches; container patch no longer double-recurses children.
- **RichEdit:** `DocPos` / `sel_anchor` cross-block selection; Shift+Arrow / drag; `BlockDoc::delete_range` / `plain_slice`; clipboard + format across text blocks; images skipped.
- **Demos:** `counter` without mandatory `with_key`; `rich_blocks_demo` cross-block tips.
- **Non-goals (Phase 26+):** full CSS cascade/selectors/inheritance/`!important`; arbitrary deep flex/grid auto-scan virt; floating dock / draggable tabs / cross-window panes; RichEdit nested tables/HTML/OT/CRDT; DataTable server filter / pivot / regex; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms; fine-grained Signal→Widget dependency graph.

### Phase 26 notes (true incremental runtime + Action + DataTable data plane)

- **Dirty truth:** `bump_paint_epoch` vs `bump_measure_epoch`; Label/Button text patches paint-only without store layout bypass.
- **Subtree skip:** content fingerprints + `DiffStats.skipped_subtrees`; survivor detach only on parent change.
- **ActionId:** `register_action` / `View::on_action`; patchable across rebuilds.
- **Virt sync:** host registry + dirty set; no idle full-store ×4 walks.
- **DataTable:** `Arc<[Vec<String>]>` rows + `set_filter_predicate`; compare-before-mutate / Arc ptr equality.
- **RichEdit:** `block_layout` cache; single-block a11y value when selection is local.
- **Non-goals (Phase 27+):** full CSS cascade/selectors/inheritance/`!important`; fine-grained Signal→Widget dependency graph (skip `view_fn` materialization); arbitrary deep flex/grid auto-scan virt; floating dock / cross-window panes; RichEdit nested tables/HTML/OT/CRDT; DataTable server filter / pivot / regex; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms; full-scene damage regions.

### Phase 27 notes (Memo reactivity + Phase 26 debt closeout)

- **Virt dirty:** DataTable/TreeView scroll marks hosts dirty; structure insert/remove calls `mark_all_virt_dirty`.
- **Action DX:** `counter` + gallery Dark toggle use `on_action`; `reactive.md` prefers Action over `on_press`.
- **DataTable demo:** `data_browser` holds `Arc<[Vec<String>]>` rows; DiffStats-driven layout/paint dirty.
- **`View::keyed_map`:** business-id keys for dynamic lists.
- **Memo:** `Signal` epoch + `View::memo` / `take_memo_stats`; empty-deps static subtrees skip builder on unrelated ticks.
- **Non-goals (Phase 28+):** *(superseded by Phase 28 delivery of root skip; remaining items listed under Phase 28)*.

### Phase 28 notes (root auto-deps + Memo/Action debt closeout)

- **Root skip:** `DIRTY_SIGNALS` + root `BuildCtx`; dirty ∉ root deps → skip `view_fn` build+apply (`App::skipped_frames` / `DiffStats.skipped_frames`); paint/bindings still run. Debug: `force_full_rebuild(true)`.
- **Multi-Signal DX:** gallery splits `category` / `chrome` / `controls`; `data_browser` splits `data` / `status`.
- **Action/menu:** `register_action_alias` + `dispatch_action_str`; MenuBar/ContextMenu take paths dispatch aliases; gallery chrome + About menu use `ActionId`.
- **Virt register:** `ListView` / `TreeView` / `DataTable` / virtual `ScrollView` call `register_virt_host` at create (not only structure rediscover).
- **Docs:** prefer `on_action` inside Memo / under root skip — `on_press` is cleared by `clone_structure`.
- **Non-goals (Phase 29+):** *(superseded by Phase 29 delivery of paint damage MVP; remaining items listed under Phase 29)*.

### Phase 29 notes (regional paint damage + Phase 28 DX closeout)

- **Damage MVP:** `mark_paint_dirty_widget` / `mark_paint_dirty_rect`; `paint_frame` skips subtrees that miss dirty rects (`PaintStats.skipped_paint_nodes`). Unscoped `mark_paint_dirty` / layout still full-tree. Debug: `force_full_paint(true)`. Success metric is **skipped paint nodes**, not OS partial present.
- **RootSkip helper:** `RootSkipState` / `RootSkipDecision` shared by `App`, gallery, `data_browser_demo`.
- **Virt:** `ScrollView::set_transparent_virt` registers virt host at enable.
- **Action DX:** gallery category/drawer + `navigation_demo` use `on_action`.
- **Non-goals (Phase 30+):** *(superseded by Phase 30 delivery of layout dirty-subtree + damage v2; remaining items listed under Phase 30)*.

### Phase 30 notes (layout dirty-subtree + damage v2 + Phase 29 debt)

- **Layout subtree:** `mark_layout_dirty_widget` / `layout_full` / `LayoutStats.partial_layout_passes`; winit marks `LayoutEngine::mark_dirty` then `compute` when structure-stable.
- **Damage v2:** `tick_animations` returns dirty widget ids → `mark_paint_dirty_widget`; App paint-only path no longer escalates to full damage when patches already scoped.
- **Action DX:** gallery Containers dialog/sheet/snack use `on_action`.
- **Non-goals (Phase 31+):** *(superseded by Phase 31 delivery of retained-scene paint; remaining items listed under Phase 31)*.

### Phase 31 notes (retained-scene paint + Phase 30 debt)

- **PaintMode:** `Full` → `scene.reset` + `paint_all`; `Damaged` → retain scene, erase dirty rects, `paint_frame` skip clean subtrees. Winit and headless share this policy.
- **Layout→paint:** partial layout marks old+new widget bounds (scoped), not unconditional `mark_paint_dirty`; structure / `layout_full` still full paint.
- **Metric:** `PaintStats.skipped_paint_nodes` on retained paint-only frames (not OS multi-rect present).
- **Non-goals (Phase 32+):** *(superseded by Phase 32 delivery of bounded retained-scene; remaining items listed under Phase 32)*.

### Phase 32 notes (bounded retained-scene + Phase 31 debt)

- **Damage budget:** default `DAMAGED_FRAME_BUDGET = 32` consecutive Damaged presents; `prepare_damage_compact` escalates to Full (`scene.reset` + `paint_all`) then `note_paint_presented` clears the counter. Override via `set_damaged_frame_budget` or `SYNUI_DAMAGED_FRAME_BUDGET`. Shared by winit and headless.
- **Hygiene:** pointer/touch move prefers `mark_paint_dirty_widget(hit)` over unconditional Full; resize / structure / IME commit / press still Full where needed.
- **Clip honesty:** Damaged `paint_frame` clips **per dirty rect** (not AABB union) so ancestor surface fills cannot blank retained siblings in gaps between disjoint dirty regions (e.g. Morph + Lottie label, or button ripple + animation). See `damaged_paint_clips_to_dirty_union` / `damaged_paint_preserves_gap_between_disjoint_dirty`.
- **Non-goals (Phase 33+):** *(superseded by Phase 33 delivery of Damaged escalate closeout + headless host parity; remaining items listed under Phase 33)*.

### Phase 33 notes (Damaged escalate closeout + headless host parity)

- **Scroll Damaged:** `MouseWheel` → `mark_scroll_paint_dirty` (ScrollView host + viewport rect), not unconditional Full. See `scroll_paint_damage_skips_sibling`.
- **IME / keyboard:** Preedit, Commit, and caret keys → `mark_paint_dirty_widget(focused)` (Damaged); Escape-structure / overlays may still Full.
- **Headless parity:** `HeadlessRenderer::render_store` uses `sync_pre_layout` → layout → `sync_post_layout` → `sync_pre_paint` (same order as winit). Shared helpers in `synui_widgets::frame_sync`.
- **Non-goals (Phase 34+):** *(superseded by Phase 34 delivery of Signal→WidgetId graph MVP; remaining items listed under Phase 34)*.

### Phase 34 notes (Signal→WidgetId graph MVP + Phase 33 debt)

- **Keyed/Memo registry:** `View::keyed` / `View::memo` record Signal deps per view key; `ViewEngine` keeps a `SignalDepGraph` (SignalId→WidgetId).
- **Skip apply:** `apply_with_dirty` skips keyed/Memo subtrees whose registered Signals miss the drained dirty set (`DiffStats.skipped_apply_subtrees`). Fingerprint skip still applies when Signals force re-apply is false.
- **Boundaries:** MVP still requires keyed or Memo scopes — not a boundary-free auto graph.
- **Thumb scroll:** mouse press on a ScrollView host uses `mark_scroll_paint_dirty` (Phase 33 wheel parity).
- **Non-goals (Phase 35+):** *(superseded by Phase 35 delivery of CSS Style Cascade MVP; remaining items listed under Phase 35)*.

### Phase 35 notes (CSS Style Cascade MVP + Phase 34 debt)

- **Inheritance:** `Style::cascade` / `ComputedStyle` merges narrow visual tokens (`color`, `font_size`, `opacity`) parent→child before apply; layout does not inherit. Labels read inherited color/size from `PaintCtx`.
- **Selectors:** `View::class` + `ViewEngine::set_stylesheet` / `StyleSheet` — descendant `.class` rules, source order, **local `style_css` wins** over sheet over inherit. No `!important`, pseudos, `@media`, or id selectors.
- **Keyed build-skip:** `keyed_view` caches structure like Memo when Signal deps are unchanged (`take_keyed_build_stats`).
- **Non-goals (Phase 36+):** *(superseded by Phase 36 delivery of Reactive Runtime v2; remaining items listed under Phase 36)*.

### Phase 36 notes (Reactive Runtime v2 — auto Signal graph + Driver DX)

- **Auto attribution:** `Signal::get` (in a dep scope) fills pending deps; `View::with_key` drains them into `SignalDepGraph` via `merge_key_signal_deps`. Apply-skip no longer requires `View::keyed` / Memo.
- **keyed / Memo:** optional **build-skip** optimizations (`take_keyed_build_stats` / memo hits), not a hard registration boundary.
- **Driver DX:** `ReactiveFrame` shares RootSkip decide → apply + dirty flags for custom `AppDriver`s (gallery / data_browser).
- **Non-goals (Phase 37+):** *(superseded by Phase 37 delivery of Reactive Runtime v3; remaining items listed under Phase 37)*.

### Phase 37 notes (Reactive Runtime v3 — FrameWork + section + key DX)

- **FrameWork:** typed schedule flags (`REBUILD_ROOT`, `PAINT_WIDGETS`, `SYNC_BINDINGS`); `schedule_paint` / `EventCtx::request_paint` for ephemeral visuals without rebuild.
- **Dual path:** Signal → rebuild/apply; `HANDLED` / `request_paint` → paint only (selection, caret, scroll).
- **Bindings:** `auto_pull_bindings_scoped` / push skip clean keys (`take_binding_pull_stats`).
- **Section:** `View::section` skips builder when deps unchanged (`take_section_build_stats`); pending deps merge to multi-leaf `with_key` inside sections.
- **Key DX:** `key_scope` + `with_key_local` for sibling-local ids.
- **Non-goals (Phase 38+):** `!important` / full specificity / pseudos / `@media`; floating dock / cross-window panes; RichEdit nested tables/HTML/OT/CRDT; DataTable server filter / pivot / regex; deep flex/grid auto-scan virt; OS-level multi-rect partial present completeness; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms; skipping `view_fn` with **no** section/key id; IME Commit Damaged-only.

### Phase 38 notes (Reactive Runtime v4 + Paint Closeout)

- **Apply-only:** when dirty Signals are graph-covered and `View::section` builders can refresh, skip root `view_fn`, splice `last_view`, `apply_with_dirty` (`App::apply_only_frames`, `schedule_apply_keys`).
- **Paint-only:** `schedule_paint` / `PAINT_WIDGETS` frames do not call `view_fn` / apply.
- **Host branching:** `take_frame_work()` → Rebuild / Apply-only / Paint-only (gallery / data_browser / `App`).
- **IME Commit + press:** focused Commit and structure-stable press use `mark_paint_dirty_widget` (Damaged); measure bump when text length may change.
- **Non-goals (Phase 39+):** *(superseded by Phase 39 delivery of host closeout; remaining items listed under Phase 39)*.

### Phase 39 notes (Reactive Runtime v5 Host Closeout)

- **Host three-tier:** gallery / data_browser / demos align with `App::on_frame` — Rebuild / Apply-only / Paint-only via `take_frame_work`.
- **`PAINT_WIDGETS` → store:** `land_frame_paint(store, work)` after take; flag must land store paint dirty (no dual-source present).
- **Region truth:** gallery chrome / category strip / body are `View::section`s (no root `chrome.get()` / `category.get()`); category switch can demote to Apply-only (`apply_only_frames`).
- **data_browser:** status section Apply-only in Phase 39; Phase 41 extends tabs/filter/table/preview/detail sections for workspace Apply-only.
- **Demos:** prefer `take_frame_work` + `land_frame_paint` over bare `take_scheduled`.
- **Non-goals (Phase 40+):** *(superseded by Phase 40 delivery of Style Cascade v2 + theme token refresh; remaining items listed under Phase 40)*.

### Phase 40 notes (Style Cascade v2 + Theme Token Refresh)

- **Cascade:** specificity (class count), `!important`, `.a.b`, descendant `.a .b`, source order within a layer.
- **Pseudos:** `:hover` / `:focus` / `:disabled` via `StyleSheet` + `refresh_interaction_styles` (pointer hover on `AppDriver::on_hover_changed`).
- **Media:** `@media (prefers-color-scheme: dark|light)` bridged by `ViewEngine::set_color_scheme` / `PrefersColorScheme`.
- **Theme flip:** light/dark clears text layout caches and marks all widgets Damaged paint **without** `scene.reset()`; design/seed changes still Full-reset. Escape: `force_theme_full_reset` / `SYNUI_THEME_FULL_RESET`. Metrics: `take_scene_reset_count`.
- **Non-goals (Phase 41+):** *(superseded by Phase 41 delivery of Reactive Runtime v6 Schedule Truth; remaining items listed under Phase 41)*.

### Phase 41 notes (Reactive Runtime v6 Schedule Truth)

- **Set demote:** graph-covered `Signal::set` / `update` schedules `APPLY_KEYS`; unregistered ids still `REBUILD_ROOT`. Frame `try_apply_only` remains the safety net. Metrics: registered-set TLS + `apply_only_frames`.
- **data_browser:** tabs / filter / table / preview / detail are sibling `View::section`s (no root Signal reads); tab and filter paths are measurable Apply-only beyond status. Split ratios register via status section; live ratio from bindings.
- **Overlay:** open/close/move marks old+new overlay root dirty rects → `PaintMode::Damaged` (escape: `force_overlay_full_paint` / `SYNUI_OVERLAY_FULL_PAINT`).
- **Pseudos:** `:focus` subtree-symmetric with `:hover`; `AppDriver::on_focus_changed`; `Button` / `TextField` real `is_disabled()`; gallery focus/disabled samples. Light/dark `take_scene_reset_count` does not increment (`theme_change_paint_policy`).
- **Non-goals (Phase 42+):** floating dock / cross-window panes; RichEdit nested tables/HTML/OT/CRDT; DataTable server filter / pivot / regex; deep flex/grid auto-scan virt; OS-level multi-rect partial present completeness; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms; skipping `view_fn` with **no** section/key id; Style Cascade v3 (id selectors / full `@media` / container queries).

### Phase 42 notes (Reactive Runtime v7 Region Ergonomics)

- **Section Fill:** `View::section` hugs content (Auto wrap); `View::section_fill` / `section_fill_view` wraps with `height:fill;flex-grow:1;min-height:0` so nested ScrollView scrolls without pinning `with_key(section_id)`. Gallery body uses `section_fill`; regression in `gallery_media_repro`.
- **Browser region truth:** data_browser preview builds markdown from `data` ∪ selection ∪ filter Signals (no `preview_md` store side-channel). Tab Signal is omitted from TABLE/PREVIEW section deps; pane visibility is imperative via `ViewEngine::set_keyed_fill_visible` on pinned Styled hosts.
- **Disabled inputs:** Checkbox / Switch / Slider / Dropdown implement real `is_disabled()` + event gating; `View::disabled` covers them; gallery Input/Style samples.
- **Host DX:** `take_signal_schedule_stats()` counts registered→APPLY vs unregistered→REBUILD demotions; first `Signal::set` before post-apply TLS sync remains Rebuild (tested).
- **Non-goals (Phase 43+):** floating dock / cross-window panes; RichEdit nested tables/HTML/OT/CRDT; DataTable server filter / pivot / regex; deep flex/grid auto-scan virt; OS-level multi-rect partial present completeness; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms; skipping `view_fn` with **no** section/key id; Style Cascade v3 (id selectors / full `@media` / container queries); implicit builder slicing without section/key.

### Phase 43 notes (Facade Scroll + VirtualList / Wave A)

- **Prelude Elements:** `Scroll` → `ScrollView`; `VirtualList` → `ListView` (fixed extent MVP, optional `measure_heights`); `Styled` for sticky/fixed/z-index CSS-subset child metadata.
- **EngineAdapter:** `"scroll"` / `"virtual-list"` / `"styled"` mount real widgets (no empty `Flex` fallback); frame path still runs `sync_virtual_mounts` via host sync.
- **Demos restored:** `list_demo` (10k virt + select/activate; MenuBar/ContextMenu N/A), `todo_list` (`Scroll` + toggle/delete), `style_sticky_demo` (sticky bar + scroll).
- **Bound:** facade `VirtualList` 10k mount ≤~40 (`view::tests::facade_scroll_and_virtual_list_mount_native_hosts`).
- **Non-goals (Phase 44+):** VirtualTree / DataTable runtime wiring / SplitPane / TabBar / RichDocument; MenuBar/ContextMenu in prelude; floating dock; Style Cascade v3; arbitrary deep flex/grid auto-scan virt; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 44 notes (Facade Table / Tree / Split / Tab / Wave B)

- **Prelude Elements:** `DataTable` (runtime host + `TableColumn`), `VirtualTree` / `TreeNode`, `SplitPane` / `SplitDirection`, `TabBar`.
- **EngineAdapter:** `"data-table"` / `"virtual-tree"` / `"split-pane"` / `"tab-bar"` mount real widgets; poll select/sort/edit/expand/tab/ratio Actions each frame.
- **Demos restored:** `table_demo`, `tree_list_demo`, `data_browser_demo` (Preview = `Scroll`+text; RichDocument Phase 45). Menus N/A.
- **Bound:** facade DataTable 10k mount test (`facade_data_table_mounts_native_host_with_bounded_cells`).
- **Non-goals (Phase 45+):** RichDocument / rich_* true editing; AppBar/Drawer/BottomNav; MenuBar/ContextMenu in prelude; floating dock; Style Cascade v3; server filter/pivot; deep flex/grid auto-scan virt; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

### Phase 45 notes (Facade RichDocument / Nav / Overlay / Wave C)

- **Prelude Elements:** `RichDocument` (+ prelude `BlockDoc`/`Block`/`TextSpan`), `AppBar`, `NavigationDrawer`, `BottomNavigation`, `Dialog`, `BottomSheet`, `SnackBar`, `Icon`, `Image`.
- **EngineAdapter:** `"rich-document"` / `"app-bar"` / `"navigation-drawer"` / `"bottom-navigation"` / `"dialog"` / `"bottom-sheet"` / `"snack-bar"` / `"icon"` / `"image"` mount real widgets; poll doc/drawer/overlay/tab Actions.
- **Platform:** `Window::with_image(id, path)` preloads host `ImageCache` before the event loop (raster or SVG; SVG re-renders crisp and tints `currentColor` with theme `on_surface`). `Window::with_icon` preloads custom SVG into `IconCache`; curated `Icon::new("home")` / `material-symbols:*` lazy-load (see book `icons.md`).
- **Demos restored:** `rich_edit_demo`, `rich_blocks_demo`, `navigation_demo`, `show_image`; `data_browser_demo` Preview → `RichDocument`. Menus N/A.
- **Bound/smoke:** `facade_rich_document_mounts_native_richedit_host`.
- **Non-goals (Phase 46+ / Wave D):** MenuBar/ContextMenu strategy (platform vs Menu Element); floating dock; Style Cascade v3; RichEdit OT/HTML/nested tables; deep flex/grid auto-scan virt; GTK real menus; plugin CDN; cold `layout_10k` ≤8ms; `examples_old` full retirement.

### Phase 46 notes (Facade Menu + DX / Wave D)

- **D1 strategy (locked):** prelude **in-app** `MenuBar` / `ContextMenu` / `Menu` / `MenuItem` reuse `synui-widgets` overlay menus + `MenuPanel::apply_pending`. Typed `Action<String>` carries item ids. Platform `DesktopMenu`/muda stays an optional shell and must not be documented as prelude. GTK true menus remain hard-locked.
- **EngineAdapter:** kinds `"menu-bar"` / `"context-menu"`; poll `last_action`; `VirtualList::on_context` + auto-open first mounted ContextMenu on secondary-click.
- **Demos restored:** `list_demo`, `tree_list_demo` remove menu N/A; `widget_gallery_facade` covers Menu + TabBar + VirtualList smoke.
- **D3:** [`docs/gallery_facade_vs_engine.md`](gallery_facade_vs_engine.md) capability matrix.
- **D5 / `examples_old` retirement gate (write-only this phase):** retire/rename/delete the tree only after (1) Wave A–D checklists are green for every corresponding `examples/*` package, (2) `cargo check -p <demo>` smoke passes for those packages, (3) menu Action smoke `facade_menu_bar_and_context_menu_mount_and_dispatch_actions` passes, (4) List/Grid a11y `row_count` bound tests still pass, (5) an explicit cleanup task is opened — Phase 46 does **not** delete `examples_old`.
- **E1–E3:** menu mount + Action smoke; List/Grid a11y not regressed; non-goals asserted: no Style Cascade v3, no floating dock, no RichEdit OT.
- **Bound/smoke:** `facade_menu_bar_and_context_menu_mount_and_dispatch_actions`; `cargo check -p list_demo -p tree_list_demo -p widget_gallery_facade`.
- **Non-goals (Phase 47+):** GTK `init_for_gtk_window` true menus; muda as sole menu API; floating dock / cross-window pane; Style Cascade v3; RichEdit OT/HTML/nested tables; deep flex/grid auto-scan virt; DataTable server filter/pivot; plugin CDN; cold `layout_10k` ≤8ms; forced physical deletion of `examples_old` without the D5 gate.

### Phase 47 notes (Migration closeout + Facade DX / Wave F)

- **Product pole:** DX / migration closeout — not a fifth Element capability wave; not dock / Cascade v3 / OT unlock.
- **F1–F2:** Removed stale “menus N/A (Wave D)” wording; attached prelude `MenuBar` to `navigation_demo` and `data_browser_demo`. Floating dock remains hard-locked Non-equivalent.
- **F3:** Expanded `widget_gallery_facade` smoke for DataTable / RichDocument / Icon / AppBar / Dialog; updated `docs/gallery_facade_vs_engine.md`.
- **F4 / D5 executed:** Renamed `examples_old` → in-tree gold archive (non-member; not ordinary facade evidence; deleted in Phase 52).
- **Bound/smoke:** `cargo check -p navigation_demo -p data_browser_demo -p widget_gallery_facade`; menu Action smoke; List/Grid a11y bound tests; `python3 scripts/migration_manifest.py --check`.
- **Non-goals (Phase 48+):** GTK true menus; muda as sole menu API; floating dock / cross-window pane; Style Cascade v3; RichEdit OT/HTML/nested tables; deep flex/grid auto-scan virt; DataTable server filter/pivot; plugin CDN; cold `layout_10k` ≤8ms; forced physical deletion of the gold archive.

### Phase 48 notes (Facade DX polish / Wave G)

- **Product pole:** Facade DX polish — not dock / Cascade v3 / OT / GTK true-menu unlock; **not** removing `feature = "legacy"` or deleting the gold archive.
- **G1:** Prelude `Label` Element (`"label"` → native Label); optional truncate/max_lines + a11y name; mount smoke `facade_label_mounts_native_label_with_a11y_name`; CURATED / api-reference / migration manifest.
- **G2:** `multi_window_demo` uses `Window::with_second_window` + shared App-owned `State`; tray remains host Non-equivalent.
- **G3:** `style_demo` rebuilds grid/flex/absolute with prelude `Styled` + `Label`; StyleCache diagnostics retained; Cascade v3 still hard-locked.
- **G4:** `widget_gallery_facade` smoke adds VirtualTree / SplitPane / Image; matrix in [`gallery_facade_vs_engine.md`](gallery_facade_vs_engine.md).
- **G5:** v0.3 legacy retirement **prep checklist only** (see below); hard locks → Phase 49+.
- **Bound/smoke:** `cargo check -p multi_window_demo -p style_demo -p widget_gallery_facade`; Label mount smoke; `python3 scripts/migration_manifest.py --check`.
- **Non-goals (Phase 49+):** GTK true menus; muda as sole menu API; floating dock / cross-window pane; Style Cascade v3; RichEdit OT/HTML/nested tables; deep flex/grid auto-scan virt; DataTable server filter/pivot; plugin CDN; cold `layout_10k` ≤8ms; physical deletion of the gold archive; default removal of `feature = "legacy"` (formal v0.3).

### Phase 49 notes (Facade quality gate + v0.3 prep / Wave H)

- **Product pole:** Facade quality gate + execute v0.3 retirement **prep** — not unlocking hard locks; **not** removing default `feature = "legacy"`; **not** deleting the gold archive.
- **H0:** Session UI regressions locked: drawer overlay, CSS Grid `1fr`, rich toolbar/caret/double-click, ASCII caret, Label ghost (Phase 57: facade Label length-change → layout dirty; same-length Volume stays paint-only), text geometry/paint cache (Phase 58); hand-test checklist in [`examples_facade_gap_and_next_steps.md`](examples_facade_gap_and_next_steps.md) §8.
- **H1:** `widget_gallery_facade` smoke for NavigationDrawer / BottomNavigation / BottomSheet / SnackBar; matrix + baseline updated.
- **H2:** `LEGACY_VIEW_COMPATIBILITY` audit table in [`migration-v0.2.md`](migration-v0.2.md); `adapt_legacy_view` tests-only; Conditional→Removed deferred.
- **H3:** `python3 scripts/migration_gate.py --legacy-free-check` (static ordinary-example scan + facade `cargo check` without legacy); wired in CI `validation-matrix` (ubuntu).
- **H4:** This notes block; gap §6/§9 Wave H closed; sticky baseline hygiene; `migration_manifest.py --check`.
- **Bound/smoke:** H0 test filters; `cargo check -p widget_gallery_facade`; `--legacy-free-check`; `python3 scripts/migration_manifest.py --check`.
- **Non-goals (Phase 50+ / formal v0.3):** GTK true menus; muda as sole menu API; floating dock / cross-window pane; Style Cascade v3; RichEdit OT/HTML/nested tables; deep flex/grid auto-scan virt; DataTable server filter/pivot; plugin CDN; cold `layout_10k` ≤8ms; physical deletion of the gold archive; default removal of `feature = "legacy"` (requires product sign-off).

### Phase 50 notes (Facade quality gate continuation / Wave I)

- **Product pole:** Facade quality continuation — not formal v0.3 cut; not unlocking hard locks; **not** removing default `feature = "legacy"`; **not** deleting the gold archive.
- **I0:** H0 + Slider regressions must-green via `python3 scripts/migration_gate.py --facade-regression` (CI ubuntu validation-matrix); report `target/migration-gate/facade-regression.json`.
- **I1:** Facade NumberInput drag/grab smoke (`facade_number_input_drag_grabs_pointer_and_updates_value`); `LinearProgress` already uses `flex_shrink: 0` / fixed height — no Slider-class crush risk (audit closed); §8 hand-test: gallery Volume drag must not vanish.
- **I2:** Label paint-width ellipsis (`truncate` + measure width budget → `…`); `TextRenderer::fit_text_with_ellipsis`; api-reference Label boundary updated.
- **I3:** Prelude `DataTable::span` / `spans` → native `set_spans`; `table_demo` merge-cell demo; baseline updated.
- **I4:** This notes block; gap §6/§9 Wave I closed; sticky gallery row **frozen Partial** with pointer to `style_sticky_demo`; allowlist shrink remains document-only; `migration_manifest.py --check`.
- **Bound/smoke:** `--facade-regression`; `cargo check -p table_demo -p widget_gallery_facade`; `python3 scripts/migration_manifest.py --check`.
- **Non-goals (formal v0.3 / hard-lock unlock):** GTK true menus; muda as sole menu API; floating dock / cross-window pane; Style Cascade v3; RichEdit OT/HTML/nested tables; deep flex/grid auto-scan virt; DataTable server filter/pivot / edit-merge UI; plugin CDN; cold `layout_10k` ≤8ms; physical deletion of the gold archive; default removal of `feature = "legacy"` (requires product sign-off); Conditional→Removed batch allowlist cut.

### Phase 51 notes (Formal v0.3 legacy retirement / Wave J)

- **Product pole:** Product-signed **v0.3 legacy cut** — delete `synui` `legacy` feature (already opt-in; not “turn off default”); Conditional→Removed; relocate Engine gallery/hosts out of workspace. **Not** hard-lock unlock; **not** physical delete of the gold archive (Phase 52).
- **J0:** Corrected docs: late v0.2 already had `default = []`; cut = feature removal. Frozen opt-in disposal table in [`migration-v0.2.md`](migration-v0.2.md).
- **J1:** `LEGACY_VIEW_COMPATIBILITY` all rows `Unsupported` / `RemovedWithSteps`; runtime `adapt_legacy_view` removed.
- **J2:** `widget_gallery` / `android_host` / `ios_host` moved into the gold archive (deleted Phase 52); `synui-testing` dropped ViewEngine tests; `synui-bench` dropped `legacy-bench` / `declarative_diff`.
- **J3:** Removed `legacy` (+ View-tied `plugins`/`hot_reload` features) and all `cfg(legacy)` modules from [`crates/synui`](../crates/synui).
- **J4:** `LEGACY_OPT_IN_PACKAGES` empty; CI builds `widget_gallery_facade` / `mobile_hello` mobile targets; `--legacy-free-check` asserts no `legacy` feature on `synui`.
- **J5:** This notes block; gap next pole = hard-lock unlock (sign-off) or Phase 52 gold-tree delete.
- **Bound/smoke:** `--legacy-free-check`; `--facade-regression`; `migration_manifest.py --check`; `cargo check -p widget_gallery_facade -p synui-testing`.
- **Non-goals:** dock / Cascade v3 / OT / GTK; Phase 52 gold-archive delete (scheduled only).

### v0.3 legacy retirement checklist

Milestone: [`LEGACY_VIEW_ADAPTER_RETIREMENT = "v0.3.0"`](../crates/synui/src/compatibility.rs).

1. [x] Audit every `adapt_legacy_view` / allowlisted `View` path (Wave H2).
2. [x] Keep CI green for facade packages without `legacy`.
3. [x] CI gate `--legacy-free-check` (Wave H3; hardened Wave J4).
4. [x] Conditional→Removed + migration notes (Wave J1).
5. [x] Remove `legacy` feature (Wave J3).
6. [x] **Phase 52:** physical deletion of the gold archive (Wave K).


### Phase 52 notes (Gold tree deletion / Wave K)

- **Product pole:** Physically delete the in-tree v0.1 gold archive (post-Phase-47 rename of `examples_old`) — complete v0.3 checklist item 6. **Not** hard-lock unlock; **not** new Elements.
- **K1:** Removed the gold archive tree from the repo root (`git rm -r` on the rename target).
- **K2:** Rewrote docs/README/demo/lib/scripts pointers; ordinary evidence = `examples/*` + facade gates.
- **K3:** `migration_manifest.py --check`; `--legacy-free-check`; `--facade-regression`; live-path string for the deleted archive clean outside `.cursor/plans`.
- **K4:** This notes block; gap §6/§9 next pole = hard-lock unlock (sign-off) or Facade DX.
- **Bound/smoke:** gates above; `cargo check -p widget_gallery_facade`.
- **Non-goals:** dock / Cascade v3 / OT / GTK true menus; restoring `legacy` or ViewEngine tests.

### Phase 53 notes (Facade DX / Wave L)

- **Product pole:** Facade DX — dead-command hygiene + thin prelude controls. **Not** hard-lock unlock; **not** gallery sticky unfreeze; **not** Card/ListItem/CircularProgress/Lottie.
- **L0:** Replaced dead `widget_gallery` / `android_host` runbook commands with `widget_gallery_facade` / `mobile_hello`; fixed book/notes/plugin_sample pointers.
- **L1:** Prelude `Switch` / `Radio` / `LinearProgress` (Checkbox / Icon patterns); EngineAdapter mount/reconcile/poll; CURATED + compat replacements; mount smoke `facade_switch_radio_and_linear_progress_mount_native_hosts`.
- **L2:** `widget_gallery_facade` smoke; matrix/baseline updated; sticky remains Frozen Partial → `style_sticky_demo`.
- **L3:** `migration_manifest.py --check`; `--legacy-free-check`; `--facade-regression`; this notes block; gap §6/§9 next pole = hard-lock unlock (sign-off) or next Facade DX.
- **Bound/smoke:** gates above; `cargo check -p widget_gallery_facade`; Switch/Radio/Progress mount test.
- **Non-goals:** dock / Cascade v3 / OT / GTK true menus; sticky gallery unfreeze; restoring gold archive / `legacy`.

### Phase 54 notes (Facade DX / Wave M)

- **Product pole:** Facade DX — Card + ListItem + CircularProgress prelude + doc hygiene. **Not** hard-lock unlock; **not** gallery sticky unfreeze; **not** TextArea / Lottie / MorphingShape.
- **M0:** `Card::replace_children` / `ListItem::replace_children`; optional `CircularProgress::a11y`.
- **M1:** Prelude Elements + EngineAdapter (kinds `card` / `list-item` / `circular-progress`); CURATED + compat + manifest/api-ref; mount smoke `facade_card_list_item_and_circular_progress_mount_native_hosts`.
- **M2:** `widget_gallery_facade` smoke; README/matrix/baseline; gap §2 Engine gallery row fixed; sticky remains Frozen Partial.
- **M3:** `migration_manifest.py --check`; `--legacy-free-check`; `--facade-regression`; this notes block; gap §6/§9 next pole = hard-lock unlock (sign-off) or next DX (TextArea / sticky unfreeze).
- **Bound/smoke:** gates above; `cargo check -p widget_gallery_facade`; Card/ListItem/CircularProgress mount test + `component_contract`.
- **Non-goals:** dock / Cascade v3 / OT / GTK true menus; sticky gallery unfreeze; restoring gold archive / `legacy`.

### Phase 55 notes (Facade DX / Wave N)

- **Product pole:** Facade DX — prelude `TextArea` + gallery sticky/fixed unfreeze. **Not** hard-lock unlock; **not** Lottie / MorphingShape; **not** CircularProgress determinate-arc paint rewrite.
- **N0:** Prelude `TextArea` (kind `text-area`) cloning `TextInput` / `InputConfig<String>`; EngineAdapter mount/patch/read/write; leaf denylist; CURATED + prelude.
- **N1:** Manifest `text-area`; api-ref/migration/compat (`widgets::TextArea`); `component_contract` + `facade_text_area_mounts_and_polls_controlled_changes`.
- **N2:** `widget_gallery_facade` TextArea smoke; `Styled::sticky_top` + fillers + root fixed badge; matrix/baseline/README flip Partial → Yes (smoke); full sticky suite remains `style_sticky_demo`.
- **N3:** `migration_manifest.py --check`; `--legacy-free-check`; `--facade-regression`; this notes block; gap §1/§3.4/§6/§9 next pole = hard-lock unlock (sign-off) or next DX (Lottie / advanced).
- **Bound/smoke:** gates above; `cargo check -p widget_gallery_facade`; TextArea mount test + `component_contract`.
- **Non-goals:** dock / Cascade v3 / OT / GTK true menus; restoring gold archive / `legacy`; moving full sticky suite out of `style_sticky_demo`.

### Phase 56 notes (Quality + Advanced DX / Wave O)

- **Product pole:** Quality (CircularProgress determinate/indeterminate arcs + TextInput/TextArea focus-protected controlled sync) + advanced non-prelude `Lottie` / `MorphingShape`. **Not** hard-lock unlock; **not** prelude promotion for Lottie/Morph; **not** Label ghost engine fix; **not** `Window::with_lottie`.
- **O0:** CircularProgress track + arc paint; controlled text patch skips when `focus_manager().focused()`; tests `circular_progress_advance_*` / `facade_text_input_and_area_focus_protects_controlled_edit`.
- **O1:** `MorphingShape::{set_playing,set_speed,set_size}`; [`examples/assets/lottie_pulse.json`](../examples/assets/lottie_pulse.json).
- **O2:** Facade Elements kinds `lottie` / `morphing-shape`; EngineAdapter; manifest `advanced_integration` (`prelude = false`); compat/migration/api-ref Advanced; `facade_lottie_and_morphing_shape_mount_native_hosts`.
- **O3:** Gallery animation smoke with `playing(false)`; matrix/baseline/README; this notes block; gap §1/§3.4/§6/§9 next pole = hard-lock unlock (sign-off) or Label ghost / Lottie preload.
- **Bound/smoke:** `migration_manifest.py --check`; `--legacy-free-check`; `--facade-regression`; `cargo check -p widget_gallery_facade`.
- **Non-goals:** dock / Cascade v3 / OT / GTK true menus; Lottie/Morph in prelude or CURATED; restoring gold archive / `legacy`.

### Phase 57 notes (Label ghost + Lottie preload + Prelude / Wave P)

- **Product pole:** Label ghost (facade Label text **length** change → layout dirty) + `Window::with_lottie` / `Lottie::from_asset` + promote `Lottie` / `MorphingShape` to prelude. **Not** hard-lock unlock; **not** Label glyph-cache invisible-glyph fix; **not** shared Lottie Composition/Player across frames.
- **P0:** `mark_reconcile_dirty` facade `label` property("text") length change → `mark_layout_dirty_widget`; same-length stays paint-only; tests `facade_label_length_change_marks_layout_dirty` / `facade_label_same_length_text_stays_paint_only`; H0/gap §8.
- **P1:** `LottieId` / `LottieCache` (preloaded JSON strings); `Application::lotties_mut`; `PaintCtx.lotties` via `LottieJsonSource`; `Window::with_lottie`; `PlatformError::Lottie`.
- **P2:** Facade `Lottie::from_asset` / `from_json` / `new`; Adapter id/json branches; native lazy asset player from cache on first paint.
- **P3:** prelude + CURATED + manifest `stable` + `STABLE_COMPONENTS`; compat/migration/api-ref Stable animation hosts.
- **P4:** Gallery `with_lottie` + `from_asset` (no `include_str`); this notes block; gap §1/§3.4/§6/§9 next pole = hard-lock unlock (sign-off).
- **Bound/smoke:** `migration_manifest.py --check`; `--legacy-free-check`; `--facade-regression`; `cargo check -p widget_gallery_facade`.
- **Non-goals:** dock / Cascade v3 / OT / GTK true menus; restoring gold archive / `legacy`.

### Phase 58 notes (Soft quality / Render DX / Wave Q)

- **Product pole:** Soft quality / Render DX — text layout **geometry vs paint** cache split + `LottieCache` pre-parsed shared `Composition` (clone into per-widget players). **Not** hard-lock unlock; **not** shared `LottiePlayer`/`Renderer` across widgets.
- **Q0:** `GeometryLayoutKey` + `geometry_cache` for measure/hit/caret; brush-keyed `paint_cache` for paint only; regressions `measure_does_not_populate_paint_cache`, `geometry_cache_survives_paint_lru_pressure`, `dark_on_surface_paint_uses_light_brush_not_black`, `clear_layout_cache_drops_geometry_and_paint`. Phase 57 = hug/layout ghost; Phase 58 = brush/geometry cache.
- **Q1:** `LottieCache` stores JSON + `Composition` (parse at `load_*`); `LottiePlayer::from_composition`; native asset path clones Composition via `LottieJsonSource::as_any` → `LottieCache`; invalid JSON fails at preload (`LottieError::Parse` / `PlatformError::Lottie`).
- **Q2:** This notes block; gap §1/§6/§9 Wave Q; api-ref Composition preload; next pole = hard-lock unlock (sign-off).
- **Bound/smoke:** `migration_manifest.py --check`; `--legacy-free-check`; `--facade-regression`; `cargo check -p widget_gallery_facade`; `cargo test -p synui-render --lib cache_tests` / `lottie`.
- **Non-goals:** dock / Cascade v3 / OT / GTK true menus; shared Player/Renderer; restoring gold archive / `legacy`.

### Phase 59 notes (Soft hygiene + hard-lock sign-off packet / Wave R)

- **Product pole:** Soft closeout of gallery/Dialog/Lottie UX docs + **hard-lock unlock sign-off packet** ([`docs/hard_lock_signoff.md`](hard_lock_signoff.md)). **Not** implementing dock / Cascade v3 / OT·HTML / GTK true menus; **not** bumping Production ABI.
- **R0:** BottomSheet `align_items: Center` (same Auto→measured Button issue as Dialog); gallery Animation `playing(true)` while category mounted; baseline / README / gap §8 / api-ref / matrix / migration `boundary_note` aligned; Dialog Close already centered (Phase 59 soft).
- **R1:** Sign-off packet for four locks with MVP / non-goals / recommended order (Cascade v3 → dock → RichEdit OT/HTML → GTK menus) and per-item checkboxes. Unsigned = no implementation wave.
- **R2:** This notes block; gap §1/§6/§9 Wave R; boundary reminder: Rust apps use `prelude`; non-Rust uses `synui-abi` v1.1 (no ABI expansion this phase).
- **Bound/smoke:** `migration_manifest.py --check`; `--legacy-free-check`; `--facade-regression`; `cargo check -p widget_gallery_facade`; `cargo test -p synui-testing --test golden_tests golden_dialog_open`; `cargo test -p synui-abi --lib`.
- **Non-goals:** implementing any hard-lock quartet; `SYNUI_ABI_VERSION_MINOR` bump / new Lottie C symbols; shared Lottie Player/Renderer; restoring gold archive / `legacy`.

### Alloc counter

```bash
cargo test -p synui-core --features alloc_counter
```

`synui_core::alloc_counter` is a cooperative frame budget helper. Do **not**
allocate large buffers inside `paint`; reuse `Scene` / text layout contexts.
