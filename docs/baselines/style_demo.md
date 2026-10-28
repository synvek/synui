# style_demo behavior baseline

- **Classification:** `migrated` Wave G; prelude `Styled` layout MVP plus StyleCache diagnostics.
- **Before:** `run_app`, legacy `View`, `style_css`, and Engine theme/design-system paths.
- **After:** `App`, `Styled`, `Label`, `StyleInput`, `StyleCache`, and `StyleDiagnostic`.
- **Preserved:** grid / flex-grow / absolute presentation, and observable invalid-runtime-style diagnostic behavior that retains the last successful style.
- **Invariant:** invalid style is a diagnostic success; no panic-prone entry is called. Style Cascade v3 remains hard-locked.

## Regression ownership

`golden_tests.rs` and `snapshot_tests.rs` remain `synui-testing` Engine-bound visual owners. Style parser/cache assertions remain facade-owned by `crates/synui/src/style.rs` tests.

## Commands

`cargo check -p style_demo`; `cargo test -p synui --lib style::tests`; `python3 scripts/migration_manifest.py --check`.
