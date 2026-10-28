# show_image

Phase 45 Wave C. **Status: migrated**.

## Mapping

Engine `WidgetStore` + `ImageId` load → facade `Window::with_image` + prelude `Image`.

## Commands

```bash
cargo check -p show_image
python3 scripts/migration_manifest.py --check
```
