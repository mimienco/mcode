use mcode_agent_status::claude_statusline::*;
use mcode_agent_status::http::{
    HookRequestParts, HookServerConfig, StatusLineOutcome, handle_hook_request,
};
use mcode_agent_status::store::StatusStore;
use serde_json::{Value, json};

fn parse(payload: Value, config_dir: Option<&str>) -> Option<ClaudeStatusLineRateLimits> {
    parse_claude_statusline_body(
        json!({"payload": payload.to_string(), "configDir": config_dir})
            .as_object()
            .unwrap(),
    )
}

fn event(percent: f64) -> ClaudeStatusLineRateLimits {
    parse(
        json!({"rate_limits": {"five_hour": {"used_percentage": percent}}}),
        None,
    )
    .unwrap()
}

fn account() -> TrustedClaudeAccount {
    TrustedClaudeAccount {
        config_dir: None,
        provenance: "system".to_string(),
    }
}

fn present(raw: &ClaudeResetValue) -> ClaudeResetPresentation {
    let timestamp = match raw {
        ClaudeResetValue::Number(value) => Some(if *value > 10_000_000_000.0 {
            *value
        } else {
            *value * 1000.0
        }),
        ClaudeResetValue::Text(value) if value == "2026-07-20T10:00:00Z" => {
            Some(1_784_541_600_000.0)
        }
        _ => None,
    };
    ClaudeResetPresentation {
        timestamp_ms: timestamp,
        description: timestamp.map(|_| "host locale description".to_string()),
    }
}

fn updated(outcome: ClaudeLiveUsageOutcome) -> ClaudeUsageSnapshot {
    match outcome {
        ClaudeLiveUsageOutcome::Updated(snapshot) => *snapshot,
        other => panic!("expected update: {other:?}"),
    }
}

#[test]
fn parser_matches_electron_windows_and_nullable_config_dir() {
    let parsed = parse(
        json!({"rate_limits": {
            "five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
            "seven_day": {"used_percentage": 41.2, "resets_at": 1712059200}
        }}),
        Some(" /home/dev/managed "),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        json!({
            "configDir": "/home/dev/managed",
            "fiveHour": {"used_percentage": 23.5, "resets_at": 1738425600.0},
            "sevenDay": {"used_percentage": 41.2, "resets_at": 1712059200.0}
        })
    );
    for config in [None, Some(""), Some(" \t\r\n"), Some("\u{feff}")] {
        assert_eq!(
            parse(
                json!({"rate_limits": {"five_hour": {"used_percentage": 0}}}),
                config
            )
            .unwrap()
            .config_dir,
            None
        );
    }
}

#[test]
fn parser_tolerates_utilization_and_iso_reset_but_used_percentage_wins() {
    let parsed = parse(
        json!({"rate_limits": {
            "five_hour": {"utilization": 37, "resets_at": "2026-07-20T10:00:00Z"},
            "seven_day": {"used_percentage": 8, "utilization": 99, "resets_at": "   "}
        }}),
        None,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        json!({"configDir": null,
            "fiveHour": {"utilization": 37.0, "resets_at": "2026-07-20T10:00:00Z"},
            "sevenDay": {"used_percentage": 8.0}
        })
    );
}

#[test]
fn parser_rejects_junk_and_omits_unrelated_payload_fields() {
    for payload in [
        Value::Null,
        json!([]),
        json!("text"),
        json!({}),
        json!({"context_window": {"used_percentage": 8}}),
        json!({"rate_limits": {}}),
        json!({"rate_limits": {"five_hour": {"used_percentage": "NaN"}}}),
    ] {
        assert!(parse(payload, None).is_none());
    }
    for payload in [
        json!(null),
        json!({}),
        json!("not-json"),
        json!("\u{feff}{}"),
    ] {
        assert!(
            parse_claude_statusline_body(json!({"payload": payload}).as_object().unwrap())
                .is_none()
        );
    }
    let parsed = parse(json!({"extra": "not-forwarded", "rate_limits": {"five_hour": {"used_percentage": 5, "extra": "not-forwarded"}}}), None).unwrap();
    assert!(
        !serde_json::to_string(&parsed)
            .unwrap()
            .contains("not-forwarded")
    );
}

