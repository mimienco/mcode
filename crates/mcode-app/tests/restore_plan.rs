//! Startup restore plan tests, using fixtures consistent with the shared v1
//! checkpoint fixture in `mcode-settings`
//! (`crates/mcode-settings/tests/fixtures/shutdown-checkpoint-v1.json`).

use mcode_app::{
    RestorableSession, RestorableWindow, RestoreError, SkipReason, SkippedSession,
    build_restore_plan, build_restore_plan_from_bytes, plan_from_error,
};
use mcode_platform::window::{MonitorBounds, WindowBounds};
use mcode_settings::{ExecutionHostId, ShutdownCheckpoint, ShutdownSession, WorkspaceSessionState};
use serde_json::{Value, json};

const LEAF_ID: &str = "11111111-1111-4111-8111-111111111111";
const WORKTREE: &str = "repo-1::D:/workspace";

fn monitor() -> MonitorBounds {
    MonitorBounds {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    }
}

fn bounds(x: i32, y: i32, width: u32, height: u32) -> WindowBounds {
    WindowBounds {
        x,
        y,
        width,
        height,
    }
}

fn minimal_session_state(tabs: Value, layouts: Value, extra: Value) -> WorkspaceSessionState {
    let mut state = json!({
        "activeRepoId": "repo-1",
        "activeWorktreeId": WORKTREE,
        "activeTabId": "tab-1",
        "tabsByWorktree": tabs,
        "terminalLayoutsByTabId": layouts
    });
    let extra = extra.as_object().cloned().unwrap_or_default();
    for (key, value) in extra {
        state[key] = value;
    }
    serde_json::from_value(state).unwrap()
}

fn terminal_tab(id: &str, pty_id: Option<&str>) -> Value {
    json!({
        "id": id,
        "ptyId": pty_id,
        "worktreeId": WORKTREE,
        "title": "Terminal",
        "customTitle": null,
        "color": null,
        "sortOrder": 0,
        "createdAt": 1
    })
}

fn leaf_layout(bindings: Value) -> Value {
    json!({
        "root": { "type": "leaf", "leafId": LEAF_ID },
        "activeLeafId": LEAF_ID,
        "expandedLeafId": null,
        "ptyIdsByLeafId": bindings,
        "buffersByLeafId": { (LEAF_ID): "buffer" },
        "scrollbackRefsByLeafId": { (LEAF_ID): "scrollback-1" },
        "titlesByLeafId": { (LEAF_ID): "Pane title" }
    })
}

fn checkpoint_with_state(state: WorkspaceSessionState) -> ShutdownCheckpoint {
    ShutdownCheckpoint {
        version: mcode_settings::SHUTDOWN_CHECKPOINT_VERSION,
        sessions: vec![ShutdownSession {
            host_id: None,
            state,
        }],
        ui: serde_json::from_value(json!({})).unwrap(),
    }
}

#[test]
fn happy_restore_reopens_the_window_and_rebinds_local_sessions() {
    // Window placement lives in the persisted profile root (Electron's
    // store.getUI()), so the planner reads it from there, not from the
    // typed checkpoint patch.
    let persisted = json!({
        "ui": {
            "windowBounds": { "x": 100, "y": 80, "width": 1280, "height": 800 },
            "windowMaximized": false
        }
    });
    let checkpoint = checkpoint_with_state(minimal_session_state(
        json!({ (WORKTREE): [terminal_tab("tab-1", Some("pty-1"))] }),
        json!({ "tab-1": leaf_layout(json!({ (LEAF_ID): "pty-1" })) }),
        json!({}),
    ));

    let plan = build_restore_plan(&persisted, &checkpoint, &[monitor()]).unwrap();

    assert!(!plan.incomplete);
    assert_eq!(
        plan.windows,
        vec![RestorableWindow {
            bounds: Some(bounds(100, 80, 1280, 800)),
            maximized: false
        }]
    );
    assert_eq!(
        plan.sessions,
        vec![RestorableSession {
            workspace_id: WORKTREE.into(),
            session_id: format!("tab-1:{LEAF_ID}"),
            kind: "local".into(),
        }]
    );
    assert!(plan.skipped.is_empty());
}

