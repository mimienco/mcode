//! Port of `orca/src/relay/relay-handshake.ts` — the wire-level handshake
//! state machine on top of the frame decoder: sentinel exchange, version
//! equality check, and credential comparison.
//!
//! I/O (socket writes/reads, deadlines, process exit codes) stays with the
//! host; this module provides the byte-level operations and typed decisions
//! the TS functions encode:
//!
//! - `setupDaemonHandshake` (relay-handshake.ts:58-90) + `handleDaemonHandshakeFrame`
//!   (relay-handshake.ts:100-155) → [`server_handle_handshake_frame`].
//! - `runConnectHandshake` first write (relay-handshake.ts:247-253) →
//!   [`client_handshake_frame`].
//! - Client reply handling (relay-handshake.ts:193-232) →
//!   [`client_evaluate_reply`].
//! - Sentinel write (relay-connect side of the daemon greeting) →
//!   [`SENTINEL`]/[`write_sentinel`]/[`read_and_check_sentinel`].

use subtle::ConstantTimeEq;

use crate::frame::DecodedFrame;
use crate::protocol::{HandshakeError, HandshakeMessage, message_type};

/// relay-handshake.ts:17 — `EXIT_CODE_VERSION_MISMATCH = 42`
pub const EXIT_CODE_VERSION_MISMATCH: i32 = 42;
/// protocol.ts:28 — `RELAY_SENTINEL` (`MCODE-RELAY v0.1.0 READY\n`).
pub const SENTINEL: &[u8] = crate::protocol::RELAY_SENTINEL;

/// Decision returned by [`server_handle_handshake_frame`] — mirrors the
/// return-boolean + stderr/side-effect structure of
/// `handleDaemonHandshakeFrame` (relay-handshake.ts:100-155).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHandshakeDecision {
    /// relay-handshake.ts:152-154 — handshake OK; host writes the `ok` reply
    /// frame (built via [`HandshakeMessage::Ok::encode_frame`]) and attaches
    /// the dispatcher.
    Accept {
        version: String,
        /// relay-handshake.ts:153 — the ok frame bytes to write.
        reply: Vec<u8>,
    },
    /// relay-handshake.ts:130-142 — version mismatch; `reply` carries the
    /// `mismatch` frame (best-effort write), then the host closes the socket
    /// and exits with [`EXIT_CODE_VERSION_MISMATCH`].
    Mismatch {
        expected: String,
        got: String,
        reply: Vec<u8>,
    },
    /// relay-handshake.ts:106-112 — wrong frame type pre-handshake.
    WrongFrameType(u8),
    /// relay-handshake.ts:113-125 — parse failure or unexpected handshake type.
    BadHandshake(String),
    /// relay-handshake.ts:144-151 — endpoint credential mismatch.
    CredentialMismatch,
}

/// relay-handshake.ts:247-253 — the connect-side first frame:
/// `{ type: 'mcode-relay-handshake', version, ...(endpointCredential ? {endpointCredential} : {}) }`.
/// The spread preserves the TS optional-field omission (empty string or `None`
/// → field absent from JSON).
pub fn client_handshake_frame(my_version: &str, endpoint_credential: Option<&str>) -> Vec<u8> {
    HandshakeMessage::Handshake {
        version: my_version.to_string(),
        endpoint_credential: endpoint_credential
            .filter(|c| !c.is_empty())
            .map(|c| c.to_string()),
    }
    .encode_frame()
}

