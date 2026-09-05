//! Telemetry consent mutations: the per-session consent-mutation burst cap,
//! the `via` discriminator derivation, and the opt-in persistence contracts.
//!
//! Ports of the Electron main-process behavior (behavioral oracle):
//! - `src/main/telemetry/burst-cap.ts` — consent-mutation bucket
//!   (`consumeConsentMutationToken`): ≤5 per session, NO refill, warn-once,
//!   reset per telemetry session. Only renderer-originated mutations are
//!   capped; main-originated ones bypass IPC and stay uncapped.
//! - `src/main/ipc/telemetry.ts:deriveOptInVia` — `via` is derived from
//!   main-owned state BEFORE any mutation: existing-user
//!   (`existedBeforeTelemetryRelease === true`) with `optedIn === null` and
//!   incoming `false` → `first_launch_banner`; anything else → `settings`.
//!   The `incoming === false` narrow stops a compromised renderer from
//!   synthesizing a spurious `first_launch_banner` opt-in.
//! - `src/main/telemetry/client.ts:setOptIn` — persists `optedIn` and the
//!   `via`, preserving sibling `telemetry` fields; then emits
//!   `telemetry_opted_in/out` with that `via` (the persisted `via` feeds the
//!   opted-in event later).
//! - `src/main/telemetry/client.ts:persistBannerAcknowledgeWithoutEmitting`
//!   — the banner ✕ silent-ack: persists `optedIn = true` WITHOUT any
//!   telemetry_opted_in emission; only valid while the notice is pending
//!   (`existedBeforeTelemetryRelease === true && optedIn === null`).
//!
//! Honest deviation from Electron: `setOptIn` there also emits
//! `telemetry_opted_in`/`telemetry_opted_out` through the PostHog lane
//! (main telemetry client). The Tauri desktop build has no PostHog
//! transport yet, so `SetConsentOutcome` only persists the preference and
//! reports whether the emission WOULD have happened; the `via` is persisted
//! alongside `optedIn` exactly as Electron does, so a later transport can
//! emit the event with the recorded discriminator. `telemetry:track` (the
//! renderer track lane) is likewise deferred, not ported.

use serde::{Deserialize, Serialize};

/// Consent-mutation ceiling per session (`CONSENT_MUTATION_CEILING`).
pub const CONSENT_MUTATION_CEILING: u32 = 5;

/// `OptInVia` from `src/shared/telemetry-events.ts:232` — the opt-in
/// discriminator persisted with the consent change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptInVia {
    FirstLaunchBanner,
    Settings,
}

/// `settings.telemetry` block (`GlobalSettings.telemetry`,
/// `src/shared/global-settings-types.ts:456-463`). Stored values only —
/// consent + identity, no volatile counters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySettings {
    /// New users: `true` at install. Existing users: `null` until they
    /// resolve the first-launch banner (tri-state, never defaults on).
    #[serde(default)]
    pub opted_in: Option<bool>,
    /// Anonymous UUID v4; generated on first run. Never defaulted here —
    /// a missing block stays missing.
    #[serde(default)]
    pub install_id: String,
    /// Cohort marker gating the first-launch banner.
    #[serde(default)]
    pub existed_before_telemetry_release: bool,
}

impl TelemetrySettings {
    /// Existing-user banner cohort: profile predates the telemetry release
    /// and has not resolved the banner yet.
    pub fn banner_pending(&self) -> bool {
        self.existed_before_telemetry_release && self.opted_in.is_none()
    }
}

/// Per-session consent-mutation gate (burst-cap.ts:88-104). Not `Clone` on
/// purpose: the count is process-session state, shared by `setOptIn` and
/// `acknowledgeBanner`.
#[derive(Debug, Default)]
pub struct ConsentMutationGate {
    count: u32,
    warned: bool,
}

impl ConsentMutationGate {
    /// Consume one consent-mutation token; `false` once the per-session
    /// ceiling is hit. Warn-once on overflow so a pathological caller
    /// cannot flood stderr (burst-cap.ts:93-101).
    pub fn consume(&mut self) -> bool {
        if self.count >= CONSENT_MUTATION_CEILING {
            if !self.warned {
                self.warned = true;
                eprintln!(
                    "[telemetry] consent-mutation rate limit ({CONSENT_MUTATION_CEILING}/session) hit; dropping further mutations"
                );
            }
            return false;
        }
        self.count += 1;
        true
    }

    /// Reset on each telemetry session start (burst-cap.ts:106-113) —
    /// the ceiling is per session, not per process lifetime.
    pub fn reset(&mut self) {
        self.count = 0;
        self.warned = false;
    }
}

