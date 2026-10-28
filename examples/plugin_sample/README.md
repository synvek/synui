# Local plugin sample (non-production experimental boundary)

**`synui_plugin_sample`** is a real **cdylib** used only to exercise the Experimental Plugin ABI and JSON View Creation paths. It is not a Stable Component, ordinary Prelude dependency, or Production ABI fixture. The plugin and JSON exports are intentionally preserved for preview/hot-reload/example/test tooling and are not a production replacement for the curated facade.

## Build

```bash
cargo build -p synui_plugin_sample --lib
```

Produces `target/debug/libsynui_plugin_sample.{dylib,so,dll}` (platform-dependent).

## ABI exports

| Symbol | Purpose |
|--------|---------|
| `synui_plugin_version()` | ABI version (`u32`) |
| `synui_plugin_name()` | Registry name — must match `plugins.toml` `name` |
| `synui_plugin_label()` | NUL-terminated label string |

## Load at runtime

1. Ensure `plugins.toml` points at the build output (default: `path = "target/debug"`).
2. Call **`load_plugins_from_index("plugins.toml")`** from a host that links the experimental plugin loader — missing cdylib **hard-fails**.

   For docs/tests without a built library, use **`load_plugins_from_index_opts(path, true)`**
   (`allow_stub`) instead.

   Ordinary UI evidence is `widget_gallery_facade` / `synui::prelude::*` (plugin hot-reload is not a facade gallery path).

See [`docs/book/src/plugins.md`](../../docs/book/src/plugins.md).

## Boundary and replacement

The `synui_plugin_*` exports, `plugins.rs` registry/loading helpers, and `view_json.rs` parser/factory are **non-production experimental** paths. Their JSON payload and plugin-created views must never be cited as Stable Component or Production ABI evidence. For ordinary UI, use the documented `synui::prelude::*` components and `widget_gallery_facade`; there is no lossless stable replacement for plugin loading or JSON View Creation.
