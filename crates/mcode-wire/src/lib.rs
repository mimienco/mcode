//! Byte-for-byte port of the MCode relay wire protocol and E2EE crypto
//! framing from the TypeScript implementation (`src/relay/protocol.ts`,
//! `src/shared/relay-frame-decoder.ts`, `src/shared/e2ee-crypto.ts`,
//! `src/shared/mobile-e2ee-v2-framing.ts`, `src/relay/relay-handshake.ts`).
//!
//! Every constant carries a doc comment naming the exact TS source line it
//! was derived from. The wire format is frozen until versioned negotiation
//! says otherwise (docs/migration/WIRE_PROTOCOL.md).

pub mod crypto;
pub mod frame;
pub use frame::FrameDecoderError;
pub mod agent_exec;
pub mod handshake;
pub mod pairing;
pub mod protocol;
pub mod server;
pub mod v2;
#[cfg(test)]
pub mod vectors;

/// Base64 engine matching Node `Buffer.from(.., 'base64')` decode tolerance:
/// standard alphabet with padding accepted but not required. (Encoding is
/// always padded-standard, like `Buffer.toString('base64')`.)
pub(crate) mod b64 {
    use base64::Engine;
    use base64::engine::GeneralPurpose;
    use base64::engine::GeneralPurposeConfig;

    pub(crate) const ENGINE: GeneralPurpose = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
    );

    pub(crate) fn encode(data: &[u8]) -> String {
        ENGINE.encode(data)
    }

    pub(crate) fn decode(data: &str) -> Result<Vec<u8>, base64::DecodeError> {
        ENGINE.decode(data)
    }
}
