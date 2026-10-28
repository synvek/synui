# data_browser_demo

Phase 44–47. **Status: migrated**.

## Mapping

`MenuBar` + `VirtualTree | SplitPane | TabBar(Table|RichDocument Preview) | Detail` with client filter. Floating dock remains hard-locked.

## Checklist

| Item | Status |
|------|--------|
| Tree / Split / Tab / Table / RichDocument Preview | Yes |
| Client substring filter | Yes |
| MenuBar | Yes |
| Floating dock | Hard-locked |

## Commands

```bash
cargo check -p data_browser_demo
python3 scripts/migration_manifest.py --check
```
