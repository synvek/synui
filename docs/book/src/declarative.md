# Declarative Views

Use `synui::View` and `ViewEngine` for state-driven UI:

```rust
use synui::{register_action, Signal, View, run_app_signal};

let count = Signal::new(0);
const INC: u64 = 1;
{
    let c = count.clone();
    register_action(INC, move || c.update(|v| *v += 1));
}
run_app_signal(count.clone(), move |n| {
    // Prefer on_action: handlers survive Memo / root skip (Phase 26–29).
    View::column(vec![
        View::label(format!("{n}")),
        View::button("+1").on_action(INC),
    ])
    .style_css("padding:24;gap:12;align-items:center")
})?;
```

**Keys (Phase 25 / 37):**

- Each rebuild, `App` calls `reset_view_key_alloc()` so auto-allocated keys are **stable for static construction order**.
- `View::style` / `style_css` derive a **stable** wrapper key from `child.key ⊕ style fingerprint` (no per-frame `StyledBox` churn).
- Prefer `with_key` for **bindings** and **dynamic list items** (business ids).
- Prefer `key_scope(ns, || …)` + `with_key_local(sibling)` so uniqueness is only required among siblings (Phase 37); absolute `with_key` remains for binding ids.
- Global uniqueness is still enforced by the engine after mixing — debug builds panic on duplicate locals in one scope.

## Style (CSS subset)

Wrap any view with layout/visual style (see [Style](style.md)):

```rust
View::column(vec![/* … */])
    .style_css("padding:24;gap:12;align-items:center;background:surface-container")
```

Default column/row padding is **0** — add padding via `style` / `with_padding`.

`Flex` / `Column` / `Row` paint **transparent** by default (no implicit surface).
Set `background:surface` (or `StyledBox` / `Card`) when you need a filled panel.

## Incremental apply (Phase 25–26)

`ViewEngine::apply` reports `DiffStats` (`needs_layout` / `needs_paint` / `skipped_subtrees`).
`App` only `mark_layout_dirty` when structure or layout props change; text-only patches mark paint
(and use `bump_paint_epoch`, not measure). Unchanged keyed subtrees are fingerprint-skipped.
Survivor widgets detach only when their parent key changes.

### Actions (Phase 26)

```rust
synui::register_action(1, || count.set(count.get() + 1));
View::button("+").on_action(1).with_key(2);
```

`on_press` still works for insert-time closures; prefer `on_action` when the handler must
survive Signal rebuilds / subtree skip.

### DataTable rows (Phase 26)

Pass `Vec` or `Arc<[Vec<String>]>`; shared Arc avoids cloning the matrix on every rebuild.
`DataTable::set_filter_predicate` ANDs an external row predicate with substring filters.

### Memo (Phase 27)

```rust
View::memo(100, || {
    // Signal::get inside is tracked; unchanged deps → builder skipped.
    View::label(format!("{}", count.get())).with_key(1)
});
```

Use `on_action` inside memo (structural clone drops `on_press`). Inspect
`take_memo_stats()` / `memo_stats()` for hits/misses. Dynamic lists: `View::keyed_map`.

### Root skip (Phase 28)

`App` tracks root Signal deps. Mutations to Signals **not** read in `view_fn` skip
build+apply for that frame. Split chrome vs content into separate Signals so Memo
and root deps stay narrow. Menu string ids: `register_action_alias`. Custom
`AppDriver`s should reuse `RootSkipState` (Phase 29).

### Paint damage (Phase 29 / 31)

Paint-only frames use `PaintMode::Damaged`: retain the scene, erase dirty rects,
then `WidgetStore::paint_frame` skips clean subtrees
(`PaintStats.skipped_paint_nodes`). Label/Button text patches use
`mark_paint_dirty_widget`. Structure / full layout / unscoped `mark_paint_dirty`
use `PaintMode::Full` (`scene.reset` + `paint_all`). Success criterion is node
skip — not OS partial present. Debug: `force_full_paint(true)`.

### Layout dirty-subtree (Phase 30)

`mark_layout_dirty_widget` + `LayoutStats.partial_layout_passes` for
structure-stable relayout. Prefer this over `mark_layout_dirty` when only one
leaf's measure changed.

## ListView, TreeView, DataTable, SplitPane, virtual_scroll & menus

