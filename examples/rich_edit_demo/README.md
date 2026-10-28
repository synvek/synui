# rich_edit_demo

Phase 45 Wave C. **Status: migrated**. Facade-only `RichDocument` host.

## Mapping

Legacy `RichBinding` / `View::rich_edit` → prelude `RichDocument` + `State<BlockDoc>` + `on_change`. Export MD / Load sample are typed Actions. OT/HTML/nested tables remain hard-locked.

## Commands

```bash
cargo check -p rich_edit_demo
cargo test -p synui --lib view::tests::facade_rich_document_mounts_native_richedit_host
python3 scripts/migration_manifest.py --check
```
