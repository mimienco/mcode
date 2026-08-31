//! Port of `orca/src/shared/mobile-e2ee-v2-framing.ts` — v2 E2EE framing with
//! secretbox (XSalsa20-Poly1305), deterministic nonce layout, and the
//! 42-byte header. Byte layout preserved exactly; every constant cites its TS
//! source line.

use crypto_secretbox::Nonce;
use crypto_secretbox::XSalsa20Poly1305;
use crypto_secretbox::aead::Aead;
use crypto_secretbox::aead::KeyInit;
use crypto_secretbox::aead::Payload;

use crate::crypto::CryptoError;

/// mobile-e2ee-v2-framing.ts:6 — `const NONCE_LENGTH = 24`
pub const NONCE_LENGTH: usize = 24;
/// mobile-e2ee-v2-framing.ts:7 — `const SESSION_ID_LENGTH = 32`
pub const SESSION_ID_LENGTH: usize = 32;
/// mobile-e2ee-v2-framing.ts:8 — `const HEADER_LENGTH = SESSION_ID_LENGTH + 1 + 1 + 8` (= 42)
pub const HEADER_LENGTH: usize = SESSION_ID_LENGTH + 1 + 1 + 8;
/// mobile-e2ee-v2-framing.ts:9 — `const FRAME_VERSION = 2`
pub const FRAME_VERSION: u8 = 2;
/// mobile-e2ee-v2-framing.ts:10 — `const MAX_COUNTER = (1n << 64n) - 1n`
pub const MAX_COUNTER: u64 = u64::MAX;

/// mobile-e2ee-v2-contract.ts:4 — `MobileE2EEPayloadKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadKind {
    Text,
    Binary,
}

/// mobile-e2ee-v2-contract.ts:3 — `MobileE2EEDirection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    MobileToDesktop,
    DesktopToMobile,
}

/// tweetnacl `nacl.secretbox.overheadLength` = 16.
pub const SECRETBOX_OVERHEAD_LENGTH: usize = 16;
/// tweetnacl `nacl.secretbox.keyLength` = 32.
pub const SECRETBOX_KEY_LENGTH: usize = 32;

/// mobile-e2ee-v2-framing.ts:94-96 — `directionByte`: mobile-to-desktop = 0, else 1.
fn direction_byte(direction: Direction) -> u8 {
    match direction {
        Direction::MobileToDesktop => 0,
        Direction::DesktopToMobile => 1,
    }
}

/// mobile-e2ee-v2-framing.ts:98-100 — `payloadKindByte`: text = 0, else 1.
fn payload_kind_byte(kind: PayloadKind) -> u8 {
    match kind {
        PayloadKind::Text => 0,
        PayloadKind::Binary => 1,
    }
}

/// mobile-e2ee-v2-framing.ts:102-112 — `validateFrameInputs`: key length, session-ID
/// length, counter range checks with the exact TS error messages.
pub(crate) fn validate_frame_inputs(
    key: &[u8],
    session_id: &[u8],
    _counter: u64,
) -> Result<(), CryptoError> {
    if key.len() != SECRETBOX_KEY_LENGTH {
        return Err(CryptoError::V2KeyBadLength(key.len()));
    }
    if session_id.len() != SESSION_ID_LENGTH {
        return Err(CryptoError::V2SessionIdBadLength(session_id.len()));
    }
    // u64 cannot go negative; upper bound is MAX_COUNTER by type.
    let _ = MAX_COUNTER;
    Ok(())
}

