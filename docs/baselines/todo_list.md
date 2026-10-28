# todo_list behavior baseline

- **Classification:** `migrated` (Wave 3), facade-only for ordinary UI code.
- **Before:** global `Signal`, `TextBinding`, `FormState`, root `View`, `WidgetStore`, `Application`, menu, and file dialog.
- **After:** App-owned `State`/typed `Action`, `TextInput`/`InputConfig`, `Node`/`Element`, and an external list/scroll adapter with `BusinessKey` row identities.
- **Behavior:** add/delete, required validation, scrolling, stable ids, task-list rendering, and error rendering remain observable. File menu/import is explicit platform/advanced evidence.
- **Retirement:** legacy bindings and root Engine re-exports are retired from this package; no ordinary Prelude compatibility promise covers file-dialog/menu host glue.

## Imports

Before:

```rust
use synui::{App, Application, DesktopMenu, FileDialog, FormState, Signal,
    TextBinding, View, WidgetStore};
```

After:

```rust
use synui::prelude::*;
```

## Commands

- **Facade-only:** `cargo check -p todo_list`; `cargo test -p synui --test counter_prelude_fixture`; `cargo test -p synui --test external_component_fixture`
- **Metadata/manifest:** `cargo metadata --format-version 1 --no-deps`; `python3 scripts/migration_manifest.py --check`
- **Engine-bound:** none for ordinary package code; retained host behavior must be separately labelled.
