use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::mem;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::ptr;

use synui_abi::*;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("synui-abi must be inside the workspace crates directory")
        .to_path_buf()
}

fn run_checked(command: &mut Command, purpose: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to start {purpose}: {error}"));
    assert!(
        output.status.success(),
        "{purpose} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn prepend_search_path(command: &mut Command, directory: &Path) {
    let variable = if cfg!(target_os = "windows") {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    let mut paths = vec![directory.to_path_buf()];
    if let Some(existing) = env::var_os(variable) {
        paths.extend(env::split_paths(&existing));
    }
    command.env(
        variable,
        env::join_paths(paths).expect("dynamic-library search path must be valid"),
    );
}
fn build_cdylib(target_dir: &Path) -> PathBuf {
    let mut command = Command::new(env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")));
    command
        .current_dir(workspace_root())
        .arg("build")
        .arg("-p")
        .arg("synui-abi")
        .arg("--lib")
        .arg("--target-dir")
        .arg(target_dir);
    run_checked(&mut command, "synui-abi cdylib build");

    let library = if cfg!(target_os = "windows") {
        target_dir.join("debug").join("synui_abi.dll")
    } else if cfg!(target_os = "macos") {
        target_dir.join("debug").join("libsynui_abi.dylib")
    } else {
        target_dir.join("debug").join("libsynui_abi.so")
    };
    assert!(library.is_file(), "missing cdylib at {}", library.display());
    library
}

#[cfg(target_env = "msvc")]
fn import_library(directory: &Path) -> PathBuf {
    fn visit(directory: &Path, matches: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).expect("read native ABI target directory") {
            let path = entry.expect("read native ABI target entry").path();
            if path.is_dir() {
                visit(&path, matches);
            } else {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if name == "synui_abi.dll.lib" || name == "synui_abi.lib" {
                    matches.push(path);
                }
            }
        }
    }
    let mut matches = Vec::new();
    visit(directory, &mut matches);
    matches
        .into_iter()
        .next()
        .expect("cargo did not produce an MSVC import library")
}

#[cfg(not(target_env = "msvc"))]
fn compile_unix(source: &Path, executable: &Path, library_dir: &Path, cpp: bool) {
    let compiler = if cpp {
        env::var_os("CXX").unwrap_or_else(|| OsString::from("c++"))
    } else {
        env::var_os("CC").unwrap_or_else(|| OsString::from("cc"))
    };
    let mut command = Command::new(compiler);
    command
        .arg(if cpp { "-std=c++17" } else { "-std=c11" })
        .args(["-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("include"))
        .arg(source)
        .arg("-L")
        .arg(library_dir)
        .arg("-lsynui_abi")
        .arg(format!("-Wl,-rpath,{}", library_dir.display()))
        .arg("-pthread")
        .arg("-o")
        .arg(executable);
    run_checked(
        &mut command,
        if cpp {
            "C++17 ABI harness compile"
        } else {
            "C11 ABI harness compile"
        },
    );
}
#[cfg(target_env = "msvc")]
fn compile_msvc(
    source: &Path,
    executable: &Path,
    library_dir: &Path,
    import_library: &Path,
    cpp: bool,
) {
    let compiler = if cpp {
        env::var_os("CXX").unwrap_or_else(|| OsString::from("cl.exe"))
    } else {
        env::var_os("CC").unwrap_or_else(|| OsString::from("cl.exe"))
    };
    let mut command = Command::new(compiler);
    command
        .current_dir(executable.parent().expect("harness output directory"))
        .args(["/nologo", "/W4", "/WX"])
        .arg(if cpp { "/std:c++17" } else { "/std:c11" })
        .arg(if cpp { "/EHsc" } else { "/TC" })
        .arg(format!(
            "/I{}",
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("include")
                .display()
        ))
        .arg(source)
        .arg(format!("/Fe:{}", executable.display()))
        .arg("/link")
        .arg(format!("/LIBPATH:{}", library_dir.display()))
        .arg(import_library);
    run_checked(
        &mut command,
        if cpp {
            "C++17 ABI harness compile"
        } else {
            "C11 ABI harness compile"
        },
    );
}

fn compile_harnesses(target_dir: &Path, library: &Path) -> (PathBuf, PathBuf) {
    let native = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/native");
    let output = target_dir.join("harness");
    fs::create_dir_all(&output).expect("create native ABI harness output directory");
    let suffix = env::consts::EXE_SUFFIX;
    let c_executable = output.join(format!("synui-abi-c-smoke{suffix}"));
    let cpp_executable = output.join(format!("synui-abi-cpp-smoke{suffix}"));
    let library_dir = library.parent().expect("cdylib directory");

    #[cfg(target_env = "msvc")]
    {
        let import = import_library(target_dir);
        compile_msvc(
            &native.join("smoke.c"),
            &c_executable,
            library_dir,
            &import,
            false,
        );
        compile_msvc(
            &native.join("smoke.cpp"),
            &cpp_executable,
            library_dir,
            &import,
            true,
        );
    }
    #[cfg(not(target_env = "msvc"))]
    {
        compile_unix(&native.join("smoke.c"), &c_executable, library_dir, false);
        compile_unix(
            &native.join("smoke.cpp"),
            &cpp_executable,
            library_dir,
            true,
        );
    }
    (c_executable, cpp_executable)
}
fn run_harness(executable: &Path, library_dir: &Path, purpose: &str) -> Output {
    let mut command = Command::new(executable);
    prepend_search_path(&mut command, library_dir);
    run_checked(&mut command, purpose)
}

fn assert_c_layout(stdout: &[u8]) {
    let stdout = String::from_utf8(stdout.to_vec()).expect("C harness output must be UTF-8");
    let line = stdout
        .lines()
        .find(|line| line.starts_with("LAYOUT "))
        .expect("C harness must report struct layouts");
    let actual = line
        .split_whitespace()
        .skip(1)
        .map(|value| value.parse::<usize>().expect("layout size must be numeric"))
        .collect::<Vec<_>>();
    let expected = vec![
        mem::size_of::<SynuiStatus>(),
        mem::size_of::<SynuiAbiVersion>(),
        mem::size_of::<SynuiAbiInfo>(),
        mem::size_of::<SynuiRuntimeOptions>(),
        mem::size_of::<SynuiRuntimeInfo>(),
        mem::size_of::<SynuiErrorRecord>(),
        mem::size_of::<SynuiBuffer>(),
        mem::size_of::<SynuiCallbackPayload>(),
        mem::size_of::<SynuiPost>(),
        mem::size_of::<SynuiPostTokenInfo>(),
        mem::size_of::<SynuiTableDimensions>(),
        mem::size_of::<SynuiCellRange>(),
        mem::size_of::<SynuiCellRequest>(),
        mem::size_of::<SynuiCellCoordinate>(),
        mem::size_of::<SynuiCellValue>(),
        mem::size_of::<SynuiSortDescriptor>(),
        mem::size_of::<SynuiPrefetchBand>(),
        mem::size_of::<SynuiVirtualDataTableModelVtable>(),
    ];
    assert_eq!(actual, expected, "C header and Rust repr(C) layouts differ");
}

fn reference_policy() -> (BTreeSet<String>, Vec<String>) {
    let reference = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("abi/synui_v1.reference.toml"),
    )
    .expect("read ABI reference metadata");
    let mut in_exports = false;
    let mut exports = BTreeSet::new();
    let mut forbidden = Vec::new();
    for line in reference.lines() {
        let trimmed = line.trim();
        if trimmed == "exported_symbols = [" {
            in_exports = true;
            continue;
        }
        if in_exports {
            if trimmed == "]" {
                in_exports = false;
            } else if let Some(value) = trimmed.strip_prefix('"') {
                exports.insert(value.trim_end_matches(',').trim_end_matches('"').to_owned());
            }
        } else if let Some(values) = trimmed.strip_prefix("forbidden_export_families = [") {
            forbidden.extend(
                values
                    .trim_end_matches(']')
                    .split(',')
                    .map(|value| value.trim().trim_matches('"').to_owned())
                    .filter(|value| !value.is_empty()),
            );
        }
    }
    assert_eq!(
        exports.len(),
        41,
        "ABI v1 export allowlist must remain explicit"
    );
    (exports, forbidden)
}
fn exported_symbols(library: &Path) -> BTreeSet<String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("dumpbin.exe");
        command.args(["/NOLOGO", "/EXPORTS"]);
        command
    } else {
        let mut command = Command::new("nm");
        if cfg!(target_os = "macos") {
            command.args(["-gU"]);
        } else {
            command.args(["-D", "--defined-only"]);
        }
        command
    };
    command.arg(library);
    let output = run_checked(&mut command, "Production ABI export inspection");
    let text = String::from_utf8(output.stdout).expect("symbol tool output must be UTF-8");
    text.lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(|symbol| {
            if let Some(stripped) = symbol.strip_prefix('_') {
                if stripped.starts_with("synui_") {
                    return stripped.to_owned();
                }
            }
            symbol.to_owned()
        })
        .collect()
}

fn assert_export_policy(library: &Path) {
    let actual = exported_symbols(library);
    let (expected, mut forbidden) = reference_policy();
    for symbol in &expected {
        assert!(
            actual.contains(symbol),
            "missing Production ABI export {symbol}"
        );
    }
    for symbol in actual.iter().filter(|symbol| symbol.starts_with("synui_")) {
        assert!(
            expected.contains(symbol),
            "unversioned or undeclared SynUI export escaped the ABI: {symbol}"
        );
    }

    forbidden.extend([
        "view_from_json".to_owned(),
        "viewengine".to_owned(),
        "widgetstore".to_owned(),
        "synui_core".to_owned(),
    ]);
    for symbol in &actual {
        let lower = symbol.to_ascii_lowercase();
        for family in &forbidden {
            assert!(
                !lower.contains(family),
                "forbidden {family} symbol escaped the Production ABI: {symbol}"
            );
        }
        assert!(
            !(lower.contains("synui") && (symbol.starts_with("_ZN") || symbol.starts_with("_R"))),
            "Rust-mangled SynUI symbol escaped the Production ABI: {symbol}"
        );
    }
}

#[test]
fn c_and_cpp_harnesses_match_header_layout_and_export_policy() {
    let target_dir = workspace_root()
        .join("target")
        .join(format!("native-abi-smoke-{}", env::consts::OS));
    let library = build_cdylib(&target_dir);
    let (c_harness, cpp_harness) = compile_harnesses(&target_dir, &library);
    let library_dir = library.parent().expect("cdylib directory");
    let c_output = run_harness(&c_harness, library_dir, "C11 ABI smoke");
    assert_c_layout(&c_output.stdout);
    run_harness(&cpp_harness, library_dir, "C++17 ABI smoke");
    assert_export_policy(&library);
}
unsafe extern "C-unwind" fn panic_callback(
    _user_data: *mut std::ffi::c_void,
    _payload: *const SynuiCallbackPayload,
) -> SynuiStatus {
    panic!("native ABI panic containment smoke")
}

#[test]
fn public_drain_entrypoint_contains_callback_panics() {
    let options = SynuiRuntimeOptions {
        struct_size: SynuiRuntimeOptions::STRUCT_SIZE,
        requested_version: SYNUI_ABI_VERSION,
        requested_capabilities: SYNUI_CAPABILITY_BASE,
    };
    let mut runtime = ptr::null_mut();
    assert_eq!(
        unsafe { synui_runtime_create(&options, &mut runtime, ptr::null_mut()) },
        SYNUI_OK
    );
    let mut app = ptr::null_mut();
    assert_eq!(
        unsafe { synui_app_create(runtime, &mut app, ptr::null_mut()) },
        SYNUI_OK
    );
    let work = SynuiPost {
        struct_size: SynuiPost::STRUCT_SIZE,
        callback: Some(panic_callback),
        user_data: ptr::null_mut(),
        payload: SynuiBytesView {
            ptr: ptr::null(),
            len: 0,
        },
    };
    let mut token = ptr::null_mut();
    assert_eq!(
        unsafe { synui_app_post(app, &work, &mut token, ptr::null_mut()) },
        SYNUI_OK
    );
    let mut drained = 0;
    assert_eq!(
        unsafe { synui_app_drain_posts(app, &mut drained, ptr::null_mut()) },
        SYNUI_PANIC_CONTAINED
    );
    assert_eq!(drained, 1);
    let mut info = SynuiPostTokenInfo {
        struct_size: SynuiPostTokenInfo::STRUCT_SIZE,
        state: u32::MAX,
        callback_status: u32::MAX,
    };
    assert_eq!(
        unsafe { synui_post_token_query(token, &mut info, ptr::null_mut()) },
        SYNUI_OK
    );
    assert_eq!(info.state, SYNUI_POST_FAILED);
    assert_eq!(info.callback_status, SYNUI_PANIC_CONTAINED);
    assert_eq!(
        unsafe { synui_post_token_release(token, ptr::null_mut()) },
        SYNUI_OK
    );
    assert_eq!(unsafe { synui_app_release(app, ptr::null_mut()) }, SYNUI_OK);
    assert_eq!(
        unsafe { synui_runtime_release(runtime, ptr::null_mut()) },
        SYNUI_OK
    );
}
