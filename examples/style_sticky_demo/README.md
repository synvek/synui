# style_sticky_demo

Phase 43 Wave A. Sticky, fixed, and z-index scenarios use prelude `Scroll` + `Styled`. Style diagnostics use `StyleCache` / `StyleInput` (invalid runtime style retains the previous successful style).

```bash
cargo check -p style_sticky_demo
cargo test -p synui --lib style::tests
cargo test -p synui --lib view::tests::facade_scroll_and_virtual_list_mount_native_hosts
python3 scripts/migration_manifest.py --check
```
