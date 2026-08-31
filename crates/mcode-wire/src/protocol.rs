//! Port of `orca/src/relay/protocol.ts` protocol-level constants and the
//! handshake JSON envelope (protocol.ts:27-64).

use serde::{Deserialize, Serialize};

use crate::frame::FrameDecoderError;
use crate::frame::{HEADER_LENGTH, MAX_MESSAGE_SIZE, encode_frame};

/// protocol.ts:27 — `export const RELAY_VERSION = '0.1.0'`
pub const RELAY_VERSION: &str = "0.1.0";

/// protocol.ts:28 — `` `MCODE-RELAY v${RELAY_VERSION} READY\n` `` (the trailing
/// newline is part of the sentinel bytes).
pub const RELAY_SENTINEL: &[u8] = b"MCODE-RELAY v0.1.0 READY\n";

/// protocol.ts:30-34 — `MessageType`.
pub mod message_type {
    pub const REGULAR: u8 = 1;
    pub const HANDSHAKE: u8 = 2;
    pub const KEEPALIVE: u8 = 9;
}

/// protocol.ts:63-64.
pub const KEEPALIVE_SEND_MS: u64 = 5_000;
pub const TIMEOUT_MS: u64 = 20_000;

/// protocol.ts:40-43 — `HandshakeMessage`. JSON shapes match TS
/// `JSON.stringify`/`JSON.parse` exactly: the `endpointCredential` field is
/// omitted when absent (`skip_serializing_if`), matching the TS spread
/// `...(endpointCredential ? { endpointCredential } : {})` at
/// relay-handshake.ts:251. Field ORDER matters for byte-for-byte
/// `JSON.stringify` compatibility, and serde emits struct fields in
/// declaration order: TS object literals serialize in key-insertion order —
/// `type`, `version`, `endpointCredential` — which this order mirrors.
/// `mismatch` is `type`, `expected`, `got` (relay-handshake.ts:132-136).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HandshakeMessage {
    #[serde(rename = "mcode-relay-handshake")]
    Handshake {
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        endpoint_credential: Option<String>,
    },
    #[serde(rename = "mcode-relay-handshake-ok")]
    Ok { version: String },
    #[serde(rename = "mcode-relay-handshake-mismatch")]
    Mismatch { expected: String, got: String },
}

impl HandshakeMessage {
    /// protocol.ts:45-48 — `encodeHandshakeFrame`.
    pub fn encode_frame(&self) -> Vec<u8> {
        let payload = serde_json::to_vec(self).expect("handshake JSON serialization is infallible");
        encode_frame(message_type::HANDSHAKE, 0, 0, &payload)
    }

    /// protocol.ts:50-61 — `parseHandshakeMessage`. Unknown `type` values fail
    /// with the exact TS message `` `Unknown handshake type: ${t}` ``. The TS
    /// version also surfaces serde parse failures verbatim; here those become
    /// `HandshakeError::Json`.
    pub fn parse(payload: &[u8]) -> Result<HandshakeMessage, HandshakeError> {
        let text = std::str::from_utf8(payload).map_err(|e| HandshakeError::Json(e.to_string()))?;
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| HandshakeError::Json(e.to_string()))?;
        let t = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("<missing>");
        match t {
            "mcode-relay-handshake"
            | "mcode-relay-handshake-ok"
            | "mcode-relay-handshake-mismatch" => {}
            other => return Err(HandshakeError::UnknownType(other.to_string())),
        }
        serde_json::from_value(value).map_err(|e| HandshakeError::Json(e.to_string()))
    }
}

/// Typed failures of `parseHandshakeMessage` (protocol.ts:51-59).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError {
    /// protocol.ts:58 — `` `Unknown handshake type: ${t}` ``
    UnknownType(String),
    /// `JSON.parse` failure (protocol.ts:51).
    Json(String),
}

impl core::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HandshakeError::UnknownType(t) => write!(f, "Unknown handshake type: {t}"),
            HandshakeError::Json(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HandshakeError {}

/// protocol.ts:158-164 — `prepareJsonRpcPayload` size gate: serialized payload
/// above MAX_MESSAGE_SIZE is rejected with
/// `` `Message too large: ${payload.length} bytes` ``.
pub fn encode_json_rpc_frame(
    payload: &[u8],
    id: u32,
    ack: u32,
) -> Result<Vec<u8>, FrameDecoderError> {
    if payload.len() > MAX_MESSAGE_SIZE {
        // protocol.ts:161 — the TS throw is not a decoder error; the host maps
        // it to its own channel. Reusing the wire error keeps the message.
        return Err(FrameDecoderError::FramePayloadTooLarge(0));
    }
    Ok(encode_frame(message_type::REGULAR, id, ack, payload))
}

/// protocol.ts:174-176 — `encodeKeepAliveFrame`.
pub fn encode_keep_alive_frame(id: u32, ack: u32) -> Vec<u8> {
    encode_frame(message_type::KEEPALIVE, id, ack, &[])
}

/// Exposed for tests/hosts that need the header size without importing frame.
pub const PROTOCOL_HEADER_LENGTH: usize = HEADER_LENGTH;