#[test]
fn live_usage_requires_trusted_matching_account_before_mapping() {
    let mut event = event(50.0);
    event.five_hour.as_mut().unwrap().resets_at = Some(ClaudeResetValue::Number(1750000000.0));
    let never_map =
        |_: &ClaudeResetValue| -> ClaudeResetPresentation { panic!("unattributed event mapped") };
    assert_eq!(
        reduce_live_claude_usage(&event, None, None, 1000, never_map),
        ClaudeLiveUsageOutcome::NoTrustedAccount
    );
    let managed = TrustedClaudeAccount {
        config_dir: Some("/managed/account".into()),
        provenance: "managed:account".into(),
    };
    assert_eq!(
        reduce_live_claude_usage(&event, Some(&managed), None, 1000, never_map),
        ClaudeLiveUsageOutcome::AccountMismatch
    );
}

#[test]
fn config_attribution_normalizes_separators_and_trailing_slashes_without_case_folding() {
    let mut event = event(20.0);
    event.config_dir = Some(" C:\\Users\\Account\\ ".into());
    let account = TrustedClaudeAccount {
        config_dir: Some("C:/Users/Account".into()),
        provenance: "managed:account".into(),
    };
    let result = updated(reduce_live_claude_usage(
        &event,
        Some(&account),
        None,
        1000,
        present,
    ));
    assert_eq!(
        result.usage_metadata.unwrap().auth_provenance.as_deref(),
        Some("managed:account")
    );
    event.config_dir = Some("C:/Users/account".into());
    assert_eq!(
        reduce_live_claude_usage(&event, Some(&account), None, 1000, present),
        ClaudeLiveUsageOutcome::AccountMismatch
    );
}

#[test]
fn live_usage_clamps_percent_maps_reset_and_preserves_missing_windows_and_fable() {
    let both = parse(json!({"rate_limits": {"five_hour": {"used_percentage": -10, "resets_at": 1750000000}, "seven_day": {"utilization": 150}}}), None).unwrap();
    let mut previous = updated(reduce_live_claude_usage(
        &both,
        Some(&account()),
        None,
        1000,
        present,
    ));
    assert_eq!(previous.session.as_ref().unwrap().used_percent, 0.0);
    assert_eq!(previous.session.as_ref().unwrap().window_minutes, 300);
    assert_eq!(
        previous.session.as_ref().unwrap().resets_at,
        Some(1_750_000_000_000.0)
    );
    assert_eq!(previous.weekly.as_ref().unwrap().used_percent, 100.0);
    assert_eq!(previous.weekly.as_ref().unwrap().window_minutes, 10080);
    previous.fable_weekly = previous.weekly.clone();
    previous.usage_metadata.as_mut().unwrap().credential_source = Some("keychain".into());
    previous.status = "error".into();
    previous.error = Some("rate limited".into());
    let result = updated(reduce_live_claude_usage(
        &event(25.0),
        Some(&account()),
        Some(&previous),
        2000,
        present,
    ));
    assert_eq!(result.session.as_ref().unwrap().used_percent, 25.0);
    assert_eq!(result.weekly, previous.weekly);
    assert_eq!(result.fable_weekly, previous.fable_weekly);
    assert_eq!(result.error, None);
    assert_eq!(result.status, "ok");
    assert_eq!(
        result.usage_metadata.unwrap().credential_source.as_deref(),
        Some("keychain")
    );
}

#[test]
fn duplicate_window_ignores_presentation_but_changed_usage_and_thirty_seconds_update() {
    let event = parse(json!({"rate_limits": {"five_hour": {"used_percentage": 20, "resets_at": "2026-07-20T10:00:00Z"}}}), None).unwrap();
    let prior = updated(reduce_live_claude_usage(
        &event,
        Some(&account()),
        None,
        1000,
        present,
    ));
    let different_description = |raw: &ClaudeResetValue| {
        let mut value = present(raw);
        value.description = Some("new locale".into());
        value
    };
    assert_eq!(
        reduce_live_claude_usage(
            &event,
            Some(&account()),
            Some(&prior),
            30_999,
            different_description
        ),
        ClaudeLiveUsageOutcome::Duplicate
    );
    assert!(matches!(
        reduce_live_claude_usage(&event, Some(&account()), Some(&prior), 31_000, present),
        ClaudeLiveUsageOutcome::Updated(_)
    ));
    let mut changed = event;
    changed.five_hour.as_mut().unwrap().used_percentage = Some(21.0);
    assert!(matches!(
        reduce_live_claude_usage(&changed, Some(&account()), Some(&prior), 1001, present),
        ClaudeLiveUsageOutcome::Updated(_)
    ));
}

