# style_sticky_demo behavior baseline

- **Classification:** `migrated` Wave 5 / Wave A; ordinary facade `Node`/`Element` path.
- **Before:** `style_css`, `ScrollView`, `View`, `ViewEngine`, and global `Signal`.
- **After:** App-owned diagnostic state, typed Action, `Node`, `BusinessKey`, prelude `Scroll` + `Styled` (`sticky_top` / fixed / absolute + z-index), `StyleCache` / `StyleDiagnostic`.
- **Preserved:** sticky header / filler rows / fixed badge / z-index overlap scenario and observable style re-check updates.
- **Non-equivalent:** Engine ScrollView regression ownership stays in `synui-testing` (not proven by this demo). `widget_gallery_facade` now carries a **smoke subset** (sticky bar + fillers + fixed badge) as of Phase 55 Wave N; full sticky/z-index/StyleCache suite remains owned by this demo.

## Regression ownership

| Regression | Owner | Boundary |
|---|---|---|
| `scroll_tests.rs` | `synui-testing` | Engine-bound ScrollView/viewport owner |
| `snapshot_tests.rs` | `synui-testing` | Engine-bound layout/snapshot owner |
| `golden_tests.rs` | `synui-testing` | Engine-bound PNG owner |
| `gallery_media_repro.rs` | `synui-testing` | Engine-bound keyed-tree owner |

Unavailable a11y bridges remain `unavailable`; they are never rewritten as passed.

## Commands

`cargo check -p style_sticky_demo`; `cargo test -p synui --lib style::tests`; `cargo test -p synui-testing --test scroll_tests`; `python3 scripts/migration_manifest.py --check`.
