//! App-scoped PTY id routing keys: the port of `src/shared/ssh-pty-id.ts`
//! (app-scoped `ssh:` ids) plus the `remote:` runtime-terminal guard from
//! `src/shared/remote-runtime-pty-id.ts`.
//!
//! Ids stay opaque strings; the `remote:` prefix and `ssh:` parsing are
//! routing keys, never renderer authority (PTY_CONTRACT_SPEC.md §4, spec row
//! grounded at `pty.ts:7864-7866,7994-7996`). The `pty_kill`/`pty_has_pty`
//! guards use [`is_remote_pty_id`] and [`parse_app_ssh_ppty_id` routing]
//! through [`parse_app_ssh_pty_id`] so a remote id can never fall through to
//! the local provider and read as an authoritative local answer.

use crate::error::SshError;

pub const SSH_PTY_ID_PREFIX: &str = "ssh:";
pub const SSH_PTY_ID_SEPARATOR: &str = "@@";
pub const REMOTE_PTY_ID_PREFIX: &str = "remote:";

/// A parsed `ssh:<encodedConnectionId>@@<relayPtyId>` app-scoped pty id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSshPtyId {
    pub connection_id: String,
    pub relay_pty_id: String,
}

/// `remote:`-prefixed ids are runtime terminal handles owned by
/// `terminal.close` (original `pty.ts:7863-7867`); unowned routing into the
/// local provider is rejected by the callers of this guard.
pub fn is_remote_pty_id(pty_id: &str) -> bool {
    pty_id.starts_with(REMOTE_PTY_ID_PREFIX)
}

/// Port of `parseExecutionHostId`'s ssh branch (`execution-host.ts:79-90`):
/// reconnect/restore paths hand over the execution-host id form
/// (`ssh:<targetId>`, from a workspace `hostId`) instead of the bare SSH
/// target id that app pty ids embed. Both name the same connection, so
/// collapse to the bare id before comparing/encoding — otherwise a valid
/// reattach throws a spurious "belongs to SSH connection" error at the user.
fn normalize_connection_id(connection_id: &str) -> String {
    if let Some(encoded) = connection_id.strip_prefix(SSH_PTY_ID_PREFIX)
        && !encoded.is_empty()
        && let Some(target_id) = percent_decode(encoded)
        && !target_id.is_empty()
    {
        return target_id;
    }
    connection_id.to_string()
}

/// Port of `parseAppSshPtyId` (`ssh-pty-id.ts:23-46`): `None` for anything
/// that is not a well-formed `ssh:`-prefixed app id, including malformed
/// percent escapes (the original's `decodeURIComponent` throw).
pub fn parse_app_ssh_pty_id(pty_id: &str) -> Option<ParsedSshPtyId> {
    let rest = pty_id.strip_prefix(SSH_PTY_ID_PREFIX)?;
    let separator_index = rest.find(SSH_PTY_ID_SEPARATOR)?;
    let encoded_connection_id = &rest[..separator_index];
    let relay_pty_id = &rest[separator_index + SSH_PTY_ID_SEPARATOR.len()..];
    if encoded_connection_id.is_empty() || relay_pty_id.is_empty() {
        return None;
    }
    let connection_id = percent_decode(encoded_connection_id)?;
    Some(ParsedSshPtyId {
        connection_id,
        relay_pty_id: relay_pty_id.to_string(),
    })
}

/// Port of `toAppSshPtyId` (`ssh-pty-id.ts:48-63`). Embeds the connection id
/// so two relays cannot collide after restore. An already app-scoped
/// `relay_pty_id` passes through only when it names the same connection; the
/// original throws (`Error`), which is [`SshError::InvalidArgument`] here.
pub fn to_app_ssh_pty_id(connection_id: &str, relay_pty_id: &str) -> Result<String, SshError> {
    let normalized_connection_id = normalize_connection_id(connection_id);
    if let Some(parsed) = parse_app_ssh_pty_id(relay_pty_id) {
        if parsed.connection_id != normalized_connection_id {
            return Err(SshError::InvalidArgument(
                "relay pty id belongs to a different SSH connection",
            ));
        }
        return Ok(relay_pty_id.to_string());
    }
    Ok(format!(
        "{SSH_PTY_ID_PREFIX}{}{SSH_PTY_ID_SEPARATOR}{relay_pty_id}",
        percent_encode(&normalized_connection_id)
    ))
}

/// Port of `toRelaySshPtyId` (`ssh-pty-id.ts:65-75`): strips the app scope
/// before an id reaches a relay, refusing cross-connection misuse the same
/// way [`to_app_ssh_pty_id`] does.
pub fn to_relay_ssh_pty_id(connection_id: &str, pty_id: &str) -> Result<String, SshError> {
    let Some(parsed) = parse_app_ssh_pty_id(pty_id) else {
        return Ok(pty_id.to_string());
    };
    if parsed.connection_id != normalize_connection_id(connection_id) {
        return Err(SshError::InvalidArgument(
            "pty id belongs to a different SSH connection",
        ));
    }
    Ok(parsed.relay_pty_id)
}

