# Hot Reload

Desktop-only (feature `hot_reload`, default off).

```bash
cargo build -p synui_hot_view --lib
SYNUI_HOT_RELOAD=1 cargo run -p counter --features hot_reload
```

Edit `crates/synui-hot-view/src/lib.rs`, save, and the counter rebuilds from the
reloaded dylib. Host-side `Signal` state is preserved.

## ABI

The loader expects a cdylib named by `SYNUI_HOT_LIB` (default `synui_hot_view`):

| Symbol | Type | Purpose |
|--------|------|---------|
| `synui_hot_version` | `extern "C" fn() -> u32` | Monotonic version |
| `synui_hot_label` | `extern "C" fn() -> *const c_char` | Primary label string |
| `synui_hot_button` | `extern "C" fn() -> *const c_char` | Button title string |
| `synui_create_view_kind` | `extern "C" fn() -> u32` | View template id (0=default, 1=alt) |
| `synui_create_view` | `extern "C" fn(*mut c_char, usize) -> i32` | Writes NUL-terminated view descriptor into `buf`; returns byte count or `-1` |

### `synui_create_view` drives the whole page (Phase 11–12)

When the dylib exports `synui_create_view`, the host stores the descriptor string
and **`hot_counter_view`** builds the full `View` tree from it.

Descriptor fields: `kind`, `title`, `button`, and optional quoted **`style`**
(CSS subset). Example:

```text
kind=1;title=Hot view;button=Hot +1;style="padding:24;gap:12;background:surface-container"
```

Invalid CSS hard-fails at load. Legacy `synui_hot_label` / `synui_hot_button`
still populate defaults when descriptor fields are empty.

If the descriptor **starts with `{`**, it is treated as a JSON view tree
(`column` / `row` / `label` / `button` / `text_field` / `text_area` / `rich_edit`,
optional per-node `style`). See [plugins.md](plugins.md) for the same schema
used by `synui_plugin_create_view`.

On `.rs` changes the watcher runs `cargo build -p <lib>`, loads the dylib with
`libloading`, refreshes hot strings / view descriptor, and `schedule_rebuild()`.
Load failures keep the previous library and log a warning.

## Paths

| Platform | Artifact |
|----------|----------|
| macOS | `target/debug/lib{name}.dylib` |
| Linux | `target/debug/lib{name}.so` |
| Windows | `target/debug/{name}.dll` (best-effort) |
