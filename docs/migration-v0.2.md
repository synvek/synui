# SynUI v0.1 → v0.2 migration

SynUI v0.2 is an intentional public API reset. The retained Engine remains in place, but ordinary applications now use the curated `synui` facade and App-owned resources. The machine-readable source of truth is [`compatibility.rs`](../crates/synui/src/compatibility.rs): `V01_FACADE_COMPATIBILITY`, `LEGACY_VIEW_COMPATIBILITY`, and `RELEASE_BOUNDARIES`.

## Compatibility boundaries

| Boundary | v0.2 commitment |
|---|---|
| Rust source | Reset to `app`, `view`, `widgets`, `style`, `model`, `platform`, and `prelude`; `advanced` is explicit Engine integration. |
| Production ABI | Versioned `synui-abi` v1.1 C header and ABI-only SDK contract. |
| Experimental Plugin ABI | Preserved only for experimental, preview, hot-reload, example, and test tooling; excluded from Production ABI. |
| JSON View Creation | Preserved only for debugging, preview, hot-reload, examples, and tests; excluded from Production ABI. |

Each v0.1 entry family is classified as `Preserve`, `DeprecatedWithReplacement`, or `RemovedWithSteps`. Removed root APIs are absent by default; the `legacy` feature is a temporary in-tree migration boundary, not a v0.2 production contract.

## Migration map

| v0.1 area | Classification | Migration |
|---|---|---|
| `App`/`ViewFn`/`run_app`, `Application`/`AppDriver` | Removed with steps | Use `App::new`, App-owned resources, and `App::run(Window, Node builder)`. |
| global `Signal` state | Removed with steps | Create `State<T>` with `AppContext::state`. |
| numeric/global actions and `View::on_action` | Removed with steps | Use typed `AppContext::action` or Element event closures. |
| Text/Bool/Number bindings | Deprecated with replacement | Use homogeneous `InputConfig<T>` with `TextInput`, `Checkbox`, or `NumberInput`. |
| selection binding | Deprecated with replacement | Use Model selection callbacks; choice inputs use `InputConfig<ChoiceId>`. |
| other WidgetStore bindings and registries | Removed with steps | Move data into State/Model or an external Element; no lossless generic adapter exists. |
| numeric keys, `key_scope`, section/memo scheduling | Removed with steps | Use Automatic Identity and `BusinessKey` only for repeated siblings. |
| supported legacy `View` snapshots | Removed with steps | v0.3 removed `adapt_legacy_view`; rebuild with `Node`/`Element` / prelude (see `LEGACY_VIEW_COMPATIBILITY`). |
| unsupported legacy `View` variants | Removed with steps | Rebuild as an external Element, Model-backed component, or platform integration according to the variant manifest. |
| fallible legacy style | Deprecated with replacement | Use `StyleInput`, `StyleCache`, and structured `StyleDiagnostic`. |
| panic CSS/class/theme paths | Removed with steps | Use non-panicking style input; theme/class behavior has no direct curated-facade replacement. |
| plugins/hot reload | Preserve, experimental | Keep only in labelled development/tooling paths; never expose through Production ABI. |
| `parse_view_json`/`view_from_json` | Preserve, non-production | Restrict to debugging/preview/hot-reload/example/test-only use. |

## Legacy View adapter

The adapter validates the complete tree before conversion. It ignores legacy numeric keys, never reads legacy binding/signal/memo/section registries, and uses v0.2 positional identity plus optional `BusinessKey`. Supported and conditional variants, exact conditions, replacements, lifecycle limits, and retirement milestone are listed in `LEGACY_VIEW_COMPATIBILITY`; unsupported variants return a structured child-index path error. Retirement is scheduled for v0.3.0 (`LEGACY_VIEW_ADAPTER_RETIREMENT`).

### v0.3 retirement (Phase 51 Wave J — product sign-off)

**Correction:** by late v0.2, `synui` already had `default = []` and optional `legacy = []`. The cut is **not** “turn off default legacy”; it is **delete the `legacy` feature and its modules**.

Prep (Phases 48–50) then cut (Phase 51):

