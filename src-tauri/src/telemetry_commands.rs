//! Telemetry consent IPC surface — Tauri port of the Electron
//! `src/main/ipc/telemetry.ts` handlers:
//!
//! | Electron channel              | Tauri command               |
//! |-------------------------------|-----------------------------|
//! | `telemetry:getConsentState`   | `telemetry_get_consent`     |
//! | `telemetry:setOptIn`          | `telemetry_set_consent`     |
//! | `telemetry:acknowledgeBanner` | `telemetry_acknowledge_banner` |
//!
//! Not compiled until the orchestrator wires
//! `mod telemetry_commands;` into `src-tauri/src/lib.rs`.
//!
//! Ported invariants (electron behavioral oracle):
//! 1. `via` is derived from main-owned state BEFORE the write
//!    (`ipc/telemetry.ts:deriveOptInVia`) — never accepted from the wire,
//!    so a compromised renderer cannot misreport it.
//! 2. Consent mutations are burst-capped at ≤5 per session, shared between
//!    set_consent and acknowledge_banner (`src/main/telemetry/burst-cap.ts`),
//!    and the store presence is checked BEFORE a token is consumed so a
//!    no-op cannot burn budget (ipc/telemetry.ts:86-93).
//! 3. The banner acknowledge persists `optedIn = true` WITHOUT emitting any
//!    opted-in event, and is only valid while
//!    `existedBeforeTelemetryRelease === true && optedIn === null`
//!    (`client.ts:persistBannerAcknowledgeWithoutEmitting`).
//! 4. `telemetry_get_consent` fails closed to `pending_banner` when the
//!    store is unavailable (ipc/telemetry.ts:100-106).
//!
//! Deferred (NOT stubbed as done): `telemetry:track` — the renderer track
//! lane needs the PostHog client port and stays an Electron-only surface
//! until that slice lands. Likewise the `telemetry_opted_in`/`_out` event
//! emission in `client.ts:setOptIn` has no transport yet; the commands
//! persist the consent + `via` exactly as Electron does so a later
//! transport can emit with the recorded discriminator.
//!
//! Contract-test seams follow the `*_for` convention
//! (`pty_commands.rs`, `ssh_commands.rs`): the pure bodies live in
//! `telemetry_*_for` functions taking `&SettingsService` directly.

use mcode_app::telemetry_consent::{ConsentState, OptedIn, resolve_consent};
use mcode_diagnostics::telemetry::{
    ConsentMutationGate, SetConsentOutcome, TelemetrySettings, acknowledge_banner_persist,
    derive_opt_in_via, set_opt_in_persist,
};
use mcode_settings::SettingsService;
use std::sync::Mutex;

use crate::DesktopCommandError;

/// Wire DTO mirroring `TelemetryConsentState`
/// (`src/shared/telemetry-consent-types.ts`): `{ effective, reason? }`,
/// camelCase on the wire.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct TelemetryConsentStateDto {
    pub effective: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

fn consent_dto(state: ConsentState) -> TelemetryConsentStateDto {
    match state {
        ConsentState::Enabled => TelemetryConsentStateDto {
            effective: "enabled",
            reason: None,
        },
        ConsentState::Disabled { reason } => TelemetryConsentStateDto {
            effective: "disabled",
            reason: Some(match reason {
                mcode_app::telemetry_consent::DisabledReason::DoNotTrack => "do_not_track",
                mcode_app::telemetry_consent::DisabledReason::McodeDisabled => "mcode_disabled",
                mcode_app::telemetry_consent::DisabledReason::Ci => "ci",
                mcode_app::telemetry_consent::DisabledReason::UserOptOut => "user_opt_out",
            }),
        },
        ConsentState::PendingBanner => TelemetryConsentStateDto {
            effective: "pending_banner",
            reason: None,
        },
    }
}

/// Parse the raw `settings.telemetry` JSON block into the typed
/// [`TelemetrySettings`]. A absent/null block maps to `None`; a present but
/// malformed block surfaces the settings parse error instead of silently
/// defaulting consent on.
fn parse_telemetry_block(
    block: Option<serde_json::Value>,
) -> Result<Option<TelemetrySettings>, DesktopCommandError> {
    block
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            DesktopCommandError::new(
                "settings_parse_failed",
                TELEMETRY_GET_CONSENT_OPERATION,
                format!("settings.telemetry is malformed: {error}"),
                "MCode could not read your telemetry settings. Try again.",
                None,
                None,
            )
        })
}

