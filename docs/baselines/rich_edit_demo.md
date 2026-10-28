# rich_edit_demo behavior baseline

- **Classification:** `migrated` Wave 5; facade-only ordinary UI with an explicit deferred rich-text Element boundary.
- **Before:** `RichBinding`, `RichEdit`, global `Signal`, `View`, and `ViewEngine`.
- **After:** App-owned `State<BlockDoc>`/typed `Action`, `Node`, `BusinessKey`, and a custom facade `Element`; `synui-core` is data-model support only.
- **Preserved:** styled initial inline text, sample loading, Markdown export preview, block count, and observable state updates.
- **Non-equivalent:** editor selection/mutation and rich-text a11y editing are deferred; no legacy binding or Engine storage is exposed.

## Regression ownership

| Regression | Owner | Boundary |
|---|---|---|
| `snapshot_tests.rs` | `synui-testing` | Engine-bound layout/snapshot harness |
| `golden_tests.rs` | `synui-testing` | Engine-bound PNG/widget owner |
| `scroll_tests.rs` | `synui-testing` | Engine-bound ScrollView owner |
| `gallery_media_repro.rs` | `synui-testing` | Engine-bound legacy tree owner |

These tests remain valid regression evidence but are not relabelled as facade-only.

## Commands

`cargo check -p rich_edit_demo`; `cargo test -p synui-core --lib block_doc`; `cargo test -p synui --lib style::tests`; `python3 scripts/migration_manifest.py --check`.
