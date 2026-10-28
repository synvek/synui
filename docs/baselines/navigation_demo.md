# navigation_demo behavior baseline

- **Classification:** `migrated` (Phase 45 + Phase 47 MenuBar), facade-only for ordinary navigation UI.
- **Before:** `Router`/`Route`, numeric actions, root `View`, `ViewEngine`, `WidgetStore`, and `Application`.
- **After:** route `State`, typed `Action` handlers, prelude `AppBar`/`Drawer`/`BottomNavigation`/`Dialog`/`MenuBar`.
- **Behavior:** list/detail/settings routes, button navigation, back navigation, and MenuBar Actions remain observable. Browser history synchronization remains platform-host evidence.
- **Retirement:** Router, numeric actions, and root Engine re-exports are retired from this package; no generic legacy Router contract is retained in Prelude.

## Imports

Before:

```rust
use synui::{AppDriver, Application, Route, Router, View, ViewEngine,
    WidgetId, WidgetStore};
```

After:

```rust
use synui::prelude::*;
```

## Commands

- **Facade-only:** `cargo check -p navigation_demo`; `cargo test -p synui --test counter_prelude_fixture`; `cargo test -p synui --test external_component_fixture`
- **Metadata/manifest:** `cargo metadata --format-version 1 --no-deps`; `python3 scripts/migration_manifest.py --check`
- **Engine-bound:** none for ordinary package code; platform history checks remain separately attributed.
