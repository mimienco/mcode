//! Renderer contract for the SSH commands (`ssh_connect`, `ssh_exec`,
//! `ssh_open_pty`, `ssh_write_pty`, `ssh_resize_pty`, `ssh_close_pty`,
//! `ssh_disconnect`) — spec `docs/migration/PTY_CONTRACT_SPEC.md` §9.3. The
//! commands delegate to the `*_for` seams the same way the pty commands do,
//! so these tests exercise the exact command bodies without a Tauri runtime.
//!
//! No live sshd is required (or available on this box): the error paths run
//! against an empty `SshState`, and the serde tests pin the wire shapes the
//! renderer's `ssh-client.ts` DTOs mirror.

use mcode_desktop::{
    DesktopCommandError, SshAuthDto, SshConnectResponse, SshExecResponse, SshOpenPtyResponse,
    SshState, SshTargetDto, ssh_close_pty_for, ssh_disconnect_for, ssh_exec_for, ssh_open_pty_for,
    ssh_resize_pty_for, ssh_write_pty_for,
};
use mcode_ssh::pty_id::{parse_app_ssh_pty_id, to_app_ssh_pty_id};
use serde_json::json;

fn error_json(error: DesktopCommandError) -> serde_json::Value {
    serde_json::to_value(error).expect("desktop command error must serialize")
}

#[tokio::test]
async fn ssh_exec_reports_ssh_connection_gone_for_an_unknown_id() {
    let state = SshState::new();

    let error = ssh_exec_for(&state, 9999, "echo hi".into(), None)
        .await
        .expect_err("unknown connection id must fail");

    assert_eq!(error_json(error)["code"], json!("ssh_connection_gone"));
}

#[tokio::test]
async fn ssh_open_pty_reports_ssh_connection_gone_for_an_unknown_id() {
    let state = SshState::new();

    let error = ssh_open_pty_for(&state, 9999, 80, 24)
        .await
        .expect_err("unknown connection id must fail");

    assert_eq!(error_json(error)["code"], json!("ssh_connection_gone"));
}

#[tokio::test]
async fn ssh_write_pty_reports_ssh_pty_gone_for_an_unknown_id() {
    let state = SshState::new();

    let error = ssh_write_pty_for(&state, "ssh:1@@pty-99".into(), "ls".into())
        .await
        .expect_err("unknown pty id must fail");

    assert_eq!(error_json(error)["code"], json!("ssh_pty_gone"));
}

#[tokio::test]
async fn ssh_resize_pty_reports_ssh_pty_gone_for_an_unknown_id() {
    let state = SshState::new();

    let error = ssh_resize_pty_for(&state, "ssh:1@@pty-99".into(), 80, 24)
        .await
        .expect_err("unknown pty id must fail");

    assert_eq!(error_json(error)["code"], json!("ssh_pty_gone"));
}

#[tokio::test]
async fn ssh_close_pty_is_idempotent_for_an_unknown_id() {
    let state = SshState::new();

    ssh_close_pty_for(&state, "ssh:1@@pty-99".into())
        .await
        .expect("closing an unknown pty must succeed");
}

#[tokio::test]
async fn ssh_disconnect_is_idempotent_for_an_unknown_id() {
    let state = SshState::new();

    ssh_disconnect_for(&state, 9999)
        .await
        .expect("disconnecting an unknown connection must succeed");
}

#[tokio::test]
async fn ssh_write_pty_rejects_remote_runtime_terminal_ids() {
    let state = SshState::new();

    let error = ssh_write_pty_for(&state, "remote:7".into(), "ls".into())
        .await
        .expect_err("remote runtime ids must never route into the ssh provider");

    assert_eq!(error_json(error)["code"], json!("ssh_pty_gone"));
}

