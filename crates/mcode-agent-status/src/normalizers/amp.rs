//! Amp's multiplexed thread hooks and bounded insertion-ordered cache eviction.

use crate::listener_state::{HookListenerState, clear_pane_turn_cache_state};
use crate::normalizers::{provider_status::normalize_cached_status, tool_snapshot as tool};
use crate::types::ParsedAgentStatusPayload;
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default)]
pub struct AmpCacheOrder {
    prompts: Vec<String>,
    tools: Vec<String>,
    completed: Vec<String>,
}

impl AmpCacheOrder {
    pub fn remove_pane(&mut self, pane: &str) {
        let prefix = format!("{pane}\0");
        for keys in [&mut self.prompts, &mut self.tools, &mut self.completed] {
            keys.retain(|key| key != pane && !key.starts_with(&prefix));
        }
    }

    pub fn move_pane(&mut self, from: &str, to: &str) {
        let prefix = format!("{from}\0");
        for keys in [&mut self.prompts, &mut self.tools, &mut self.completed] {
            let moved: Vec<_> = keys
                .iter()
                .filter(|key| key.as_str() == from || key.starts_with(&prefix))
                .map(|key| format!("{to}{}", &key[from.len()..]))
                .collect();
            keys.retain(|key| key != from && !key.starts_with(&prefix));
            keys.extend(moved);
        }
    }

    fn remove(&mut self, key: &str) {
        self.prompts.retain(|value| value != key);
        self.tools.retain(|value| value != key);
        self.completed.retain(|value| value != key);
    }
}

pub fn normalize_amp_event(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let thread = tool::read_first_string(payload, &["threadId", "threadID", "thread_id"])
        .filter(|thread| thread.encode_utf16().count() <= 256);
    let key = thread
        .map(|thread| format!("{pane}\0amp:{thread}"))
        .unwrap_or_else(|| pane.to_string());
    if event == Some("session.start") {
        clear_pane_turn_cache_state(state, &key);
        state.amp_cache_order.prompts.retain(|value| value != &key);
        state.amp_cache_order.tools.retain(|value| value != &key);
        if key != pane {
            clear_pane_turn_cache_state(state, pane);
        }
        return None;
    }
    let state_name = match event {
        Some("agent.start" | "tool.call" | "tool.result") => "working",
        Some("agent.end") => "done",
        _ => return None,
    };
    if event == Some("agent.start") {
        state.amp_completed_cache_keys.remove(&key);
        state
            .amp_cache_order
            .completed
            .retain(|value| value != &key);
    } else if matches!(event, Some("tool.call" | "tool.result"))
        && state.amp_completed_cache_keys.contains(&key)
    {
        return None;
    }
    let explicit = tool::read_first_string(
        payload,
        &[
            "prompt",
            "user_prompt",
            "userPrompt",
            "initial_prompt",
            "initialPrompt",
            "user_message",
        ],
    );
    let message_is_prompt = event == Some("agent.start")
        || (event == Some("agent.end") && !state.last_prompt_by_pane_key.contains_key(&key));
    let prompt = explicit
        .as_deref()
        .unwrap_or(if message_is_prompt { prompt } else { "" });
    let reset = event == Some("agent.start");
    if reset {
        state.amp_cache_order.prompts.retain(|value| value != &key);
        state.amp_cache_order.tools.retain(|value| value != &key);
    }
    let mut result = normalize_cached_status(
        state,
        &key,
        prompt,
        "amp",
        state_name,
        reset,
        extract_fields(event, payload),
    )?;
    result.interrupted = event == Some("agent.end")
        && payload.get("status").and_then(Value::as_str) == Some("cancelled");
    if state.last_prompt_by_pane_key.contains_key(&key)
        && !state.amp_cache_order.prompts.contains(&key)
    {
        state.amp_cache_order.prompts.push(key.clone());
    }
    if !state.amp_cache_order.tools.contains(&key) {
        state.amp_cache_order.tools.push(key.clone());
    }
    if event == Some("agent.end") && state.amp_completed_cache_keys.insert(key.clone()) {
        state.amp_cache_order.completed.push(key.clone());
    }
    prune(state, pane, &key);
    Some(result)
}

fn prune(state: &mut HookListenerState, pane: &str, current: &str) {
    let prefix = format!("{pane}\0amp:");
    if !current.starts_with(&prefix) {
        return;
    }
    let mut keys = Vec::new();
    for key in state
        .amp_cache_order
        .prompts
        .iter()
        .chain(&state.amp_cache_order.tools)
        .chain(&state.amp_cache_order.completed)
    {
        if key.starts_with(&prefix) && !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    let overflow = keys.len().saturating_sub(32);
    for key in keys.into_iter().filter(|key| key != current).take(overflow) {
        state.last_prompt_by_pane_key.remove(&key);
        state.last_tool_by_pane_key.remove(&key);
        state.amp_completed_cache_keys.remove(&key);
        state.amp_cache_order.remove(&key);
    }
}

fn extract_fields(event: Option<&str>, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    if !matches!(event, Some("tool.call" | "tool.result")) {
        return tool::ToolSnapshot::default();
    }
    let name = tool::read_first_string(payload, &["tool", "toolName", "name"]);
    let keys = ["input", "tool_input", "arguments"];
    let input = keys
        .iter()
        .find_map(|key| tool::derive_tool_input_preview(name.as_deref(), payload.get(*key)))
        .or_else(|| {
            keys.iter()
                .find_map(|key| tool::derive_fallback_tool_input_preview(payload.get(*key)))
        });
    let mut update = tool::tool_update(name, input, None, tool::has_any_own_field(payload, &keys));
    if event == Some("tool.result") {
        update.last_assistant_message =
            tool::read_first_string(payload, &["error", "output", "result", "message"]).or_else(
                || {
                    ["output", "result"]
                        .iter()
                        .find_map(|key| tool::extract_tool_response_text(payload.get(*key)))
                },
            );
    }
    update
}
