# rich_blocks_demo behavior baseline

- **Classification:** `migrated` Wave 5; facade-only document/state path with a deferred rich editor boundary.
- **Before:** `RichBinding`, `RichEdit`, global `Signal`, `ViewEngine`, and `WidgetStore`.
- **After:** `BlockDoc` Markdown round-trip, App-owned State/Actions, external `Element`, and BusinessKey block identities.
- **Preserved:** headings, paragraphs, nested bullets, ordered items, image slots, Markdown import/export, preview, and block-count updates.
- **Non-equivalent:** interactive selection/mutation and image editor lifecycle remain deferred; Engine-bound tests stay explicitly Engine-bound.

## Regression ownership

`snapshot_tests.rs`, `golden_tests.rs`, `scroll_tests.rs`, and `gallery_media_repro.rs` are owned by the `synui-testing` Engine harness. They are retained as equivalent regression coverage and are not claimed as facade-only tests.

## Commands

`cargo check -p rich_blocks_demo`; `cargo test -p synui-core --lib block_doc`; `cargo test -p synui-testing --test scroll_tests`; `python3 scripts/migration_manifest.py --check`.
