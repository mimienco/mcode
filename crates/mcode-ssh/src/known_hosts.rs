//! OpenSSH `known_hosts` parsing and matching, mirroring `ssh-known-hosts.ts`.
//!
//! v1 scope matches the original: plaintext (non-hashed) lines, comma lists,
//! `!` negations, `@revoked` and `@cert-authority` markers. Hashed `|1|`
//! entries are parsed but never match a candidate in v1 — the original store
//! only does plaintext matching against MCode endpoints.

use crate::fingerprint::AlgorithmName;

/// Ordered by severity: the first outcome that applies decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownHostsOutcome {
    Match,
    Mismatch,
    Revoked,
    CaOnly,
    /// This host is known, but no entry of the presented key type exists.
    UnknownTypeKnownHost,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownHostsEntry {
    pub marker: Option<KnownHostsMarker>,
    pub patterns: Vec<String>,
    pub negations: Vec<String>,
    pub key_type: AlgorithmName,
    pub key: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownHostsMarker {
    Revoked,
    CertAuthority,
}

/// Returns `None` for blank lines, comments, and anything malformed — never throws.
pub fn parse_known_hosts_line(line: &str) -> Option<KnownHostsEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let mut fields = trimmed.split_whitespace();
    let mut marker = None;
    let host_field;
    {
        let first = fields.next()?;
        if first.starts_with('@') {
            marker = Some(match first {
                "@revoked" => KnownHostsMarker::Revoked,
                "@cert-authority" => KnownHostsMarker::CertAuthority,
                // An unrecognised marker may restrict the line in a way we do
                // not model, so honouring the line as unmarked would
                // over-trust it.
                _ => return None,
            });
            host_field = fields.next()?;
        } else {
            host_field = first;
        }
    }
    let declared_type = fields.next()?;
    let key_base64 = fields.next()?;
    if fields.next().is_some() {
        // Trailing fields beyond key are not part of the format we model.
        return None;
    }

    let key = decode_canonical_base64(key_base64)?;
    let wire_type = crate::store::read_wire_algorithm(&key)?;
    if wire_type.as_str() != declared_type {
        return None;
    }

    if host_field.starts_with("|1|") {
        // Hashed entries: parsed but not matched in v1. Dropping them mirrors
        // the original's plaintext-only scope.
        return None;
    }

    let mut patterns = Vec::new();
    let mut negations = Vec::new();
    for raw in host_field.split(',') {
        let pattern = raw.trim().to_lowercase();
        if pattern.is_empty() {
            continue;
        }
        if let Some(negated) = pattern.strip_prefix('!') {
            negations.push(negated.to_string());
        } else {
            patterns.push(pattern);
        }
    }
    if patterns.is_empty() {
        // A negation-only line can never match a host: dropping it mirrors
        // the original store's parse behavior.
        return None;
    }

    Some(KnownHostsEntry {
        marker,
        patterns,
        negations,
        key_type: wire_type,
        key,
    })
}

pub fn parse_known_hosts(contents: &str) -> Vec<KnownHostsEntry> {
    contents
        .lines()
        .filter_map(parse_known_hosts_line)
        .collect()
}

/// The candidate forms, in the order OpenSSH tries them: a non-default port
/// looks up `[host]:port` first and only retries the bare host when the first
/// pass knows no plain entry of any type.
pub fn host_candidate_passes(host: &str, port: u16) -> Vec<Vec<String>> {
    let lower = host.to_lowercase();
    if port == 22 {
        vec![vec![lower]]
    } else {
        vec![vec![format!("[{lower}]:{port}")], vec![lower]]
    }
}

fn pattern_matches(pattern: &str, candidate: &str) -> bool {
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == candidate;
    }
    glob_to_regex(pattern)
        .map(|regex| regex.is_match(candidate))
        .unwrap_or(false)
}