#[test]
fn offscreen_window_bounds_are_not_restored() {
    let persisted = json!({
        "ui": {
            "windowBounds": { "x": 4000, "y": 0, "width": 1280, "height": 800 },
            "windowMaximized": true
        }
    });
    let checkpoint = checkpoint_with_state(minimal_session_state(
        json!({ (WORKTREE): [terminal_tab("tab-1", Some("pty-1"))] }),
        json!({ "tab-1": leaf_layout(json!({ (LEAF_ID): "pty-1" })) }),
        json!({}),
    ));

    let plan = build_restore_plan(&persisted, &checkpoint, &[monitor()]).unwrap();

    assert!(!plan.incomplete);
    assert_eq!(plan.windows.len(), 1);
    assert_eq!(plan.windows[0].bounds, None);
    // A maximized window without restorable bounds falls back to default
    // placement, which is not a maximized restore.
    assert!(!plan.windows[0].maximized);
}

#[test]
fn tombstoned_pane_is_skipped_not_restored() {
    // A tombstone retires the pane before bindings are preserved; after
    // reconciliation the tab disappears from the partition, so nothing is
    // projected as restorable and the pane is reported as skipped.
    let pane_key = format!("tab-1:{LEAF_ID}");
    let checkpoint = checkpoint_with_state(minimal_session_state(
        json!({ (WORKTREE): [terminal_tab("tab-1", Some("pty-1"))] }),
        json!({ "tab-1": leaf_layout(json!({ (LEAF_ID): "pty-1" })) }),
        json!({
            "terminalPtyIncarnationsByPaneKey": { (&pane_key): "incarnation-1" },
            "terminalSurfaceTombstonesByPaneKey": {
                (&pane_key): {
                    "worktreeId": WORKTREE,
                    "parentTabId": "tab-1",
                    "leafId": LEAF_ID,
                    "ptyId": "pty-1",
                    "incarnationId": "incarnation-1",
                    "retiredAt": 1
                }
            }
        }),
    ));

    let plan = build_restore_plan(&json!({}), &checkpoint, &[monitor()]).unwrap();

    assert!(plan.sessions.is_empty());
    assert_eq!(
        plan.skipped,
        vec![SkippedSession {
            session_id: pane_key,
            reason: SkipReason::Retired,
        }]
    );
}

#[test]
fn terminated_ssh_lease_is_not_resurrected() {
    // Mirrors `local_checkpoint_does_not_resurrect_a_terminated_ssh_binding`
    // from the settings contract tests: the persisted root carries the last
    // merged workspace session with an SSH relay binding whose lease is
    // terminated, and the checkpoint re-sends the tabs with null ptyIds (a
    // stale renderer snapshot). Binding preservation must refuse to restore
    // the dead lease, so the reconciled session has no bindings and the plan
    // must not resurrect it.
    let ssh_pty = "ssh:ssh-target@@remote-pty";
    let pane_key = format!("tab-1:{LEAF_ID}");
    let prior_session = minimal_session_state(
        json!({ (WORKTREE): [terminal_tab("tab-1", Some(ssh_pty))] }),
        json!({ "tab-1": leaf_layout(json!({ (LEAF_ID): ssh_pty })) }),
        json!({}),
    );
    let persisted = json!({
        "repos": [{ "id": "repo-1", "connectionId": "ssh-target" }],
        "sshRemotePtyLeases": [{
            "targetId": "ssh-target",
            "ptyId": "remote-pty",
            "worktreeId": WORKTREE,
            "tabId": "tab-1",
            "leafId": LEAF_ID,
            "state": "terminated",
            "createdAt": 1,
            "updatedAt": 2
        }],
        "workspaceSession": serde_json::to_value(prior_session).unwrap()
    });
    let checkpoint = checkpoint_with_state(minimal_session_state(
        json!({ (WORKTREE): [terminal_tab("tab-1", None)] }),
        json!({ "tab-1": leaf_layout(json!({})) }),
        json!({
            "sleepingAgentSessionsByPaneKey": {
                (&pane_key): {
                    "paneKey": pane_key,
                    "tabId": "tab-1",
                    "worktreeId": WORKTREE,
                    "agent": "codex",
                    "providerSession": {
                        "key": "session_id",
                        "id": "session-1",
                        "transcriptPath": "D:/sessions/session-1.jsonl"
                    },
                    "prompt": "Continue",
                    "state": "working",
                    "capturedAt": 1,
                    "updatedAt": 2,
                    "terminalTitle": "Codex",
                    "lastAssistantMessage": "Working",
                    "interrupted": false,
                    "connectionId": null,
                    "launchConfig": {
                        "agentCommand": "codex",
                        "agentArgs": "resume session-1",
                        "agentEnv": { "MCODE_SESSION": "session-1" },
                        "ompResumeFilePath": "D:/sessions/session-1.jsonl"
                    },
                    "origin": "quit",
                    "automaticResumeBlockedBy": "legacy-orchestration-worker",
                    "restoreOnTabOpenOnly": true
                }
            }
        }),
    ));

    let plan = build_restore_plan(&persisted, &checkpoint, &[monitor()]).unwrap();

    assert!(!plan.incomplete);
    assert!(
        plan.sessions.is_empty(),
        "no session may be resurrected: {:?}",
        plan.sessions
    );
    // The pane itself is not retired (the tab shell survives, unbound); it
    // shows up as neither restorable nor skipped with a lease reason because
    // the reconciled state carries no binding for it at all.
    assert!(
        !plan
            .skipped
            .iter()
            .any(|skip| skip.reason == SkipReason::LeaseTerminated && skip.session_id == pane_key),
        "a suppressed binding never enters the plan as restorable: {:?}",
        plan.skipped
    );
}

