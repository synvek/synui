# Plugins

Phase 11 ships a **real cdylib** sample and a hard-fail index loader (stub is opt-in).

## In-process registration

```rust
use synui::{register_plugin, SampleLabelPlugin, global_registry};

register_plugin(&SampleLabelPlugin);
let label = global_registry()
    .as_ref()
    .and_then(|r| r.label("sample_label"));
```

Implement `WidgetPlugin` for custom factories.

## Local `plugins.toml` index

Repo root [`plugins.toml`](../../plugins.toml) lists local plugins:

```toml
[[plugins]]
name = "sample_label"
version = "0.1.0"
path = "target/debug"
```

Build the cdylib first:

```bash
cargo build -p synui_plugin_sample --lib
```

```rust
use std::path::Path;
use synui::{load_plugin_index, load_plugins_from_index, load_plugins_from_index_opts};

let entries = load_plugin_index(Path::new("plugins.toml"))?;
let count = load_plugins_from_index(Path::new("plugins.toml"))?; // hard fail if cdylib missing
```

| API | Purpose |
|-----|---------|
| `PluginIndexEntry { name, version, path }` | One index row |
| `load_plugin_index(path)` | Parse TOML index |
| `load_plugins_from_index(path)` | Register into `global_registry()` — **missing library = hard error** |
| `load_plugins_from_index_opts(path, allow_stub)` | Same, but `allow_stub=true` registers a stub label for docs/tests |

## cdylib ABI (`synui_plugin_sample`)

Package **`synui_plugin_sample`** (`examples/plugin_sample/`) exports:

| Symbol | Type | Purpose |
|--------|------|---------|
| `synui_plugin_version` | `extern "C" fn() -> u32` | ABI version |
| `synui_plugin_name` | `extern "C" fn() -> *const c_char` | Registry name (matches `plugins.toml`) |
| `synui_plugin_label` | `extern "C" fn() -> *const c_char` | NUL-terminated label string |
| `synui_plugin_style` | `extern "C" fn() -> *const c_char` | Optional CSS-subset style (Phase 12) |
| `synui_plugin_create_view` | buffer write / JSON | Optional full-page JSON tree (Phase 13+) |

Hosts can build a styled panel with `plugin_styled_panel("sample_label", key)`, or a
full page with `plugin_view_page("sample_label", start_key)` (subtree is
`rekey_from(start_key)` so keys do not collide with host chrome).

JSON `type` values: `column`, `row`, `label`, `button`, `text_field`, `text_area`,
`rich_edit` (optional `style` CSS subset on any node).

Invalid plugin style fails the load (hard error).

Point `path` at the directory or file containing `libsynui_plugin_sample.{dylib,so,dll}`.
Requires `--features plugins` (or `hot_reload`) on the host for dynamic load.

Phase 10 “path exists → stub success” is removed as the default; use **`allow_stub`** explicitly when you want stub behavior without a built cdylib.

See `crates/synui/src/plugins.rs` and `examples/plugin_sample/README.md`.
