# tree_list_demo

Phase 46 Wave D data/virtualization example. **Status: migrated**. Ordinary UI code is facade-only.

## Mapping

Before v0.2 this package used `View`, `ViewEngine`, `WidgetStore`, selection bindings, numeric keys, and Engine menu polling. After Phase 46 it uses prelude `VirtualTree` + `VirtualList` plus in-app `MenuBar` / `ContextMenu` (`Action<String>` item ids; list right-click opens the mounted context menu).

## Checklist (facade)

| Item | Status |
|------|--------|
| Tree expand/select/activate | Yes |
| List select + right-click context | Yes |
| MenuBar (F10/Alt by host) | Yes |
| ContextMenu actions | Yes |

## Exact commands

```bash
cargo check -p tree_list_demo
cargo metadata --format-version 1 --no-deps
python3 scripts/migration_manifest.py --check
```

The a11y bridge check is `unavailable` when the bridge is unavailable, never `passed`.