1. Audit every `adapt_legacy_view` / allowlisted `View` path. **Done (Wave H2).**
2. Ordinary `examples/*` facade-only. **Done.**
3. CI `--legacy-free-check`. **Done (Wave H3); emptied opt-in allowlist in Wave J4.**
4. Shrink `LEGACY_VIEW_COMPATIBILITY` Conditional→Removed. **Done (Wave J1)** — see Wave J1 table below.
5. Remove `legacy` feature from `synui`. **Done (Wave J3).**
6. Physical deletion of the in-tree gold archive. **Done (Phase 52 Wave K).**

#### Wave H2 — `LEGACY_VIEW_COMPATIBILITY` audit (2026-08-03, historical)

Pre-cut counts: Supported 1 / Conditional 25 / Unsupported 10.  
`adapt_legacy_view` call sites were tests-only.

#### Wave J0 — opt-in package disposal (frozen)

| Package | Wave J disposal |
|---------|-----------------|
| `examples/widget_gallery` | Removed from workspace (Phase 51); gold archive deleted (Phase 52) |
| `examples/android_host` / `ios_host` | Removed from workspace (Phase 51); gold archive deleted (Phase 52); ordinary mobile = `mobile_hello` |
| `crates/synui-testing` | Drop `synui/legacy`; delete ViewEngine-dependent tests |
| `crates/synui-bench` | Drop `legacy-bench` / `declarative_diff` |

#### Wave J1 — allowlist shrink (Conditional→Removed)

All former Supported/Conditional rows are now `adapter_support = Unsupported` / `RemovedWithSteps`. Prelude replacements:

| Variant | Replacement |
|---------|-------------|
| Label | `Node::text` / `widgets::Label` |
| Button | `widgets::Button` |
| TextField | `widgets::TextInput` |
| Icon | `widgets::Icon` |
| Checkbox | `widgets::Checkbox` |
| Slider | `widgets::NumberInput` |
| Image | `widgets::Image` |
| Dialog / AppBar / BottomNavigation / NavigationDrawer / BottomSheet / SnackBar | matching `widgets::*` |
| Dropdown | `widgets::Select` |
| TabBar / Split / TreeView / DataTable / ListView / Scroll | matching prelude Elements |
| RichEdit | `widgets::RichDocument` |
| MenuBar / ContextMenu | prelude in-app menus |
| Flex / Column / Row | `Node::container` / `row` / `column` |
| Switch | `widgets::Switch` |
| Radio | `widgets::Radio` |
| LinearProgress | `widgets::LinearProgress` |
| CircularProgress | `widgets::CircularProgress` |
| Card | `widgets::Card` |
| ListItem | `widgets::ListItem` |
| Styled | `widgets::Styled` |
| MorphingShape, Lottie | `widgets::MorphingShape` / `widgets::Lottie` (prelude; prefer `Lottie::from_asset` + `Window::with_lottie`) |
| VirtualScroll | external Element / host (unchanged Unsupported) |
| TextArea | `widgets::TextArea` |

Runtime `adapt_legacy_view` is **removed** with the `legacy` feature.

## Release scope

v0.2 includes the curated Rust facade, Context ownership, closure Actions, Automatic Identity, unified inputs, open Element extension, style diagnostics, Production ABI, and Python ABI validation path. DSL/parser/formatter, language server, code generation, live-preview tooling, and broad new component families remain future work.

## Component migration index

The complete example/component inventory and validation evidence is the machine-readable [component API migration manifest](component-api-example-migration.toml). The stable public names and lifecycle contracts are indexed in the [v0.2 API Reference](api-reference-v0.2.md). Engine, plugin, JSON View, ABI, Python SDK, and platform host checks remain explicitly separate boundaries.

## Wave 3 low-risk example evidence

Task 4.2 records the completed low-risk package boundaries in the [migration manifest](component-api-example-migration.toml). `todo_list`, `navigation_demo`, `list_demo`, and `multi_window_demo` now use `synui::prelude::*` for ordinary UI code. Their manifests contain no `legacy`/`plugins` features or direct Engine-crate dependencies. Desktop and browser host behavior remains explicitly bounded, and retained Engine examples are never used as facade proof.