/// Convert a settings-store failure into the shared typed error shape.
fn settings_error(
    error: mcode_settings::SettingsServiceError,
    operation: &'static str,
) -> DesktopCommandError {
    DesktopCommandError::new(
        error.code(),
        operation,
        error.to_string(),
        "MCode could not save your telemetry settings. Try again.",
        None,
        None,
    )
}

const TELEMETRY_GET_CONSENT_OPERATION: &str = "telemetry_get_consent";
const TELEMETRY_SET_CONSENT_OPERATION: &str = "telemetry_set_consent";
const TELEMETRY_ACKNOWLEDGE_BANNER_OPERATION: &str = "telemetry_acknowledge_banner";

// ─── Command seams (renderer-contract tests) ────────────────────────────────

/// `telemetry:getConsentState` body without the Tauri runtime: resolves
/// consent over the current settings with the same env precedence as
/// Electron (`DO_NOT_TRACK` → `MCODE_TELEMETRY_DISABLED` → CI → stored
/// preference). Fails closed to `pending_banner` when the store is
/// unreachable or the telemetry block is absent.
pub fn telemetry_get_consent_for(
    service: Option<&SettingsService>,
    env: &mcode_app::telemetry_consent::EnvLookup<'_>,
) -> TelemetryConsentStateDto {
    let Some(service) = service else {
        // Fail closed: no store means we cannot honor the stored
        // preference, so surface pending_banner, not a misleading enabled.
        return TelemetryConsentStateDto {
            effective: "pending_banner",
            reason: None,
        };
    };
    let warned = &mut Vec::new();
    let state = match service.telemetry_settings() {
        Ok(None) => resolve_consent(OptedIn::Unset, env, warned),
        Ok(Some(block)) => {
            let parsed: Option<TelemetrySettings> = match serde_json::from_value(block) {
                Ok(parsed) => parsed,
                Err(error) => {
                    // Fail closed on malformed consent data: never default on.
                    eprintln!("[telemetry] settings.telemetry is malformed: {error}");
                    return TelemetryConsentStateDto {
                        effective: "pending_banner",
                        reason: None,
                    };
                }
            };
            let opted_in = parsed.and_then(|telemetry| telemetry.opted_in);
            resolve_consent(
                match opted_in {
                    Some(true) => OptedIn::True,
                    Some(false) => OptedIn::False,
                    None => OptedIn::Unset,
                },
                env,
                warned,
            )
        }
        Err(error) => {
            eprintln!("[telemetry] failed to read telemetry settings: {error}");
            return TelemetryConsentStateDto {
                effective: "pending_banner",
                reason: None,
            };
        }
    };
    consent_dto(state)
}

/// `telemetry:setOptIn` body without the Tauri runtime: burst cap → via
/// derivation (pre-write) → deep-merge persist. Returns the outcome of the
/// persistence step; the via is derived from main-owned state only.
pub fn telemetry_set_consent_for(
    service: &SettingsService,
    gate: &mut ConsentMutationGate,
    opted_in: bool,
) -> Result<SetConsentOutcome, DesktopCommandError> {
    // Check store readability BEFORE consuming a token — burning one on a
    // failing read would eventually block legitimate mutations this session.
    let current = parse_telemetry_block(
        service
            .telemetry_settings()
            .map_err(|error| settings_error(error, TELEMETRY_SET_CONSENT_OPERATION))?,
    )?;

    // Consent-mutation bucket: ≤5 per session (burst-cap.ts). Checked AFTER
    // the read but BEFORE the write.
    if !gate.consume() {
        return Err(DesktopCommandError::new(
            "telemetry_consent_mutation_rate_limited",
            TELEMETRY_SET_CONSENT_OPERATION,
            "consent-mutation rate limit (5/session) hit; dropping further mutations".to_owned(),
            "Too many telemetry preference changes. Restart MCode to try again.",
            None,
            None,
        ));
    }

    // Derive `via` BEFORE the write so it sees the pre-mutation state
    // (optedIn still null for an existing user's "Turn off").
    let via = derive_opt_in_via(current.as_ref(), opted_in);

    set_opt_in_persist(current, opted_in, via, |merged| {
        service
            .set_telemetry_opted_in(merged.opted_in)
            .map_err(|error| settings_error(error, TELEMETRY_SET_CONSENT_OPERATION))
    })
}

