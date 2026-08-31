//! Host-key decision flow, mirroring `ssh-host-key-decision.ts` in the
//! reduced form the rewrite needs: union the outcomes of the MCode accepted-key
//! store and the user's `known_hosts` (known_hosts outranks the store for the
//! same verdicts), reject changed keys, and classify first contact.

use crate::error::SshError;
use crate::fingerprint::AlgorithmName;
use crate::known_hosts::{KnownHostsOutcome, match_known_hosts};
use crate::store::StoreOutcome;

/// What the verifier decided about one presented host key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// Trusted by one of the sources.
    Accept,
    /// First contact; caller decides whether to `accept_host_key` it.
    Unknown { fingerprint: String },
    /// Contradicts a known key; refuse and surface both fingerprints.
    Reject {
        fingerprint: String,
        algorithm: AlgorithmName,
    },
}

/// Combine the known_hosts outcome and the store outcome for one presented key.
///
/// Order matters and matches the original:
/// 1. `revoked` outranks everything.
/// 2. known_hosts `match` accepts — after `ssh-keygen -R`, known_hosts holds
///    the NEW key while our store still holds the old one, and refusing there
///    would reject exactly the state the cure produces.
/// 3. Any remaining mismatch (from either source) is a changed key.
/// 4. Store `match` accepts.
/// 5. A key type we have never seen for a host we DO know is never demoted to
///    first contact — that is the downgrade an attacker who cannot forge the
///    known key would reach for.
/// 6. Otherwise first contact.
pub fn verify_host_key(
    known_hosts_entries: &[crate::known_hosts::KnownHostsEntry],
    store_records: &[crate::store::TrustedHostKeyRecord],
    host: &str,
    port: u16,
    key_algorithm: &AlgorithmName,
    key_bytes: &[u8],
    fingerprint: &str,
) -> Result<HostKeyDecision, SshError> {
    let known_hosts_outcome = match_known_hosts(
        known_hosts_entries,
        host,
        port,
        key_algorithm.as_str(),
        key_bytes,
    );

    if known_hosts_outcome == KnownHostsOutcome::Revoked {
        return Ok(HostKeyDecision::Reject {
            fingerprint: fingerprint.to_string(),
            algorithm: key_algorithm.clone(),
        });
    }
    if known_hosts_outcome == KnownHostsOutcome::Match {
        return Ok(HostKeyDecision::Accept);
    }

    let store_outcome = crate::store::HostKeyStore::lookup(
        store_records,
        host,
        port,
        key_algorithm.as_str(),
        key_bytes,
    );
    match store_outcome {
        StoreOutcome::Mismatch | StoreOutcome::UnknownTypeKnownHost => {
            Ok(HostKeyDecision::Reject {
                fingerprint: fingerprint.to_string(),
                algorithm: key_algorithm.clone(),
            })
        }
        StoreOutcome::Match => Ok(HostKeyDecision::Accept),
        StoreOutcome::Unknown => {
            if matches!(
                known_hosts_outcome,
                KnownHostsOutcome::Mismatch | KnownHostsOutcome::UnknownTypeKnownHost
            ) {
                // known_hosts holds a disagreeing entry for this host that the
                // store does not know about — still a changed key.
                Ok(HostKeyDecision::Reject {
                    fingerprint: fingerprint.to_string(),
                    algorithm: key_algorithm.clone(),
                })
            } else {
                // `ca-only` and `unknown` are first contact, as in the original:
                // OpenSSH itself connects to a CA-covered host that presents a
                // plain key.
                Ok(HostKeyDecision::Unknown {
                    fingerprint: fingerprint.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprint::tests::{ED25519_A_BYTES, ED25519_B_BYTES};
    use crate::fingerprint::{AlgorithmName, fingerprint_of};
    use base64::Engine as _;

    fn ed_algorithm() -> AlgorithmName {
        AlgorithmName("ssh-ed25519".into())
    }

    fn entry_for(host: &str, key: &[u8]) -> crate::known_hosts::KnownHostsEntry {
        crate::known_hosts::parse_known_hosts_line(&format!(
            "{host} ssh-ed25519 {}",
            base64::engine::general_purpose::STANDARD.encode(key)
        ))
        .unwrap()
    }

    #[test]
    fn unknown_host_is_first_contact() {
        let fingerprint = fingerprint_of(&ED25519_A_BYTES);
        let decision = verify_host_key(
            &[],
            &[],
            "h",
            22,
            &ed_algorithm(),
            &ED25519_A_BYTES,
            &fingerprint,
        )
        .unwrap();
        assert_eq!(
            decision,
            HostKeyDecision::Unknown {
                fingerprint: fingerprint.clone()
            }
        );
    }

    #[test]
    fn known_match_accepts_from_either_source() {
        let fingerprint = fingerprint_of(&ED25519_A_BYTES);
        let key = ED25519_A_BYTES.to_vec();
        // via known_hosts
        let entries = vec![entry_for("h", &key)];
        assert_eq!(
            verify_host_key(&entries, &[], "h", 22, &ed_algorithm(), &key, &fingerprint).unwrap(),
            HostKeyDecision::Accept
        );
        // via store
        let record = crate::store::TrustedHostKeyRecord {
            host: "h".into(),
            port: 22,
            algorithm: "ssh-ed25519".into(),
            key_base64: base64::engine::general_purpose::STANDARD.encode(&key),
            fingerprint: fingerprint.clone(),
            accepted_at: "t".into(),
        };
        assert_eq!(
            verify_host_key(&[], &[record], "h", 22, &ed_algorithm(), &key, &fingerprint).unwrap(),
            HostKeyDecision::Accept
        );
    }

    #[test]
    fn changed_key_rejects_with_fingerprint() {
        let key = ED25519_B_BYTES.to_vec();
        let fingerprint = fingerprint_of(&key);
        let entries = vec![entry_for("h", &ED25519_A_BYTES)];
        let decision =
            verify_host_key(&entries, &[], "h", 22, &ed_algorithm(), &key, &fingerprint).unwrap();
        match decision {
            HostKeyDecision::Reject {
                fingerprint: fp,
                algorithm,
            } => {
                assert_eq!(fp, fingerprint);
                assert_eq!(algorithm.as_str(), "ssh-ed25519");
            }
            other => panic!("expected reject, got {other:?}"),
        }
    }

    #[test]
    fn known_hosts_match_outranks_a_stale_store_record() {
        // The rotation cure: ssh-keygen -R leaves known_hosts holding the NEW
        // key while the store still holds the old one. known_hosts wins.
        let new_key = ED25519_B_BYTES.to_vec();
        let new_fingerprint = fingerprint_of(&new_key);
        let entries = vec![entry_for("h", &new_key)];
        let stale_record = crate::store::TrustedHostKeyRecord {
            host: "h".into(),
            port: 22,
            algorithm: "ssh-ed25519".into(),
            key_base64: base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES),
            fingerprint: fingerprint_of(&ED25519_A_BYTES),
            accepted_at: "t".into(),
        };
        assert_eq!(
            verify_host_key(
                &entries,
                &[stale_record],
                "h",
                22,
                &ed_algorithm(),
                &new_key,
                &new_fingerprint
            )
            .unwrap(),
            HostKeyDecision::Accept
        );
    }

    #[test]
    fn other_type_for_known_host_rejects_even_when_store_is_empty() {
        // known_hosts holds an ed25519 key; the host offers RSA. Not first contact.
        let rsa_key = crate::fingerprint::tests::rsa_a_bytes();
        let fingerprint = fingerprint_of(&rsa_key);
        let entries = vec![entry_for("h", &ED25519_A_BYTES)];
        let decision = verify_host_key(
            &entries,
            &[],
            "h",
            22,
            &AlgorithmName("ssh-rsa".into()),
            &rsa_key,
            &fingerprint,
        )
        .unwrap();
        assert!(matches!(decision, HostKeyDecision::Reject { .. }));
    }

    #[test]
    fn revoked_key_rejects_before_any_other_source() {
        let key = ED25519_A_BYTES.to_vec();
        let fingerprint = fingerprint_of(&key);
        let contents = format!(
            "@revoked h ssh-ed25519 {}",
            base64::engine::general_purpose::STANDARD.encode(&key)
        );
        let entries = crate::known_hosts::parse_known_hosts(&contents);
        // Even the store agreeing cannot rescue a revoked key.
        let record = crate::store::TrustedHostKeyRecord {
            host: "h".into(),
            port: 22,
            algorithm: "ssh-ed25519".into(),
            key_base64: base64::engine::general_purpose::STANDARD.encode(&key),
            fingerprint: fingerprint.clone(),
            accepted_at: "t".into(),
        };
        assert_eq!(
            verify_host_key(
                &entries,
                &[record],
                "h",
                22,
                &ed_algorithm(),
                &key,
                &fingerprint
            )
            .unwrap(),
            HostKeyDecision::Reject {
                fingerprint,
                algorithm: ed_algorithm()
            }
        );
    }
}
