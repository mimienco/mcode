//! Claude statusline input contract and selected-account live-usage reduction.
//! Account selection and locale/date presentation belong to the rate-limit service.

use crate::normalizers::fields::trim_ecma;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const CLAUDE_STATUSLINE_PATHNAME: &str = "/statusline/claude";
pub const LIVE_CLAUDE_INGEST_DEDUPE_MS: i64 = 30_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClaudeResetValue {
    Number(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaudeStatusLineWindow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utilization: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<ClaudeResetValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatusLineRateLimits {
    pub config_dir: Option<String>,
    pub five_hour: Option<ClaudeStatusLineWindow>,
    pub seven_day: Option<ClaudeStatusLineWindow>,
}

fn parse_window(value: Option<&Value>) -> Option<ClaudeStatusLineWindow> {
    let fields = value?.as_object()?;
    let used_percentage = fields
        .get("used_percentage")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite());
    let utilization = if used_percentage.is_none() {
        fields
            .get("utilization")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
    } else {
        None
    };
    if used_percentage.is_none() && utilization.is_none() {
        return None;
    }
    let resets_at = match fields.get("resets_at") {
        Some(Value::Number(value)) => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(ClaudeResetValue::Number),
        Some(Value::String(value)) if !trim_ecma(value).is_empty() => {
            Some(ClaudeResetValue::Text(value.clone()))
        }
        _ => None,
    };
    Some(ClaudeStatusLineWindow {
        used_percentage,
        utilization,
        resets_at,
    })
}

pub fn parse_claude_statusline_body(
    body: &Map<String, Value>,
) -> Option<ClaudeStatusLineRateLimits> {
    let raw = body.get("payload")?.as_str()?;
    let payload: Value = serde_json::from_str(raw).ok()?;
    let limits = payload.get("rate_limits")?.as_object()?;
    let five_hour = parse_window(limits.get("five_hour"));
    let seven_day = parse_window(limits.get("seven_day"));
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }
    let config_dir = body
        .get("configDir")
        .and_then(Value::as_str)
        .map(trim_ecma)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(ClaudeStatusLineRateLimits {
        config_dir,
        five_hour,
        seven_day,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeUsageWindow {
    pub used_percent: f64,
    pub window_minutes: u32,
    pub resets_at: Option<f64>,
    pub reset_description: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClaudeResetPresentation {
    pub timestamp_ms: Option<f64>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeUsageMetadata {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_successful_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_provenance: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeUsageSnapshot {
    pub provider: String,
    pub session: Option<ClaudeUsageWindow>,
    pub weekly: Option<ClaudeUsageWindow>,
    pub fable_weekly: Option<ClaudeUsageWindow>,
    pub updated_at: i64,
    pub error: Option<String>,
    pub status: String,
    pub usage_metadata: Option<ClaudeUsageMetadata>,
}

/// Only the selected-account auth resolver may supply this, never a hook payload.
#[derive(Clone, Debug, PartialEq)]
pub struct TrustedClaudeAccount {
    pub config_dir: Option<String>,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClaudeLiveUsageOutcome {
    NoTrustedAccount,
    AccountMismatch,
    NoUsableWindows,
    Duplicate,
    Updated(Box<ClaudeUsageSnapshot>),
}

fn normalize_config_dir(value: Option<&str>) -> Option<String> {
    let value = trim_ecma(value?).replace('\\', "/");
    let value = value.trim_end_matches('/');
    (!value.is_empty()).then(|| value.to_string())
}

fn map_window(
    raw: Option<&ClaudeStatusLineWindow>,
    minutes: u32,
    present_reset: &impl Fn(&ClaudeResetValue) -> ClaudeResetPresentation,
) -> Option<ClaudeUsageWindow> {
    let raw = raw?;
    let used = raw
        .utilization
        .or(raw.used_percentage)
        .filter(|value| value.is_finite())?;
    let reset = raw
        .resets_at
        .as_ref()
        .map(present_reset)
        .unwrap_or_default();
    Some(ClaudeUsageWindow {
        used_percent: used.clamp(0.0, 100.0),
        window_minutes: minutes,
        resets_at: reset.timestamp_ms,
        reset_description: reset.description,
    })
}

fn same_window(left: Option<&ClaudeUsageWindow>, right: Option<&ClaudeUsageWindow>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.used_percent == right.used_percent && left.resets_at == right.resets_at
        }
        _ => false,
    }
}

/// Date parsing/locale rendering is injected so the future service can preserve its host contract.
/// An Updated outcome also tells that service to clear Claude's active failure streak.
/// The caller must fence account switches and provide only that account's previous snapshot.
pub fn reduce_live_claude_usage(
    event: &ClaudeStatusLineRateLimits,
    account: Option<&TrustedClaudeAccount>,
    previous: Option<&ClaudeUsageSnapshot>,
    now: i64,
    present_reset: impl Fn(&ClaudeResetValue) -> ClaudeResetPresentation,
) -> ClaudeLiveUsageOutcome {
    let Some(account) = account else {
        return ClaudeLiveUsageOutcome::NoTrustedAccount;
    };
    if normalize_config_dir(event.config_dir.as_deref())
        != normalize_config_dir(account.config_dir.as_deref())
    {
        return ClaudeLiveUsageOutcome::AccountMismatch;
    }
    let fresh_session = map_window(event.five_hour.as_ref(), 300, &present_reset);
    let fresh_weekly = map_window(event.seven_day.as_ref(), 10080, &present_reset);
    if fresh_session.is_none() && fresh_weekly.is_none() {
        return ClaudeLiveUsageOutcome::NoUsableWindows;
    }
    let session = fresh_session.or_else(|| previous.and_then(|previous| previous.session.clone()));
    let weekly = fresh_weekly.or_else(|| previous.and_then(|previous| previous.weekly.clone()));
    if previous.is_some_and(|previous| {
        previous.status == "ok"
            && previous
                .usage_metadata
                .as_ref()
                .is_some_and(|metadata| metadata.source == "live-session")
            && now.saturating_sub(previous.updated_at) < LIVE_CLAUDE_INGEST_DEDUPE_MS
            && same_window(previous.session.as_ref(), session.as_ref())
            && same_window(previous.weekly.as_ref(), weekly.as_ref())
    }) {
        return ClaudeLiveUsageOutcome::Duplicate;
    }
    ClaudeLiveUsageOutcome::Updated(Box::new(ClaudeUsageSnapshot {
        provider: "claude".to_string(),
        session,
        weekly,
        fable_weekly: previous.and_then(|previous| previous.fable_weekly.clone()),
        updated_at: now,
        error: None,
        status: "ok".to_string(),
        usage_metadata: Some(ClaudeUsageMetadata {
            source: "live-session".to_string(),
            last_successful_source: Some("live-session".to_string()),
            credential_source: previous
                .and_then(|previous| previous.usage_metadata.as_ref())
                .and_then(|metadata| metadata.credential_source.clone()),
            auth_provenance: Some(account.provenance.clone()),
        }),
    }))
}