#[test]
fn unsupported_checkpoint_version_is_rejected_and_falls_back_incomplete() {
    let checkpoint = checkpoint_with_state(minimal_session_state(
        json!({ (WORKTREE): [terminal_tab("tab-1", Some("pty-1"))] }),
        json!({ "tab-1": leaf_layout(json!({ (LEAF_ID): "pty-1" })) }),
        json!({}),
    ));
    let mut stale = checkpoint;
    stale.version = 99;

    let plan = build_restore_plan(&json!({}), &stale, &[monitor()]);

    assert_eq!(plan.unwrap_err(), RestoreError::SchemaRejected);
    let fallback = plan_from_error(RestoreError::SchemaRejected);
    assert!(fallback.incomplete);
    assert!(fallback.windows.is_empty());
    assert!(fallback.sessions.is_empty());
    assert!(fallback.skipped.is_empty());
}

#[test]
fn oversized_checkpoint_bytes_are_rejected_as_unreadable() {
    let oversized = vec![b'a'; 64 * 1024 * 1024 + 1];

    let plan = build_restore_plan_from_bytes(&oversized, b"{}");

    assert!(plan.incomplete);
    assert!(plan.windows.is_empty());
    assert!(plan.sessions.is_empty());
    assert!(plan.skipped.is_empty());
}

#[test]
fn malformed_checkpoint_bytes_yield_an_incomplete_plan() {
    let plan = build_restore_plan_from_bytes(b"{}", b"{not-json");

    assert!(plan.incomplete);
    assert!(plan.windows.is_empty());
    assert!(plan.sessions.is_empty());
}

#[test]
fn malformed_persisted_bytes_yield_an_incomplete_plan() {
    let plan = build_restore_plan_from_bytes(b"[not-an-object", b"{}");

    assert!(plan.incomplete);
    assert!(plan.windows.is_empty());
}

