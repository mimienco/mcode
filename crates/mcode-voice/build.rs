//! Build script: generate an MSVC import archive for `sherpa-onnx-c-api.dll`
//! so `cargo test -p mcode-voice` can link. No `.lib` import archive ships in
//! `node_modules/sherpa-onnx-win-x64/` — only the raw DLLs.
//!
//! Strategy:
//! 1. Locate the DLL dir via `MCODE_SHERPA_LIB_DIR` or
//!    `<workspace>/node_modules/sherpa-onnx-win-x64`.
//! 2. Generate `sherpa_onnx_c_api.lib` next to the DLL via
//!    `lib.exe /def:` (cached; regenerated only when missing) and emit
//!    `cargo:rustc-link-search` / `cargo:rustc-link-lib` for it.
//! 3. Link with `/DELAYLOAD:sherpa-onnx-c-api.dll` (delayimp) so the DLL is
//!    only loaded on the first FFI call: unit tests that never touch the FFI
//!    run without the DLL present, and the `#[ignore]`d model/DLL tests load
//!    it on demand (the DLL dir must be on PATH, or the DLL co-located with
//!    the binary, at that moment).
//! 4. If the DLL is not found, emit nothing: `cargo check`/`clippy` still
//!    succeed (no linking happens), and `cargo test` fails at link time with
//!    the linker's unresolved-symbol error instead of a silent runtime abort.
//!
//! The generated `.lib` is written next to the DLL in node_modules. If that
//! directory is read-only, set `MCODE_SHERPA_LIB_DIR` to a writable copy of
//! the DLL directory.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=MCODE_SHERPA_LIB_DIR");
    println!("cargo:rerun-if-changed=build.rs");

    let Some(lib_dir) = sherpa_lib_dir() else {
        println!(
            "cargo:warning=mcode-voice: sherpa-onnx DLL dir not found; cargo test will not link. Set MCODE_SHERPA_LIB_DIR or install node_modules."
        );
        return;
    };

    // Reuse a cached import archive if present, else generate one from the
    // DLL's export table. Cached next to the DLL so rebuilds are cheap.
    let dll = lib_dir.join("sherpa-onnx-c-api.dll");
    if !dll.exists() {
        println!(
            "cargo:warning=mcode-voice: {} not found; cargo test will not link.",
            dll.display()
        );
        return;
    }

    let import_lib = lib_dir.join("sherpa_onnx_c_api.lib");
    if !import_lib.exists()
        && let Err(e) = generate_import_lib(&dll, &import_lib)
    {
        println!(
            "cargo:warning=mcode-voice: failed to generate import archive: {e}; cargo test will not link."
        );
        return;
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=sherpa_onnx_c_api");
    // Delay-load so a test binary that never touches FFI symbols does not
    // require the DLL at process start (STATUS_DLL_NOT_FOUND otherwise).
    // First FFI call triggers the load; the DLL dir must be on PATH or the
    // DLL co-located with the binary at that point.
    println!("cargo:rustc-link-lib=dylib=delayimp");
    println!("cargo:rustc-link-arg=/DELAYLOAD:sherpa-onnx-c-api.dll");
}

/// Resolve the DLL directory: env override, then walk up from the manifest
/// dir looking for `node_modules/sherpa-onnx-win-x64`.
fn sherpa_lib_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("MCODE_SHERPA_LIB_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").ok()?);
    let mut dir = manifest.as_path();
    while let Some(parent) = dir.parent() {
        dir = parent;
        let candidate = dir.join("node_modules").join("sherpa-onnx-win-x64");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Generate an MSVC import archive via `lib.exe /def:`.
fn generate_import_lib(dll: &std::path::Path, out: &std::path::Path) -> Result<(), String> {
    // Find lib.exe relative to link.exe that cargo already located, or fall
    // back to a well-known MSVC layout.
    let lib_exe = find_lib_exe().ok_or("lib.exe not found on PATH or standard MSVC locations")?;

    let def = out.with_extension("def");
    std::fs::write(
        &def,
        format!(
            "LIBRARY {}\nEXPORTS\n",
            dll.file_name().unwrap().to_string_lossy()
        ) + SHERPA_EXPORTS
            .iter()
            .map(|s| format!("    {s}\n"))
            .collect::<String>()
            .as_str(),
    )
    .map_err(|e| format!("write def: {e}"))?;

    let status = Command::new(&lib_exe)
        .args([
            "/nologo",
            &format!("/def:{}", def.display()),
            "/machine:x64",
            &format!("/out:{}", out.display()),
        ])
        .status()
        .map_err(|e| format!("run lib.exe: {e}"))?;
    if !status.success() {
        return Err("lib.exe failed".to_string());
    }
    let _ = std::fs::remove_file(&def);
    Ok(())
}

/// Find `lib.exe`: MSVC toolset layout used by VS2022 BuildTools.
fn find_lib_exe() -> Option<PathBuf> {
    let root = PathBuf::from(
        "C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC",
    );
    let mut versions: Vec<_> = std::fs::read_dir(&root)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    versions.sort();
    versions
        .pop()
        .map(|v| v.join("bin/HostX64/x64/lib.exe"))
        .filter(|p| p.exists())
}

/// Symbols exported by sherpa-onnx-c-api.dll (v1.12.37) that this crate
/// references. Verified with dumpbin against the shipped DLL.
const SHERPA_EXPORTS: &[&str] = &[
    "SherpaOnnxCreateOnlineRecognizer",
    "SherpaOnnxDestroyOnlineRecognizer",
    "SherpaOnnxCreateOnlineStream",
    "SherpaOnnxDestroyOnlineStream",
    "SherpaOnnxOnlineStreamAcceptWaveform",
    "SherpaOnnxIsOnlineStreamReady",
    "SherpaOnnxDecodeOnlineStream",
    "SherpaOnnxGetOnlineStreamResult",
    "SherpaOnnxDestroyOnlineRecognizerResult",
    "SherpaOnnxOnlineStreamReset",
    "SherpaOnnxOnlineStreamInputFinished",
    "SherpaOnnxOnlineStreamIsEndpoint",
];