/// `encodeURIComponent`'s unreserved set (`A-Za-z0-9-_.!~*'()`).
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `decodeURIComponent` for the single-escape sequences this module produces:
/// `None` on a truncated or malformed escape, or on bytes that are not valid
/// UTF-8 (the original throws; callers treat that as "not a valid id").
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let byte = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_prefix_guard_matches_only_the_exact_prefix() {
        assert!(is_remote_pty_id("remote:7"));
        assert!(is_remote_pty_id("remote:"));
        assert!(!is_remote_pty_id("remote-7"));
        assert!(!is_remote_pty_id("7"));
        assert!(!is_remote_pty_id(""));
        assert!(!is_remote_pty_id("ssh:target-1@@pty-1"));
    }

    #[test]
    fn parse_app_ssh_pty_id_round_trips_the_encoded_connection() {
        let id = to_app_ssh_pty_id("prod-box", "pty-1").expect("encode");
        assert_eq!(id, "ssh:prod-box@@pty-1");

        let parsed = parse_app_ssh_pty_id(&id).expect("parse");
        assert_eq!(
            parsed,
            ParsedSshPtyId {
                connection_id: "prod-box".to_string(),
                relay_pty_id: "pty-1".to_string()
            }
        );

        assert_eq!(
            to_relay_ssh_pty_id("prod-box", &id).expect("strip"),
            "pty-1"
        );
    }

    #[test]
    fn connection_ids_containing_separators_survive_encoding() {
        // A target id containing "@@" must not break the split, and a target
        // id containing "ssh:" must not be double-prefixed.
        let id = to_app_ssh_pty_id("a@@b", "pty-1").expect("encode");
        let parsed = parse_app_ssh_pty_id(&id).expect("parse");
        assert_eq!(parsed.connection_id, "a@@b");
        // A connection id that IS the execution-host form ("ssh:real-target")
        // normalizes to the bare target before encoding (normalizeConnectionId,
        // execution-host.ts:79-90), so the app id embeds the bare id.
        let nested = to_app_ssh_pty_id("ssh:real-target", "pty-1").expect("encode");
        let parsed_nested = parse_app_ssh_pty_id(&nested).expect("parse");
        assert_eq!(parsed_nested.connection_id, "real-target");
    }

    #[test]
    fn non_execution_host_ssh_prefixed_ids_encode_verbatim() {
        // The normalization only collapses `ssh:<encoded>` when the remainder
        // decodes to a non-empty target id; a bare "ssh:" prefix with no
        // payload stays as the literal connection id.
        let id = to_app_ssh_pty_id("ssh:", "pty-1").expect("encode");
        let parsed = parse_app_ssh_pty_id(&id).expect("parse");
        assert_eq!(parsed.connection_id, "ssh:");
    }

    #[test]
    fn execution_host_id_form_normalizes_to_the_bare_target_id() {
        // "ssh:prod-box" as the connection id (execution-host form) and the
        // bare "prod-box" must encode to the same app pty id.
        let from_host_form = to_app_ssh_pty_id("ssh:prod-box", "pty-1").expect("encode");
        let from_bare = to_app_ssh_pty_id("prod-box", "pty-1").expect("encode");
        assert_eq!(from_host_form, from_bare);

        // An already app-scoped relay id passes through unchanged when the
        // connection matches — even via the execution-host form.
        let passthrough = to_app_ssh_pty_id("ssh:prod-box", &from_bare).expect("passthrough");
        assert_eq!(passthrough, from_bare);
    }

    #[test]
    fn cross_connection_ids_are_refused() {
        let id = to_app_ssh_pty_id("prod-box", "pty-1").expect("encode");

        let error = to_app_ssh_pty_id("other-box", &id).expect_err("must refuse");
        assert_eq!(error.code(), "ssh_invalid_argument");

        let error = to_relay_ssh_pty_id("other-box", &id).expect_err("must refuse");
        assert_eq!(error.code(), "ssh_invalid_argument");
    }

    #[test]
    fn malformed_ids_parse_to_none() {
        assert_eq!(parse_app_ssh_pty_id("plain-id"), None);
        assert_eq!(parse_app_ssh_pty_id("ssh:no-separator"), None);
        assert_eq!(parse_app_ssh_pty_id("ssh:@@pty-1"), None);
        assert_eq!(parse_app_ssh_pty_id("ssh:target@@"), None);
        assert_eq!(parse_app_ssh_pty_id("ssh:%zz@@pty-1"), None);
        assert_eq!(parse_app_ssh_pty_id("ssh:%2@@pty-1"), None);
    }
}
