# navigation_demo

Phase 47 Wave F. **Status: migrated**.

## Mapping

App-owned routes plus prelude `AppBar` / `NavigationDrawer` / `BottomNavigation` / `Dialog` / `MenuBar`. Platform history remains an explicit host boundary. Floating dock remains hard-locked.

## Checklist

| Item | Status |
|------|--------|
| AppBar / Drawer / BottomNav / Dialog | Yes |
| MenuBar Actions | Yes |
| Platform history sync | Host boundary |
| Floating dock | Hard-locked |

## Commands

```bash
cargo check -p navigation_demo
python3 scripts/migration_manifest.py --check
```
