//! Wire types — faithful serde mirror of the Electron agentStatus contract
//! (camelCase on the wire, `Option` ⇔ absent, exact field caps).
//!
//! Originals: `src/shared/agent-status-types.ts`, `src/shared/agent-hook-types.ts`,
//! `src/main/agent-hooks/server.ts`, `src/shared/agent-hook-listener.ts`,
//! `src/shared/stable-pane-id.ts`.
//!
//! Wave 37 (D-013): claude-family provider normalizers live; the other hook
//! sources are named by [`AgentHookSource`] but land as wave-38 tranches.

use serde::{Deserialize, Serialize};

/// `agent-interrupt-intent.ts:6`.
pub const AGENT_INTERRUPT_SETTLE_MS: i64 = 500;
/// `server.ts` (`MAX_PANE_KEY_LEN`); `agent-hook-listener.ts:115`.
pub const MAX_PANE_KEY_LEN: usize = 200;
/// `server.ts:955` — staleness bound for interrupt inference.
pub const AGENT_STATUS_STALE_AFTER_MS: i64 = 30 * 60 * 1000;
/// `server.ts:206` — persisted rows older than this are dropped on hydrate.
pub const HYDRATE_MAX_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// `server.ts:201`.
pub const STATUS_PERSIST_DEBOUNCE_MS: u64 = 250;
/// `server.ts:192-193`.
pub const ASSISTANT_MESSAGE_RETRY_ATTEMPTS: u32 = 5;
pub const ASSISTANT_MESSAGE_RETRY_MS: u64 = 50;
/// `server.ts:194`.
pub const CODEX_SUBAGENT_POLL_MS: u64 = 1_000;
/// `server.ts:195`.
pub const INTERRUPTED_DONE_LATE_WORKING_SUPPRESSION_MS: i64 = 15_000;
/// `server.ts:198` — hydrate fails closed on version mismatch.
pub const LAST_STATUS_FILE_VERSION: u32 = 2;
/// `agent-hook-listener.ts:83`.
pub const HOOK_REQUEST_MAX_BYTES: usize = 1_000_000;
/// `server.ts:754` — the store's observation authority id.
pub const MAIN_AGENT_HOOKS_AUTHORITY_ID: &str = "main-agent-hooks";
/// `server.ts:209-212` — LRU bounds.
pub const CLOSED_AGENT_STATUS_TAB_IDS_MAX: usize = 1024;
pub const CLOSED_AGENT_STATUS_PANE_KEYS_MAX: usize = 1024;
pub const PANE_KEY_ALIASES_MAX: usize = 1024;
pub const RETIRED_PANE_FENCES_MAX: usize = 1024;
/// `agent-hook-types.ts:56`.
pub const MCODE_HOOK_PROTOCOL_VERSION: &str = "1";

/// `agent-status-types.ts:8`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatusState {
    #[default]
    Working,
    Blocked,
    Waiting,
    Done,
}

/// The 18 hook routes (`agent-hook-listener.ts:4660-4679`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentHookSource {
    Claude,
    Codex,
    Gemini,
    Antigravity,
    Amp,
    Opencode,
    MimoCode,
    Cursor,
    Pi,
    Omp,
    PrimeAgent,
    Droid,
    CommandCode,
    Grok,
    Copilot,
    Hermes,
    Devin,
    Kimi,
}

impl AgentHookSource {
    /// Wire literal (`agent-hook-relay.ts:36-45`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Antigravity => "antigravity",
            Self::Amp => "amp",
            Self::Opencode => "opencode",
            Self::MimoCode => "mimo-code",
            Self::Cursor => "cursor",
            Self::Pi => "pi",
            Self::Omp => "omp",
            Self::PrimeAgent => "prime-agent",
            Self::Droid => "droid",
            Self::CommandCode => "command-code",
            Self::Grok => "grok",
            Self::Copilot => "copilot",
            Self::Hermes => "hermes",
            Self::Devin => "devin",
            Self::Kimi => "kimi",
        }
    }
}

/// `agent-hook-listener.ts:4660-4679` — route table, byte-exact.
pub fn hook_source_for_pathname(path: &str) -> Option<AgentHookSource> {
    let source = match path {
        "/hook/claude" => AgentHookSource::Claude,
        "/hook/codex" => AgentHookSource::Codex,
        "/hook/gemini" => AgentHookSource::Gemini,
        "/hook/antigravity" => AgentHookSource::Antigravity,
        "/hook/amp" => AgentHookSource::Amp,
        "/hook/opencode" => AgentHookSource::Opencode,
        "/hook/mimo-code" => AgentHookSource::MimoCode,
        "/hook/cursor" => AgentHookSource::Cursor,
        "/hook/pi" => AgentHookSource::Pi,
        "/hook/omp" => AgentHookSource::Omp,
        "/hook/prime-agent" => AgentHookSource::PrimeAgent,
        "/hook/droid" => AgentHookSource::Droid,
        "/hook/command-code" => AgentHookSource::CommandCode,
        "/hook/grok" => AgentHookSource::Grok,
        "/hook/copilot" => AgentHookSource::Copilot,
        "/hook/hermes" => AgentHookSource::Hermes,
        "/hook/devin" => AgentHookSource::Devin,
        "/hook/kimi" => AgentHookSource::Kimi,
        _ => return None,
    };
    Some(source)
}