fn glob_to_regex(pattern: &str) -> Option<regex_lite::Regex> {
    let mut regex = String::with_capacity(pattern.len() + 2);
    regex.push('^');
    for character in pattern.chars() {
        match character {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            other => regex.push_str(&regex_lite::escape(&other.to_string())),
        }
    }
    regex.push('$');
    regex_lite::Regex::new(&regex).ok()
}

fn entry_matches_candidate(entry: &KnownHostsEntry, candidate: &str) -> bool {
    // A single negation vetoes the entire line even when another pattern matches.
    if entry
        .negations
        .iter()
        .any(|pattern| pattern_matches(pattern, candidate))
    {
        return false;
    }
    entry
        .patterns
        .iter()
        .any(|pattern| pattern_matches(pattern, candidate))
}

/// Decide an outcome for one presented key against a set of entries.
///
/// Entries from several files are unioned by the caller: any exact hit in any
/// file wins, and a disagreeing entry in another file does not make it a
/// mismatch.
pub fn match_known_hosts(
    entries: &[KnownHostsEntry],
    host: &str,
    port: u16,
    key_type: &str,
    key: &[u8],
) -> KnownHostsOutcome {
    let passes = host_candidate_passes(host, port);
    let matches_host = |entry: &KnownHostsEntry, candidates: &[String]| {
        candidates
            .iter()
            .any(|candidate| entry_matches_candidate(entry, candidate))
    };

    // Revocation resolves first, across every pass, so the verdict cannot
    // depend on line order.
    for candidates in &passes {
        for entry in entries {
            if entry.marker == Some(KnownHostsMarker::Revoked)
                && matches_host(entry, candidates)
                && entry.key == key
            {
                return KnownHostsOutcome::Revoked;
            }
        }
    }

    // A CA line is a marker, so it never satisfies a plain host key and never
    // stops the fallback pass.
    let mut saw_cert_authority = false;

    for candidates in passes.iter() {
        let mut saw_same_type_for_host = false;
        let mut saw_plain_entry_for_host = false;

        for entry in entries {
            if entry.marker == Some(KnownHostsMarker::Revoked) || !matches_host(entry, candidates) {
                continue;
            }
            if entry.marker == Some(KnownHostsMarker::CertAuthority) {
                saw_cert_authority = true;
                continue;
            }
            // Byte equality implies the types agree: parsing already rejected
            // any line whose declared type disagreed with the blob header.
            if entry.key == key {
                return KnownHostsOutcome::Match;
            }
            saw_plain_entry_for_host = true;
            saw_same_type_for_host |= entry.key_type.as_str() == key_type;
        }

        // The first pass that finds a plain entry for the host decides.
        // OpenSSH runs the bare-host fallback only when the port-qualified
        // lookup matched no plain entry of any type; once a pass sees one,
        // its verdict is final — including a later pass than the first.
        if saw_plain_entry_for_host {
            return if saw_same_type_for_host {
                KnownHostsOutcome::Mismatch
            } else {
                KnownHostsOutcome::UnknownTypeKnownHost
            };
        }
    }

    if saw_cert_authority {
        KnownHostsOutcome::CaOnly
    } else {
        KnownHostsOutcome::Unknown
    }
}

