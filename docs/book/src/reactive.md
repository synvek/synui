# Reactive Runtime

SynUI drives declarative updates from `Signal` changes. Prefer
**`run_app_signal`** (error-bounded `App` + `Application`) over the deprecated
`run_declarative_signal`.

## Scheduling

```rust
use synui::{register_action, run_app_signal, Signal, View};

let count = Signal::new(0);
const INC: u64 = 1;
{
    let c = count.clone();
    register_action(INC, move || c.update(|v| *v += 1));
}
run_app_signal(count.clone(), move |n| {
    View::column(vec![
        View::label(format!("{n}")),
        // Prefer on_action: handlers survive memo / subtree skip (Phase 26–29).
        // Bare on_press is insert-only and can go stale under View::memo.
        View::button("+").on_action(INC),
    ])
})?;
```

`Signal::set` / `update` demote to `schedule_apply_keys()` when the Signal id is
registered on the host graph (Phase 41); otherwise `schedule_rebuild()`
(`REBUILD_ROOT | SYNC_BINDINGS`). Registration syncs **after** apply — a first-frame
`set` before sync still Rebuilds. Inspect demotions with
`take_signal_schedule_stats()` → `(apply, rebuild)`. Prefer `take_frame_work()` in
custom drivers; `take_scheduled()` is a thin `needs_rebuild()` wrapper.

## Three-tier invalidation (Phase 38/39)

| Tier | When | Host behavior |
|------|------|----------------|
| **Rebuild** | `REBUILD_ROOT` and Apply-only cannot cover dirty | Run `view_fn` + `apply_with_dirty` |
| **Apply-only** | Dirty Signals are all on the Signal graph **and** covered by `View::section` builders; `last_view` cached | Skip root `view_fn`; refresh dirty sections; splice + apply |
| **Paint-only** | `schedule_paint` / `request_paint` / widget `HANDLED` damage | No `view_fn`, no apply; present from store paint dirty |

Decision tree for `App::on_frame` / custom drivers (Phase 39 host closeout):

1. Pull bindings (scoped).
2. `take_frame_work()` then **`land_frame_paint(store, work)`** — `PAINT_WIDGETS` must land on the store (no dual-source present path).
3. If `needs_rebuild_root()` (or hot reload): RootSkip → else try Apply-only → else Rebuild.
4. Else if `needs_apply_only()`: Apply-only (fall back to Rebuild if sections cannot cover).
5. Else: paint/binding only (do **not** early-return before step 2).

Unregistered dirty Signals (read outside `with_key` / section) **force Rebuild**.
Dirty Signals that appear in **root deps** (read in root `view_fn` outside nested
section scopes) also **force Rebuild** — Apply-only cannot refresh shell UI that
baked those values into non-section subtrees. Gallery moves `chrome` / `category` /
body into sections so category switches can demote to Apply-only.

Prefer `on_action` inside sections — Apply-only uses `clone_structure` (drops `on_press`).

Inspect: `App::apply_only_frames()`, gallery/browser `apply_only_frames`,
`take_section_build_stats()`, `schedule_apply_keys()`.

## Dual path: Signal vs paint (Phase 37/38)

| Change | Mechanism |
|--------|-----------|
| App / document state | `Signal::set` → `APPLY_KEYS` when graph-registered, else `REBUILD_ROOT` → Apply-only or Rebuild |
| Ephemeral visuals (selection, caret, hover, scroll, IME Commit) | Widget-local state + scoped `mark_paint_dirty_widget` → Damaged paint |

Selection in RichEdit does **not** go through Signal; the host marks paint dirty
when the event is handled. Do not `Signal::set` for every caret blink.

Bindings use graph-aware sync: `auto_pull_bindings_scoped` pulls only dirty
Signals ∪ focused widgets (`take_binding_pull_stats`). Focused text / textarea /
rich pulls use `Signal::set_noschedule` so keystrokes stay **paint-only** (widget is
source of truth; Signal stays readable for submit without Rebuild/Apply).

Use `take_frame_work()` + `land_frame_paint()` when implementing custom `AppDriver`
types; `take_scheduled()` remains a thin `needs_rebuild()` wrapper.

## Keyed lists and reorder

`ViewEngine` diffs keyed children with an LCS-style pass so reorders can `move`
retained `WidgetId`s instead of remove+insert. Inspect counts via
`engine.stats()` (`inserts`, `removes`, `patches`, `moves`).

Dynamic lists must pin business ids:

```rust
View::column(View::keyed_map(
    items,
    |item| item.id,
    |item| View::label(item.name),
))
```

## Memo (Phase 27)

Wrap expensive static branches in `View::memo` so Signal ticks that do not touch
tracked deps reuse the previous View subtree (no re-allocation / fingerprint).
See [Declarative UI](declarative.md).

## Root dependency skip (Phase 28)

`App` wraps each `view_fn` in a root dep scope. `Signal::set` / `update` record
ids in a dirty set. On the next scheduled frame, if every dirty id is **outside**
the previous root deps, SynUI **skips** `build` and `apply` (bindings / caret /
morph paint still run). Inspect with `App::skipped_frames()` or
`DiffStats.skipped_frames`. Custom drivers: `RootSkipState`.

Side effects that are not read through `Signal::get` inside `view_fn` will not
invalidate the root — put them in a Signal, or call `force_full_rebuild(true)`
while debugging.

