# table_demo behavior baseline

- **Classification:** `migrated`, Wave 4 / Phase 44 Wave B (+ Phase 50 Wave I merge spans), facade-only ordinary path.
- **Before:** Engine `DataTable`, `ViewEngine`, `WidgetStore`, Signals, and selection/binding polling.
- **After:** Prelude runtime `DataTable` / `TableColumn`, cell factory, typed select/sort/edit Actions, client filter, declarative merge spans (`.span` / `.spans`).
- **Behavior:** virtualized table with bounded cell mounts; selection, edits, sorting, and filter remain available; a second static table demonstrates `row_span`/`col_span` merge cells (data coordinates).
- **Non-equivalent:** edit-merge UI / server pivot deferred; Engine virtual/a11y harnesses are not facade evidence.

## Import mapping

```rust
// Before
use synui::{Application, Signal, View, ViewEngine, WidgetStore};
// After
use synui::prelude::*;
```

## Commands

- Facade: `cargo check -p table_demo`
- Bound mount: `cargo test -p synui --lib view::tests::facade_data_table_mounts_native_host_with_bounded_cells`
- Merge spans: `cargo test -p synui --lib facade_data_table_applies_merge_spans`
- Manifest: `cargo metadata --format-version 1 --no-deps`; `python3 scripts/migration_manifest.py --check`
- A11y: bridge unavailable is recorded as `unavailable`, never passed.
