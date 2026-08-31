//! Consent resolver — byte-for-byte port of
//! `src/main/telemetry/consent.ts` (the pure consent resolver every
//! transmit path must go through). Env-var/CI paths are non-persistent:
//! they set effective consent at runtime only and never mutate
//! `GlobalSettings.telemetry.optedIn`.
//!
//! Precedence (consent.ts:74-105):
//! 1. `DO_NOT_TRACK` truthy → disabled (community kill switch)
//! 2. `MCODE_TELEMETRY_DISABLED` truthy → disabled (product kill switch)
//! 3. Any CI env var present (not just truthy) → disabled
//! 4. `optedIn === true` → enabled; `false` → disabled(user_opt_out);
//!    `null`/missing → pending_banner (fail closed — no transmit).

use serde::{Deserialize, Serialize};

/// consent.ts:26-35 — CI detection list. Any presence counts.
pub const CI_ENV_VARS: [&str; 8] = [
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "CIRCLECI",
    "TRAVIS",
    "BUILDKITE",
    "JENKINS_URL",
    "TEAMCITY_VERSION",
];

/// consent.ts:13-23 — discriminated union so UI can render the right
/// "disabled because X" helper text and distinguish the first-launch banner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effective", rename_all = "snake_case")]
pub enum ConsentState {
    Enabled,
    Disabled { reason: DisabledReason },
    PendingBanner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisabledReason {
    DoNotTrack,
    McodeDisabled,
    Ci,
    UserOptOut,
}

/// Stored telemetry preference: tri-state — `None` is the existing-user
/// cohort awaiting the banner (never defaults on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptedIn {
    True,
    False,
    Unset,
}

/// Consent lookup seam: the resolver never reads `std::env` directly, so
/// tests and hosts can inject values.
pub type EnvLookup<'a> = dyn Fn(&str) -> Option<String> + 'a;

fn is_env_var_truthy(name: &str, env: &EnvLookup<'_>, warned: &mut Vec<String>) -> bool {
    let Some(value) = env(name) else {
        return false;
    };
    if value.is_empty() {
        return false;
    }
    let normalized = value.trim().to_lowercase();
    if normalized == "1" || normalized == "true" {
        return true;
    }
    // consent.ts:39-47 — warn once per name; `0`/`yes`/`on`/`FALSE`
    // silently no-oping is exactly the bug class to surface.
    if !warned.iter().any(|entry| entry == name) {
        warned.push(name.to_string());
        eprintln!(
            "[telemetry] {name}={value:?} is not a recognized truthy value (expected \"1\" or \"true\"); treating as unset."
        );
    }
    false
}

/// consent.ts:73-105 — `resolveConsent`. `warned` carries the one-shot
/// misconfiguration warning dedupe across calls (process-lifetime in TS).
pub fn resolve_consent(
    opted_in: OptedIn,
    env: &EnvLookup<'_>,
    warned: &mut Vec<String>,
) -> ConsentState {
    if is_env_var_truthy("DO_NOT_TRACK", env, warned) {
        return ConsentState::Disabled {
            reason: DisabledReason::DoNotTrack,
        };
    }
    if is_env_var_truthy("MCODE_TELEMETRY_DISABLED", env, warned) {
        return ConsentState::Disabled {
            reason: DisabledReason::McodeDisabled,
        };
    }
    // Presence counts, not truthiness: legacy CI systems set CI to "" or a
    // build id; none of that is human intent to opt in.
    if CI_ENV_VARS
        .iter()
        .any(|name| env(name).is_some_and(|v| !v.is_empty()))
    {
        return ConsentState::Disabled {
            reason: DisabledReason::Ci,
        };
    }
    match opted_in {
        OptedIn::True => ConsentState::Enabled,
        OptedIn::False => ConsentState::Disabled {
            reason: DisabledReason::UserOptOut,
        },
        OptedIn::Unset => ConsentState::PendingBanner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(map: HashMap<&str, &str>) -> impl Fn(&str) -> Option<String> {
        move |key: &str| map.get(key).map(|v| (*v).to_string())
    }

    fn no_env(_key: &str) -> Option<String> {
        None
    }

    #[test]
    fn do_not_track_wins_over_everything() {
        let mut warned = Vec::new();
        let env = env_of(HashMap::from([
            ("DO_NOT_TRACK", "1"),
            ("MCODE_TELEMETRY_DISABLED", "1"),
            ("CI", "true"),
        ]));
        assert_eq!(
            resolve_consent(OptedIn::True, &env, &mut warned),
            ConsentState::Disabled {
                reason: DisabledReason::DoNotTrack
            }
        );
    }

    #[test]
    fn product_kill_switch_second() {
        let mut warned = Vec::new();
        let env = env_of(HashMap::from([
            ("MCODE_TELEMETRY_DISABLED", "true"),
            ("CI", "true"),
        ]));
        assert_eq!(
            resolve_consent(OptedIn::True, &env, &mut warned),
            ConsentState::Disabled {
                reason: DisabledReason::McodeDisabled
            }
        );
    }

    #[test]
    fn kill_switch_rejects_unrecognized_values_with_warning() {
        let mut warned = Vec::new();
        let env = env_of(HashMap::from([("DO_NOT_TRACK", "yes")]));
        assert_eq!(
            resolve_consent(OptedIn::True, &env, &mut warned),
            ConsentState::Enabled
        );
        assert_eq!(warned, vec!["DO_NOT_TRACK"]);
        // Second call does not warn again (one-shot dedupe).
        let env = env_of(HashMap::from([("DO_NOT_TRACK", "0")]));
        let mut second = warned.clone();
        let _ = resolve_consent(OptedIn::True, &env, &mut second);
        assert_eq!(second.len(), 1, "warning deduped per env var name");
    }

    #[test]
    fn ci_presence_not_truthiness_disables() {
        let mut warned = Vec::new();
        let env = env_of(HashMap::from([("CIRCLECI", "build-123")]));
        assert_eq!(
            resolve_consent(OptedIn::True, &env, &mut warned),
            ConsentState::Disabled {
                reason: DisabledReason::Ci
            }
        );
        // Empty string does NOT count (explicitly documented exception).
        let env = env_of(HashMap::from([("CI", "")]));
        assert_eq!(
            resolve_consent(OptedIn::True, &env, &mut warned),
            ConsentState::Enabled
        );
        let _ = no_env("CI");
    }

    #[test]
    fn tri_state_opt_in_never_defaults_on() {
        let mut warned = Vec::new();
        assert_eq!(
            resolve_consent(OptedIn::True, &no_env, &mut warned),
            ConsentState::Enabled
        );
        assert_eq!(
            resolve_consent(OptedIn::False, &no_env, &mut warned),
            ConsentState::Disabled {
                reason: DisabledReason::UserOptOut
            }
        );
        // Existing-user cohort: banner, no transmit.
        assert_eq!(
            resolve_consent(OptedIn::Unset, &no_env, &mut warned),
            ConsentState::PendingBanner
        );
    }
}
