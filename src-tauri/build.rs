fn main() {
    for variable in [
        "MCODE_TAURI_UPDATER_PUBLIC_KEY",
        "MCODE_TAURI_UPDATER_STABLE_ENDPOINT",
        "MCODE_TAURI_UPDATER_RC_ENDPOINT",
        "MCODE_TAURI_UPDATER_HOURLY_ENDPOINT",
        "MCODE_TAURI_UPDATER_DAILY_ENDPOINT",
        "MCODE_TAURI_UPDATER_ADHOC_ENDPOINT",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    tauri_build::build();
}
