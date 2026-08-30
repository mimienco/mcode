fn main() {
    // `tauri::include_image!` reads `OUT_DIR`, which only exists when the
    // crate has a build script; this stub provides it for the tray icon.
    println!("cargo:rerun-if-changed=assets/icon.png");
}