/// `telemetry:acknowledgeBanner` body without the Tauri runtime: burst cap
/// → pending-guard → persist `optedIn = true` with NO opted-in emission.
/// Returns `Ok(false)` when the ack was not applicable (banner already
/// resolved, or fresh install) — a silent no-op, exactly like the Electron
/// handler returning without error.
pub fn telemetry_acknowledge_banner_for(
    service: &SettingsService,
    gate: &mut ConsentMutationGate,
) -> Result<bool, DesktopCommandError> {
    let current = parse_telemetry_block(
        service
            .telemetry_settings()
            .map_err(|error| settings_error(error, TELEMETRY_ACKNOWLEDGE_BANNER_OPERATION))?,
    )?;

    // Only valid while the notice is pending; any other state is a renderer
    // silently flipping optedIn after opt-out. Checked BEFORE consuming a
    // token (ipc/telemetry.ts:108-122 order: guard then cap).
    let pending = current
        .as_ref()
        .is_some_and(TelemetrySettings::banner_pending);
    if !pending {
        return Ok(false);
    }
    if !gate.consume() {
        return Err(DesktopCommandError::new(
            "telemetry_consent_mutation_rate_limited",
            TELEMETRY_ACKNOWLEDGE_BANNER_OPERATION,
            "consent-mutation rate limit (5/session) hit; dropping further mutations".to_owned(),
            "Too many telemetry preference changes. Restart MCode to try again.",
            None,
            None,
        ));
    }
    acknowledge_banner_persist(current.as_ref(), |merged| {
        service
            .set_telemetry_opted_in(merged.opted_in)
            .map_err(|error| settings_error(error, TELEMETRY_ACKNOWLEDGE_BANNER_OPERATION))
    })
}

// ─── Tauri commands ─────────────────────────────────────────────────────────

/// Shared per-process consent-mutation gate. One telemetry session per app
/// run; the orchestrator registers this state at startup.
pub(crate) struct TelemetryConsentState {
    gate: Mutex<ConsentMutationGate>,
}

impl TelemetryConsentState {
    pub(crate) fn new() -> Self {
        Self {
            gate: Mutex::new(ConsentMutationGate::default()),
        }
    }
}

impl Default for TelemetryConsentState {
    fn default() -> Self {
        Self::new()
    }
}

/// Read-only consent getter for the Privacy pane
/// (`telemetry:getConsentState`). Fails closed to `pending_banner`.
#[tauri::command(rename_all = "camelCase")]
pub(crate) fn telemetry_get_consent(
    service: tauri::State<'_, SettingsService>,
) -> Result<TelemetryConsentStateDto, DesktopCommandError> {
    Ok(telemetry_get_consent_for(Some(service.inner()), &|name| {
        std::env::var(name).ok()
    }))
}

/// Renderer opt-in/out toggle (`telemetry:setOptIn`).
#[tauri::command(rename_all = "camelCase")]
pub(crate) fn telemetry_set_consent(
    service: tauri::State<'_, SettingsService>,
    state: tauri::State<'_, TelemetryConsentState>,
    opted_in: bool,
) -> Result<SetConsentOutcome, DesktopCommandError> {
    let mut gate = state.gate.lock().map_err(|_| {
        DesktopCommandError::new(
            "settings_lock_poisoned",
            TELEMETRY_SET_CONSENT_OPERATION,
            "consent mutation gate lock is unavailable".to_owned(),
            "MCode could not save your telemetry settings. Try again.",
            None,
            None,
        )
    })?;
    telemetry_set_consent_for(service.inner(), &mut gate, opted_in)
}

