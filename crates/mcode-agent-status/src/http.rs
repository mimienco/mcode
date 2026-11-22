//! Pure loopback hook-request handler — Rust port of `handleRequest`
//! (`server.ts:2430-2508`) + `readRequestBody`/`parseFormEncodedBody`
//! (`agent-hook-listener.ts:459-558`). No tokio: the tauri layer adapts hyper
//! to these parts. Response semantics: 204 always on accepted/malformed
//! (fail-open), 403 on token mismatch, 404 on non-POST/unknown route.

use serde_json::Value;

pub use crate::claude_statusline::CLAUDE_STATUSLINE_PATHNAME;
use crate::claude_statusline::{ClaudeStatusLineRateLimits, parse_claude_statusline_body};
use crate::listener_state::HookListenerState;
use crate::normalizers::{NormalizeError, normalize_hook_payload};
/// Request parts the transport layer hands over.
pub struct HookRequestParts {
    pub method: String,
    pub path: String,
    pub token_header: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

pub struct HookResponse {
    pub status: u16,
    pub statusline: Option<StatusLineOutcome>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StatusLineOutcome {
    NoUsableWindows,
    /// Parsing succeeded; selected-account attribution and its service consumer are not ported.
    AwaitingTrustedAccountConsumer(ClaudeStatusLineRateLimits),
}

impl HookResponse {
    fn status(status: u16) -> Self {
        Self {
            status,
            statusline: None,
        }
    }
}

pub struct HookServerConfig {
    pub token: String,
    pub env: String,
}

/// Form fields the CLI curl posts (`installer-utils.ts:236-276`).
struct HookForm {
    pane_key: String,
    #[allow(dead_code)]
    tab_id: Option<String>,
    #[allow(dead_code)]
    launch_token: Option<String>,
    #[allow(dead_code)]
    worktree_id: Option<String>,
    #[allow(dead_code)]
    env: Option<String>,
    #[allow(dead_code)]
    version: Option<String>,
    payload: Option<String>,
    extra_fields: Vec<(String, String)>,
}

fn parse_form(body: &[u8]) -> HookForm {
    let mut form = HookForm {
        pane_key: String::new(),
        tab_id: None,
        launch_token: None,
        worktree_id: None,
        env: None,
        version: None,
        payload: None,
        extra_fields: Vec::new(),
    };
    for (key, value) in form_urlencoded::parse(body) {
        let value = value.into_owned();
        match key.as_ref() {
            "paneKey" => form.pane_key = value,
            "tabId" => form.tab_id = Some(value),
            "launchToken" => form.launch_token = Some(value),
            "worktreeId" => form.worktree_id = Some(value),
            "env" => form.env = Some(value),
            "version" => form.version = Some(value),
            "payload" => form.payload = Some(value),
            _ => form.extra_fields.push((key.into_owned(), value)),
        }
    }
    form
}

/// Ingest through the store so lifecycle fences and persistence apply to HTTP too.
pub fn handle_hook_request(
    store: &mut crate::store::StatusStore,
    config: &HookServerConfig,
    req: HookRequestParts,
    now: i64,
) -> (HookResponse, Vec<crate::store::StoreEffect>) {
    let (source, mut record) = match parse_request(config, req) {
        Ok(parsed) => parsed,
        Err(status) => return (HookResponse::status(status), Vec::new()),
    };
    let Some(source) = source else {
        let statusline = parse_claude_statusline_body(&record)
            .map(StatusLineOutcome::AwaitingTrustedAccountConsumer)
            .unwrap_or(StatusLineOutcome::NoUsableWindows);
        return (
            HookResponse {
                status: 204,
                statusline: Some(statusline),
            },
            Vec::new(),
        );
    };
    let pane_key = record
        .get("paneKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if !store.accepts_pane(pane_key) {
        return (HookResponse::status(204), Vec::new());
    }
    let resolved = store.resolve_pane_key(pane_key);
    if resolved != pane_key {
        if let Some(tab) = record
            .get("tabId")
            .and_then(Value::as_str)
            .filter(|tab| !tab.is_empty())
            && crate::types::parse_pane_key(pane_key).is_none_or(|(expected, _)| expected != tab)
        {
            return (HookResponse::status(204), Vec::new());
        }
        if let Some((tab, _)) = crate::types::parse_pane_key(&resolved) {
            record.insert("tabId".into(), Value::String(tab));
        }
    }
    record.insert("paneKey".into(), Value::String(resolved));
    let event = normalize_record(store.state_mut(), config, source, &record);
    let effects = event
        .map(|event| store.apply(event, now))
        .unwrap_or_default();
    (HookResponse::status(204), effects)
}

/// Adapter for isolated normalizer tests without lifecycle state.
pub fn handle_hook_request_with_status(
    state: &mut HookListenerState,
    entries: &mut std::collections::HashMap<String, crate::types::EnrichedAgentHookEventPayload>,
    config: &HookServerConfig,
    req: HookRequestParts,
    now: i64,
) -> (u16, Option<crate::types::AgentHookEventPayload>) {
    let (source, record) = match parse_request(config, req) {
        Ok(parsed) => parsed,
        Err(status) => return (status, None),
    };
    let Some(source) = source else {
        return (204, None);
    };
    let event = normalize_record(state, config, source, &record);
    let accepted = event
        .filter(|event| crate::store::apply_enriched(state, entries, event.clone(), now).is_some());
    (204, accepted)
}

fn parse_request(
    config: &HookServerConfig,
    req: HookRequestParts,
) -> Result<
    (
        Option<crate::types::AgentHookSource>,
        serde_json::Map<String, Value>,
    ),
    u16,
> {
    // Non-POST → 404 (server.ts:2431-2435).
    if req.method != "POST" {
        return Err(404);
    }
    // Token check → 403 (server.ts:2437-2441).
    if req.token_header.as_deref() != Some(config.token.as_str()) {
        return Err(403);
    }
    let pathname = req.path.split(['?', '#']).next().unwrap_or(&req.path);
    let source = crate::types::hook_source_for_pathname(pathname);
    if source.is_none() && pathname != CLAUDE_STATUSLINE_PATHNAME {
        return Err(404);
    }
    if req.body.len() > crate::types::HOOK_REQUEST_MAX_BYTES {
        return Err(204);
    }
    // Body parse: JSON content-type → object; otherwise form-urlencoded
    // (default JSON per readRequestBody:468-558 — but the installed hooks
    // always post the form shape, so form is the default here; a raw JSON
    // object body with a JSON content type is the alternate).
    let content_type = req
        .content_type
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    let body_text = String::from_utf8_lossy(&req.body);
    // Strip a UTF-8 BOM like parseAgentHookJson.
    let body_text = body_text.strip_prefix('\u{feff}').unwrap_or(&body_text);
    let (form_pane_key, form_fields, payload_string, json_object) =
        if content_type.contains("application/json") {
            match serde_json::from_str::<Value>(body_text) {
                Ok(Value::Object(record)) => (
                    record
                        .get("paneKey")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    Vec::new(),
                    record.get("payload").and_then(|payload| match payload {
                        Value::String(text) => Some(text.clone()),
                        _ if source.is_none() => None,
                        other => Some(other.to_string()),
                    }),
                    Some(record),
                ),
                _ => (String::new(), Vec::new(), None, None),
            }
        } else {
            let form = parse_form(&req.body);
            let mut fields: Vec<(String, String)> = Vec::new();
            for (key, value) in [
                ("tabId", &form.tab_id),
                ("launchToken", &form.launch_token),
                ("worktreeId", &form.worktree_id),
                ("env", &form.env),
                ("version", &form.version),
            ] {
                if let Some(value) = value {
                    fields.push((key.to_string(), value.clone()));
                }
            }
            fields.extend(form.extra_fields);
            (form.pane_key, fields, form.payload, None)
        };
    if source.is_some() && form_pane_key.is_empty() && json_object.is_none() {
        // Fail-open 204 on unattributable bodies (track('agent_hook_unattributed')).
        return Err(204);
    }
    let envelope = json_object.map(|mut record| {
        if !form_pane_key.is_empty() {
            record.insert("paneKey".into(), Value::String(form_pane_key.clone()));
        }
        if let Some(payload) = &payload_string {
            record.insert("payload".into(), Value::String(payload.clone()));
        }
        record
    });
    let body_record: serde_json::Map<String, Value> = match envelope {
        Some(record) => record,
        None => {
            let mut record = serde_json::Map::new();
            record.insert("paneKey".into(), Value::String(form_pane_key.clone()));
            for (key, value) in &form_fields {
                record.insert(key.clone(), Value::String(value.clone()));
            }
            if let Some(payload) = payload_string {
                record.insert("payload".into(), Value::String(payload));
            }
            record
        }
    };
    Ok((source, body_record))
}

fn normalize_record(
    state: &mut HookListenerState,
    config: &HookServerConfig,
    source: crate::types::AgentHookSource,
    record: &serde_json::Map<String, Value>,
) -> Option<crate::types::AgentHookEventPayload> {
    match normalize_hook_payload(
        state,
        source,
        record,
        record.get("payload").and_then(Value::as_str),
        &config.env,
    ) {
        Ok(event) => Some(event),
        Err(NormalizeError::UnsupportedProvider(_)) | Err(NormalizeError::NoPayload) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listener_state::HookListenerState;
    use crate::types::{AgentStatusState, ParsedAgentStatusPayload};

    const UUID: &str = "3f2b8c64-9a1e-4c7d-8b2a-1f6e5d4c3b2a";

    fn config() -> HookServerConfig {
        HookServerConfig {
            token: "tok".into(),
            env: "development".into(),
        }
    }

    fn run(
        method: &str,
        path: &str,
        token: Option<&str>,
        body: &[u8],
    ) -> (u16, Option<crate::types::AgentHookEventPayload>) {
        let mut state = HookListenerState::default();
        let mut entries = std::collections::HashMap::new();
        handle_hook_request_with_status(
            &mut state,
            &mut entries,
            &config(),
            HookRequestParts {
                method: method.into(),
                path: path.into(),
                token_header: token.map(str::to_string),
                content_type: None,
                body: body.to_vec(),
            },
            1_000,
        )
    }

    fn request(body: Vec<u8>) -> HookRequestParts {
        HookRequestParts {
            method: "POST".into(),
            path: "/hook/claude".into(),
            token_header: Some("tok".into()),
            content_type: Some("application/x-www-form-urlencoded".into()),
            body,
        }
    }

    #[test]
    fn malformed_unicode_percent_sequences_never_panic() {
        let form = parse_form("pane%4Bey=test&payload=%\u{e9}+%25%2B".as_bytes());
        assert_eq!(form.pane_key, "test");
        assert_eq!(form.payload.as_deref(), Some("%\u{e9} %+"));
    }

    #[test]
    fn request_body_limit_prevents_state_mutation() {
        let pane = format!("t1:{UUID}");
        let payload =
            serde_json::json!({"hook_event_name":"UserPromptSubmit","prompt":"should not apply"});
        let mut body = format!(
            "paneKey={}&payload={}&padding=",
            form_encode(&pane),
            form_encode(&payload.to_string())
        )
        .into_bytes();
        body.resize(crate::types::HOOK_REQUEST_MAX_BYTES + 1, b'x');
        let mut store = crate::store::StatusStore::new(None);
        let (response, effects) = handle_hook_request(&mut store, &config(), request(body), 1_000);
        assert_eq!(response.status, 204);
        assert!(effects.is_empty());
        assert!(store.snapshot_ipc().is_empty());
        assert!(store.state().last_prompt_by_pane_key.is_empty());
    }

    #[test]
    fn form_envelope_event_name_survives_empty_antigravity_stdin() {
        let pane = format!("t1:{UUID}");
        let mut store = crate::store::StatusStore::new(None);
        let body = format!(
            "paneKey={}&hook_event_name=PreInvocation",
            form_encode(&pane)
        );
        let mut req = request(body.into_bytes());
        req.path = "/hook/antigravity".into();
        let (response, effects) = handle_hook_request(&mut store, &config(), req, 1_000);
        assert_eq!(response.status, 204);
        assert_eq!(effects.len(), 1);
        assert_eq!(
            store.snapshot_ipc()[0].payload.agent_type.as_deref(),
            Some("antigravity")
        );
    }

    #[test]
    fn transferred_process_keeps_posting_original_pane_and_tab() {
        let from = format!("t1:{UUID}");
        let to = format!("t2:{UUID}");
        let mut store = crate::store::StatusStore::new(None);
        assert!(store.transfer_pane_authority(&from, &to, Some("pty")));
        let payload = serde_json::json!({"hook_event_name":"UserPromptSubmit","prompt":"late"});
        let body = format!(
            "paneKey={}&tabId=t1&payload={}",
            form_encode(&from),
            form_encode(&payload.to_string())
        )
        .into_bytes();
        let (_, effects) = handle_hook_request(&mut store, &config(), request(body), 1_000);
        assert_eq!(effects.len(), 1);
        assert_eq!(store.snapshot_ipc()[0].pane_key, to);
        assert_eq!(store.snapshot_ipc()[0].tab_id.as_deref(), Some("t2"));
    }

    #[test]
    fn closed_tab_request_cannot_resurrect_status_or_prompt_cache() {
        let pane = format!("t1:{UUID}");
        let payload = serde_json::json!({"hook_event_name":"UserPromptSubmit","prompt":"late"});
        let body = format!(
            "paneKey={}&payload={}",
            form_encode(&pane),
            form_encode(&payload.to_string())
        )
        .into_bytes();
        let mut store = crate::store::StatusStore::new(None);
        store.drop_status_entries_by_tab_prefix("t1");
        let (response, effects) = handle_hook_request(&mut store, &config(), request(body), 1_000);
        assert_eq!(response.status, 204);
        assert!(effects.is_empty());
        assert!(store.snapshot_ipc().is_empty());
        assert!(store.state().last_prompt_by_pane_key.is_empty());
    }

    #[test]
    fn non_post_is_404() {
        let (status, _) = run("GET", "/hook/claude", Some("tok"), b"");
        assert_eq!(status, 404);
    }

    #[test]
    fn bad_token_is_403() {
        let (status, _) = run("POST", "/hook/claude", Some("wrong"), b"");
        assert_eq!(status, 403);
        let (status, _) = run("POST", "/hook/claude", None, b"");
        assert_eq!(status, 403);
    }

    #[test]
    fn unknown_route_is_404() {
        let (status, _) = run("POST", "/hook/unknown", Some("tok"), b"");
        assert_eq!(status, 404);
    }

    #[test]
    fn malformed_body_is_204_fail_open() {
        let (status, payload) = run("POST", "/hook/claude", Some("tok"), b"paneKey=&payload=");
        assert_eq!(status, 204);
        assert!(payload.is_none());
    }

    #[test]
    fn form_encoded_plus_and_percent_decoding() {
        let pane = format!("t%2F1:{UUID}");
        let body = format!(
            "paneKey={}&payload={}",
            pane, "%7B%22hook_event_name%22%3A%22UserPromptSubmit%22%7D"
        );
        let mut state = HookListenerState::default();
        let mut entries = std::collections::HashMap::new();
        let (status, payload) = handle_hook_request_with_status(
            &mut state,
            &mut entries,
            &config(),
            HookRequestParts {
                method: "POST".into(),
                path: "/hook/claude".into(),
                token_header: Some("tok".into()),
                content_type: None,
                body: body.as_bytes().to_vec(),
            },
            1_000,
        );
        assert_eq!(status, 204);
        let event = payload.expect("decoded event");
        assert_eq!(event.pane_key, format!("t/1:{UUID}"));
        assert_eq!(event.hook_event_name.as_deref(), Some("UserPromptSubmit"));
    }

    #[test]
    fn claude_user_prompt_submit_ingests_working_row() {
        let pane = format!("t1:{UUID}");
        let payload = serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "prompt": "fix the bug"
        })
        .to_string();
        let body = format!(
            "paneKey={}&tabId=t1&env=development&version=1&payload={}",
            form_encode(&pane),
            form_encode(&payload)
        );
        let mut state = HookListenerState::default();
        let mut entries = std::collections::HashMap::new();
        let (status, event) = handle_hook_request_with_status(
            &mut state,
            &mut entries,
            &config(),
            HookRequestParts {
                method: "POST".into(),
                path: "/hook/claude".into(),
                token_header: Some("tok".into()),
                content_type: None,
                body: body.as_bytes().to_vec(),
            },
            1_000,
        );
        assert_eq!(status, 204);
        let event = event.unwrap();
        assert_eq!(event.payload.state, AgentStatusState::Working);
        assert_eq!(event.payload.prompt, "fix the bug");
        assert_eq!(event.payload.agent_type.as_deref(), Some("claude"));
        // The row is in the store.
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn claude_stop_ingests_done_row() {
        let pane = format!("t1:{UUID}");
        let payload = serde_json::json!({
            "hook_event_name": "Stop",
            "last_assistant_message": "all done"
        })
        .to_string();
        let body = format!(
            "paneKey={}&payload={}",
            form_encode(&pane),
            form_encode(&payload)
        );
        let mut state = HookListenerState::default();
        let mut entries = std::collections::HashMap::new();
        let (_, event) = handle_hook_request_with_status(
            &mut state,
            &mut entries,
            &config(),
            HookRequestParts {
                method: "POST".into(),
                path: "/hook/claude".into(),
                token_header: Some("tok".into()),
                content_type: None,
                body: body.as_bytes().to_vec(),
            },
            1_000,
        );
        let event = event.unwrap();
        assert_eq!(event.payload.state, AgentStatusState::Done);
        assert_eq!(
            event.payload.last_assistant_message.as_deref(),
            Some("all done")
        );
    }

    #[test]
    fn unhandled_provider_event_still_answers_204() {
        let pane = format!("t1:{UUID}");
        let payload = serde_json::json!({"hook_event_name": "unknown-event"}).to_string();
        let body = format!(
            "paneKey={}&payload={}",
            form_encode(&pane),
            form_encode(&payload)
        );
        let mut state = HookListenerState::default();
        let mut entries = std::collections::HashMap::new();
        let (status, payload) = handle_hook_request_with_status(
            &mut state,
            &mut entries,
            &config(),
            HookRequestParts {
                method: "POST".into(),
                path: "/hook/amp".into(),
                token_header: Some("tok".into()),
                content_type: None,
                body: body.as_bytes().to_vec(),
            },
            1_000,
        );
        assert_eq!(status, 204);
        assert!(payload.is_none());
    }

    #[test]
    fn tab_id_mismatch_drops_event() {
        let pane = format!("t1:{UUID}");
        let payload =
            serde_json::json!({"hook_event_name": "UserPromptSubmit", "prompt": "x"}).to_string();
        let body = format!(
            "paneKey={}&tabId=t2&payload={}",
            form_encode(&pane),
            form_encode(&payload)
        );
        let mut state = HookListenerState::default();
        let mut entries = std::collections::HashMap::new();
        let (status, payload) = handle_hook_request_with_status(
            &mut state,
            &mut entries,
            &config(),
            HookRequestParts {
                method: "POST".into(),
                path: "/hook/claude".into(),
                token_header: Some("tok".into()),
                content_type: None,
                body: body.as_bytes().to_vec(),
            },
            1_000,
        );
        assert_eq!(status, 204);
        assert!(payload.is_none());
    }

    #[test]
    fn statusline_route_accepted() {
        let (status, _) = run("POST", CLAUDE_STATUSLINE_PATHNAME, Some("tok"), b"{}");
        assert_eq!(status, 204);
    }

    fn form_encode(value: &str) -> String {
        let mut out = String::new();
        for byte in value.as_bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(*byte as char)
                }
                _ => out.push_str(&format!("%{byte:02X}")),
            }
        }
        out
    }

    #[allow(dead_code)]
    fn silence(_: &ParsedAgentStatusPayload) {}
}
