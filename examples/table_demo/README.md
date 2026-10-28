# table_demo

Phase 44 Wave B data/virtualization example. **Status: migrated**. The package depends only on the `synui` facade.

## Mapping

The old example used a materialized Engine `DataTable`, `ViewEngine`, `WidgetStore`, `Signal`, and selection/binding callbacks. The migrated example uses prelude runtime `DataTable` / `TableColumn`, a cell factory, typed select/sort/edit Actions, and client substring filter. `EngineAdapter` mounts the native virtualized host (no `external-element` shell).

Merge-cell UI and server filter/pivot remain deferred. A missing a11y bridge is reported as `unavailable`, not success.

## Exact commands

```bash
cargo check -p table_demo
cargo test -p synui --lib view::tests::facade_data_table_mounts_native_host_with_bounded_cells
cargo metadata --format-version 1 --no-deps
python3 scripts/migration_manifest.py --check
```