fn post(payload: &str, token: &str, pane: Option<&str>) -> HookRequestParts {
    let mut body = form_urlencoded::Serializer::new(String::new());
    body.append_pair("payload", payload)
        .append_pair("configDir", " C:/accounts/plus+percent% ");
    if let Some(pane) = pane {
        body.append_pair("paneKey", pane);
    }
    HookRequestParts {
        method: "POST".into(),
        path: CLAUDE_STATUSLINE_PATHNAME.into(),
        token_header: Some(token.into()),
        content_type: Some("application/x-www-form-urlencoded".into()),
        body: body.finish().into_bytes(),
    }
}

#[test]
fn http_statusline_has_explicit_unwired_outcome_without_agent_status_or_pane_requirement() {
    let config = HookServerConfig {
        token: "token".into(),
        env: "production".into(),
    };
    let payload = json!({"rate_limits": {"five_hour": {"used_percentage": 12.5}, "seven_day": {"used_percentage": 40}}}).to_string();
    for pane in [None, Some("pane-1")] {
        let mut store = StatusStore::new(None);
        let (response, effects) =
            handle_hook_request(&mut store, &config, post(&payload, "token", pane), 1000);
        assert_eq!(response.status, 204);
        assert!(effects.is_empty());
        assert!(store.snapshot_ipc().is_empty());
        let Some(StatusLineOutcome::AwaitingTrustedAccountConsumer(parsed)) = response.statusline
        else {
            panic!("missing typed statusline outcome")
        };
        assert_eq!(
            parsed.config_dir.as_deref(),
            Some("C:/accounts/plus+percent%")
        );
        assert_eq!(parsed.five_hour.unwrap().used_percentage, Some(12.5));
    }
}

#[test]
fn http_statusline_auth_method_size_and_junk_remain_fail_open() {
    let config = HookServerConfig {
        token: "token".into(),
        env: "production".into(),
    };
    let mut store = StatusStore::new(None);
    let (wrong, effects) =
        handle_hook_request(&mut store, &config, post("{}", "wrong", None), 1000);
    assert_eq!(wrong.status, 403);
    assert!(wrong.statusline.is_none() && effects.is_empty());
    let mut request = post("{}", "token", None);
    request.method = "GET".into();
    assert_eq!(
        handle_hook_request(&mut store, &config, request, 1000)
            .0
            .status,
        404
    );
    for payload in ["{}", "not-json", "null", "[]"] {
        let (response, effects) =
            handle_hook_request(&mut store, &config, post(payload, "token", None), 1000);
        assert_eq!(response.status, 204);
        assert_eq!(
            response.statusline,
            Some(StatusLineOutcome::NoUsableWindows)
        );
        assert!(effects.is_empty());
    }
    let mut request = post("{}", "token", None);
    request.body = vec![b'a'; mcode_agent_status::types::HOOK_REQUEST_MAX_BYTES + 1];
    let (response, effects) = handle_hook_request(&mut store, &config, request, 1000);
    assert_eq!(response.status, 204);
    assert!(response.statusline.is_none() && effects.is_empty());
}

#[test]
fn statusline_json_envelope_requires_string_payload_and_ignores_query_string() {
    let config = HookServerConfig {
        token: "token".into(),
        env: "production".into(),
    };
    let payload = json!({"rate_limits": {"five_hour": {"used_percentage": 5}}});
    for (value, accepted) in [
        (payload.clone(), false),
        (Value::String(payload.to_string()), true),
    ] {
        let mut request = post("{}", "token", None);
        request.path.push_str("?source=managed");
        request.content_type = Some("application/json".into());
        request.body = json!({"payload": value}).to_string().into_bytes();
        let response = handle_hook_request(&mut StatusStore::new(None), &config, request, 1000).0;
        assert_eq!(
            matches!(
                response.statusline,
                Some(StatusLineOutcome::AwaitingTrustedAccountConsumer(_))
            ),
            accepted
        );
    }
}