**`on_press` risk:** Memo `clone_structure` drops press closures. Prefer
`register_action` + `View::on_action`. Legacy string MenuBar / ContextMenu ids can
`register_action_alias` to the same `ActionId`. Facade apps use prelude
`MenuBar`/`ContextMenu` with typed `Action<String>` item ids instead.

Split large app state into multiple Signals (e.g. chrome vs category) so Memo
and root deps invalidate locally.

## Signal→Widget apply skip (Phase 34–36)

**Phase 36 (preferred):** read Signals then pin the consuming view with `with_key` —

```rust
let n = chrome.get();
View::label(format!("chrome {n}")).with_key(10);
```

Pending `Signal::get` deps are attributed to that key automatically. On rebuild,
`apply_with_dirty` skips keys whose registered Signals miss the dirty set
(`DiffStats.skipped_apply_subtrees`). Dirty hits still apply (no silent stale UI).

**Optional build-skip:** `View::keyed` / `View::memo` still cache structure when deps
are unchanged (`take_keyed_build_stats`) — useful when you want to avoid allocating
the View tree, not required for apply-skip.

**Custom drivers:** use `ReactiveFrame::decide` → build under `begin_dep_scope` →
`set_deps_after_build` → `ReactiveFrame::apply` (see gallery / data_browser).

Unkeyed Signal reads remain on root deps only — those always rebuild the root
`view_fn` when dirty (RootSkip).

## Section build-skip (Phase 37) + Fill hosts (Phase 42)

`View::section(id, || { … })` caches the materialized View when section Signal
deps are unchanged — the builder **does not run** (`take_section_build_stats`).
Inside a section, pending `Signal::get` deps are **merged** into each `with_key`
(not one-shot drained), so one read can attribute multiple leaves.

**Hug vs Fill:** default `View::section` wraps mismatched keys in an Auto-height
Row (chrome). For remaining-height regions (ScrollView fill), use
`View::section_fill(id, || { … })` — wrap host is
`height:fill;flex-grow:1;min-height:0` (no `with_key(section_id)` pin ceremony).

```rust
View::section(200, || {
    let n = export.get();
    View::column(vec![
        View::label(format!("{n}")).with_key(201),
        View::label("hint").with_key(202),
    ]).with_key(200)
});

View::section_fill(9302, || {
    View::scroll(tall_children)
        .style_css("height:fill;width:fill;flex-grow:1;min-height:0")
});
```

Prefer `on_action` inside sections (structure clone drops `on_press`).

Tab panes that must stay mounted without re-running builders on tab change:
omit `tabs` from section deps and call `ViewEngine::set_keyed_fill_visible` on a
pinned Styled host (`height:0` when hidden).

## Sibling-local keys (Phase 37)

```rust
use synui::key_scope;

key_scope(1, || {
    View::column(vec![
        View::label("a").with_key_local(1),
        View::button("+").with_key_local(2),
    ]).with_key_local(0)
});
```

Locals need only be unique **within the scope**; the engine mixes `(scope, local)`
into a stable global key. Duplicate locals in one build panic. Binding keys that
must match `TextBinding` / `RichBinding` ids can stay as absolute `with_key(n)`.

## Paint damage (Phase 29 / 31 / 32 / 38)

`PaintMode::Full` resets the scene and paints every node. `PaintMode::Damaged`
**retains** the scene, erases dirty rects, then `paint_frame` skips widgets whose
bounds miss the damage set. Prefer `mark_paint_dirty_widget` for leaf updates.
Unscoped `mark_paint_dirty` forces Full. See `PaintStats.skipped_paint_nodes`.
Success is **skipped paint nodes**, not OS-level partial present.
Debug: `force_full_paint(true)`.

Phase 38: IME **Commit** and structure-stable pointer presses use widget-scoped
Damaged (not unconditional Full).

Damaged paint is **clipped per dirty rect** (not the AABB union of all dirty
rects) so ancestor surface fills cannot wipe retained siblings that sit in gaps
between disjoint dirty regions (blank/flicker UI).

**Bounded retained-scene (Phase 32):** after `DAMAGED_FRAME_BUDGET` (default 32)
consecutive Damaged presents, `prepare_damage_compact` escalates to Full so
`scene.reset()` can reclaim encoding growth. Winit/headless call
`prepare_damage_compact` / `note_paint_presented` around each present.
Override with `set_damaged_frame_budget` or `SYNUI_DAMAGED_FRAME_BUDGET`.

**Escalate closeout (Phase 33 / 38):** scroll uses `mark_scroll_paint_dirty` (ScrollView
viewport), IME preedit / Commit and keyboard caret use `mark_paint_dirty_widget(focused)`.
Structure-changing presses and overlays may still Full. Headless `render_store` shares
`sync_pre_layout` → `sync_post_layout` → `sync_pre_paint` with winit.

Winit and headless share this policy (Phase 31+). Animation ticks
(`tick_animations`) return dirty widget ids so Morph/Lottie/caret use scoped
paint (Phase 30 damage v2).

## Layout dirty-subtree (Phase 30)

`mark_layout_dirty_widget` records structure-stable layout dirt. The winit frame
calls `LayoutEngine::mark_dirty` for those ids before `compute`, instead of
forcing a full structure rebuild. Inspect `LayoutStats.partial_layout_passes`.
Unscoped `mark_layout_dirty` / structure changes still full-layout.
Partial layout paints with scoped old+new bounds (Phase 31); full layout still
escalates to Full paint.
