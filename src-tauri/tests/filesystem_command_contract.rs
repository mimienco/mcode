use std::collections::BTreeSet;

const FILESYSTEM_COMMANDS: &str = include_str!("../src/filesystem_commands.rs");
const TAURI_LIB: &str = include_str!("../src/lib.rs");
const EXACT_PATH_CAPABILITY: &str = include_str!("../../crates/mcode-fs/src/external.rs");
const EXTERNAL_PATH_GRANTS: &str = include_str!("../src/external_path_grants.rs");
const NATIVE_FILE_DROP: &str = include_str!("../../src/shared/native-file-drop.ts");

const FILESYSTEM_COMMAND_NAMES: [&str; 7] = [
    "fs_read_dir",
    "fs_read_file",
    "fs_write_file_atomic",
    "fs_stat",
    "fs_path_exists",
    "fs_authorize_external_path",
    "fs_revoke_external_path",
];

fn command_declaration(command: &str) -> &str {
    let marker = format!("pub(crate) async fn {command}(");
    let start = FILESYSTEM_COMMANDS
        .find(&marker)
        .unwrap_or_else(|| panic!("missing typed command {command}"));
    let declaration = &FILESYSTEM_COMMANDS[start..];
    declaration
        .split_once(") ->")
        .map_or(declaration, |(signature, _)| signature)
}

#[test]
fn scoped_filesystem_commands_are_typed_and_registered_for_camel_case_arguments() {
    for command in FILESYSTEM_COMMAND_NAMES {
        assert!(
            FILESYSTEM_COMMANDS.contains(&format!("pub(crate) async fn {command}(")),
            "missing typed command {command}"
        );
        let declaration = command_declaration(command);
        assert!(
            declaration.contains("window: tauri::WebviewWindow"),
            "{command} must receive the invoking window so grants cannot cross windows"
        );
        assert!(
            FILESYSTEM_COMMANDS[..FILESYSTEM_COMMANDS
                .find(&format!("pub(crate) async fn {command}("))
                .unwrap()]
                .trim_end()
                .ends_with("#[tauri::command(rename_all = \"camelCase\")]"),
            "{command} must preserve camelCase invoke arguments"
        );
    }
    assert_eq!(
        FILESYSTEM_COMMANDS
            .matches("#[tauri::command(rename_all = \"camelCase\")]")
            .count(),
        FILESYSTEM_COMMAND_NAMES.len()
    );
    assert!(FILESYSTEM_COMMANDS.contains("Result<Vec<FsDirectoryEntry>"));
    assert!(FILESYSTEM_COMMANDS.contains("Result<FsFileContent"));
    assert!(FILESYSTEM_COMMANDS.contains("write_file_atomic"));
    assert!(FILESYSTEM_COMMANDS.contains("Result<FsPathMetadata"));
    assert!(FILESYSTEM_COMMANDS.contains("Result<bool, FilesystemCommandError>"));
}

#[test]
fn filesystem_state_and_commands_are_wired_into_the_desktop_builder() {
    assert!(TAURI_LIB.contains("app.manage(NativeFilesystemState::new("));
    assert!(TAURI_LIB.contains("let scoped_filesystem = ScopedFilesystem::new();"));
    assert!(TAURI_LIB.contains("scoped_filesystem,"));
    assert!(TAURI_LIB.contains("tauri::generate_handler!["));
    for command in FILESYSTEM_COMMAND_NAMES {
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
        7
    );
    assert_eq!(
        FILESYSTEM_COMMANDS
            .matches("window: tauri::WebviewWindow")
            .count(),
        7
    );
    assert!(FILESYSTEM_COMMANDS.contains("state.refresh_roots_if_changed(operation).and_then"));
    assert!(FILESYSTEM_COMMANDS.contains(".replace_roots(roots.iter())"));
    assert!(FILESYSTEM_COMMANDS.contains(".local_filesystem_roots()"));
    assert!(FILESYSTEM_COMMANDS.contains("cached_roots.as_ref() == Some(&roots)"));
}

#[test]
fn external_grants_are_exact_window_scoped_and_revoked_on_navigation_or_destroy() {
    assert!(FILESYSTEM_COMMANDS.contains("ExternalPathGrantRegistry"));
    assert!(FILESYSTEM_COMMANDS.contains("capability_for(&window_label, &target_path)"));
    assert!(FILESYSTEM_COMMANDS.contains(".claim(window_label, target_path)"));
    assert!(TAURI_LIB.contains("tauri::webview::PageLoadEvent::Started"));
    assert!(TAURI_LIB.contains("tauri::WindowEvent::DragDrop"));
    assert!(TAURI_LIB.contains("stage_external_paths(&destroyed_window_label, paths)"));
    assert!(TAURI_LIB.contains("tauri::WindowEvent::Destroyed"));
    assert!(TAURI_LIB.contains("revoke_window_grants(window.label())"));
    assert!(TAURI_LIB.contains("revoke_window_grants(&destroyed_window_label)"));
    assert!(TAURI_LIB.contains("fs_authorize_external_path,"));
    assert!(TAURI_LIB.contains("fs_revoke_external_path,"));
    assert!(EXTERNAL_PATH_GRANTS.contains("pending_native_grants: VecDeque<RetainedGrant>"));
    let claim = EXTERNAL_PATH_GRANTS
        .split_once("pub(crate) fn claim")
        .unwrap()
        .1
        .split_once("pub(crate) fn capability_for")
        .unwrap()
        .0;
    assert!(!claim.contains("ExactPathCapability::authorize"));
    assert!(EXTERNAL_PATH_GRANTS.contains("DEFAULT_MAX_RETAINED_GRANTS: usize = 256"));
    assert!(EXTERNAL_PATH_GRANTS.contains("MAX_NATIVE_DROP_PATHS: usize = 256"));
    assert!(NATIVE_FILE_DROP.contains("NATIVE_FILE_DROP_MAX_PATHS = 256"));
    assert!(EXTERNAL_PATH_GRANTS.contains("windows: HashMap<String, WindowGrants>"));
    assert!(FILESYSTEM_COMMANDS.contains("FsAuthorizeExternalPathResult"));
    assert!(FILESYSTEM_COMMANDS.contains("FsExternalPathAuthorization::TrustedRoot"));
}

#[test]
fn exact_external_writes_keep_the_typed_read_only_denial() {
    assert!(
        FILESYSTEM_COMMANDS.contains("Self::Exact(capability) => capability.write_file_atomic")
    );
    assert!(EXACT_PATH_CAPABILITY.contains("FsError::ExternalWriteUnsupported"));
    assert!(FILESYSTEM_COMMANDS.contains("_ => error.code()"));
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
