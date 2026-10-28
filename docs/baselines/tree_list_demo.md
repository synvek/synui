# tree_list_demo behavior baseline

- **Classification:** `migrated`, Wave D / Phase 46, facade-only ordinary path.
- **Before:** Engine TreeView/ListView, selection bindings, numeric keys, MenuBar/ContextMenu polling.
- **After:** Prelude `VirtualTree` + `VirtualList` + in-app `MenuBar` / `ContextMenu`.
- **Behavior:** true tree expand/select/activate; list select; MenuBar Actions; list right-click opens ContextMenu.
- **Non-equivalent:** deep keyboard menu MVP only (host F10/Alt); no floating dock.

## Import mapping

```rust
// Before
use synui::{Application, Signal, View, ViewEngine, WidgetStore};
// After
use synui::prelude::*;
```

## Commands

- Facade: `cargo check -p tree_list_demo`
- Manifest: `cargo metadata --format-version 1 --no-deps`; `python3 scripts/migration_manifest.py --check`