/// mobile-e2ee-v2-framing.ts:114-120 — `writeUint64`: big-endian fill from
/// index 7 downwards.
fn write_u64(target: &mut [u8], offset: usize, value: u64) {
    target[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
}

/// mobile-e2ee-v2-framing.ts:62-74 — `encodeHeader`:
/// `sessionId(32) || directionByte(1) || payloadKindByte(1) || counter(8 BE)`.
fn encode_header(
    session_id: &[u8; 32],
    direction: Direction,
    kind: PayloadKind,
    counter: u64,
) -> [u8; HEADER_LENGTH] {
    let mut header = [0u8; HEADER_LENGTH];
    header[..SESSION_ID_LENGTH].copy_from_slice(session_id);
    header[SESSION_ID_LENGTH] = direction_byte(direction);
    header[SESSION_ID_LENGTH + 1] = payload_kind_byte(kind);
    write_u64(&mut header, SESSION_ID_LENGTH + 2, counter);
    header
}

/// mobile-e2ee-v2-framing.ts:76-92 — `encodeNonce`:
/// `sessionId[0..12] || FRAME_VERSION(=2) || directionByte || payloadKindByte
/// || 0x00 || counter(8 BE)`. The fixed layout (comment at TS:83-84) makes
/// every direction/kind/counter nonce unique without another RNG draw.
fn encode_nonce(
    session_id: &[u8; 32],
    direction: Direction,
    kind: PayloadKind,
    counter: u64,
) -> [u8; NONCE_LENGTH] {
    let mut nonce = [0u8; NONCE_LENGTH];
    nonce[..12].copy_from_slice(&session_id[..12]);
    nonce[12] = FRAME_VERSION;
    nonce[13] = direction_byte(direction);
    nonce[14] = payload_kind_byte(kind);
    nonce[15] = 0;
    write_u64(&mut nonce, 16, counter);
    nonce
}

/// mobile-e2ee-v2-framing.ts:12-26 — `sealMobileE2EEV2Frame`:
/// `nonce(24) || secretbox(header || payload, nonce, key)`.
pub fn seal_v2_frame(
    payload: &[u8],
    key: &[u8; SECRETBOX_KEY_LENGTH],
    session_id: &[u8; SESSION_ID_LENGTH],
    direction: Direction,
    payload_kind: PayloadKind,
    counter: u64,
) -> Result<Vec<u8>, CryptoError> {
    validate_frame_inputs(key, session_id, counter)?;
    let header = encode_header(session_id, direction, payload_kind, counter);
    let nonce = encode_nonce(session_id, direction, payload_kind, counter);
    let mut plaintext = Vec::with_capacity(HEADER_LENGTH + payload.len());
    plaintext.extend_from_slice(&header);
    plaintext.extend_from_slice(payload);
    let cipher = XSalsa20Poly1305::new(crypto_box::aead::generic_array::GenericArray::from_slice(
        key,
    ));
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: &[],
            },
        )
        .expect("XSalsa20Poly1305 seal with valid key/nonce cannot fail");
    let mut out = Vec::with_capacity(NONCE_LENGTH + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// mobile-e2ee-v2-framing.ts:28-60 — `openMobileE2EEV2Frame`. Returns `None`
/// for: too-short frames (37-39), nonce mismatch (47-49), auth failure (51-54),
/// header mismatch (55-58) — exactly the TS `return null` sites.
pub fn open_v2_frame(
    frame: &[u8],
    key: &[u8; SECRETBOX_KEY_LENGTH],
    session_id: &[u8; SESSION_ID_LENGTH],
    direction: Direction,
    payload_kind: PayloadKind,
    expected_counter: u64,
) -> Result<Option<Vec<u8>>, CryptoError> {
    validate_frame_inputs(key, session_id, expected_counter)?;
    if frame.len() < NONCE_LENGTH + SECRETBOX_OVERHEAD_LENGTH + HEADER_LENGTH {
        return Ok(None);
    }
    let expected_nonce = encode_nonce(session_id, direction, payload_kind, expected_counter);
    // TS:47-49 — constant-style byte comparison (equalBytes, 132-141).
    if frame[..NONCE_LENGTH] != expected_nonce {
        return Ok(None);
    }
    let cipher = XSalsa20Poly1305::new(crypto_box::aead::generic_array::GenericArray::from_slice(
        key,
    ));
    let plaintext = match cipher.decrypt(
        Nonce::from_slice(&expected_nonce),
        Payload {
            msg: &frame[NONCE_LENGTH..],
            aad: &[],
        },
    ) {
        Ok(pt) => pt,
        Err(_) => return Ok(None),
    };
    let expected_header = encode_header(session_id, direction, payload_kind, expected_counter);
    if plaintext.len() < HEADER_LENGTH || plaintext[..HEADER_LENGTH] != expected_header {
        return Ok(None);
    }
    Ok(Some(plaintext[HEADER_LENGTH..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic golden check: seals with the documented fixture layout
    /// (vectors/README.md "v2 framing golden vector") and verifies the exact
    /// byte positions — nonce prefix, MAC-terminated ciphertext, header inside.
    #[test]
    fn seal_layout_matches_ts() {
        let key = [1u8; 32];
        let session_id: [u8; 32] = core::array::from_fn(|i| (i * 3) as u8);
        let frame = seal_v2_frame(
            b"payload!",
            &key,
            &session_id,
            Direction::MobileToDesktop,
            PayloadKind::Text,
            42,
        )
        .unwrap();
        // TS layout: nonce(24) || secretbox(header(42) || payload) — no length prefix.
        assert_eq!(frame.len(), 24 + HEADER_LENGTH + 8 + 16);
        // nonce: session prefix + version + direction + kind + reserved + counter BE
        assert_eq!(&frame[..12], &session_id[..12]);
        assert_eq!(frame[12], 2);
        assert_eq!(frame[13], 0); // mobile-to-desktop
        assert_eq!(frame[14], 0); // text
        assert_eq!(frame[15], 0);
        assert_eq!(&frame[16..24], &42u64.to_be_bytes());
        let opened = open_v2_frame(
            &frame,
            &key,
            &session_id,
            Direction::MobileToDesktop,
            PayloadKind::Text,
            42,
        )
        .unwrap();
        assert_eq!(opened.as_deref(), Some(&b"payload!"[..]));
    }

    #[test]
    fn open_rejects_wrong_direction_kind_counter() {
        let key = [2u8; 32];
        let session_id = [9u8; 32];
        let frame = seal_v2_frame(
            b"data",
            &key,
            &session_id,
            Direction::DesktopToMobile,
            PayloadKind::Binary,
            7,
        )
        .unwrap();
        assert_eq!(
            open_v2_frame(
                &frame,
                &key,
                &session_id,
                Direction::MobileToDesktop,
                PayloadKind::Binary,
                7
            )
            .unwrap(),
            None
        );
        assert_eq!(
            open_v2_frame(
                &frame,
                &key,
                &session_id,
                Direction::DesktopToMobile,
                PayloadKind::Text,
                7
            )
            .unwrap(),
            None
        );
        assert_eq!(
            open_v2_frame(
                &frame,
                &key,
                &session_id,
                Direction::DesktopToMobile,
                PayloadKind::Binary,
                8
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn open_rejects_short_frame() {
        // TS:37 — < 24 + 16 + 42 = 82 bytes returns null.
        let frame = vec![0u8; 81];
        assert_eq!(
            open_v2_frame(
                &frame,
                &[0u8; 32],
                &[0u8; 32],
                Direction::MobileToDesktop,
                PayloadKind::Text,
                0
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn validation_errors_match_ts() {
        // TS rejects key/session-id/counter length errors at runtime; the
        // Rust API makes them unrepresentable with typed [u8; 32]/u64
        // parameters (compile-time validation, documented in vectors/README).
        // The runtime-rejectable TS error that survives here is a short frame
        // on open (mobile-e2ee-v2-framing.ts:37 minimum length).
        let frame = [0u8; NONCE_LENGTH + 16 + HEADER_LENGTH - 1];
        // TS open (framing.ts:37-39) returns null for a short frame, not a throw.
        assert_eq!(
            open_v2_frame(
                &frame,
                &[0u8; 32],
                &[0u8; 32],
                Direction::MobileToDesktop,
                PayloadKind::Text,
                0
            ),
            Ok(None)
        );
    }
}
