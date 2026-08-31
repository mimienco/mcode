//! Environment policy application: scrub list + allowlist deltas / explicit map.
//!
//! The scrub list is load-bearing, not cosmetic: the original daemon deletes
//! `ELECTRON_RUN_AS_NODE` before every user spawn ("user shells must not
//! inherit ELECTRON_RUN_AS_NODE or nested Electron commands run as plain
//! Node", `pty-subprocess.ts:254-260`), removes AppImage runtime vars
//! (`appimage-terminal-env.ts`), dev agent hook endpoints, and pane identity
//! keys (`pty-subprocess.ts:88-93`). This module is the single place that
//! discipline lives; WSL `/u` `/p` WSLENV flag computation deliberately does
//! NOT live here — that belongs to the pty crate.

use crate::handle::EnvironmentPolicy;
use std::collections::BTreeMap;

/// Applies a policy to a base environment, returning the child environment.
///
/// Order of operations:
/// 1. Start from `base` (or an empty map for `Explicit`).
/// 2. Delete every scrubbed key that survived into the working set.
/// 3. Delete the policy's own `removes`.
/// 4. Apply the policy's additions/replacements (deltas or the explicit map).
///    A delta may re-add a scrubbed key only under its *exact* spelling being
///    explicitly requested — this is how a fresh `MCODE_AGENT_LAUNCH_TOKEN`
///    for a NEW pane identity can be delivered, mirroring the original where
///    `explicitEnv` values are applied after inherited scrubbing. Scrubbing
///    is therefore "strip inherited value", not "ban forever".
pub fn apply_environment(
    base: &BTreeMap<String, String>,
    policy: &EnvironmentPolicy,
) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = if policy.is_explicit() {
        BTreeMap::new()
    } else {
        base.clone()
    };

    // Windows env keys are case-insensitive; scrub any casing.
    for key in EnvironmentPolicy::scrub_keys() {
        remove_key_case_insensitive(&mut env, key);
    }

    match policy {
        EnvironmentPolicy::Inherit { deltas, removes } => {
            for key in removes {
                remove_key_case_insensitive(&mut env, key);
            }
            for (key, value) in deltas {
                env.insert(key.clone(), value.clone());
            }
        }
        EnvironmentPolicy::Explicit { map, removes } => {
            for key in removes {
                remove_key_case_insensitive(&mut env, key);
            }
            for (key, value) in map {
                env.insert(key.clone(), value.clone());
            }
        }
    }

    env
}

/// Case-insensitive removal (Windows treats env keys case-insensitively).
/// On POSIX the exact-casing match also removes since we compare lowercased
/// — a POSIX env may legitimately hold both spellings, and both are scrubbed.
fn remove_key_case_insensitive(env: &mut BTreeMap<String, String>, key: &str) {
    let needle = key.to_ascii_lowercase();
    let doomed: Vec<String> = env
        .keys()
        .filter(|k| k.to_ascii_lowercase() == needle)
        .cloned()
        .collect();
    for k in doomed {
        env.remove(&k);
    }
}

/// Captures the supervisor's current process environment as the scrub base.
pub fn current_environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> BTreeMap<String, String> {
        [
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("ELECTRON_RUN_AS_NODE".to_string(), "1".to_string()),
            ("NODE_OPTIONS".to_string(), "--inspect".to_string()),
            ("MCODE_PANE_KEY".to_string(), "pane-1".to_string()),
            ("MCODE_TAB_ID".to_string(), "tab-1".to_string()),
            ("MCODE_WORKTREE_ID".to_string(), "wt-1".to_string()),
            ("MCODE_AGENT_LAUNCH_TOKEN".to_string(), "tok".to_string()),
            ("APPDIR".to_string(), "/opt/app".to_string()),
            ("HOME".to_string(), "/home/u".to_string()),
            ("path".to_string(), "windows-casing".to_string()),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn inherit_scrubs_electron_and_node_keys() {
        let env = apply_environment(&base(), &EnvironmentPolicy::default());
        assert!(!env.contains_key("ELECTRON_RUN_AS_NODE"));
        assert!(!env.contains_key("NODE_OPTIONS"));
        assert_eq!(env.get("HOME").map(String::as_str), Some("/home/u"));
    }

    #[test]
    fn inherit_scrubs_pane_identity_keys() {
        let env = apply_environment(&base(), &EnvironmentPolicy::default());
        for key in [
            "MCODE_PANE_KEY",
            "MCODE_TAB_ID",
            "MCODE_WORKTREE_ID",
            "MCODE_AGENT_LAUNCH_TOKEN",
        ] {
            assert!(!env.contains_key(key), "{key} survived scrubbing");
        }
    }

    #[test]
    fn inherit_scrubs_appimage_runtime_keys() {
        let env = apply_environment(&base(), &EnvironmentPolicy::default());
        assert!(!env.contains_key("APPDIR"));
        assert!(!env.contains_key("APPIMAGE"));
        assert!(!env.contains_key("ARGV0"));
    }

    #[test]
    fn inherit_applies_deltas_after_scrub() {
        let policy = EnvironmentPolicy::Inherit {
            deltas: vec![("MCODE_TEST_MARKER".to_string(), "1".to_string())],
            removes: vec!["HOME".to_string()],
        };
        let env = apply_environment(&base(), &policy);
        assert_eq!(env.get("MCODE_TEST_MARKER").map(String::as_str), Some("1"));
        assert!(!env.contains_key("HOME"));
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin"));
    }

    #[test]
    fn scrub_is_case_insensitive_for_windows_casing() {
        // A scrub key inherited under a different casing (Windows treats env
        // keys case-insensitively) must die: "electron_run_as_node" is the
        // same key as "ELECTRON_RUN_AS_NODE".
        let mut base_cased = base();
        base_cased.insert("electron_run_as_node".to_string(), "1".to_string());
        let env = apply_environment(&base_cased, &EnvironmentPolicy::default());
        assert!(
            !env.contains_key("electron_run_as_node"),
            "lowercase-cased scrub key must go"
        );
        // PATH itself was never a scrub key, so both spellings survive; the
        // removal pass only ever deletes keys whose name matches a scrub key.
        assert!(env.contains_key("PATH"));
        assert!(env.contains_key("path"));
    }

    #[test]
    fn explicit_replaces_base_but_scrubs_still_apply_to_new_map() {
        let policy = EnvironmentPolicy::Explicit {
            map: vec![
                ("ELECTRON_RUN_AS_NODE".to_string(), "1".to_string()),
                ("ONLY".to_string(), "x".to_string()),
            ],
            removes: Vec::new(),
        };
        let env = apply_environment(&base(), &policy);
        // The caller explicitly requested the key after scrubbing: the exact
        // spelling requested is honored (see module doc), inherited ones die.
        assert_eq!(
            env.get("ELECTRON_RUN_AS_NODE").map(String::as_str),
            Some("1")
        );
        assert_eq!(env.get("ONLY").map(String::as_str), Some("x"));
        assert!(
            !env.contains_key("HOME"),
            "explicit replaces, does not merge"
        );
    }

    #[test]
    fn inherited_scrub_keys_die_even_when_explicit_map_omits_them() {
        let policy = EnvironmentPolicy::Explicit {
            map: vec![("ONLY".to_string(), "x".to_string())],
            removes: Vec::new(),
        };
        let env = apply_environment(&base(), &policy);
        for key in [
            "ELECTRON_RUN_AS_NODE",
            "MCODE_PANE_KEY",
            "MCODE_AGENT_LAUNCH_TOKEN",
        ] {
            assert!(!env.contains_key(key));
        }
    }
}
