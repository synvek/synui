# SynUI v0.2 Stable Component API Reference

This document is the human-readable companion to [`component-api-example-migration.toml`](component-api-example-migration.toml). The Migration Manifest is the canonical index for component IDs, canonical paths, Prelude membership, stability, ownership, lifecycle, errors, identity, events, accessibility, and validation. The facade source of truth is [`crates/synui/src/lib.rs`](../crates/synui/src/lib.rs); legacy mappings remain in [`compatibility.rs`](../crates/synui/src/compatibility.rs).

## Reading the contract

The ordinary application surface is the `synui` facade and its documented `synui::prelude`. A stable component is an application-level descriptor or open [`synui::view::Element`](../crates/synui/src/view.rs), never a retained Engine storage type. `AppContext` owns `State`, typed `Action`, `Model`, mounted resources, and event routes. Build, mount, update, event dispatch, and unmount are UI-thread operations unless a record explicitly identifies a platform host boundary.

- **Identity:** static children use automatic identity; repeated or dynamic siblings use [`synui::view::BusinessKey`](../crates/synui/src/view.rs). Numeric keys, global registries, `key_scope`, `memo`, and `section` scheduling are not v0.2 application APIs.
- **Errors:** component errors are returned through `ElementError`, `AppError`, `InputConfigError`, `VirtualModelError`, `StyleDiagnostic`/`StyleApplyResult`, or `PlatformError` at the documented boundary. Invalid input must not become a panic or a partial commit.
- **Lifecycle:** `mount` creates the retained Element/resource route, `update` reconciles the next descriptor while preserving identity, and `unmount` releases routes/resources with the parent. The exact rule for each stable export is repeated below.
- **Prelude:** `yes` means the canonical export is available from `synui::prelude::*`; `no` means the path is an explicit external, advanced, or deferred boundary and is not a Prelude promise.

## Stable Component API index

The following records are the Manifest records with `stability = "stable"`. Canonical paths are copied exactly from the Manifest; the type name and Prelude membership are stated separately so a path cannot be mistaken for a re-export.

| Canonical path | Module path | Type name | Prelude | Stability |
|---|---|---|---:|---|
| `synui::view::Node::text` | `synui::view` | `Node` | yes | stable |
| `synui::view::Node::container` | `synui::view` | `Node` | yes | stable |
| `synui::widgets::Button` | `synui::widgets` | `Button` | yes | stable |
| `synui::widgets::TextInput` | `synui::widgets` | `TextInput` | yes | stable |
| `synui::widgets::TextArea` | `synui::widgets` | `TextArea` | yes | stable |
| `synui::widgets::Checkbox` | `synui::widgets` | `Checkbox` | yes | stable |
| `synui::widgets::NumberInput` | `synui::widgets` | `NumberInput` | yes | stable |
| `synui::widgets::Select` | `synui::widgets` | `Select` | yes | stable |
| `synui::widgets::DataTable` | `synui::widgets` | `DataTable` | yes | stable |
| `synui::widgets::Scroll` | `synui::widgets` | `Scroll` | yes | stable |
| `synui::widgets::VirtualList` | `synui::widgets` | `VirtualList` | yes | stable |
| `synui::widgets::VirtualTree` | `synui::widgets` | `VirtualTree` | yes | stable |
| `synui::widgets::SplitPane` | `synui::widgets` | `SplitPane` | yes | stable |
| `synui::widgets::TabBar` | `synui::widgets` | `TabBar` | yes | stable |
| `synui::widgets::RichDocument` | `synui::widgets` | `RichDocument` | yes | stable |
| `synui::widgets::AppBar` | `synui::widgets` | `AppBar` | yes | stable |
| `synui::widgets::NavigationDrawer` | `synui::widgets` | `NavigationDrawer` | yes | stable |
| `synui::widgets::BottomNavigation` | `synui::widgets` | `BottomNavigation` | yes | stable |
| `synui::widgets::Dialog` | `synui::widgets` | `Dialog` | yes | stable |
| `synui::widgets::BottomSheet` | `synui::widgets` | `BottomSheet` | yes | stable |
| `synui::widgets::SnackBar` | `synui::widgets` | `SnackBar` | yes | stable |
| `synui::widgets::Icon` | `synui::widgets` | `Icon` | yes | stable |
| `synui::widgets::Label` | `synui::widgets` | `Label` | yes | stable |
| `synui::widgets::Image` | `synui::widgets` | `Image` | yes | stable |
| `synui::widgets::MenuItem` | `synui::widgets` | `MenuItem` | yes | stable |
| `synui::widgets::Menu` | `synui::widgets` | `Menu` | yes | stable |
| `synui::widgets::MenuBar` | `synui::widgets` | `MenuBar` | yes | stable |
| `synui::widgets::ContextMenu` | `synui::widgets` | `ContextMenu` | yes | stable |
| `synui::widgets::Styled` | `synui::widgets` | `Styled` | yes | stable |
| `synui::style::StyleInput` | `synui::style` | `StyleInput` | yes | stable |
| `synui::platform::Window` | `synui::platform` | `Window` | yes | stable |

## Stable export contracts

### `synui::view::Node::text`