#[test]
fn raw_bytes_path_restores_the_shared_v1_fixture_shape() {
    // Mirrors the session payload of the shared fixture, exercising the
    // TUI agent (launchAgent codex) and the resumable sleeping-agent
    // variants through the raw-bytes entry point against an empty profile.
    let pane_key = format!("tab-1:{LEAF_ID}");
    let payload = json!({
        "version": 1,
        "sessions": [{
            "hostId": "runtime:host-1",
            "state": {
                "activeRepoId": "repo-1",
                "activeWorktreeId": "worktree-1",
                "activeTabId": "tab-1",
                "tabsByWorktree": {
                    "worktree-1": [{
                        "id": "tab-1",
                        "ptyId": "pty-1",
                        "worktreeId": "worktree-1",
                        "title": "Terminal",
                        "defaultTitle": "Terminal",
                        "generatedTitle": null,
                        "aiVaultTitle": {
                            "agent": "codex",
                            "sessionId": "session-1",
                            "title": "Parity session"
                        },
                        "quickCommandLabel": null,
                        "customTitle": null,
                        "color": null,
                        "isPinned": true,
                        "sortOrder": 0,
                        "createdAt": 1,
                        "generation": 1,
                        "startupCwd": "D:/workspace",
                        "launchAgent": "codex"
                    }]
                },
                "terminalLayoutsByTabId": {
                    "tab-1": {
                        "root": { "type": "leaf", "leafId": LEAF_ID },
                        "activeLeafId": LEAF_ID,
                        "expandedLeafId": null,
                        "ptyIdsByLeafId": { (LEAF_ID): "pty-1" },
                        "buffersByLeafId": { (LEAF_ID): "buffer" },
                        "scrollbackRefsByLeafId": { (LEAF_ID): "scrollback-1" },
                        "titlesByLeafId": { (LEAF_ID): "Terminal" }
                    }
                },
                "sleepingAgentSessionsByPaneKey": {
                    (&pane_key): {
                        "paneKey": pane_key,
                        "tabId": "tab-1",
                        "worktreeId": "worktree-1",
                        "agent": "codex",
                        "providerSession": {
                            "key": "session_id",
                            "id": "session-1",
                            "transcriptPath": "D:/sessions/session-1.jsonl"
                        },
                        "prompt": "Continue",
                        "state": "working",
                        "capturedAt": 1,
                        "updatedAt": 2,
                        "terminalTitle": "Codex",
                        "lastAssistantMessage": "Working",
                        "interrupted": false,
                        "connectionId": null,
                        "launchConfig": {
                            "agentCommand": "codex",
                            "agentArgs": "resume session-1",
                            "agentEnv": { "MCODE_SESSION": "session-1" },
                            "ompResumeFilePath": "D:/sessions/session-1.jsonl"
                        },
                        "origin": "quit",
                        "automaticResumeBlockedBy": "legacy-orchestration-worker",
                        "restoreOnTabOpenOnly": true
                    }
                },
                "terminalPtyIncarnationsByPaneKey": { (&pane_key): "incarnation-1" },
                "terminalTopologyRevisionByRepoId": { "repo-1": 1 }
            }
        }],
        "ui": { "activeView": "terminal" }
    });
    let bytes = serde_json::to_vec(&payload).unwrap();

    let plan = build_restore_plan_from_bytes(b"{}", &bytes);

    assert!(!plan.incomplete);
    assert_eq!(plan.windows, vec![RestorableWindow::default()]);
    assert_eq!(plan.sessions.len(), 1);
    let session = &plan.sessions[0];
    assert_eq!(session.workspace_id, "worktree-1");
    assert_eq!(session.session_id, pane_key);
    assert_eq!(session.kind, "runtime:host-1");
    assert!(plan.skipped.is_empty());
}

#[test]
fn host_partition_sessions_carry_their_execution_host_kind() {
    let checkpoint = ShutdownCheckpoint {
        version: mcode_settings::SHUTDOWN_CHECKPOINT_VERSION,
        sessions: vec![ShutdownSession {
            host_id: Some(ExecutionHostId::parse("ssh:target-1").unwrap()),
            state: minimal_session_state(
                json!({ (WORKTREE): [terminal_tab("tab-1", Some("pty-1"))] }),
                json!({ "tab-1": leaf_layout(json!({ (LEAF_ID): "pty-1" })) }),
                json!({}),
            ),
        }],
        ui: serde_json::from_value(json!({})).unwrap(),
    };

    let plan = build_restore_plan(&json!({}), &checkpoint, &[monitor()]).unwrap();

    assert_eq!(plan.sessions.len(), 1);
    assert_eq!(plan.sessions[0].kind, "ssh:target-1");
    assert_eq!(plan.sessions[0].workspace_id, WORKTREE);
}
