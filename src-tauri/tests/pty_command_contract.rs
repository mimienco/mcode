//! Renderer contract for the new PTY commands (`pty_ack_data`,
//! `pty_get_size`) — spec `docs/migration/TERMINAL_UI_INTEGRATION.md` §4
//! rollout step 1. The commands delegate to the `*_for` seams the same way
//! the shutdown commands do (`app_shutdown_commands.rs`), so these tests
//! exercise the exact command bodies without a Tauri runtime, and spawn real
//! ConPTY sessions for the live-session paths (`mcode-pty` session tests are
//! the reference for the spec shape).

use mcode_desktop::{
    NativePtyState, PtyAppliedSize, pty_ack_data_for, pty_get_size_for, pty_register_session_for,
};
use mcode_pty::{PtyHandle, PtySession, PtySize, PtySpec};
use serde_json::json;

fn echo_spec() -> PtySpec {
    PtySpec {
        program: "cmd".to_string(),
        args: vec!["/c".to_string(), "echo".to_string(), "hello".to_string()],
        cwd: None,
        env_policy: mcode_process::EnvironmentPolicy::default(),
        size: PtySize { cols: 80, rows: 24 },
        scrollback_rows: 1000,
    }
}

fn error_json(error: mcode_desktop::DesktopCommandError) -> serde_json::Value {
    serde_json::to_value(error).expect("desktop command error must serialize")
}

#[test]
fn pty_ack_data_reports_pty_session_gone_for_an_unknown_id() {
    let state = NativePtyState::new();

    let error = pty_ack_data_for(&state, 9999, 16).expect_err("unknown id must fail");

    assert_eq!(error_json(error)["code"], json!("pty_session_gone"));
}

#[test]
fn pty_get_size_reports_pty_session_gone_for_an_unknown_id() {
    let state = NativePtyState::new();

    let error = pty_get_size_for(&state, 9999).expect_err("unknown id must fail");

    assert_eq!(error_json(error)["code"], json!("pty_session_gone"));
}

#[test]
fn pty_ack_data_acks_a_live_session() {
    let state = NativePtyState::new();
    let session = PtySession::spawn(PtyHandle(1), echo_spec()).expect("spawn");
    pty_register_session_for(&state, 1, session);

    pty_ack_data_for(&state, 1, 1024).expect("ack on a live session must succeed");
}

#[test]
fn pty_get_size_reports_the_applied_kernel_size() {
    let state = NativePtyState::new();
    let session = PtySession::spawn(PtyHandle(2), echo_spec()).expect("spawn");
    pty_register_session_for(&state, 2, session);

    let size = pty_get_size_for(&state, 2).expect("size on a live session must succeed");

    assert_eq!(size, Some(PtyAppliedSize { cols: 80, rows: 24 }));
}

#[test]
fn pty_applied_size_serializes_the_renderer_contract() {
    let value = serde_json::to_value(PtyAppliedSize {
        cols: 120,
        rows: 40,
    })
    .unwrap();

    assert_eq!(value, json!({ "cols": 120, "rows": 40 }));
}
