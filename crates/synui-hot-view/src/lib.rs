//! Minimal cdylib used by SynUI hot-reload.
//!
//! Build: `cargo build -p synui_hot_view --lib`
//!
//! ABI (Phase 10–13):
//! - `synui_hot_version() -> u32`
//! - `synui_hot_label() / synui_hot_button()` — string hooks
//! - `synui_create_view_kind() -> u32` — 0/1 flat, 2 = JSON tree
//! - `synui_create_view(buf, len) -> i32` — writes descriptor (JSON when kind=2)

use std::os::raw::c_char;

static VERSION: u32 = 6;
static LABEL: &[u8] = b"Hot view v6\0";
static BUTTON: &[u8] = b"Hot +1\0";
/// 2 = JSON nested view tree (Phase 13).
static VIEW_KIND: u32 = 2;
/// Entire descriptor is JSON when it starts with `{`.
static VIEW_DESC: &[u8] = br#"{"type":"column","style":"padding:24;gap:12;align-items:center;background:surface-container;border-radius:12","children":[{"type":"label","text":"Hot JSON page"},{"type":"row","children":[{"type":"button","text":"Hot +1"},{"type":"button","text":"Also +1"}]}]}"#;

#[no_mangle]
pub extern "C" fn synui_hot_version() -> u32 {
    VERSION
}

#[no_mangle]
pub extern "C" fn synui_hot_label() -> *const c_char {
    LABEL.as_ptr() as *const c_char
}

#[no_mangle]
pub extern "C" fn synui_hot_button() -> *const c_char {
    BUTTON.as_ptr() as *const c_char
}

/// Host uses this to swap the entire view template while keeping Signal state.
#[no_mangle]
pub extern "C" fn synui_create_view_kind() -> u32 {
    VIEW_KIND
}

/// Write a NUL-terminated view descriptor into `buf` (max `len` bytes).
/// Returns bytes written (excluding NUL) or -1 on error.
///
/// # Safety
/// `buf` must be valid for writes of `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn synui_create_view(buf: *mut c_char, len: usize) -> i32 {
    if buf.is_null() || len == 0 {
        return -1;
    }
    let need = VIEW_DESC.len() + 1; // + NUL
    if need > len {
        return -1;
    }
    std::ptr::copy_nonoverlapping(VIEW_DESC.as_ptr(), buf as *mut u8, VIEW_DESC.len());
    *buf.add(VIEW_DESC.len()) = 0;
    VIEW_DESC.len() as i32
}
