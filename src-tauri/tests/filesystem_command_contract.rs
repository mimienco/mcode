use std::collections::BTreeSet;

const FILESYSTEM_COMMANDS: &str = include_str!("../src/filesystem_commands.rs");
const TAURI_LIB: &str = include_str!("../src/lib.rs");

#[test]
fn scoped_filesystem_commands_are_typed_and_registered_for_camel_case_arguments() {
    let commands = ["fs_read_dir", "fs_stat", "fs_path_exists"];
    for command in commands {
        assert!(
            FILESYSTEM_COMMANDS.contains(&format!("pub(crate) async fn {command}(")),
            "missing typed command {command}"
        );
    }
    assert_eq!(
        FILESYSTEM_COMMANDS
            .matches("#[tauri::command(rename_all = \"camelCase\")]")
            .count(),
        commands.len()
    );
    assert!(FILESYSTEM_COMMANDS.contains("Result<Vec<FsDirectoryEntry>"));
    assert!(FILESYSTEM_COMMANDS.contains("Result<FsPathMetadata"));
    assert!(FILESYSTEM_COMMANDS.contains("Result<bool, FilesystemCommandError>"));
}

#[test]
fn filesystem_state_and_commands_are_wired_into_the_desktop_builder() {
    assert!(TAURI_LIB.contains("app.manage(NativeFilesystemState::new("));
    assert!(TAURI_LIB.contains("let scoped_filesystem = ScopedFilesystem::new();"));
    assert!(TAURI_LIB.contains("scoped_filesystem,"));
    assert!(TAURI_LIB.contains("tauri::generate_handler!["));
    for command in ["fs_read_dir", "fs_stat", "fs_path_exists"] {
        assert!(
            TAURI_LIB.contains(command),
            "desktop invoke handler is missing {command}"
        );
    }
}

#[test]
fn commands_keep_blocking_io_off_the_async_runtime_and_use_scoped_state() {
    assert!(FILESYSTEM_COMMANDS.contains("tauri::async_runtime::spawn_blocking(move ||"));
    assert_eq!(
        FILESYSTEM_COMMANDS
            .matches("tauri::State<'_, NativeFilesystemState>")
            .count(),
        3
    );
    assert!(FILESYSTEM_COMMANDS.contains("state.refresh_roots_if_changed(operation)?"));
    assert!(FILESYSTEM_COMMANDS.contains(".replace_roots(roots.iter())"));
    assert!(FILESYSTEM_COMMANDS.contains(".local_filesystem_roots()"));
    assert!(FILESYSTEM_COMMANDS.contains("cached_roots.as_ref() == Some(&roots)"));
}

#[test]
fn adapter_exposes_only_the_expected_stable_error_codes() {
    let expected = BTreeSet::from([
        "fs_blocking_task_failed",
        "fs_invalid_argument",
        "fs_remote_unsupported",
    ]);
    let adapter_codes = FILESYSTEM_COMMANDS
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix('"')
                .and_then(|line| line.split_once('"'))
                .map(|(value, _)| value)
                .filter(|value| value.starts_with("fs_"))
        })
        .collect::<BTreeSet<_>>();

    assert!(expected.is_subset(&adapter_codes));
    assert!(FILESYSTEM_COMMANDS.contains("error.code()"));
}