| Package | Before | After | Compatibility and retirement |
|---|---|---|---|
| [`todo_list`](../examples/todo_list/README.md) | Global `Signal`, `TextBinding`, `FormState`, root `View`, `WidgetStore`, Application/menu/file-dialog | App `State`/typed `Action`, `TextInput`/`InputConfig`, prelude `Scroll` | Legacy bindings/root re-exports retired; file-dialog/menu remains platform/advanced evidence. |
| [`navigation_demo`](../examples/navigation_demo/README.md) | `Router`/`Route`, numeric actions, `View`, `ViewEngine`, `WidgetStore`, Application | Route `State`, typed actions, `Node`/external Element, host-owned history property | Legacy Router/numeric actions retired; browser history remains a platform boundary. |
| [`list_demo`](../examples/list_demo/README.md) | `Signal`, `View`, `ViewEngine`, `WidgetStore`, ListView/context-menu internals | Prelude `VirtualList` + in-app `MenuBar`/`ContextMenu`, App-owned Actions | Phase 46 Wave D; 10k virt mount + clickable menus. |
| [`multi_window_demo`](../examples/multi_window_demo/README.md) | Application/AppDriver, `Signal`, `View`, `ViewEngine`, `WidgetStore`, menu/tray host | Facade `App`/`Window::with_second_window`, shared App-owned State/Action | Second window shares State; tray/DesktopMenu remain host Non-equivalent. |

### Before/after import boundary

Each migrated package has a before/after import mapping in [`docs/baselines/`](baselines/). The current source uses the facade-only form:

```rust
// Before: v0.1 root/Engine imports
use synui::{AppDriver, Application, Signal, View, ViewEngine, WidgetStore};

// After: v0.2 ordinary application import
use synui::prelude::*;
```

The after form excludes `WidgetStore`, `ViewEngine`, Engine layout/render types, numeric/global actions, legacy bindings, plugins, and JSON View creation. `synui::advanced` is reserved for separately labelled adapters and is not a facade-only promise.

### Exact command labels

**Facade-only commands** validate these migrated package paths and the facade fixtures:

```bash
cargo check -p todo_list
cargo check -p navigation_demo
cargo check -p list_demo
cargo check -p multi_window_demo
cargo test -p synui --test counter_prelude_fixture
cargo test -p synui --test external_component_fixture
```

**Engine-bound commands** are reserved for retained/advanced examples and must not be substituted for the commands above. For example:

```bash
cargo check -p show_image
cargo check -p widget_gallery
```

**Manifest and metadata commands** validate package coverage and declared dependency boundaries without creating a window:

```bash
cargo metadata --format-version 1 --no-deps
python3 scripts/migration_manifest.py --check
```

A retained Engine example may pass its own Engine-bound check, but it cannot establish a Stable Component or facade-only migration result.

## Wave 4 data and virtualization evidence (Phase 44 Wave B)

`tree_list_demo`, `table_demo`, and `data_browser_demo` are ordinary facade examples. They import only `synui::prelude::*` and mount real hosts through `EngineAdapter` (`"data-table"` / `"virtual-tree"` / `"split-pane"` / `"tab-bar"`). Typed Actions cover selection, edit, sort, filter, expand, tab, and preview without `WidgetStore`, `ViewEngine`, global bindings, or numeric ranges.

| Package | Before | After | Non-equivalent boundary |
|---|---|---|---|
| `tree_list_demo` | Engine TreeView/ListView, selection bindings, numeric keys | Prelude `VirtualTree` + `VirtualList` + in-app `MenuBar`/`ContextMenu` | Phase 46 Wave D menus restored |
| `table_demo` | materialized Engine DataTable and binding polling | Prelude runtime `DataTable` with cell factory, select/sort/edit, client filter | Merge-cell UI / server pivot remain deferred |
| `data_browser_demo` | Engine panes, sections, tree/table bindings, global Signals | `VirtualTree` \| `SplitPane` \| `TabBar(Table\|RichDocument)` \| Detail + filter + MenuBar | Floating dock hard-locked; Phase 47 MenuBar |

Phase 44 bound-mount evidence uses `facade_data_table_mounts_native_host_with_bounded_cells`. A missing a11y bridge is `unavailable`, not `passed`. See [`docs/baselines/tree_list_demo.md`](baselines/tree_list_demo.md), [`table_demo.md`](baselines/table_demo.md), and [`data_browser_demo.md`](baselines/data_browser_demo.md).