```rust
View::list_view(10_000, 32.0).with_sticky_header("Files").with_key(1);
View::list_view(80, 36.0).with_measure_heights().with_key(10);
View::tree_view(vec![TreeNode::new("a", "Alpha")]).with_key(2);
View::data_table(
    vec![("n".into(), "Name".into(), 120.0)],
    vec![vec!["Alice".into()]],
)
.with_spans(vec![(0, 0, 2, 1)]) // Phase 23 merge (data coords)
.with_key(3);
View::split(
    SplitDirection::Horizontal,
    View::tree_view(roots).with_key(4),
    View::data_table(cols, rows).with_key(5),
).with_key(6);
// Nested splits + ratio binding (Phase 22)
View::split(
    SplitDirection::Horizontal,
    View::tree_view(roots).with_key(4),
    View::split(
        SplitDirection::Vertical,
        View::column(vec![
            View::tab_bar(vec!["Table".into(), "Preview".into()], 0).with_key(8),
            View::data_table(cols, rows).with_key(5),
        ]),
        View::label("detail").with_key(9),
    ).with_key(7),
).with_key(6);
// Slot-builder virt (Phase 20/21)
View::virtual_scroll(10_000, 28.0).with_key(17);
engine.set_scroll_slot_builder(store, 17, |i| Box::new(Label::new(format!("row {i}"))));
// Transparent virt — flat View::scroll children (Phase 23)
View::scroll(labels).with_transparent_virt().with_key(18);
```

Slot `0` is always mounted and pinned as a sticky top bar in **slot-builder**
`virtual_scroll` MVP. Transparent virt does **not** pin slot 0.
CSS `position:sticky` on other virtual slots is **not** guaranteed.
Transparent virt is **flat direct children** (row templates) — not arbitrary deep
flex/grid auto-scan. Phase 24: shared `VirtualMountHost` / `sync_virtual_mounts`;
nested scrollables chain wheel events at edges; List/DataTable expose Grid/List
`row_count` a11y without O(rows) nodes.

`DataTable` has a sticky **header** (click to sort; **drag** to reorder within freeze/scroll zones;
**drag column edge** to resize; double-click edge to auto-fit). Freeze leading columns with
`with_freeze_leading(n)`. Read layout via `ViewEngine::table_column_layout` /
`ColumnLayoutBinding`. **Filter MVP:** `set_filter_query` / column filters (case-insensitive
substring); `filtered_count`/`total_count`; `TableFilterBinding`. **Merge MVP:** `CellSpan` /
`with_spans` / `set_table_spans` (data coords; hit/edit → anchor; no freeze-cross; cleared on
sort). List/Table multi-select uses Shift (range) and Ctrl/Cmd (toggle);
`SelectionBinding` is `Signal<HashSet<usize>>`.

Composable List/Tree rows: `ViewEngine::set_list_item_builder` /
`set_tree_item_builder`. Variable height: `set_list_extent_fn`,
`set_list_measure_heights`, or `View::with_measure_heights`.

`SplitPane` is a single divider, two children — drag to resize; min sizes via
`with_split_mins`. Children may themselves be splits (fixed nested layout tree; not a dock).
`SplitRatioBinding` / `TabBinding` sync ratios and tab selection. JSON: `split` with `text`
`h|0.35` or `v|0.4` and two children.

JSON: `data_table` with nested `column` (`id|Title|width`) and `row` (`c0|c1|…`) nodes.

## Focus-protected inputs

Focused `TextField` / `TextArea` / `RichEdit` ignore patch/binding text pushes
so Signal rebuilds do not clobber typing. See [Text Editing](text_editing.md).

**Not supported (Phase 43+):** id selectors / full `@media` / container queries;
skipping `view_fn` local materialization with **no** key/section id; floating dock /
cross-window panes; RichEdit nested tables/HTML/OT/CRDT; DataTable server filter /
pivot / regex; deep flex/grid auto-scan virt; full-scene OS partial present;
GTK real menus; plugin CDN; cold `layout_10k` ≤8ms.

## Style cascade MVP (Phase 35) / v2 (Phase 40)

Narrow inheritance (`color`, `font_size`, `opacity`) plus class-scoped sheets with
Phase 40 specificity, `!important`, `:hover`/`:focus`/`:disabled`, and
`@media (prefers-color-scheme)`. Phase 41: `:focus` is subtree-symmetric with
`:hover`; `AppDriver::on_focus_changed`; `Button` / `TextField` expose real
`is_disabled()` for `:disabled`. Phase 42: Checkbox / Switch / Slider / Dropdown
also implement `is_disabled()`; `View::disabled` covers them.

```rust
engine.set_stylesheet(StyleSheet::from_css(".row { padding:12; height:48 }")?);
View::label("A").class("row");
// local wins over sheet:
View::label("B").class("row").style_css("color:#6750a4");
View::button("Nope").disabled(true).class("btn");
```

Precedence: **local > sheet > inherited**. Layout properties do not inherit.
