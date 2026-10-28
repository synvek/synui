# list_demo behavior baseline

- **Classification:** `migrated` (Wave D / Phase 46), facade-only ordinary path.
- **Before:** `Signal`, root `View`, `ViewEngine`, `WidgetStore`, ListView/context-menu internals, numeric keys, and callback polling.
- **After:** Prelude `VirtualList` + in-app `MenuBar` / `ContextMenu`, App-owned select/activate/context/menu Actions.
- **Behavior:** 10,000 logical rows, bounded viewport mounting, scrolling, selection/status updates, MenuBar item Actions, and right-click ContextMenu remain observable.
- **Retirement:** root Engine re-exports and legacy bindings are retired from this package.

## Imports

Before:

```rust
use synui::{AppDriver, Application, Signal, View, ViewEngine,
    WidgetId, WidgetStore};
```

After:

```rust
use synui::prelude::*;
```

## Commands

- **Facade-only:** `cargo check -p list_demo`; `cargo test -p synui --lib view::tests::facade_menu_bar_and_context_menu_mount_and_dispatch_actions`
- **Metadata/manifest:** `cargo metadata --format-version 1 --no-deps`; `python3 scripts/migration_manifest.py --check`