/// Canonical base64 only: the ONE way this module turns a base64 field into
/// bytes. Standard alphabet, no whitespace, no URL-safe characters.
fn decode_canonical_base64(raw: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    if raw.is_empty() || raw.contains(['-', '_']) {
        return None;
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .ok()
        .filter(|bytes| !bytes.is_empty())?;
    // Re-encoding must reproduce the field exactly, rejecting mixed padding.
    if base64::engine::general_purpose::STANDARD.encode(&decoded) != raw {
        return None;
    }
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprint::tests::{ED25519_A_BYTES, ED25519_B_BYTES, rsa_a_bytes};
    use base64::Engine as _;

    #[test]
    fn zz_debug_parse_single() {
        let ed = crate::fingerprint::tests::ED25519_A_BYTES;
        let b64 = base64::engine::general_purpose::STANDARD.encode(ed);
        let line = format!("example.com ssh-ed25519 {}", b64);
        let parsed = parse_known_hosts_line(&line);
        eprintln!(
            "DEBUG parsed: {:?}",
            parsed.as_ref().map(|e| (&e.key_type, e.patterns.clone()))
        );
        eprintln!("DEBUG wire: {:?}", crate::store::read_wire_algorithm(&ed));
        eprintln!(
            "DEBUG canon: {:?}",
            decode_canonical_base64(&b64).map(|b| b.len())
        );
        assert!(parsed.is_some());
    }

    #[test]
    fn parses_plaintext_entries_with_comma_hosts_and_comments() {
        let contents = format!(
            "# comment\n\n{},another.host ssh-ed25519 {}\nwild* ssh-rsa {}\n",
            "example.com",
            base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES),
            base64::engine::general_purpose::STANDARD.encode(rsa_a_bytes())
        );
        let entries = parse_known_hosts(&contents);
        assert_eq!(entries.len(), 2, "comments and blanks skipped");
        assert_eq!(entries[0].patterns, vec!["example.com", "another.host"]);
        assert_eq!(entries[1].key_type.as_str(), "ssh-rsa");
        assert!(entries[1].patterns[0].ends_with('*'));
    }

    #[test]
    fn skips_negated_hosts_and_unrecognized_markers() {
        let contents = format!(
            "!trusted.example.com ssh-ed25519 {}\n@bogus example.com ssh-ed25519 {}\n@cert-authority *.example.com ssh-ed25519 {}\n",
            base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES),
            base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES),
            base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES),
        );
        let entries = parse_known_hosts(&contents);
        assert_eq!(entries.len(), 1, "negation-only line and @bogus dropped");
        assert_eq!(entries[0].marker, Some(KnownHostsMarker::CertAuthority));
        assert_eq!(entries[0].negations.len(), 0);
    }

    #[test]
    fn drops_lines_whose_declared_type_disagrees_with_the_blob() {
        let contents = format!(
            "example.com ssh-rsa {}",
            base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES)
        );
        assert!(parse_known_hosts_line(&contents).is_none());
    }

    #[test]
    fn decision_matrix_match_mismatch_unknown_and_revoked() {
        let ed_a = base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES);
        let ed_b = base64::engine::general_purpose::STANDARD.encode(ED25519_B_BYTES);
        let contents =
            format!("example.com ssh-ed25519 {ed_a}\n@revoked revoked.com ssh-ed25519 {ed_b}\n");
        let entries = parse_known_hosts(&contents);

        assert_eq!(
            match_known_hosts(&entries, "example.com", 22, "ssh-ed25519", &ED25519_A_BYTES),
            KnownHostsOutcome::Match
        );
        assert_eq!(
            match_known_hosts(&entries, "example.com", 22, "ssh-ed25519", &ED25519_B_BYTES),
            KnownHostsOutcome::Mismatch
        );
        assert_eq!(
            match_known_hosts(
                &entries,
                "never-seen.com",
                22,
                "ssh-ed25519",
                &ED25519_A_BYTES
            ),
            KnownHostsOutcome::Unknown
        );
        assert_eq!(
            match_known_hosts(&entries, "revoked.com", 22, "ssh-ed25519", &ED25519_B_BYTES),
            KnownHostsOutcome::Revoked
        );
    }

    #[test]
    fn reports_unknown_type_for_a_known_host_of_another_type() {
        let rsa = base64::engine::general_purpose::STANDARD.encode(rsa_a_bytes());
        let contents = format!("example.com ssh-rsa {rsa}\n");
        let entries = parse_known_hosts(&contents);
        assert_eq!(
            match_known_hosts(&entries, "example.com", 22, "ssh-ed25519", &ED25519_A_BYTES),
            KnownHostsOutcome::UnknownTypeKnownHost
        );
    }

    #[test]
    fn matches_bracketed_non_default_port_and_falls_back_to_bare_host() {
        let contents = format!(
            "[example.com]:2222 ssh-ed25519 {}\nother.com ssh-ed25519 {}\n",
            base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES),
            base64::engine::general_purpose::STANDARD.encode(ED25519_B_BYTES),
        );
        let entries = parse_known_hosts(&contents);

        assert_eq!(
            match_known_hosts(
                &entries,
                "example.com",
                2222,
                "ssh-ed25519",
                &ED25519_A_BYTES
            ),
            KnownHostsOutcome::Match
        );
        // A bare entry is found via the fallback pass for a non-default port.
        assert_eq!(
            match_known_hosts(&entries, "other.com", 2222, "ssh-ed25519", &ED25519_B_BYTES),
            KnownHostsOutcome::Match
        );
        // A bare entry of ANOTHER type does not affect the port-qualified
        // lookup: the first-pass candidates only contain the bracketed form,
        // so the bracketed ed25519 entry still matches (TS
        // ssh-known-hosts.ts:316-330 — the bare entry never joins pass 0).
        let bare_rsa = format!(
            "example.com ssh-rsa {}",
            base64::engine::general_purpose::STANDARD.encode(rsa_a_bytes())
        );
        let both = parse_known_hosts(&format!("{contents}{bare_rsa}\n"));
        assert_eq!(
            match_known_hosts(&both, "example.com", 2222, "ssh-ed25519", &ED25519_A_BYTES),
            KnownHostsOutcome::Match
        );
        // A bare entry of the SAME type with different bytes still does not
        // reach the port-qualified pass: the bracketed ed25519 entry matches
        // in pass 0 before the fallback is consulted (TS
        // ssh-known-hosts.ts:324-330 — bare lines never join pass 0).
        let bare_ed_b = format!(
            "example.com ssh-ed25519 {}",
            base64::engine::general_purpose::STANDARD.encode(ED25519_B_BYTES)
        );
        let both_ed = parse_known_hosts(&format!("{contents}{bare_ed_b}\n"));
        assert_eq!(
            match_known_hosts(
                &both_ed,
                "example.com",
                2222,
                "ssh-ed25519",
                &ED25519_A_BYTES
            ),
            KnownHostsOutcome::Match
        );
    }

    #[test]
    fn ca_only_never_satisfies_a_plain_key() {
        let contents = format!(
            "@cert-authority example.com ssh-ed25519 {}",
            base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES)
        );
        let entries = parse_known_hosts(&contents);
        assert_eq!(
            match_known_hosts(&entries, "example.com", 22, "ssh-ed25519", &ED25519_B_BYTES),
            KnownHostsOutcome::CaOnly
        );
        assert_eq!(
            match_known_hosts(&entries, "example.com", 22, "ssh-ed25519", &ED25519_A_BYTES),
            KnownHostsOutcome::CaOnly,
            "a CA line is a marker, not a match for the bare key"
        );
    }

    #[test]
    fn negation_vetoes_a_matching_pattern() {
        let contents = format!(
            "*.example.com,!bad.example.com ssh-ed25519 {}",
            base64::engine::general_purpose::STANDARD.encode(ED25519_A_BYTES)
        );
        let entries = parse_known_hosts(&contents);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].negations, vec!["bad.example.com"]);
        assert_eq!(
            match_known_hosts(
                &entries,
                "bad.example.com",
                22,
                "ssh-ed25519",
                &ED25519_A_BYTES
            ),
            KnownHostsOutcome::Unknown
        );
        assert_eq!(
            match_known_hosts(
                &entries,
                "good.example.com",
                22,
                "ssh-ed25519",
                &ED25519_A_BYTES
            ),
            KnownHostsOutcome::Match
        );
    }
}
