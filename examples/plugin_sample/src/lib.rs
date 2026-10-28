//! Sample SynUI plugin cdylib (Phase 11–13).
//!
//! **Boundary:** this crate is non-production Experimental Plugin ABI and JSON
//! View Creation evidence only. Its `synui_plugin_*` exports must never be
//! treated as Stable Components, ordinary Prelude imports, or Production ABI.
//!
//! Build: `cargo build -p synui_plugin_sample --lib`
//! Load via `plugins.toml` → `load_plugins_from_index`.

use std::os::raw::c_char;

static LABEL: &[u8] = b"Hello from synui_plugin_sample\0";
static STYLE: &[u8] = b"padding:16;gap:8;background:#e3f2fd;border-radius:12;margin:8\0";
static VERSION: u32 = 3;
/// Full-page JSON via `synui_plugin_create_view` (Phase 13).
static VIEW_JSON: &[u8] = br#"{"type":"column","style":"padding:16;gap:8;background:#e3f2fd;border-radius:12;margin:8","children":[{"type":"label","text":"Plugin sample page"},{"type":"label","text":"Hello from synui_plugin_sample"},{"type":"row","children":[{"type":"button","text":"Primary"},{"type":"button","text":"Secondary"}]}]}"#;

/// Plugin ABI version.
#[no_mangle]
pub extern "C" fn synui_plugin_version() -> u32 {
    VERSION
}

/// NUL-terminated label string for the registry.
#[no_mangle]
pub extern "C" fn synui_plugin_label() -> *const c_char {
    LABEL.as_ptr() as *const c_char
}

/// Optional CSS-subset style for a styled plugin panel (Phase 12).
#[no_mangle]
pub extern "C" fn synui_plugin_style() -> *const c_char {
    STYLE.as_ptr() as *const c_char
}

/// Plugin registry name (must match plugins.toml `name`).
#[no_mangle]
pub extern "C" fn synui_plugin_name() -> *const c_char {
    b"sample_label\0".as_ptr() as *const c_char
}

/// Write a NUL-terminated JSON view tree into `buf` (Phase 13 nested create_view).
/// Returns bytes written (excluding NUL) or -1 on error.
///
/// # Safety
/// `buf` must be valid for writes of `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn synui_plugin_create_view(buf: *mut c_char, len: usize) -> i32 {
    if buf.is_null() || len == 0 {
        return -1;
    }
    let need = VIEW_JSON.len() + 1;
    if need > len {
        return -1;
    }
    std::ptr::copy_nonoverlapping(VIEW_JSON.as_ptr(), buf as *mut u8, VIEW_JSON.len());
    *buf.add(VIEW_JSON.len()) = 0;
    VIEW_JSON.len() as i32
}