/// `agent-status-types.ts:82-95`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSubagentSnapshot {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub state: AgentSubagentState,
    pub started_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentSubagentState {
    Working,
    Blocked,
    Waiting,
    Idle,
}

/// `agent-status-types.ts:245-258` (`ParsedAgentStatusPayload`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParsedAgentStatusPayload {
    pub state: AgentStatusState,
    #[serde(default)]
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_assistant_message: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupted: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub session_boundary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagents: Option<Vec<AgentSubagentSnapshot>>,
}

/// `agent-session-resume.ts:24-34`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProviderSessionMetadata {
    pub key: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
}

/// `agent-status-observation.ts:37-52` — provenance facet stamped at every
/// ingress; consumers must tolerate absence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusObservation {
    pub origin: AgentStatusObservationOrigin,
    pub authority_id: String,
    pub incarnation: u64,
    pub revision: u64,
    pub observed_at: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub boundary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AgentStatusObservationKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatusObservationOrigin {
    Hook,
    Osc,
    Title,
    Process,
    Launch,
    Orchestration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentStatusObservationKind {
    Transition,
    Snapshot,
    IdentityOnly,
}

/// Transport envelope — `agent-hook-listener.ts:329-369`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct AgentHookEventPayload {
    pub pane_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<AgentHookSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    /// Required on the wire; null for local panes.
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_explicit_prompt: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restored_unconfirmed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_interaction_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_event_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_prompt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_trigger: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teammate_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_session: Option<AgentProviderSessionMetadata>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub provider_session_only: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_replay: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub claude_running_non_agent_task: bool,
    pub payload: ParsedAgentStatusPayload,
}

/// `server.ts:64-77` — the store's row shape (envelope + timing + facet).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedAgentHookEventPayload {
    #[serde(flatten)]
    pub event: AgentHookEventPayload,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub claude_lead_boundary_child_only: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub retained_for_liveness: bool,
    pub received_at: i64,
    pub state_started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<AgentStatusObservation>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restored_unconfirmed: bool,
}

/// Renderer wire row — `agent-status-types.ts:270-292`
/// (`AgentStatusIpcPayload`; `terminalHandle`/`orchestration` enrichment is
/// absent under Tauri and the renderer tolerates absence).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusIpcPayload {
    pub pane_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    #[serde(default)]
    pub connection_id: Option<String>,
    pub received_at: i64,
    pub state_started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_session: Option<AgentProviderSessionMetadata>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub provider_session_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_interaction_key: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restored_unconfirmed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<AgentStatusObservation>,
    #[serde(flatten)]
    pub payload: ParsedAgentStatusPayload,
}

/// `agent-status-types.ts:295-299` — untagged union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
pub enum AgentStatusClearIpcPayload {
    Pane {
        pane_key: String,
    },
    Transient {
        transient: bool,
        connection_id: String,
        cleared_at: i64,
    },
}

/// `agent-status-types.ts:197-206`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationUnsupportedPtyEntry {
    pub pty_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaf_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_key: Option<String>,
    /// Fixed literal in the original.
    pub reason: String,
    pub source: String,
    pub updated_at: i64,
}

/// `agent-interrupt-intent.ts:8-15`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInterruptInferenceRequest {
    pub pane_key: String,
    pub baseline_updated_at: i64,
    pub baseline_state_started_at: i64,
    pub baseline_prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_agent_type: Option<String>,
    pub intent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_count: Option<u32>,
}

/// `agent-question-answered-intent.ts:8-15`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentQuestionAnsweredInferenceRequest {
    pub pane_key: String,
    pub baseline_updated_at: i64,
    pub baseline_state_started_at: i64,
    pub baseline_prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_agent_type: Option<String>,
}

/// `agent-hook-types.ts:31-38`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHookInstallStatus {
    pub agent: String,
    pub state: String,
    pub config_path: String,
    pub managed_hooks_present: bool,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
}

/// `stable-pane-id.ts` UUID grammar (versions 1-5, RFC variant bits).
fn is_stable_pane_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            14 => {
                if !(b'1'..=b'5').contains(byte) {
                    return false;
                }
            }
            19 => {
                if !(b'8'..=b'9').contains(byte) && *byte != b'a' && *byte != b'b' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase() {
                    return false;
                }
            }
        }
    }
    true
}

/// `stable-pane-id.ts:parsePaneKey` — `${tabId}:${leafUuid}`, tabId non-empty
/// without `:`, exactly one colon, trailing leaf a stable UUID.
pub fn parse_pane_key(pane_key: &str) -> Option<(String, String)> {
    let first = pane_key.find(':')?;
    if first == 0 || pane_key[first + 1..].contains(':') || first == pane_key.len() - 1 {
        return None;
    }
    let tab_id = &pane_key[..first];
    let leaf_id = &pane_key[first + 1..];
    if !is_stable_pane_id(leaf_id) {
        return None;
    }
    Some((tab_id.to_string(), leaf_id.to_string()))
}

