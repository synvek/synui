# list_demo

Phase 46 Wave D. **Status: restored**. Ordinary UI uses prelude `VirtualList` with in-app `MenuBar` / `ContextMenu`.

## Checklist (facade)

| Item | Status |
|------|--------|
| 10k virtualized scroll | Yes |
| Click / ↑↓ selection feedback | Yes (`on_select`) |
| Activate feedback | Yes (`on_activate`) |
| MenuBar | Yes (File/Edit + `Action<String>`) |
| ContextMenu | Yes (right-click row → overlay + action) |

See [the behavior baseline](../../docs/baselines/list_demo.md) and [the migration guide](../../docs/migration-v0.2.md).

## Exact commands

```bash
cargo check -p list_demo
cargo test -p synui --lib view::tests::facade_menu_bar_and_context_menu_mount_and_dispatch_actions
python3 scripts/migration_manifest.py --check
```