/// relay-handshake.ts:100-155 — `handleDaemonHandshakeFrame`.
///
/// `expected_credential` is `Some` only when the daemon was launched with one
/// (TS `endpointCredential !== undefined`, relay-handshake.ts:145). The
/// comparison then uses a timing-safe equality over the UTF-8 bytes of both
/// values, replacing the TS plain `!==` with the standard timing-safe
/// construction required by the migration brief (subtle `ConstantTimeEq`).
pub fn server_handle_handshake_frame(
    frame: &DecodedFrame,
    launch_version: &str,
    expected_credential: Option<&str>,
) -> ServerHandshakeDecision {
    // relay-handshake.ts:106-112
    if frame.frame_type != message_type::HANDSHAKE {
        return ServerHandshakeDecision::WrongFrameType(frame.frame_type);
    }
    // relay-handshake.ts:113-125
    let msg = match HandshakeMessage::parse(&frame.payload) {
        Ok(msg) => msg,
        Err(HandshakeError::UnknownType(t)) => {
            return ServerHandshakeDecision::BadHandshake(format!("Unknown handshake type: {t}"));
        }
        Err(e) => return ServerHandshakeDecision::BadHandshake(e.to_string()),
    };
    let HandshakeMessage::Handshake {
        version,
        endpoint_credential,
    } = msg
    else {
        // relay-handshake.ts:121-125 — unexpected handshake type from client.
        return ServerHandshakeDecision::BadHandshake(match msg {
            HandshakeMessage::Ok { .. } => "mcode-relay-handshake-ok".to_string(),
            HandshakeMessage::Mismatch { .. } => "mcode-relay-handshake-mismatch".to_string(),
            HandshakeMessage::Handshake { .. } => unreachable!(),
        });
    };
    // relay-handshake.ts:126-143 — version equality check (exact match).
    if version != launch_version {
        let reply = HandshakeMessage::Mismatch {
            expected: launch_version.to_string(),
            got: version.clone(),
        }
        .encode_frame();
        return ServerHandshakeDecision::Mismatch {
            expected: launch_version.to_string(),
            got: version,
            reply,
        };
    }
    // relay-handshake.ts:144-151 — credential check, timing-safe.
    if let Some(expected) = expected_credential {
        let provided = endpoint_credential.as_deref().unwrap_or("");
        if !timing_safe_eq(provided.as_bytes(), expected.as_bytes()) {
            return ServerHandshakeDecision::CredentialMismatch;
        }
    }
    // relay-handshake.ts:152-154
    let reply = HandshakeMessage::Ok {
        version: launch_version.to_string(),
    }
    .encode_frame();
    ServerHandshakeDecision::Accept { version, reply }
}

/// Client-side evaluation of the daemon's reply frame
/// (relay-handshake.ts:193-232).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHandshakeDecision {
    /// relay-handshake.ts:210-218 — accepted; leftover bytes go to the dispatcher.
    Accepted { leftover: Vec<u8> },
    /// relay-handshake.ts:219-228 — daemon reported mismatch; host exits with
    /// [`EXIT_CODE_VERSION_MISMATCH`].
    Mismatch { expected: String, got: String },
    /// relay-handshake.ts:193-199 — non-handshake frame: protocol violation.
    WrongFrameType(u8),
    /// relay-handshake.ts:200-209, 230-232 — parse failure / unexpected type.
    BadHandshake(String),
    /// Not a terminal decision — no handshake-relevant frame yet.
    Pending,
}

/// relay-handshake.ts:188-239 + 214 — feed a decoded reply frame; `leftover`
/// is the decoder drain (`decoder.drain()`) taken on acceptance. A host passes
/// the same decoder buffer on every call and only consumes `Accepted`.
pub fn client_evaluate_reply(
    frame: Option<&DecodedFrame>,
    leftover: &[u8],
) -> ClientHandshakeDecision {
    let Some(frame) = frame else {
        return ClientHandshakeDecision::Pending;
    };
    if frame.frame_type != message_type::HANDSHAKE {
        return ClientHandshakeDecision::WrongFrameType(frame.frame_type);
    }
    match HandshakeMessage::parse(&frame.payload) {
        Ok(HandshakeMessage::Ok { version }) => {
            // The TS client accepts the Ok frame and surfaces the version to
            // the caller for logging only; acceptance is not version-gated
            // here (the server already refused mismatched sentinels upstream).
            let _ = version;
            ClientHandshakeDecision::Accepted {
                leftover: leftover.to_vec(),
            }
        }
        Ok(HandshakeMessage::Mismatch { expected, got }) => {
            ClientHandshakeDecision::Mismatch { expected, got }
        }
        Ok(HandshakeMessage::Handshake { .. }) => {
            ClientHandshakeDecision::BadHandshake("mcode-relay-handshake".to_string())
        }
        Err(e) => ClientHandshakeDecision::BadHandshake(e.to_string()),
    }
}

