# data_browser_demo behavior baseline

- **Classification:** `migrated`, Phase 44–47, facade-only ordinary path.
- **Before:** Engine panes, sections, tree/table bindings, global Signals, RichEdit preview.
- **After:** `VirtualTree | SplitPane | TabBar(Table|RichDocument Preview) | Detail` + client filter + in-app `MenuBar`.
- **Behavior:** nested fixed split, tab switch Table/Preview, selection-driven detail, filter, menu Actions.
- **Non-equivalent:** Floating dock remains hard-locked (Phase 48+).

## Import mapping

```rust
// Before
use synui::{Application, Signal, View, ViewEngine, WidgetStore};
// After
use synui::prelude::*;
```

## Commands

- Facade: `cargo check -p data_browser_demo`
- Manifest: `cargo metadata --format-version 1 --no-deps`; `python3 scripts/migration_manifest.py --check`