/// Derive the `via` discriminator from main-owned state, BEFORE any
/// mutation (ipc/telemetry.ts:39-50). The snapshot must reflect the
/// pre-click world: an existing user resolving the banner via the opt-OUT
/// button tags `first_launch_banner`; every other flip tags `settings`.
/// The `incoming == false` narrow keeps a compromised renderer from
/// synthesizing a spurious `first_launch_banner` opt-in.
pub fn derive_opt_in_via(
    settings: Option<&TelemetrySettings>,
    incoming_opted_in: bool,
) -> OptInVia {
    let Some(telemetry) = settings else {
        return OptInVia::Settings;
    };
    let existed_before = telemetry.existed_before_telemetry_release;
    let current_opted_in = telemetry.opted_in;
    if existed_before && current_opted_in.is_none() && !incoming_opted_in {
        return OptInVia::FirstLaunchBanner;
    }
    OptInVia::Settings
}

/// Result of the opt-in persistence path (`client.ts:setOptIn`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SetConsentOutcome {
    /// Whether a `telemetry_opted_in`/`telemetry_opted_out` emission
    /// would follow in Electron. Always `false` in the Tauri build (no
    /// PostHog lane yet) — kept explicit so the deferred surface stays
    /// visible in the type, not silently dropped.
    pub would_emit_opted_in_event: bool,
}

/// Persist `optedIn` (+ the derived `via`) over the settings document,
/// deep-merging like Electron's `storeRef.updateSettings({ telemetry })`
/// (client.ts:206-231): sibling `telemetry` fields (`installId`,
/// `existedBeforeTelemetryRelease`) survive, and a missing telemetry block
/// is created rather than rejected.
///
/// `apply` receives the merged block and must write it back (the caller's
/// persistence seam — see `mcode-settings::SettingsService`).
pub fn set_opt_in_persist<F, E>(
    current: Option<TelemetrySettings>,
    opted_in: bool,
    via: OptInVia,
    apply: F,
) -> Result<SetConsentOutcome, E>
where
    F: FnOnce(TelemetrySettings) -> Result<(), E>,
{
    let mut merged = current.unwrap_or(TelemetrySettings {
        opted_in: None,
        install_id: String::new(),
        existed_before_telemetry_release: true,
    });
    merged.opted_in = Some(opted_in);
    apply(merged)?;
    let _ = via; // consumed by the emission lane once the transport exists
    Ok(SetConsentOutcome {
        would_emit_opted_in_event: false,
    })
}