- **Module path / type / Prelude:** `synui::view`; `Node`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Node::text(text)` with a value convertible to `String`; compose it as a child of `Node::container` or return it from an `Element` descriptor. Optional accessibility metadata belongs to the enclosing Element contract.
- **Errors:** `ElementError` is returned by validation or mount; missing text is rejected before Engine access.
- **App Context ownership and thread:** `AppContext` owns the mounted Element tree; text has no external mutable resource. Build, update, and unmount run on the owning App UI thread.
- **Mount / update / unmount:** Node composition resolves identity at mount, updates from the next Node snapshot, and unmounts with its parent.
- **Identity and events:** automatic positional identity; use `BusinessKey` only for repeated sibling text nodes. Text is non-interactive and declares no event slot.
- **Accessibility:** optional `Accessibility` role/name/value metadata is carried by the Element contract.
- **Advanced Engine replacement:** no replacement is needed; use the canonical `synui::view::Node::text`. Retained Engine text widgets are internal and are not ordinary imports.

### `synui::view::Node::container`

- **Module path / type / Prelude:** `synui::view`; `Node`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Node::container(children)` with an iterator of `Node`; use Node row/column composition and `Node::key(BusinessKey)` for repeated dynamic children. `Node::row` / `Node::column` provide direction only (no CSS). For padding / gap / background / align on the **same** flex host as children, use [`Styled::column`](#synuiwidgetsstyled) / `Styled::row`; to wrap any existing node in a style host use `Node::styled(css)`.
- **Errors:** `ElementError` reports duplicate `BusinessKey`, invalid child metadata, or mount failures.
- **App Context ownership and thread:** `AppContext` owns composed child Elements and their retained lifecycle. Build and lifecycle operations run on the owning App UI thread.
- **Mount / update / unmount:** children mount, update, and unmount as a subtree; omitted children are released by the Element lifecycle.
- **Identity and events:** automatic child-slot identity; repeated dynamic children use `BusinessKey`, never numeric keys. The container forwards child event routing and does not expose `WidgetStore`.
- **Accessibility:** accessibility metadata may describe the container and logical descendants.
- **Advanced Engine replacement:** no replacement is needed; use the canonical `synui::view::Node::container` and Node row/column composition. Engine layout and render types remain private.

### `synui::widgets::Button`

- **Module path / type / Prelude:** `synui::widgets`; `Button`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Button::new(label)`, then configure `on_press(Action<()>)` or `on_click` with an application event closure. Optional `.tooltip(text)` for hover tips (see book `tooltips.md`). The typed Action is created by `AppContext`.
- **Errors:** press dispatch returns `AppError` through the typed Action path; `ElementError` covers mount failures.
- **App Context ownership and thread:** the mounted Button borrows or registers a typed Action owned by `AppContext`; press dispatch and state mutation are UI-thread gated by that context.
- **Mount / update / unmount:** Button mounts as an Element, updates its label/handler, and unregisters generated handlers on unmount.
- **Identity and events:** automatic identity; repeated buttons use a stable `BusinessKey`. `Event::Press` carries `Action<()>` or an `App`/`EventContext` closure; there is no numeric action ID.
- **Accessibility:** maps to `role=button` and supports semantic click/focus actions.
- **Advanced Engine replacement:** no replacement is needed; use the canonical `synui::widgets::Button`. `WidgetStore` and `ViewEngine` are not part of the contract.

### `synui::widgets::TextInput`

- **Module path / type / Prelude:** `synui::widgets`; `TextInput`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `TextInput::new(placeholder)`, optional `.password(true)` to mask glyphs (value stays the real string), then use `controlled(State<String>, Action<String>)`, `uncontrolled(initial, Option<Action<String>>)` or `with_config(InputConfig<String>)`.
- **Errors:** `InputConfigError` rejects unsupported or invalid input before native store access.
- **App Context ownership and thread:** controlled values and typed change Actions are `AppContext`-owned; uncontrolled values are native-owned after mount. Input polling and Action invocation occur on the owning UI thread.
- **Mount / update / unmount:** initial configuration is validated at mount; updates preserve the controlled/uncontrolled contract; unmount releases input routes.
- **Identity and events:** automatic identity; repeated fields require a `BusinessKey`. `Event::Change` dispatches `Action<String>` through `InputConfig`.
- **Accessibility:** maps to text-input semantics with optional name/value/state metadata; password mode reports a masked value to a11y (not plaintext).
- **Advanced Engine replacement:** no replacement is needed; use `synui::widgets::TextInput` and `InputConfig<String>`. Legacy `TextBinding` is only a compatibility mapping.

### `synui::widgets::TextArea`

- **Module path / type / Prelude:** `synui::widgets`; `TextArea`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `TextArea::new(label)` (`label` is accessibility name only — native TextArea has no placeholder glyph), then use `controlled(State<String>, Action<String>)`, `uncontrolled(initial, Option<Action<String>>)` or `with_config(InputConfig<String>)`.
- **Errors:** `InputConfigError` rejects unsupported or invalid input before native store access.
- **App Context ownership and thread:** controlled values and typed change Actions are `AppContext`-owned; uncontrolled values are native-owned after mount. Input polling and Action invocation occur on the owning UI thread.
- **Mount / update / unmount:** initial configuration is validated at mount; updates preserve the controlled/uncontrolled contract; unmount releases input routes.
- **Identity and events:** automatic identity; repeated fields require a `BusinessKey`. `Event::Change` dispatches `Action<String>` through `InputConfig`.
- **Accessibility:** maps to text-input semantics; `new(label)` sets the accessibility name.
- **Advanced Engine replacement:** no replacement; use `synui::widgets::TextArea` and `InputConfig<String>`. Legacy `TextAreaBinding` is removed (use `InputConfig` + `Action<String>`).

### `synui::widgets::Checkbox`

- **Module path / type / Prelude:** `synui::widgets`; `Checkbox`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Checkbox::new(checked)`, or configure `controlled(State<bool>, Action<bool>)`, `uncontrolled(initial, Option<Action<bool>>)` or `with_config(InputConfig<bool>)`.
- **Errors:** `InputConfigError` and `ElementError` report invalid configuration or lifecycle failures.
- **App Context ownership and thread:** controlled `State<bool>` and `Action<bool>` belong to `AppContext`; uncontrolled state is native-owned after mount. Change dispatch is restricted to the owning UI thread.
- **Mount / update / unmount:** configuration validates at mount, updates preserve input mode, and unmount removes the typed route.
- **Identity and events:** automatic identity; repeated controls use `BusinessKey`. `Event::Change` dispatches a typed boolean Action.
- **Accessibility:** maps to `role=checkbox` with checked and disabled state metadata.
- **Advanced Engine replacement:** no replacement is needed; use `synui::widgets::Checkbox` and `InputConfig<bool>`. Legacy `BoolBinding` is not re-exported.

### `synui::widgets::Switch`

- **Module path / type / Prelude:** `synui::widgets`; `Switch`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Switch::new(on)`, or configure `controlled` / `uncontrolled` / `with_config(InputConfig<bool>)`.
- **Errors:** `InputConfigError` and `ElementError` report invalid configuration or lifecycle failures.
- **App Context ownership and thread:** controlled `State<bool>` and `Action<bool>` belong to `AppContext`; uncontrolled state is native-owned after mount.
- **Mount / update / unmount:** mounts native Switch (`kind = "switch"`); must not silently degrade to Checkbox.
- **Identity and events:** automatic identity; `Event::Change` dispatches a typed boolean Action.
- **Accessibility:** maps to `role=switch` with checked state metadata.
- **Advanced Engine replacement:** no replacement; use `synui::widgets::Switch`.

### `synui::widgets::Radio`

- **Module path / type / Prelude:** `synui::widgets`; `Radio`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Radio::new(label, selected)`, or configure controlled/uncontrolled `InputConfig<bool>`.
- **Errors:** `InputConfigError` and `ElementError` report invalid configuration or lifecycle failures.
- **App Context ownership and thread:** label is Element-owned; controlled value/Action are `AppContext`-owned.
- **Mount / update / unmount:** mounts native Radio; grouping is application-composed.
- **Identity and events:** automatic identity; `Event::Change` dispatches a typed boolean Action.
- **Accessibility:** checkbox-equivalent checked metadata until a dedicated Radio role exists.
- **Advanced Engine replacement:** no replacement; use `synui::widgets::Radio`.

### `synui::widgets::LinearProgress`

- **Module path / type / Prelude:** `synui::widgets`; `LinearProgress`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `LinearProgress::new(value)` (`0.0..=1.0`); optional `indeterminate(true)`.
- **Errors:** `ElementError` on lifecycle failures.
- **App Context ownership and thread:** properties are Element-owned; no InputConfig.
- **Mount / update / unmount:** display-only property sync.
- **Identity and events:** automatic identity; no Change Action.
- **Accessibility:** maps to `role=progress-bar` with percent value metadata.
- **Advanced Engine replacement:** no replacement; use `synui::widgets::LinearProgress`. See also `CircularProgress`.

### `synui::widgets::CircularProgress`

- **Module path / type / Prelude:** `synui::widgets`; `CircularProgress`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `CircularProgress::new(value)`; optional `indeterminate(bool)` / `value(f32)`.
- **Errors:** `ElementError` on lifecycle failures.
- **App Context ownership and thread:** properties are Element-owned; no InputConfig.
- **Mount / update / unmount:** display-only property sync. Native paint draws a track plus a determinate arc from `value` (0..=1), or a short rotating arc when `indeterminate` is true.
- **Identity and events:** automatic identity; no Change Action.
- **Accessibility:** maps to `role=progress-bar` with percent value metadata.
- **Advanced Engine replacement:** no replacement; use `synui::widgets::CircularProgress`.

### `synui::widgets::Card`

- **Module path / type / Prelude:** `synui::widgets`; `Card`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Card::new(children)`; optional `elevation(u8)` (default `1`).
- **Errors:** `ElementError` on lifecycle failures.
- **App Context ownership and thread:** elevation and children are Element-owned.
- **Mount / update / unmount:** mounts native Card; children sync each update.
- **Identity and events:** automatic identity; children carry their own events.
- **Accessibility:** generic host labeled `Card`.
- **Advanced Engine replacement:** no replacement; use `synui::widgets::Card`.

### `synui::widgets::ListItem`

- **Module path / type / Prelude:** `synui::widgets`; `ListItem`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `ListItem::new(title)`; optional `subtitle`, `trailing(Nodes)`, `on_press(Action<()>)`.
- **Errors:** `ElementError` on lifecycle failures.
- **App Context ownership and thread:** title/subtitle/trailing Element-owned; press Action is `AppContext`-owned.
- **Mount / update / unmount:** mounts native ListItem; trailing children sync; press routes like Button.
- **Identity and events:** automatic identity; `Event::Press` dispatches `Action<()>`.
- **Accessibility:** `role=list-item` with title name and optional Click.
- **Advanced Engine replacement:** no replacement; use `synui::widgets::ListItem` (not ActionId).

### `synui::widgets::NumberInput`

- **Module path / type / Prelude:** `synui::widgets`; `NumberInput`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `NumberInput::new(value)`, then configure controlled/uncontrolled input or `with_config(InputConfig<f64>)`; apply `range(min, max)` for bounds.
- **Errors:** `InputConfigError::OutOfRange` and `ElementError` reject invalid values without partial native mutation.
- **App Context ownership and thread:** range and `State<f64>`/`Action<f64>` configuration are owned by `AppContext`; value changes and range validation run on the owning UI thread.
- **Mount / update / unmount:** range and input mode are validated at mount/update; unmount releases typed routes.
- **Identity and events:** automatic identity; repeated numeric inputs use `BusinessKey`. `Event::Change` dispatches a typed numeric Action, never a numeric action identifier.
- **Accessibility:** maps to slider/value semantics with optional range metadata.
- **Advanced Engine replacement:** no replacement is needed; use `synui::widgets::NumberInput` and `InputConfig<f64>`. Numeric action identifiers remain denied.

### `synui::widgets::Select`

- **Module path / type / Prelude:** `synui::widgets`; `Select`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Select::new(options, selected)`, or configure `controlled(State<ChoiceId>, Action<ChoiceId>)`, `uncontrolled(initial, Option<Action<ChoiceId>>)` or `with_config(InputConfig<ChoiceId>)`; create IDs with `ChoiceId::new(index)`.
- **Errors:** `InputConfigError::InvalidChoice` rejects an out-of-range selection.
- **App Context ownership and thread:** choices are application data; selected `ChoiceId` and typed Action are `AppContext`-owned when controlled. Choice changes dispatch on the owning UI thread.
- **Mount / update / unmount:** choices and selection are validated at mount/update; unmount removes typed routes.
- **Identity and events:** automatic identity; repeated Select controls use `BusinessKey`. `Event::Change` dispatches `Action<ChoiceId>`; selection is not a global registry.
- **Accessibility:** exposes choice/value semantics through the Element accessibility contract.
- **Advanced Engine replacement:** no replacement is needed; use `synui::widgets::Select` and `ChoiceId`. Legacy `SelectionBinding` maps to Model/InputConfig in compatibility metadata only.

### `synui::widgets::DataTable`

- **Module path / type / Prelude:** `synui::widgets`; `DataTable`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `DataTable::new()`, configure `columns(TableColumn…)` / `rows(n)` / optional `filter` / `freeze_leading` / `span`/`spans` (data-coordinate merge cells), supply `cell(|row, col| …)`, and wire `on_select` / `on_sort` / `on_edit` Actions. Helpers `logical_dimensions` / `mounted_position` remain for a11y smoke. Optional App-owned `VirtualDataTableModel` may still drive bounded `CellRange` requests.
- **Errors:** `ElementError` / `VirtualModelError` reject invalid ranges, coordinates, and requests before Engine access.
- **App Context ownership and thread:** columns, cell factory, filter query, and select/sort/edit Actions are `AppContext`-owned; mount sync runs on the owning UI thread.
- **Mount / update / unmount:** `EngineAdapter` mounts native `synui_widgets::DataTable`; `sync_virtual_mounts` keeps the viewport cell window bounded.
- **Identity and events:** cell and row identities use logical coordinates and `BusinessKey`, never materialized positional keys. `SelectionChange`, `SortDescriptor`, and typed edit `Action<(usize,usize,String)>` carry interaction.
- **Accessibility:** exposes grid role plus logical and mounted row/column metadata for the accessibility bridge without O(rows) Element nodes.
- **Advanced Engine replacement:** no replacement is needed; use prelude `DataTable`. WidgetStore/ViewEngine table internals remain private. Server filter/pivot/regex remain deferred. Merge spans are declarative only (no edit-merge UI).

### `synui::widgets::Scroll`

- **Module path / type / Prelude:** `synui::widgets`; `Scroll`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Scroll::new(children)` and optionally `scroll_chaining(bool)` (default true). Sticky/fixed children use [`Styled`](#synuiwidgetsstyled).
- **Errors:** `ElementError` for lifecycle failures; invalid `Styled` CSS surfaces `InvalidProperty` / `StyleDiagnostic`.
- **App Context ownership and thread:** child Nodes and Actions remain App-owned; `ScrollView` host lifecycle is UI-thread only.
- **Mount / update / unmount:** `EngineAdapter` mounts a native `ScrollView` and reconciles children through ordinary Element identity.
- **Identity and events:** automatic identity; repeated scroll children use `BusinessKey`. Wheel chaining is host-owned.
- **Accessibility:** generic host semantics; children declare their own labels/roles.
- **Advanced Engine replacement:** no replacement is needed for full-mount scroll; transparent virt of arbitrary flex/grid remains deferred.

### `synui::widgets::VirtualList`

- **Module path / type / Prelude:** `synui::widgets`; `VirtualList`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `VirtualList::new(row_count)`, configure `item_extent`, optional `measure_heights`, `item(|index| label)`, `on_select(Action<usize>)`, `on_activate(Action<usize>)`, and optional `on_context(Action<(usize, f64, f64)>)`.
- **Errors:** `ElementError` for lifecycle failures; applications must not downcast `WidgetStore`.
- **App Context ownership and thread:** select/activate/context Actions are `AppContext`-owned; mount window sync runs on the UI thread.
- **Mount / update / unmount:** `EngineAdapter` mounts `ListView`; `sync_virtual_mounts` keeps the viewport bounded (10k → ≤~40 mounts).
- **Identity and events:** automatic identity for the list host; rows are not one Element each. Typed `Action<usize>` carries selection/activation; secondary-click opens the first mounted `ContextMenu` and fires `on_context`.
- **Accessibility:** list role plus logical `row_count` without O(rows) Element nodes.
- **Advanced Engine replacement:** no replacement is needed for fixed-extent lists; pair with prelude `MenuBar` / `ContextMenu` for menus.

### `synui::widgets::VirtualTree`

- **Module path / type / Prelude:** `synui::widgets`; `VirtualTree`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `VirtualTree::new(roots: impl IntoIterator<Item = TreeNode>)`, wire `on_expand` / `on_select` / `on_activate` as `Action<String>`.
- **Errors:** `ElementError` for lifecycle failures; applications must not downcast `WidgetStore`.
- **App Context ownership and thread:** root nodes and Actions are `AppContext`-owned; expand sync runs on the UI thread.
- **Mount / update / unmount:** `EngineAdapter` mounts `TreeView`; labels use the paint path so the virtual window stays bounded.
- **Identity and events:** automatic identity for the tree host; node ids are application strings. Typed `Action<String>` carries expand/select/activate.
- **Accessibility:** tree role plus logical `row_count` without O(nodes) Element nodes.
- **Advanced Engine replacement:** no replacement is needed for Wave B MVP; menus are sibling prelude Elements (Phase 46).

### `synui::widgets::SplitPane`

- **Module path / type / Prelude:** `synui::widgets`; `SplitPane`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `SplitPane::new(direction, ratio, primary, secondary)` with `SplitDirection::{Horizontal, Vertical}`; optional `on_ratio(Action<f64>)`.
- **Errors:** `ElementError` for lifecycle failures; floating dock is not supported.
- **App Context ownership and thread:** child Nodes and ratio Action are `AppContext`-owned; host lifecycle is UI-thread only.
- **Mount / update / unmount:** `EngineAdapter` mounts native `SplitPane` and reconciles the two children through ordinary Element identity.
- **Identity and events:** automatic identity; optional ratio Action. Nesting is fixed two-pane trees only.
- **Accessibility:** generic host; pane children declare their own semantics.
- **Advanced Engine replacement:** no replacement is needed for fixed nested splits; dock/cross-window panes remain deferred.

### `synui::widgets::TabBar`

- **Module path / type / Prelude:** `synui::widgets`; `TabBar`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `TabBar::new(labels)`, configure controlled `selected(State<usize>)` and/or `on_change(Action<usize>)`. Pane content is application-composed (switch child Nodes; MVP does not force dual-pane keep-alive).
- **Errors:** `ElementError` for lifecycle failures.
- **App Context ownership and thread:** labels and selection State/Action are `AppContext`-owned; selection polling is UI-thread only.
- **Mount / update / unmount:** `EngineAdapter` mounts native `TabBar`; pane visibility remains application-owned.
- **Identity and events:** automatic identity for the tab host; `Action<usize>` carries tab changes.
- **Accessibility:** tab/selected-pane semantics via host selection index.
- **Advanced Engine replacement:** no replacement is needed for Wave B MVP.

### `synui::widgets::RichDocument`

- **Module path / type / Prelude:** `synui::widgets`; `RichDocument`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `RichDocument::new(BlockDoc)` and optional `on_change(Action<BlockDoc>)`. Prelude also re-exports `Block`, `BlockDoc`, `TextSpan`.
- **Errors:** `ElementError` for lifecycle failures.
- **App Context ownership and thread:** document State/Action are App-owned; while focused the host owns the edit buffer and poll commits changes on the UI thread.
- **Mount / update / unmount:** `EngineAdapter` mounts `RichEdit`; unfocused updates may `set_doc`; focused updates do not clobber the caret.
- **Identity and events:** automatic host identity; `Action<BlockDoc>` carries edits. Shortcuts B/I/U/Z are host-owned.
- **Accessibility:** text-input role with plain-text value metadata.
- **Advanced Engine replacement:** no replacement for Wave C MVP; OT/HTML/nested tables remain deferred.

### `synui::widgets::AppBar`

- **Module path / type / Prelude:** `synui::widgets`; `AppBar`; yes; **stable**.
- **Construction:** `AppBar::new(title).trailing([...Nodes])`.
- **Mount:** native `AppBar`; trailing children reconcile as Element children.

### `synui::widgets::NavigationDrawer`

- **Module path / type / Prelude:** `synui::widgets`; `NavigationDrawer`; yes; **stable**.
- **Construction:** `NavigationDrawer::new(children).open(bool).on_open_change(Action<bool>)`.
- **Mount:** native drawer; children visible when open.

### `synui::widgets::BottomNavigation`

- **Module path / type / Prelude:** `synui::widgets`; `BottomNavigation`; yes; **stable**.
- **Construction:** `BottomNavigation::new(labels).selected(i).on_change(Action<usize>)`.
- **Mount:** native bottom nav; selection polled each frame.

### `synui::widgets::Dialog` / `BottomSheet` / `SnackBar`

- **Prelude:** yes; **stable**.
- **Construction:** open/visible State + `on_dismiss(Action<()>)` + content/message.
- **Mount:** native overlays; dismiss fires when open/visible transitions true→false.

### `synui::widgets::Icon` / `Image`

- **Prelude:** yes; **stable**.
- **Construction:** `Icon::new(name).size(px)` — short alias (`home`) or `material-symbols:*`; curated set lazy-loads. Optional `.tooltip(text)`. Custom: `Window::with_icon(id, path)` then `Icon::new(id)`. Tint follows style cascade `color` / theme `on_surface` (see book `icons.md` / `tooltips.md`).
- **Construction:** `Image::new(id).size(w, h)`.
- **Platform:** preload with `Window::with_image(id, path)` (PNG/JPEG/GIF/WebP **or** SVG). SVG stays vector: painted into layout bounds each frame (crisp scaling) and `currentColor` is replaced with theme `on_surface` for icon-style assets.

### `synui::widgets::Label`

- **Prelude:** yes; **stable**.
- **Construction:** `Label::new(text)`, optional `.truncate(true).max_lines(n)`, optional `.a11y_name(name)`.
- **Mount:** `EngineAdapter` kind `"label"` → native `Label`. `Node::text` remains available for plain text without truncate/a11y overrides.
- **Accessibility:** role=`label`; name defaults to display text unless overridden.
- **Boundary:** `truncate(true)` enables paint-width ellipsis when measure sees a width budget (`…`); optional `max_lines` still applies a line-budget ellipsis in the facade adapter.

### `synui::widgets::MenuBar`

- **Module path / type / Prelude:** `synui::widgets`; `MenuBar` (+ `Menu` / `MenuItem`); `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `MenuBar::new([Menu::new(title, items)])` and `on_action(Action<String>)` (item id).
- **Errors:** `ElementError` for lifecycle failures; applications must not downcast `WidgetStore`.
- **App Context ownership and thread:** menus and Actions are `AppContext`-owned; overlay sync is UI-thread only.
- **Mount / update / unmount:** `EngineAdapter` mounts native `MenuBar`; panels use the overlay stack (`MenuPanel::apply_pending`).
- **Identity and events:** automatic identity; `Action<String>` carries the selected item id. F10/Alt keyboard is host-owned.
- **Accessibility:** generic focusable host labeled as a menu bar.
- **Advanced Engine replacement:** none for in-app menus. Platform `DesktopMenu`/muda remains an optional shell and is **not** a prelude Element. GTK true menus are hard-locked.

### `synui::widgets::ContextMenu`

- **Module path / type / Prelude:** `synui::widgets`; `ContextMenu`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `ContextMenu::new(items)`, `on_action(Action<String>)`, optional `show_at(x, y)`.
- **Errors:** `ElementError` for lifecycle failures.
- **App Context ownership and thread:** items and Actions are `AppContext`-owned; show/hide is UI-thread only.
- **Mount / update / unmount:** `EngineAdapter` mounts native `ContextMenu`. A `VirtualList` secondary-click opens the first mounted context menu at the pointer.
- **Identity and events:** automatic identity; `Action<String>` carries the selected item id.
- **Accessibility:** generic host labeled as a context menu.
- **Advanced Engine replacement:** none for in-app context menus.

### `synui::widgets::Styled`

- **Module path / type / Prelude:** `synui::widgets`; `Styled`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Styled::new(css, children)`, `Styled::column(css, children)` / `Styled::row(css, children)` (injects `flex-direction` when omitted so gap/align apply to children), or `Styled::sticky_top(top, child)` for vertical sticky MVP metadata. Any `Node` can also call `.styled(css)` to wrap itself in this host.
- **Errors:** invalid CSS text fails validation before native style commit (`StyleDiagnostic` / `ElementError::InvalidProperty`).
- **App Context ownership and thread:** CSS text and child Nodes are application-owned; style apply is UI-thread lifecycle.
- **Mount / update / unmount:** mounts a `StyledBox` host; children reconcile through Element identity.
- **Identity and events:** automatic identity; sticky/fixed/z-index are layout metadata, not event slots.
- **Accessibility:** generic host; children declare their own semantics.
- **Advanced Engine replacement:** no replacement is needed for sticky/fixed Wave A; Style Cascade v3 remains deferred.

### `synui::style::StyleInput`

- **Module path / type / Prelude:** `synui::style`; `StyleInput`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** construct validated style input with `StyleInput` builders such as `StyleInput::text(...)`, and apply it through an Element host's `StyleCache`; use `Style`, `ValidatedStyle`, and `StyleApplyResult` as documented support types.
- **Errors:** `StyleDiagnostic`/`StyleApplyResult` report invalid input without panic or partial style commit.
- **App Context ownership and thread:** each Element host owns its `StyleCache`; application code supplies `StyleInput`. Style application occurs during the Element's UI-thread lifecycle.
- **Mount / update / unmount:** validate before commit; invalid updates retain the last successful style; unmount drops the host cache.
- **Identity and events:** `StyleCache` belongs to one Element host and is not a global stylesheet. Style has no input event; visual changes follow Node/Element updates.
- **Accessibility:** Style does not invent semantic metadata; Element accessibility remains explicit.
- **Advanced Engine replacement:** no replacement is needed; use `synui::style::{Style, StyleInput, StyleCache, StyleDiagnostic}`. `style_css`, class, and theme Engine paths are non-ordinary and denied.

### `synui::platform::Window`

- **Module path / type / Prelude:** `synui::platform`; `Window`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction and configuration:** call `Window::new(title)` (optional `.with_second_window(title)`, `.with_image(id, path)`, `.with_lottie(id, path)`) and pass it to `RunLoop::run` or `App::run`; `RunLoop::new()` owns the native host loop. A second window shares the same App-owned root builder / `State`. Tray/DesktopMenu remain shell boundaries. Web/mobile host crates remain explicit platform boundaries.
- **Errors:** `PlatformError` reports App, Element, or event-loop failures.
- **App Context ownership and thread:** `RunLoop` owns the native host while `AppContext` owns application resources; Window and RunLoop execution are native UI-thread operations.
- **Mount / update / unmount:** Window configuration is consumed by `RunLoop::run`; the host mounts, updates, and unmounts the Node root.
- **Identity and events:** Window is host configuration; Element identity remains automatic within `AppContext`. `RunLoop` routes native events to typed Element/App handlers.
- **Accessibility:** Window and child Elements may declare semantic accessibility metadata.
- **Advanced Engine replacement:** no replacement is needed; use `synui::platform::{Window, RunLoop}` or `App::run`. Web/mobile host integration is a platform boundary, not a Prelude component.

## Stable support types and module index

The stable components above use these documented support types. Their canonical module paths are stable facade paths; none requires a direct Engine crate.

| Module | Stable support types and role | Prelude |
|---|---|---:|
| `synui::app` | `App`, `AppConfig`, `AppContext`, `State`, `Action`, `LifecycleState`, `AppError`; application ownership and typed event state | yes |
| `synui::view` | `Node`, `Element`, `ElementSpec`, `ElementKind`, `BusinessKey`, `IdentityPath`, `Accessibility`, `Event`, `ElementError`; composition and lifecycle | yes |
| `synui::widgets` | inputs + `DataTable`/`VirtualList`/`VirtualTree`/`SplitPane`/`TabBar`/`RichDocument`/`AppBar`/`NavigationDrawer`/`BottomNavigation`/`Dialog`/`BottomSheet`/`SnackBar`/`Icon`/`Label`/`Image`/`MenuBar`/`ContextMenu`/`Scroll`/`Styled`; stable controls | yes |
| `synui::model` | `Model`, `ModelHandle`, `VirtualDataTableModel`, `VirtualDataTableAdapter`, `CellRange`, `CellRequest`, `SelectionChange`, `SortDescriptor`, `VirtualModelError`; App-owned data | yes |
| `synui::style` | `Style`, `StyleInput`, `ValidatedStyle`, `StyleCache`, `StyleApplyResult`, `StyleDiagnostic`, `StyleSeverity`; validated non-panicking style | yes |
| `synui::platform` | `Window`, `RunLoop`, `PlatformError`; native host boundary | yes |

`AppContext`, `State`, `Action`, `Element`, `ElementSpec`, `Model`, and the support types are documented as stable application contracts only where the Manifest stable component records consume them. Internal storage, downcasts, and Engine adapters are not implied by these re-exports.

## Allowlist boundaries that are not stable Prelude exports

Phase 43–55 promoted Scroll/List/Table/Tree/Split/Tab/RichDocument/nav/overlays/Icon/Image/Label, in-app MenuBar/ContextMenu, Switch/Radio/LinearProgress/CircularProgress, Card/ListItem, and TextArea to stable Prelude (plus Label paint-width ellipsis and DataTable merge spans in Wave I; gallery sticky/fixed smoke in Wave N), plus `Window::with_second_window`. Phase 51 removed the `synui` `legacy` feature; Phase 52 deleted the in-tree Engine gold archive. Phase 56 wired `Lottie` / `MorphingShape`; Phase 57 promoted both to stable Prelude and added `Window::with_lottie`. Phase 58 hardened text geometry vs paint layout caches and pre-parses shared Lottie `Composition` in the host cache (per-widget players remain private). Remaining non-Prelude boundaries are hard-locked items pending product sign-off (dock / Cascade v3 / OT / GTK true menus) and optional platform shells (DesktopMenu/muda).

### Stable animation hosts

#### `synui::widgets::Lottie`

- **Module path / type / Prelude:** `synui::widgets`; `Lottie`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction:** Prefer `Lottie::from_asset(id)` after `Window::with_lottie(id, path)`; or `Lottie::from_json(json)` / `Lottie::new(json)` (alias) then `size(w,h)` / `playing(bool)`. Inline JSON is validated at mount (`ElementError::InvalidProperty` on failure). Asset hosts mount without validation; missing preload paints empty (no panic). Preload I/O or parse failures return `PlatformError::Lottie` before the event loop starts. The host `LottieCache` stores pre-parsed `Composition` (cloned into each widget’s private `LottiePlayer`); Players/Renderers are never shared across widgets.
- **Behavior:** Display-only; `tick_animations` advances while `playing`. The facade gallery mounts Animation-category Lottie/Morph with `playing(true)` (motion only while that category is mounted); prefer `playing(false)` in other dense trees if continuous present is unwanted.

#### `synui::widgets::MorphingShape`

- **Module path / type / Prelude:** `synui::widgets`; `MorphingShape`; `synui::prelude::*`: **yes**; stability: **stable**.
- **Construction:** `MorphingShape::new()` then `size` / `playing` / `speed`.
- **Behavior:** Display-only MD3 corner morph; ticks while `playing`.

For every non-stable record, the adapter must preserve AppContext ownership, UI-thread callbacks, BusinessKey identity, typed events, returned errors, and role/name/value/state/action metadata where applicable. `ElementError` and `VirtualModelError` remain the boundary errors; an internal WidgetStore downcast is never a replacement.
## Minimal facade-only example

This example uses only the ordinary facade and demonstrates App Context ownership, typed Action, Node identity, and the stable Window host. It corresponds to the `counter` Manifest record.

```rust
use synui::prelude::*;

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let cx = app.context();
    let count = cx.state(0_i32)?;
    let increment = cx.action({
        let count = count.clone();
        move |_, (): ()| count.set(count.get()? + 1)
    })?;
    let button = Button::new("+1").on_press(increment);
    app.run(Window::new("counter"), move |_| {
        Ok(Node::container([
            Node::text(format!("Count: {}", count.get()?)),
            button.node(),
        ]))
    })
}
```

Validation boundary: `cargo check -p counter` is a facade-only package check; `cargo test -p synui --test counter_prelude_fixture` is the corresponding facade fixture. Native window execution is separate from headless validation.

## External Element and model examples

An external component implements `Element::describe` and returns `ElementSpec`; it must not add a `View` variant or downcast `WidgetStore`. Repeated children use `BusinessKey`, and data components use `VirtualDataTableModel`/`CellRange` rather than materializing a whole table.

```rust
use synui::prelude::*;