/// `server.ts:249-253`.
pub fn is_valid_pane_key(value: &str) -> bool {
    value.len() <= MAX_PANE_KEY_LEN && parse_pane_key(value).is_some()
}

/// `stable-pane-id.ts:parseLegacyNumericPaneKey` — `${tabId}:${digits}`.
pub fn parse_legacy_numeric_pane_key(pane_key: &str) -> Option<(String, String)> {
    if pane_key.len() > 256 {
        return None;
    }
    let trimmed = pane_key.trim();
    let delimiter = trimmed.find(':')?;
    if delimiter == 0 || trimmed[delimiter + 1..].contains(':') || delimiter == trimmed.len() - 1 {
        return None;
    }
    let numeric = &trimmed[delimiter + 1..];
    if numeric.is_empty() || !numeric.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((trimmed[..delimiter].to_string(), numeric.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "3f2b8c64-9a1e-4c7d-8b2a-1f6e5d4c3b2a";

    #[test]
    fn clear_source_and_observation_wire_literals_match_electron() {
        assert_eq!(
            serde_json::to_value(AgentHookSource::CommandCode).unwrap(),
            "command-code"
        );
        assert_eq!(
            serde_json::to_value(AgentHookSource::Claude).unwrap(),
            "claude"
        );
        assert_eq!(
            serde_json::to_value(AgentStatusObservationOrigin::Hook).unwrap(),
            "hook"
        );
        assert_eq!(
            serde_json::to_value(AgentStatusClearIpcPayload::Pane {
                pane_key: "pane".into()
            })
            .unwrap(),
            serde_json::json!({"paneKey":"pane"})
        );
        assert_eq!(
            serde_json::to_value(AgentStatusClearIpcPayload::Transient {
                transient: true,
                connection_id: "ssh".into(),
                cleared_at: 10
            })
            .unwrap(),
            serde_json::json!({"transient":true,"connectionId":"ssh","clearedAt":10})
        );
    }

    #[test]
    fn pane_key_grammar_round_trip() {
        assert!(is_valid_pane_key(&format!("tab-1:{UUID}")));
        assert!(!is_valid_pane_key(&format!("tab-1:{UUID}x")));
        assert!(!is_valid_pane_key(&format!(":tab:{UUID}")));
        assert!(!is_valid_pane_key(&format!("tab-1:{UUID}:extra")));
        assert!(!is_valid_pane_key("tab-1:"));
        // Version nibble + variant bits enforced.
        assert!(!is_valid_pane_key(
            "tab-1:3f2b8c64-9a1e-6c7d-8b2a-1f6e5d4c3b2a"
        ));
        assert!(!is_valid_pane_key(
            "tab-1:3f2b8c64-9a1e-4c7d-1b2a-1f6e5d4c3b2a"
        ));
        assert!(!is_valid_pane_key(
            "tab-1:3F2B8C64-9A1E-4C7D-8B2A-1F6E5D4C3B2A"
        ));
    }

    #[test]
    fn legacy_numeric_grammar() {
        assert_eq!(
            parse_legacy_numeric_pane_key("tab-1:42"),
            Some(("tab-1".into(), "42".into()))
        );
        assert_eq!(parse_legacy_numeric_pane_key("tab-1:4x2"), None);
        assert_eq!(parse_legacy_numeric_pane_key(":42"), None);
    }

    #[test]
    fn route_table_is_complete() {
        assert_eq!(
            hook_source_for_pathname("/hook/claude"),
            Some(AgentHookSource::Claude)
        );
        assert_eq!(
            hook_source_for_pathname("/hook/mimo-code"),
            Some(AgentHookSource::MimoCode)
        );
        assert_eq!(
            hook_source_for_pathname("/hook/prime-agent"),
            Some(AgentHookSource::PrimeAgent)
        );
        assert_eq!(hook_source_for_pathname("/hook/unknown"), None);
        assert_eq!(hook_source_for_pathname("/hook/"), None);
    }

    #[test]
    fn payload_wire_shape_is_camel_case() {
        let payload: ParsedAgentStatusPayload = serde_json::from_str(
            r#"{"state":"working","prompt":"fix it","agentType":"claude","turnCompletedAt":12}"#,
        )
        .unwrap();
        assert_eq!(payload.agent_type.as_deref(), Some("claude"));
        assert_eq!(payload.turn_completed_at, Some(12));
        let round: AgentHookEventPayload = serde_json::from_str(
            r#"{"paneKey":"tab-1:3f2b8c64-9a1e-4c7d-8b2a-1f6e5d4c3b2a","connectionId":null,"payload":{"state":"done","prompt":""}}"#,
        )
        .unwrap();
        assert_eq!(round.connection_id, None);
        assert_eq!(round.payload.state, AgentStatusState::Done);
    }
}