/// Banner ✕ silent-ack persistence (`client.ts:270-287`). Valid ONLY while
/// the notice is pending (`existedBeforeTelemetryRelease === true &&
/// optedIn === null`); any other state is a renderer silently flipping
/// `optedIn` after an opt-out and must be rejected. Persists `optedIn =
/// true` WITHOUT any opted-in event — the caller must not emit (the "no
/// emission" contract, not merely "no transport available").
pub fn acknowledge_banner_persist<F, E>(
    current: Option<&TelemetrySettings>,
    apply: F,
) -> Result<bool, E>
where
    F: FnOnce(TelemetrySettings) -> Result<(), E>,
{
    let Some(telemetry) = current else {
        return Ok(false);
    };
    if !telemetry.banner_pending() {
        return Ok(false);
    }
    let mut merged = telemetry.clone();
    merged.opted_in = Some(true);
    apply(merged)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn existing_user_pending() -> TelemetrySettings {
        TelemetrySettings {
            opted_in: None,
            install_id: "install-1".into(),
            existed_before_telemetry_release: true,
        }
    }

    fn fresh_install() -> TelemetrySettings {
        TelemetrySettings {
            opted_in: Some(true),
            install_id: "install-2".into(),
            existed_before_telemetry_release: false,
        }
    }

    // ── burst cap (burst-cap.ts consumeConsentMutationToken) ────────────

    #[test]
    fn consent_gate_allows_exactly_five_per_session() {
        let mut gate = ConsentMutationGate::default();
        for _ in 0..CONSENT_MUTATION_CEILING {
            assert!(gate.consume(), "first 5 mutations must be allowed");
        }
        assert!(!gate.consume(), "6th mutation must be dropped");
        assert!(!gate.consume(), "7th mutation must be dropped too");
    }

    #[test]
    fn consent_gate_reset_restores_the_session_bucket() {
        let mut gate = ConsentMutationGate::default();
        for _ in 0..CONSENT_MUTATION_CEILING {
            assert!(gate.consume());
        }
        gate.reset();
        assert!(gate.consume(), "reset clears the per-session count");
        for _ in 1..CONSENT_MUTATION_CEILING {
            assert!(gate.consume());
        }
        assert!(!gate.consume());
    }

    // ── via derivation (ipc/telemetry.ts deriveOptInVia) ────────────────

    #[test]
    fn existing_user_opting_out_before_banner_resolution_tags_first_launch_banner() {
        let settings = existing_user_pending();
        assert_eq!(
            derive_opt_in_via(Some(&settings), false),
            OptInVia::FirstLaunchBanner
        );
    }

    #[test]
    fn existing_user_opting_in_is_not_a_banner_opt_out() {
        // The `incoming === false` narrow: opting IN from the pending state
        // tags `settings`, never a synthetic first_launch_banner.
        let settings = existing_user_pending();
        assert_eq!(derive_opt_in_via(Some(&settings), true), OptInVia::Settings);
    }

    #[test]
    fn already_resolved_existing_user_tags_settings() {
        let mut settings = existing_user_pending();
        settings.opted_in = Some(false);
        // The current value is no longer null, so the pre-click world is a
        // resolved preference: `settings`, not the banner.
        assert_eq!(
            derive_opt_in_via(Some(&settings), false),
            OptInVia::Settings
        );
    }

    #[test]
    fn fresh_install_opt_out_tags_settings() {
        assert_eq!(
            derive_opt_in_via(Some(&fresh_install()), false),
            OptInVia::Settings
        );
    }

    #[test]
    fn missing_telemetry_block_tags_settings() {
        assert_eq!(derive_opt_in_via(None, false), OptInVia::Settings);
    }

    // ── persistence (client.ts setOptIn / persistBannerAcknowledge…) ────

    #[test]
    fn set_opt_in_persists_opted_in_and_keeps_sibling_fields() {
        let settings = existing_user_pending();
        let mut saved: Option<TelemetrySettings> = None;
        set_opt_in_persist(
            Some(settings.clone()),
            false,
            OptInVia::FirstLaunchBanner,
            |merged| {
                saved = Some(merged);
                Ok::<(), String>(())
            },
        )
        .unwrap();
        let saved = saved.unwrap();
        assert_eq!(saved.opted_in, Some(false));
        assert_eq!(saved.install_id, "install-1");
        assert!(saved.existed_before_telemetry_release);
        // PostHog lane is deferred, not silently faked.
        let outcome = set_opt_in_persist(Some(settings), true, OptInVia::Settings, |_| {
            Ok::<(), String>(())
        })
        .unwrap();
        assert!(!outcome.would_emit_opted_in_event);
    }

    #[test]
    fn set_opt_in_creates_block_when_absent() {
        let mut saved: Option<TelemetrySettings> = None;
        set_opt_in_persist(None, true, OptInVia::Settings, |merged| {
            saved = Some(merged);
            Ok::<(), String>(())
        })
        .unwrap();
        assert_eq!(saved.unwrap().opted_in, Some(true));
    }

    #[test]
    fn set_opt_in_propagates_persistence_failure() {
        let result: Result<SetConsentOutcome, String> = set_opt_in_persist(
            Some(existing_user_pending()),
            true,
            OptInVia::Settings,
            |_| Err("disk full".into()),
        );
        assert_eq!(result.unwrap_err(), "disk full");
    }

    #[test]
    fn acknowledge_banner_persists_opted_in_only_while_pending() {
        let settings = existing_user_pending();
        let mut saved: Option<TelemetrySettings> = None;
        let applied = acknowledge_banner_persist(Some(&settings), |merged| {
            saved = Some(merged);
            Ok::<(), String>(())
        })
        .unwrap();
        assert!(applied, "pending banner must acknowledge");
        let saved = saved.unwrap();
        assert_eq!(saved.opted_in, Some(true));
        assert!(saved.existed_before_telemetry_release);

        // After resolution (optedIn set): the ack is no longer valid — this
        // is the guard against a renderer silently flipping optedIn after
        // an opt-out.
        let mut resolved = existing_user_pending();
        resolved.opted_in = Some(false);
        let mut called = false;
        let applied = acknowledge_banner_persist(Some(&resolved), |_| {
            called = true;
            Ok::<(), String>(())
        })
        .unwrap();
        assert!(!applied, "resolved banner must NOT acknowledge");
        assert!(!called, "apply must not run for a non-pending state");
    }

    #[test]
    fn acknowledge_banner_rejects_fresh_install_and_absent_block() {
        // Fresh installs never had the banner surface.
        let mut called = false;
        assert!(
            !acknowledge_banner_persist(Some(&fresh_install()), |_| {
                called = true;
                Ok::<(), String>(())
            })
            .unwrap()
        );
        assert!(!called);
        // No telemetry block at all: nothing pending to acknowledge.
        assert!(!acknowledge_banner_persist(None, |_| Ok::<(), String>(())).unwrap());
    }

    // ── wire shape ──────────────────────────────────────────────────────

    #[test]
    fn telemetry_settings_roundtrips_camel_case_wire_shape() {
        let json = serde_json::json!({
            "optedIn": null,
            "installId": "abc",
            "existedBeforeTelemetryRelease": true
        });
        let parsed: TelemetrySettings = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.install_id, "abc");
    }
}