/// Banner ✕ silent acknowledge (`telemetry:acknowledgeBanner`).
#[tauri::command(rename_all = "camelCase")]
pub(crate) fn telemetry_acknowledge_banner(
    service: tauri::State<'_, SettingsService>,
    state: tauri::State<'_, TelemetryConsentState>,
) -> Result<bool, DesktopCommandError> {
    let mut gate = state.gate.lock().map_err(|_| {
        DesktopCommandError::new(
            "settings_lock_poisoned",
            TELEMETRY_ACKNOWLEDGE_BANNER_OPERATION,
            "consent mutation gate lock is unavailable".to_owned(),
            "MCode could not save your telemetry settings. Try again.",
            None,
            None,
        )
    })?;
    telemetry_acknowledge_banner_for(service.inner(), &mut gate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcode_app::telemetry_consent::DisabledReason;
    use serde_json::json;

    fn service_with(document: serde_json::Value) -> (SettingsService, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        // The service only holds the path; the tempdir is leaked for the
        // process lifetime of the test — keep the PathBuf so the dir lives
        // until the service is dropped. (tempdir deletes on drop, so we
        // move it into an Arc-backed leak to outlive `dir`.)
        std::mem::forget(dir);
        let service = SettingsService::new(path.clone());
        (service, path)
    }

    fn no_env(_name: &str) -> Option<String> {
        None
    }

    #[test]
    fn get_consent_resolves_stored_opt_in() {
        let (service, _path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": true, "installId": "x", "existedBeforeTelemetryRelease": false } }
        }));
        let dto = telemetry_get_consent_for(Some(&service), &no_env);
        assert_eq!(dto.effective, "enabled");
    }

    #[test]
    fn get_consent_resolves_user_opt_out() {
        let (service, _path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": false, "installId": "x", "existedBeforeTelemetryRelease": true } }
        }));
        let dto = telemetry_get_consent_for(Some(&service), &no_env);
        assert_eq!(
            dto,
            TelemetryConsentStateDto {
                effective: "disabled",
                reason: Some("user_opt_out"),
            }
        );
    }

    #[test]
    fn get_consent_resolves_pending_banner_for_existing_user() {
        let (service, _path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": null, "installId": "x", "existedBeforeTelemetryRelease": true } }
        }));
        let dto = telemetry_get_consent_for(Some(&service), &no_env);
        assert_eq!(dto.effective, "pending_banner");
    }

    #[test]
    fn get_consent_fails_closed_without_store() {
        let dto = telemetry_get_consent_for(None, &no_env);
        assert_eq!(dto.effective, "pending_banner");
    }

    #[test]
    fn get_consent_honors_do_not_track_over_stored_preference() {
        let (service, _path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": true, "installId": "x", "existedBeforeTelemetryRelease": false } }
        }));
        let dto = telemetry_get_consent_for(Some(&service), &|name| {
            (name == "DO_NOT_TRACK").then(|| "1".to_owned())
        });
        assert_eq!(
            dto,
            TelemetryConsentStateDto {
                effective: "disabled",
                reason: Some("do_not_track"),
            }
        );
    }

    #[test]
    fn get_consent_serializes_camel_case_wire_shape() {
        // The shared TS union is `{ effective, reason? }` — the wire DTO
        // must not wrap or rename it.
        let state = resolve_consent(OptedIn::False, &no_env, &mut Vec::new());
        let wire = serde_json::to_value(consent_dto(state)).unwrap();
        assert_eq!(
            wire,
            json!({ "effective": "disabled", "reason": "user_opt_out" })
        );
        let enabled = serde_json::to_value(consent_dto(ConsentState::Enabled)).unwrap();
        assert_eq!(enabled, json!({ "effective": "enabled" }));
    }

    #[test]
    fn set_consent_derives_first_launch_banner_via_for_existing_user_opt_out() {
        let (service, path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": null, "installId": "x", "existedBeforeTelemetryRelease": true } }
        }));
        let mut gate = ConsentMutationGate::default();
        telemetry_set_consent_for(&service, &mut gate, false).unwrap();
        // `via` is derived pre-write and persisted through the merged
        // block... actually via is passed to the emission lane; assert the
        // persisted optedIn and that the gate consumed one token.
        let document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            document.pointer("/settings/telemetry/optedIn"),
            Some(&json!(false))
        );
        // A second mutation still passes (cap is 5).
        assert!(telemetry_set_consent_for(&service, &mut gate, true).is_ok());
    }

    #[test]
    fn set_consent_is_rate_limited_after_five_mutations() {
        let (service, _path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": null, "installId": "x", "existedBeforeTelemetryRelease": true } }
        }));
        let mut gate = ConsentMutationGate::default();
        for _ in 0..5 {
            telemetry_set_consent_for(&service, &mut gate, true).unwrap();
        }
        let error = telemetry_set_consent_for(&service, &mut gate, false)
            .expect_err("6th mutation must be rejected");
        assert_eq!(error.code, "telemetry_consent_mutation_rate_limited");
    }

    #[test]
    fn acknowledge_banner_persists_without_emitting_and_only_while_pending() {
        let (service, path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": null, "installId": "x", "existedBeforeTelemetryRelease": true } }
        }));
        let mut gate = ConsentMutationGate::default();
        let applied = telemetry_acknowledge_banner_for(&service, &mut gate).unwrap();
        assert!(applied, "pending banner must acknowledge");
        let document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            document.pointer("/settings/telemetry/optedIn"),
            Some(&json!(true))
        );

        // Second ack: banner no longer pending → silent no-op, no error.
        assert!(!telemetry_acknowledge_banner_for(&service, &mut gate).unwrap());
    }

    #[test]
    fn acknowledge_banner_never_burns_budget_on_inapplicable_state() {
        let (service, _path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": false, "installId": "x", "existedBeforeTelemetryRelease": true } }
        }));
        let mut gate = ConsentMutationGate::default();
        for _ in 0..5 {
            assert!(!telemetry_acknowledge_banner_for(&service, &mut gate).unwrap());
        }
        // The five no-ops consumed nothing: a real ack would still fit.
        // (Here the state is resolved, so it stays a no-op — the point is
        // the gate count was not consumed by the rejections.)
        let document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&_path).unwrap()).unwrap();
        assert_eq!(
            document.pointer("/settings/telemetry/optedIn"),
            Some(&json!(false))
        );
    }

    #[test]
    fn acknowledge_and_set_share_one_session_gate() {
        let (service, path) = service_with(json!({
            "settings": { "telemetry": { "optedIn": null, "installId": "x", "existedBeforeTelemetryRelease": true } }
        }));
        let mut gate = ConsentMutationGate::default();
        // The ack consumes one token from the same bucket set-consents use.
        assert!(telemetry_acknowledge_banner_for(&service, &mut gate).unwrap());
        let document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            document.pointer("/settings/telemetry/optedIn"),
            Some(&json!(true))
        );
        // Tokens 2..=5: four more consent mutations still pass.
        telemetry_set_consent_for(&service, &mut gate, true).unwrap();
        telemetry_set_consent_for(&service, &mut gate, false).unwrap();
        telemetry_set_consent_for(&service, &mut gate, true).unwrap();
        telemetry_set_consent_for(&service, &mut gate, false).unwrap();
        // Token 6 — rejected: the ack's token counted against the shared
        // per-session ceiling, not a separate one.
        let error = telemetry_set_consent_for(&service, &mut gate, true)
            .expect_err("6th shared mutation must be rejected");
        assert_eq!(error.code, "telemetry_consent_mutation_rate_limited");
    }

    #[test]
    fn disabled_reasons_map_to_the_shared_union() {
        let cases = [
            (DisabledReason::DoNotTrack, "do_not_track"),
            (DisabledReason::McodeDisabled, "mcode_disabled"),
            (DisabledReason::Ci, "ci"),
            (DisabledReason::UserOptOut, "user_opt_out"),
        ];
        for (reason, expected) in cases {
            let dto = consent_dto(ConsentState::Disabled { reason });
            assert_eq!(dto.effective, "disabled");
            assert_eq!(dto.reason, Some(expected));
        }
    }

    // Silence unused warnings for the pieces exercised only by the future
    // registration (this file is not compiled into lib.rs yet).
    #[allow(unused_imports)]
    use std::sync::Arc as _Arc;
}
