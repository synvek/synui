# todo_list

Phase 43 Wave A. **Status: restored**. Ordinary UI uses the curated facade with a real `Scroll` host for the task list.

## Checklist (facade)

| Item | Status |
|------|--------|
| Add / delete tasks | Yes |
| Toggle done | Yes |
| Required validation | Yes |
| Scrollable list | Yes (`Scroll`) |
| File-menu import | N/A (platform boundary) |

See [the behavior baseline](../../docs/baselines/todo_list.md) and [the migration guide](../../docs/migration-v0.2.md).

## Exact commands

```bash
cargo check -p todo_list
cargo test -p synui --test counter_prelude_fixture
python3 scripts/migration_manifest.py --check
```