Wave 4 commands:

```bash
cargo check -p tree_list_demo
cargo check -p table_demo
cargo check -p data_browser_demo
cargo test -p synui --lib view::tests::facade_data_table_mounts_native_host_with_bounded_cells
cargo metadata --format-version 1 --no-deps
python3 scripts/migration_manifest.py --check
```

## Wave 5 rich text and style evidence

`rich_edit_demo`, `rich_blocks_demo`, and `style_demo` use the curated facade. Phase 45 delivers prelude `RichDocument` (BlockDoc in prelude); Phase 43 delivered `Scroll` / `VirtualList` / `Styled`. The examples do not enable `legacy`, import `View`/`ViewEngine`/`WidgetStore`, or call `style_css`.

| Package | Facade path | Explicit boundary | Preserved behavior |
|---|---|---|---|
| `rich_edit_demo` | Prelude `RichDocument` + `BlockDoc` State/Action | OT/HTML/nested tables hard-locked | inline formatting, sample load, Markdown export/preview |
| `rich_blocks_demo` | Prelude `RichDocument` + BlockDoc MD round-trip | OT/HTML/nested tables hard-locked | headings, lists, image slots, import/export |
| `style_demo` | StyleInput, StyleCache, StyleDiagnostic, Node/Element | grid/flex/absolute presentation remains external | typed style showcase and invalid-runtime-style retention |
| `style_sticky_demo` | Prelude `Scroll` + `Styled`, StyleCache/StyleDiagnostic | Style Cascade v3 / nested transparent virt remain Phase 46+ | sticky bar, fixed badge, z-index overlap, diagnostic re-check |

Invalid runtime style is a diagnostic success: the previous successful style remains cached. A11y bridge unavailability remains `unavailable`, never `passed`. The package baselines in [`docs/baselines/`](baselines/) record the before/after mapping and regression owners.

### Wave 5 regression owner mapping

| Regression target | Owner | Classification |
|---|---|---|
| `crates/synui-testing/tests/snapshot_tests.rs` | `synui-testing` | Engine-bound layout/snapshot owner |
| `crates/synui-testing/tests/golden_tests.rs` | `synui-testing` | Engine-bound PNG/widget owner |
| `crates/synui-testing/tests/scroll_tests.rs` | `synui-testing` | Engine-bound ScrollView/viewport owner |
| `crates/synui-testing/tests/gallery_media_repro.rs` | `synui-testing` | Engine-bound keyed legacy-tree owner |
| `crates/synui/src/style.rs` tests | `synui` facade | StyleInput/StyleCache/StyleDiagnostic owner |
| `crates/synui-core/src/block_doc.rs` tests | `synui-core` data support | BlockDoc Markdown owner |

The four `synui-testing` targets retain their equivalent assertions and are not relabelled as facade-only. Their Engine-bound imports are intentional and do not establish Stable Component evidence.

Wave 5 exact commands:

```bash
cargo check -p rich_edit_demo
cargo check -p rich_blocks_demo
cargo check -p style_demo
cargo check -p style_sticky_demo
cargo test -p synui-core --lib block_doc
cargo test -p synui --lib style::tests
cargo test -p synui-testing --test scroll_tests
cargo test -p synui-testing --test snapshot_tests
cargo test -p synui-testing --test golden_tests
cargo test -p synui-testing --test gallery_media_repro
python3 scripts/migration_manifest.py --check
python3 scripts/validation_matrix.py --only rich-text-style,headless-render,a11y
```

The Matrix keeps facade, headless, a11y, and retained Engine regression results independently attributed. It does not treat a bridge-unavailable a11y result as passed.

## Wave 6 platform example evidence

Wave 6 keeps ordinary UI construction on the curated facade while retaining
platform launch and host glue as explicit boundaries. `mobile_hello` is a
facade example with feature-selected `synui-winit` support. Ordinary mobile
evidence is `mobile_hello` (+ CI target checks). Historical Engine
`android_host` / `ios_host` gold sources were deleted in Phase 52; ordinary mobile evidence is `mobile_hello`
(Phase 51; not workspace members). `web_hello` uses the facade for its
ordinary desktop/browser counter; DOM accessibility remains a host boundary.

