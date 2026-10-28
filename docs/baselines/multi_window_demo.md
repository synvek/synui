# multi_window_demo behavior baseline

- **Classification:** `migrated` (Wave G), facade-only with shared-State second window.
- **Before:** `Application`/`AppDriver`, `Signal`, root `View`, `ViewEngine`, `WidgetStore`, `DesktopMenu`, and `AppTray`.
- **After:** facade `App`/`Window::with_second_window`, AppContext-owned `State`, typed `Action`, prelude `Label`/`Button`.
- **Behavior:** both native windows paint the same App-owned root; counter updates are observable from either window. Tray / DesktopMenu remain host-bound Non-equivalent.
- **Retirement:** root Engine re-exports and global signals are retired from this package.

## Imports

Before:

```rust
use synui::{AppDriver, AppTray, Application, DesktopMenu, Signal, View,
    ViewEngine, WidgetId, WidgetStore};
```

After:

```rust
use synui::prelude::*;
```

## Commands

- **Facade-only:** `cargo check -p multi_window_demo`; `cargo test -p synui --test counter_prelude_fixture`
- **Metadata/manifest:** `cargo metadata --format-version 1 --no-deps`; `python3 scripts/migration_manifest.py --check`
- **Engine-bound:** none for ordinary package code; native dual-window/tray checks are desktop host workloads.
