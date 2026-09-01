use tauri_build::Attributes;

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

    // App commands must be capability-gated: declaring the app manifest makes
    // tauri-build autogenerate `allow-<command>` permissions (no default rule,
    // so nothing is granted until a capability file allows it) and makes the
    // runtime ACL check every invoke, including local origins.
    let attributes =
        Attributes::default().app_manifest(tauri_build::AppManifest::new().commands(&[
            "app_get_launch_context",
            "terminal_preview_ack",
            "terminal_preview_connect",
            "terminal_preview_fit",
            "terminal_preview_input",
            "terminal_preview_unsubscribe",
            "ui_clipboard_read_text",
            "ui_clipboard_write_text",
            "ui_clipboard_write_terminal_text",
            "ui_consume_pending_skill_share",
            "ui_export_html_to_pdf",
            "ui_open_settings",
            "ui_pick_floating_markdown_document",
            "ui_pick_floating_workspace_directory",
            "ui_consume_pending_open_settings",
            "ui_open_setup_guide",
            "ui_open_feature_tour",
            "ui_open_crash_report",
            "app_stage_shutdown_checkpoint",
            "app_request_shutdown",
            "app_acknowledge_shutdown",
            "app_cancel_shutdown",
            "app_confirm_and_execute_shutdown",
            "window_hide_to_tray_if_enabled",
            "window_set_minimize_to_tray_on_close",
            "update_get_status",
            "update_check",
            "update_download",
            "update_install",
            "fs_read_dir",
            "fs_read_file",
            "fs_write_file",
            "fs_path_exists",
            "fs_stat",
            "fs_create_dir",
            "fs_create_file",
            "fs_copy_path",
            "fs_rename_path",
            "fs_delete_path",
            "fs_write_file_atomic",
            "fs_search",
            "fs_search_replace",
            "fs_cancel_list_files",
            "fs_cancel_search_replace",
            "fs_watch_worktree",
            "fs_unwatch_worktree",
            "pty_spawn",
            "pty_write",
            "pty_resize",
            "pty_kill",
            "pty_has_pty",
            "pty_ack_data",
            "pty_get_size",
            "ssh_connect",
            "ssh_accept_host_key",
            "ssh_exec",
            "ssh_open_pty",
            "ssh_write_pty",
            "ssh_resize_pty",
            "ssh_close_pty",
            "ssh_disconnect",
            "developer_permissions_get_status",
            "developer_permissions_request",
            "developer_permissions_open_settings",
            "developer_permissions_test_local_network_connection",
        ]));
    if let Err(error) = tauri_build::try_build(attributes) {
        println!("{error:#}");
        std::process::exit(1);
    }
}