#[test]
fn ssh_connect_response_serializes_the_renderer_contract() {
    assert_eq!(
        serde_json::to_value(SshConnectResponse::Connected { connection_id: 3 }).unwrap(),
        json!({ "status": "connected", "connectionId": 3 })
    );
    assert_eq!(
        serde_json::to_value(SshConnectResponse::HostKeyUnknown {
            connect_attempt_id: 9,
            fingerprint: "SHA256:abc".into(),
            algorithm: "ssh-ed25519".into(),
            key_base64: "AAA=".into(),
        })
        .unwrap(),
        json!({
            "status": "host-key-unknown",
            "connectAttemptId": 9,
            "fingerprint": "SHA256:abc",
            "algorithm": "ssh-ed25519",
            "keyBase64": "AAA="
        })
    );
    assert_eq!(
        serde_json::to_value(SshConnectResponse::HostKeyChanged {
            fingerprint: "SHA256:abc".into(),
            algorithm: "ssh-ed25519".into(),
        })
        .unwrap(),
        json!({
            "status": "host-key-changed",
            "fingerprint": "SHA256:abc",
            "algorithm": "ssh-ed25519"
        })
    );
    assert_eq!(
        serde_json::to_value(SshConnectResponse::Rejected).unwrap(),
        json!({ "status": "rejected" })
    );
}

#[test]
fn ssh_exec_response_serializes_camel_case() {
    let value = serde_json::to_value(SshExecResponse {
        stdout: "out".into(),
        stderr: "err".into(),
        exit_code: 0,
    })
    .unwrap();

    assert_eq!(
        value,
        json!({ "stdout": "out", "stderr": "err", "exitCode": 0 })
    );
}

#[test]
fn ssh_open_pty_response_serializes_camel_case() {
    let value = serde_json::to_value(SshOpenPtyResponse {
        pty_id: "ssh:1@@pty-2".into(),
    })
    .unwrap();

    assert_eq!(value, json!({ "ptyId": "ssh:1@@pty-2" }));
}

#[test]
fn app_ssh_pty_ids_round_trip_through_the_crate_codec() {
    // The ids ssh_open_pty_for mints: plain decimal connection id, `pty-N`
    // relay id. They must parse back to the same connection.
    let app_id = to_app_ssh_pty_id("12", "pty-3").expect("plain decimal ids must encode");
    let parsed = parse_app_ssh_pty_id(&app_id).expect("app id must parse back");

    assert_eq!(parsed.connection_id, "12");
    assert_eq!(parsed.relay_pty_id, "pty-3");
    // The execution-host form (`ssh:<targetId>` hostId) names the same
    // connection: encoding through it must round-trip identically.
    let host_form = to_app_ssh_pty_id("ssh:12", "pty-3").expect("host form must encode");
    assert_eq!(host_form, app_id);
}

#[test]
fn ssh_target_dto_deserializes_the_internally_tagged_key_auth_form() {
    let target: SshTargetDto = serde_json::from_value(json!({
        "host": "build-01",
        "port": 2222,
        "username": "dev",
        "auth": { "type": "key", "keyPath": "~/.ssh/id_ed25519" },
        "connectTimeoutMs": 30_000
    }))
    .expect("internally tagged auth must deserialize");

    assert_eq!(target.host, "build-01");
    assert_eq!(target.port, 2222);
    assert_eq!(target.username, "dev");
    assert_eq!(target.connect_timeout_ms, Some(30_000));
    match target.auth {
        SshAuthDto::Key {
            key_path,
            passphrase,
        } => {
            assert_eq!(key_path, "~/.ssh/id_ed25519");
            assert_eq!(passphrase, None);
        }
        other => panic!("expected key auth, got {other:?}"),
    }
}

#[test]
fn ssh_target_dto_agent_and_password_variants_deserialize() {
    let agent: SshTargetDto = serde_json::from_value(json!({
        "host": "h", "port": 22, "username": "u", "auth": { "type": "agent" }
    }))
    .expect("agent auth must deserialize");
    assert!(matches!(agent.auth, SshAuthDto::Agent));

    let password: SshTargetDto = serde_json::from_value(json!({
        "host": "h", "port": 22, "username": "u",
        "auth": { "type": "password", "password": "secret" }
    }))
    .expect("password auth must deserialize");
    match password.auth {
        SshAuthDto::Password { password } => assert_eq!(password, "secret"),
        other => panic!("expected password auth, got {other:?}"),
    }
}