| Package | Declared targets | Ordinary UI | Explicit host boundary | Baseline |
|---|---|---|---|---|
| `mobile_hello` | desktop, Android, web | `synui::prelude::{App, State, Action, Node, Button, Window}` | `synui-winit` feature-selected target support | desktop counter; target checks for Android/wasm |
| `mobile_hello` | desktop, Android, web | facade counter | `synui-winit` feature-selected host | CI target checks |
| `web_hello` | desktop, web | facade + DomA11y | wasm / WebGPU | browser counter |

Exact target commands are recorded in each README and in the Manifest
`target_commands` fields. Run the platform Matrix selection with:

```bash
cargo metadata --format-version 1 --no-deps
python3 scripts/migration_manifest.py --check
python3 scripts/validation_matrix.py --only platform --report target/validation-matrix/component-api-example-migration.json
```

The Matrix runs each declared desktop/web/Android/iOS check independently. A
missing target, SDK/NDK, Xcode/simulator, wasm-bindgen, browser, or linker is
reported as `unavailable` for the originating platform workload; it is never
converted to `passed`. Source or behavior errors remain `failed`, and sibling
platform commands continue so attribution is not lost. Browser history/launch
and Android/iOS safe-area behavior remain host baselines rather than ordinary
facade contracts.

## Wave 7 gallery split evidence

Wave 7 separated the gallery into independently attributable stable vs Engine content. The [facade gallery README](../examples/widget_gallery_facade/README.md) is the stable replacement for App state/action/input, `BusinessKey`, controls, and style diagnostics. It has no Engine, plugin, or JSON dependency. Baseline: [`docs/baselines/widget_gallery_facade.md`](baselines/widget_gallery_facade.md).

**Phase 51–52:** Engine `widget_gallery` / hosts left the workspace (Phase 51); the gold archive was **physically deleted** (Phase 52). The `synui` `legacy` feature is gone. Matrix: [`gallery_facade_vs_engine.md`](gallery_facade_vs_engine.md).

| Regression/path | Owner | Classification | Status |
|---|---|---|---|
| `widget_gallery_facade` | `synui` facade fixtures and style tests | facade-only | stable ordinary gallery |
| Engine gallery / hosts | — | deleted (Phase 52) | no in-tree gold |

### Wave 7 commands (post Phase 51)

```bash
cargo check -p widget_gallery_facade
cargo test -p synui --test counter_prelude_fixture
cargo test -p synui --lib style::tests
cargo build -p synui_plugin_sample --lib
python3 scripts/migration_manifest.py --check
python3 scripts/migration_gate.py --legacy-free-check
python3 scripts/migration_gate.py --facade-regression
```

### Canonical stable component paths

The shared canonical index is intentionally repeated here so the API Reference, Migration Guide, Manifest, and final report cannot drift:

- `synui::view::Node::text` and `synui::view::Node::container`
- `synui::widgets::Button`
- `synui::widgets::TextInput`
- `synui::widgets::TextArea`
- `synui::widgets::Checkbox`
- `synui::widgets::NumberInput`
- `synui::widgets::Select`
- `synui::widgets::DataTable`
- `synui::widgets::Scroll`
- `synui::widgets::VirtualList`
- `synui::widgets::VirtualTree`
- `synui::widgets::SplitPane`
- `synui::widgets::TabBar`
- `synui::widgets::RichDocument`
- `synui::widgets::AppBar`
- `synui::widgets::NavigationDrawer`
- `synui::widgets::BottomNavigation`
- `synui::widgets::Dialog`
- `synui::widgets::BottomSheet`
- `synui::widgets::SnackBar`
- `synui::widgets::Icon`
- `synui::widgets::Image`
- `synui::widgets::Styled`
- `synui::style::StyleInput`
- `synui::platform::Window`

Phase 43–46 promoted list/scroll/table/tree/split/tab/rich/nav/overlays/icon/image and in-app MenuBar/ContextMenu. Platform `DesktopMenu`/muda remains an optional shell (not prelude). `WidgetStore`, `ViewEngine`, plugin registry symbols, and JSON View creation are non-ordinary retained/experimental references.