/// Sentinel write — hosts emit `RELAY_SENTINEL` on greeting channels
/// (protocol.ts:28).
pub fn write_sentinel() -> Vec<u8> {
    SENTINEL.to_vec()
}

/// Sentinel read/check: the host feeds whatever prefix arrived; the full
/// sentinel must match byte-for-byte. Returns `None` while more bytes are
/// needed, `Some(Err(consumed_so_far))` on mismatch, `Some(Ok(()))` on match.
pub fn read_and_check_sentinel(buffer: &[u8]) -> Option<Result<(), usize>> {
    if buffer.len() < SENTINEL.len() {
        if !SENTINEL.starts_with(buffer) {
            return Some(Err(buffer.len()));
        }
        return None;
    }
    if buffer.starts_with(SENTINEL) {
        Some(Ok(()))
    } else {
        Some(Err(SENTINEL.len()))
    }
}

/// Timing-safe equality (subtle `ConstantTimeEq`). Length differences still
/// return false (constant-time comparison is only defined for equal lengths);
/// this matches the standard Rust `subtle` usage and hardens the TS `!==`.
fn timing_safe_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_produces_ok_reply() {
        let frame = DecodedFrame {
            frame_type: message_type::HANDSHAKE,
            id: 0,
            ack: 0,
            payload: serde_json::json!({
                "type": "mcode-relay-handshake",
                "version": crate::protocol::RELAY_VERSION
            })
            .to_string()
            .into_bytes(),
        };
        let decision = server_handle_handshake_frame(&frame, crate::protocol::RELAY_VERSION, None);
        let ServerHandshakeDecision::Accept { version, reply } = decision else {
            panic!("expected accept, got {decision:?}");
        };
        assert_eq!(version, crate::protocol::RELAY_VERSION);
        let parsed = HandshakeMessage::parse(&reply[crate::frame::HEADER_LENGTH..]).unwrap();
        assert_eq!(
            parsed,
            HandshakeMessage::Ok {
                version: crate::protocol::RELAY_VERSION.to_string()
            }
        );
    }

    #[test]
    fn version_mismatch_produces_mismatch_reply() {
        let frame = DecodedFrame {
            frame_type: message_type::HANDSHAKE,
            id: 0,
            ack: 0,
            payload: serde_json::json!({
                "type": "mcode-relay-handshake",
                "version": "9.9.9"
            })
            .to_string()
            .into_bytes(),
        };
        let decision = server_handle_handshake_frame(&frame, crate::protocol::RELAY_VERSION, None);
        let ServerHandshakeDecision::Mismatch {
            expected,
            got,
            reply,
        } = decision
        else {
            panic!("expected mismatch, got {decision:?}");
        };
        assert_eq!(expected, crate::protocol::RELAY_VERSION);
        assert_eq!(got, "9.9.9");
        let parsed = HandshakeMessage::parse(&reply[crate::frame::HEADER_LENGTH..]).unwrap();
        assert_eq!(
            parsed,
            HandshakeMessage::Mismatch {
                expected: crate::protocol::RELAY_VERSION.to_string(),
                got: "9.9.9".to_string()
            }
        );
    }

    #[test]
    fn wrong_frame_type_and_bad_payload_rejected() {
        let regular = DecodedFrame {
            frame_type: message_type::REGULAR,
            id: 0,
            ack: 0,
            payload: b"{}".to_vec(),
        };
        assert_eq!(
            server_handle_handshake_frame(&regular, "0.1.0", None),
            ServerHandshakeDecision::WrongFrameType(message_type::REGULAR)
        );
        let bad = DecodedFrame {
            frame_type: message_type::HANDSHAKE,
            id: 0,
            ack: 0,
            payload: b"{\"type\":\"nope\"}".to_vec(),
        };
        assert!(matches!(
            server_handle_handshake_frame(&bad, "0.1.0", None),
            ServerHandshakeDecision::BadHandshake(_)
        ));
    }

    #[test]
    fn credential_semantics() {
        // No expected credential → accept regardless.
        let frame_bytes = client_handshake_frame("0.1.0", Some("sekrit"));
        let frame = decode_one(&frame_bytes);
        assert!(matches!(
            server_handle_handshake_frame(&frame, "0.1.0", None),
            ServerHandshakeDecision::Accept { .. }
        ));
        // Matching credential → accept.
        assert!(matches!(
            server_handle_handshake_frame(&frame, "0.1.0", Some("sekrit")),
            ServerHandshakeDecision::Accept { .. }
        ));
        // Wrong credential → reject.
        assert_eq!(
            server_handle_handshake_frame(&frame, "0.1.0", Some("other")),
            ServerHandshakeDecision::CredentialMismatch
        );
        // Missing credential while one is expected → reject
        // (TS: undefined !== endpointCredential).
        let bare = decode_one(&client_handshake_frame("0.1.0", None));
        assert_eq!(
            server_handle_handshake_frame(&bare, "0.1.0", Some("sekrit")),
            ServerHandshakeDecision::CredentialMismatch
        );
    }

    #[test]
    fn client_replies() {
        let ok = decode_one(
            &HandshakeMessage::Ok {
                version: "0.1.0".to_string(),
            }
            .encode_frame(),
        );
        assert!(matches!(
            client_evaluate_reply(Some(&ok), b"trailing"),
            ClientHandshakeDecision::Accepted { leftover } if leftover == b"trailing"
        ));
        let mismatch = decode_one(
            &HandshakeMessage::Mismatch {
                expected: "0.1.0".to_string(),
                got: "0.0.1".to_string(),
            }
            .encode_frame(),
        );
        assert_eq!(
            client_evaluate_reply(Some(&mismatch), &[]),
            ClientHandshakeDecision::Mismatch {
                expected: "0.1.0".to_string(),
                got: "0.0.1".to_string()
            }
        );
        let regular = DecodedFrame {
            frame_type: message_type::REGULAR,
            id: 1,
            ack: 0,
            payload: b"{}".to_vec(),
        };
        assert_eq!(
            client_evaluate_reply(Some(&regular), &[]),
            ClientHandshakeDecision::WrongFrameType(message_type::REGULAR)
        );
        assert_eq!(
            client_evaluate_reply(None, &[]),
            ClientHandshakeDecision::Pending
        );
    }

    #[test]
    fn sentinel_roundtrip() {
        assert_eq!(write_sentinel(), b"MCODE-RELAY v0.1.0 READY\n");
        assert_eq!(read_and_check_sentinel(b"MCODE-RELAY v0.1.0"), None);
        assert_eq!(read_and_check_sentinel(b"WRONG"), Some(Err(5)));
        assert_eq!(
            read_and_check_sentinel(write_sentinel().as_slice()),
            Some(Ok(()))
        );
        let mut polluted = write_sentinel();
        polluted[13] = b'X';
        assert!(matches!(read_and_check_sentinel(&polluted), Some(Err(_))));
    }

    /// The connect-side wire frame must equal the TS JSON layout:
    /// endpointCredential omitted when absent (relay-handshake.ts:251).
    #[test]
    fn client_frame_matches_ts_json_shape() {
        let with_cred = client_handshake_frame("0.1.0", Some("abc"));
        let payload = &with_cred[crate::frame::HEADER_LENGTH..];
        assert_eq!(
            std::str::from_utf8(payload).unwrap(),
            r#"{"type":"mcode-relay-handshake","version":"0.1.0","endpointCredential":"abc"}"#
        );
        let without = client_handshake_frame("0.1.0", None);
        let payload = &without[crate::frame::HEADER_LENGTH..];
        assert_eq!(
            std::str::from_utf8(payload).unwrap(),
            r#"{"type":"mcode-relay-handshake","version":"0.1.0"}"#
        );
    }

    fn decode_one(bytes: &[u8]) -> DecodedFrame {
        let mut decoder = crate::frame::FrameDecoder::new(None, None, None);
        decoder.feed(bytes).unwrap();
        let mut frames = Vec::new();
        let mut errors = Vec::new();
        let outcome = decoder.decode_turn(&mut |f| frames.push(f), &mut |e| errors.push(e));
        assert_eq!(outcome, crate::frame::TurnOutcome::Idle);
        assert!(errors.is_empty());
        frames.into_iter().next().expect("one frame decoded")
    }
}
