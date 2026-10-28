# Style demo

Wave G demonstrates the stable facade style contract with prelude `Styled` layouts (grid / flex-grow / absolute MVP), `Label`, `StyleInput`, `StyleCache`, and `StyleDiagnostic`.

A valid runtime declaration is committed first; an invalid declaration returns structured diagnostics and retains the last successful style. No legacy `View` / `style_css` path is used. Style Cascade v3 remains hard-locked.

```bash
cargo check -p style_demo
cargo test -p synui --lib style::tests
python3 scripts/migration_manifest.py --check
```