let table = DataTable::new()
    .columns([
        TableColumn::new("id", "ID", 80.0),
        TableColumn::new("name", "Name", 160.0),
    ])
    .rows(100_000)
    .cell(|row, col| format!("{row},{col}"))
    .on_select(select_action);
// Optional: pair App-owned VirtualDataTableModel with CellRange for bounded requests.
```

`DataTable` mounts a native virtualized host; cell factories and typed Actions own interaction. Bound mount evidence: `cargo test -p synui --lib view::tests::facade_data_table_mounts_native_host_with_bounded_cells`.

## Style diagnostic example

```rust
use synui::prelude::*;

let mut cache = StyleCache::default();
assert!(cache.apply(StyleInput::text("padding:8;gap:4")).applied);
let result = cache.apply(StyleInput::text("unknown-property:1"));
assert!(result.is_diagnostic_success());
assert_eq!(result.diagnostic.as_ref().unwrap().code, "unknown-property");
// The previous successful style remains committed.
```

Style validation is a facade-only `StyleDiagnostic` boundary. It is not evidence that the non-ordinary `style_css`, class, or theme Engine paths are stable.

## Non-ordinary symbols and boundaries

The following symbols are documented only to make the boundary auditable. Every row is explicitly **non-ordinary**: it is not a normal `synui::prelude` import and does not establish a Stable Component. Each row links to its stable replacement or to the explicit deferral reason in this reference or the [migration guide](migration-v0.2.md).

| Symbol/family | Non-ordinary purpose | Stable replacement or explicit deferral |
|---|---|---|
| `WidgetStore` | **non-ordinary: retained Engine storage / advanced integration** | Replace with [`synui::view::Node::container`](#synuiviewnodecontainer), [`synui::view::Element`](#allowlist-boundaries-that-are-not-stable-prelude-exports), and App-owned Model; no WidgetStore downcast is permitted. |
| `ViewEngine` | **non-ordinary: retained Engine lifecycle** | Replace with [`synui::app::App`](#minimal-facade-only-example), [`synui::platform::RunLoop`](#synuiplatformwindow), and Element lifecycle. |
| Engine layout/render types (`synui_core`, `synui_layout`, `synui_render`, `synui_theme`, `synui_winit`) | **non-ordinary: Engine/platform boundary** | Use the stable [`synui::view`](#synuiviewnodecontainer), [`synui::widgets`](#synuiwidgetsbutton), [`synui::style`](#synuistylestyleinput), [`synui::model`](#synuiwidgetsdatatable), and [`synui::platform::Window`](#synuiplatformwindow) paths. Low-level examples remain explicitly retained Engine integrations. |
| `ActionId` and numeric action identifiers | **non-ordinary: removed legacy dispatch** | Replace with typed [`synui::app::Action`](#stable-support-types-and-module-index) and [`Event::Press`](#synuiwidgetsbutton). |
| `key_scope`, numeric keys, `memo`, `section`, and global scheduling registries | **non-ordinary: removed legacy identity/scheduling** | Replace with automatic identity and [`BusinessKey`](#synuiviewnodecontainer) for repeated siblings. |
| `plugins`, `WidgetPlugin`, and plugin registry/loading symbols | **non-ordinary: Experimental Plugin ABI; preview/hot-reload/example/test-only** | Keep only in labelled experimental paths; the stable replacement for application UI is the [`synui` facade](#stable-component-api-index). `plugin_sample` remains non-production and is not evidence for a Stable Component or Production ABI. |
| `parse_view_json`, `view_from_json`, and JSON View node symbols | **non-ordinary: JSON View Creation; debugging/preview/hot-reload/example/test-only** | Construct typed [`synui::view::Node`](#synuiviewnodetext) or an external [`Element`](#allowlist-boundaries-that-are-not-stable-prelude-exports) in production. JSON construction has no stable v0.2 facade replacement and remains explicitly non-production. |
| `style_css`, class, and global theme paths | **non-ordinary: panic-prone/Engine style boundary** | Use [`synui::style::StyleInput`](#synuistylestyleinput), `StyleCache`, and `StyleDiagnostic`; unsupported theme/class behavior is deferred rather than silently restored. |
| Engine widget families used via `WidgetStore` downcast | **non-ordinary: legacy/Engine component families** | Facade apps use prelude Elements (Scroll/List/Table/Tree/Split/Tab/RichDocument/nav/overlays/Icon/Image/MenuBar/ContextMenu); DesktopMenu/muda is platform shell only. |

## Legacy mappings

The complete v0.1 mapping is machine-readable in `V01_FACADE_COMPATIBILITY` and `LEGACY_VIEW_COMPATIBILITY`. The stable v0.2 names in this reference are:

| Legacy name/family | v0.2 canonical mapping |
|---|---|
| `run_app`, `Application`, `AppDriver` | `synui::app::App` plus `synui::platform::{Window, RunLoop}`; host-specific drivers remain explicit boundaries. |
| global `Signal` | `synui::app::{AppContext, State}` |
| numeric/global actions and `View::on_action` | typed `synui::app::Action` plus `Event` handlers |
| `TextBinding`, `BoolBinding`, `NumberBinding` | `synui::widgets::InputConfig<T>` with `TextInput`, `Checkbox`, or `NumberInput` |
| selection binding | Model selection callbacks; choice inputs use `InputConfig<ChoiceId>` |
| numeric keys, `key_scope`, `View::memo/section` | automatic identity and `synui::view::BusinessKey` |
| legacy `View` snapshots | `synui::view::{Node, Element}`; `adapt_legacy_view` is transitional only |
| `View::style_css`, classes, global themes | `synui::style::{StyleInput, StyleCache, StyleDiagnostic}` where equivalent; otherwise explicit advanced/deferred boundary |
| `ListView` / `Scroll` View variants | `synui::widgets::{VirtualList, Scroll}` (Phase 43) |
| `TreeView` / `Split` / `TabBar` / runtime `DataTable` | `synui::widgets::{VirtualTree, SplitPane, TabBar, DataTable}` (Phase 44) |
| `RichEdit` / AppBar / Drawer / BottomNav / Dialog / Image | `synui::widgets::{RichDocument, AppBar, NavigationDrawer, BottomNavigation, Dialog, BottomSheet, SnackBar, Icon, Image}` (Phase 45) |
| `Label` (truncate / a11y) | `synui::widgets::Label` (Phase 48); `Node::text` remains for plain text |
| `MenuBar` / `ContextMenu` | `synui::widgets::{MenuBar, ContextMenu, Menu, MenuItem}` (Phase 46 in-app); `DesktopMenu`/muda = platform shell |

## Validation commands and boundaries

- **Manifest/document index only:** `python scripts/migration_manifest.py --check` checks package/component coverage and that every Manifest canonical path appears in this API Reference. It does not run examples, GPU, ABI, or Python workloads.
- **Manifest unit tests when present:** `python scripts/test_migration_manifest.py`.
- **Facade contract:** `cargo test -p synui --test compatibility`.
- **Facade documentation:** `cargo doc -p synui --no-deps`.
- **Facade fixture:** `cargo test -p synui --test counter_prelude_fixture --test external_component_fixture`.
- **Engine-bound examples:** low-level `cargo check`/run commands remain separate evidence and do not prove Stable Component exposure.
- **Native ABI smoke:** `cargo test -p synui-abi --test native_abi -- --nocapture` is a Production ABI check, not a facade fixture.
- **Python SDK:** `python scripts/validation_matrix.py --only python-sdk` is an ABI-only Python workload, not a Rust example check.
- **Migration workloads:** `python scripts/validation_matrix.py --only facade-compilation,component-interaction,virtual-data,rich-text-style,headless-render,a11y` reports the six migration workload boundaries independently; a missing a11y bridge is `unavailable`, not `passed`.

The shared canonical index links are maintained by the [Migration Guide](migration-v0.2.md) and the [Migration Manifest](component-api-example-migration.toml). This document does not modify Production ABI, Python SDK, or Engine implementation.

## Wave 7 gallery boundaries

The gallery split is part of the public example contract:

- [`widget_gallery_facade`](../examples/widget_gallery_facade/README.md) is **facade-only**. Its stable evidence is limited to `synui::prelude::*`, App-owned state/actions, typed inputs, `BusinessKey`, stable controls, and `synui::style::{StyleCache, StyleInput, StyleDiagnostic}`.
- Engine `widget_gallery` gold was **deleted** in Phase 52 (workspace removal in Phase 51). Historical `View`/`Signal`/`ViewEngine` sources are not ordinary facade contracts and no longer exist in-tree.
- [`examples/plugin_sample`](../examples/plugin_sample/README.md) remains a minimal ABI sample crate (no SynUI link required for the label export).

Stable replacement and exact commands are recorded in the facade gallery README and the Migration Manifest.
