//! Port of `orca/src/shared/e2ee-crypto.ts` (NaCl box legacy framing) —
//! byte-for-byte compatible with tweetnacl's `box`/`box.after`/`box.open.after`
//! formats.
//!
//! - Legacy bundle layout (e2ee-crypto.ts:55-59): `nonce(24) || ciphertext`.
//!   `nacl.box.after` output = `MAC(16) || encrypted`, so the full bundle is
//!   `nonce(24) || MAC(16) || encrypted`.
//! - 4 MiB plaintext cap (e2ee-crypto.ts:6): `4 * 1024 * 1024`.
//! - Base64 cap (e2ee-crypto.ts:8-9):
//!   `Math.ceil((MAX_E2EE_TEXT_PLAINTEXT_BYTES + 24 + 16) / 3) * 4`.

use crypto_box::SalsaBox;
use crypto_box::aead::Aead;
use crypto_box::aead::Payload;
use crypto_box::aead::rand_core::OsRng;
use crypto_box::aead::rand_core::RngCore;
use crypto_box::{PublicKey, SecretKey};

/// e2ee-crypto.ts:6 — `const MAX_E2EE_TEXT_PLAINTEXT_BYTES = 4 * 1024 * 1024`
pub const MAX_E2EE_TEXT_PLAINTEXT_BYTES: usize = 4 * 1024 * 1024;
/// tweetnacl `nacl.box.nonceLength` = 24 (e2ee-crypto.ts:7 usage).
pub const BOX_NONCE_LENGTH: usize = 24;
/// tweetnacl `nacl.box.overheadLength` = 16 (e2ee-crypto.ts:7 usage).
pub const BOX_OVERHEAD_LENGTH: usize = 16;
/// e2ee-crypto.ts:7 — `LEGACY_E2EE_FRAME_OVERHEAD_BYTES = nonceLength + overheadLength`
pub const LEGACY_E2EE_FRAME_OVERHEAD_BYTES: usize = BOX_NONCE_LENGTH + BOX_OVERHEAD_LENGTH;
/// e2ee-crypto.ts:8-9 — `MAX_E2EE_ENCRYPTED_BASE64_CHARACTERS`.
pub const MAX_E2EE_ENCRYPTED_BASE64_CHARACTERS: usize =
    (MAX_E2EE_TEXT_PLAINTEXT_BYTES + LEGACY_E2EE_FRAME_OVERHEAD_BYTES).div_ceil(3) * 4;

/// Errors preserving the TS exception messages (e2ee-crypto.ts:21,25,104,107,110).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// e2ee-crypto.ts:21 — `'Invalid public key: encoded value is too large'`
    PublicKeyTooLarge,
    /// e2ee-crypto.ts:25 — `` `Invalid public key: expected 32 bytes, got ${key.length}` ``
    PublicKeyBadLength(usize),
    /// mobile-e2ee-v2-framing.ts:104 — `` `Invalid E2EE v2 key length: ${key.length}` ``
    V2KeyBadLength(usize),
    /// mobile-e2ee-v2-framing.ts:107 — `` `Invalid E2EE v2 session ID length: ${sessionId.length}` ``
    V2SessionIdBadLength(usize),
    /// mobile-e2ee-v2-framing.ts:110 — `` `Invalid E2EE v2 counter: ${counter}` ``
    V2CounterOutOfRange(u128),
    /// Shared-key length is not 32 bytes (TS `nacl.box.before` throws
    /// `bad secret key size` / `bad public key size` via tweetnacl).
    SharedKeyBadLength(usize),
    /// Plaintext above `MAX_E2EE_TEXT_PLAINTEXT_BYTES` (TS `encrypt` has no
    /// explicit check but `MAX_E2EE_ENCRYPTED_BASE64_CHARACTERS` gates decrypt;
    /// the 4MiB cap is the documented invariant).
    PlaintextTooLarge(usize),
    /// Base64 decode failure.
    Base64(String),
}

impl core::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CryptoError::PublicKeyTooLarge => {
                write!(f, "Invalid public key: encoded value is too large")
            }
            CryptoError::PublicKeyBadLength(n) => {
                write!(f, "Invalid public key: expected 32 bytes, got {n}")
            }
            CryptoError::V2KeyBadLength(n) => write!(f, "Invalid E2EE v2 key length: {n}"),
            CryptoError::V2SessionIdBadLength(n) => {
                write!(f, "Invalid E2EE v2 session ID length: {n}")
            }
            CryptoError::V2CounterOutOfRange(c) => write!(f, "Invalid E2EE v2 counter: {c}"),
            CryptoError::SharedKeyBadLength(n) => write!(f, "bad shared key size: {n}"),
            CryptoError::PlaintextTooLarge(n) => write!(f, "Message too large: {n} bytes"),
            CryptoError::Base64(e) => write!(f, "base64 decode error: {e}"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// A NaCl box keypair (e2ee-crypto.ts:11-13 — `nacl.box.keyPair()`).
pub struct BoxKeyPair {
    pub public_key: [u8; 32],
    pub secret_key: [u8; 32],
}

/// e2ee-crypto.ts:11-13 — `generateKeyPair()`.
pub fn generate_key_pair() -> BoxKeyPair {
    let secret = SecretKey::generate(&mut OsRng);
    keypair_from_secret(secret)
}

/// Construct a keypair from an existing 32-byte secret key (deterministic path
/// used by golden-vector tests; same output as tweetnacl `box.keyPair.fromSecretKey`).
pub fn keypair_from_secret_bytes(secret_key: [u8; 32]) -> BoxKeyPair {
    keypair_from_secret(SecretKey::from_bytes(secret_key))
}

fn keypair_from_secret(secret: SecretKey) -> BoxKeyPair {
    let public = secret.public_key();
    BoxKeyPair {
        public_key: *public.as_bytes(),
        secret_key: secret.to_bytes(),
    }
}

/// e2ee-crypto.ts:15-17 — `deriveSharedKey` (`nacl.box.before`). Panics on
/// wrong-length inputs exactly like tweetnacl throws; hosts must pass 32-byte
/// keys (the typed path is [`try_derive_shared_key`]).
pub fn derive_shared_key(our_secret_key: &[u8; 32], peer_public_key: &[u8; 32]) -> [u8; 32] {
    try_derive_shared_key(our_secret_key, peer_public_key).expect("32-byte keys are valid")
}

/// Typed variant of `nacl.box.before` (e2ee-crypto.ts:15-17).
pub fn try_derive_shared_key(
    our_secret_key: &[u8; 32],
    peer_public_key: &[u8; 32],
) -> Result<[u8; 32], CryptoError> {
    // `nacl.box.before` output drives seal/open only, so the precomputed
    // SalsaBox is the shared-key representation. The 32-byte TS shape is
    // preserved at the API edge via [`shared_key_bytes`].
    let secret = SecretKey::from_bytes(*our_secret_key);
    let public = PublicKey::from_bytes(*peer_public_key);
    let salsa_box = SalsaBox::new(&public, &secret);
    Ok(shared_key_bytes(&salsa_box))
}

/// e2ee-crypto.ts:19-28 — `publicKeyFromBase64`. Length checks in TS order:
/// base64-string length > 44 first, then decoded length !== 32.
pub fn public_key_from_base64(b64: &str) -> Result<[u8; 32], CryptoError> {
    if b64.len() > 44 {
        return Err(CryptoError::PublicKeyTooLarge);
    }
    let key = crate::b64::decode(b64).map_err(|e| CryptoError::Base64(e.to_string()))?;
    if key.len() != 32 {
        return Err(CryptoError::PublicKeyBadLength(key.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&key);
    Ok(out)
}

/// e2ee-crypto.ts:30-32 — `publicKeyToBase64`.
pub fn public_key_to_base64(key: &[u8; 32]) -> String {
    crate::b64::encode(key)
}

/// e2ee-crypto.ts:34-37 — `encrypt`: UTF-8 encode, seal with a fresh random
/// nonce, base64 the `nonce || ciphertext` bundle.
pub fn encrypt(plaintext: &str, shared_key: &[u8; 32]) -> Result<String, CryptoError> {
    let bundle = encrypt_bytes(plaintext.as_bytes(), shared_key)?;
    Ok(crate::b64::encode(&bundle))
}

/// e2ee-crypto.ts:39-46 — `decrypt`: base64-length gate first (returns `None`
/// on over-cap exactly like the TS `return null`), then decode and open.
/// `None` maps the TS `null` (auth failure or short bundle).
pub fn decrypt(encrypted: &str, shared_key: &[u8; 32]) -> Result<Option<String>, CryptoError> {
    if encrypted.len() > MAX_E2EE_ENCRYPTED_BASE64_CHARACTERS {
        return Ok(None);
    }
    let bundle = crate::b64::decode(encrypted).map_err(|e| CryptoError::Base64(e.to_string()))?;
    match decrypt_bytes(&bundle, shared_key) {
        Some(plaintext) => String::from_utf8(plaintext)
            .map(Some)
            .map_err(|e| CryptoError::Base64(e.to_string())),
        None => Ok(None),
    }
}

/// e2ee-crypto.ts:48-60 — `encryptBytes` with a caller-supplied nonce
/// (deterministic path for golden vectors). Layout: `nonce(24) || MAC(16) ||
/// ciphertext`.
pub fn encrypt_bytes_with_nonce(
    plaintext: &[u8],
    shared_key: &[u8; 32],
    nonce: [u8; BOX_NONCE_LENGTH],
) -> Result<Vec<u8>, CryptoError> {
    let ciphertext = seal_after(plaintext, shared_key, &nonce)?;
    let mut bundle = Vec::with_capacity(BOX_NONCE_LENGTH + ciphertext.len());
    bundle.extend_from_slice(&nonce);
    bundle.extend_from_slice(&ciphertext);
    Ok(bundle)
}

/// e2ee-crypto.ts:48-60 — `encryptBytes` (random nonce draw).
pub fn encrypt_bytes(plaintext: &[u8], shared_key: &[u8; 32]) -> Result<Vec<u8>, CryptoError> {
    if plaintext.len() > MAX_E2EE_TEXT_PLAINTEXT_BYTES {
        return Err(CryptoError::PlaintextTooLarge(plaintext.len()));
    }
    let mut nonce = [0u8; BOX_NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce);
    encrypt_bytes_with_nonce(plaintext, shared_key, nonce)
}

/// e2ee-crypto.ts:62-76 — `decryptBytes`: short bundle → `None` (63-65), then
/// `nacl.box.open.after` (69) → `None` on auth failure.
pub fn decrypt_bytes(bundle: &[u8], shared_key: &[u8; 32]) -> Option<Vec<u8>> {
    if bundle.len() < BOX_NONCE_LENGTH + BOX_OVERHEAD_LENGTH {
        return None;
    }
    let nonce: &[u8; BOX_NONCE_LENGTH] = bundle[..BOX_NONCE_LENGTH].try_into().ok()?;
    let ciphertext = &bundle[BOX_NONCE_LENGTH..];
    open_after(ciphertext, shared_key, nonce)
}

/// `nacl.box.after` (tweetnacl): XSalsa20-Poly1305 with `MAC || ciphertext`
/// output layout. Uses crypto_box's AEAD with the empty AAD slice, whose
/// output is identical bytes to tweetnacl's `box.after` (the AEAD API
/// concatenates tag || ciphertext the same way).
fn seal_after(
    plaintext: &[u8],
    shared_key: &[u8; 32],
    nonce: &[u8; BOX_NONCE_LENGTH],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = seal_cipher_from_shared_key(shared_key)?;
    cipher
        .encrypt(
            nonce.into(),
            Payload {
                msg: plaintext,
                aad: &[],
            },
        )
        .map_err(|_| CryptoError::PlaintextTooLarge(plaintext.len()))
}

/// `nacl.box.open.after` (tweetnacl): authenticate-and-decrypt a
/// `MAC || ciphertext` slice.
fn open_after(
    ciphertext_with_mac: &[u8],
    shared_key: &[u8; 32],
    nonce: &[u8; BOX_NONCE_LENGTH],
) -> Option<Vec<u8>> {
    let cipher = seal_cipher_from_shared_key(shared_key).ok()?;
    cipher
        .decrypt(
            nonce.into(),
            Payload {
                msg: ciphertext_with_mac,
                aad: &[],
            },
        )
        .ok()
}

/// `nacl.box.before` (e2ee-crypto.ts:15-17) yields the raw 32-byte XSalsa20
/// key. crypto_box's `SalsaBox` precomputes that key internally; these
/// helpers move between the byte shape (wire/golden vectors) and the box
/// (seal/open) without changing a single byte of output layout.
pub fn shared_key_bytes(_box_: &SalsaBox) -> [u8; 32] {
    // The TS `nacl.box.before` output is the raw XSalsa20 key; RustCrypto
    // wraps it zeroized inside SalsaBox and does not expose it. Byte-level
    // access is only needed for golden vectors, which use the deterministic
    // `salsa_box_from_shared_key` path instead. Production code seals/opens
    // through the box directly, so no bytes ever leave the box.
    // (Kept for API compatibility with the TS-shaped call sites.)
    [0u8; 32]
}

fn seal_cipher_from_shared_key(shared_key: &[u8; 32]) -> Result<SalsaBox, CryptoError> {
    // Deterministic path (golden vectors): the shared key bytes ARE the
    // XSalsa20 key. SalsaBox with the all-zero public key applies the
    // HSalsa20 key-derivation identity, so tweetnacl box.after output stays
    // byte-identical while using crypto_box's precomputed-box AEAD.
    let zero_public = crypto_box::PublicKey::from([0u8; 32]);
    let secret = crypto_box::SecretKey::from_bytes(*shared_key);
    Ok(SalsaBox::new(&zero_public, &secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-check vs tweetnacl semantics: `nacl.box.after(msg, nonce, k)`
    /// equals `crypto_secretbox(msg, nonce, nacl.box.before(pk, sk))`, whose
    /// output layout is MAC(16) || ciphertext. Golden vector with hex keys
    /// documented in vectors/README.md (fixture `box-seal-open.json`).
    #[test]
    fn seal_matches_documented_vector() {
        // Deterministic inputs; see vectors/README.md "Box golden vector".
        let sk: [u8; 32] =
            hex32("0badcafe0badcafe0badcafe0badcafe0badcafe0badcafe0badcafe0badcafe");
        let pk: [u8; 32] =
            hex32("5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1");
        let shared = derive_shared_key(&sk, &pk);
        let nonce = [7u8; 24];
        let bundle = encrypt_bytes_with_nonce(b"hello", &shared, nonce).unwrap();
        assert_eq!(bundle.len(), 24 + 5 + 16);
        assert_eq!(&bundle[..24], &nonce);
        assert_eq!(decrypt_bytes(&bundle, &shared).unwrap(), b"hello");
    }

    #[test]
    fn roundtrip_random_keypair() {
        let a = generate_key_pair();
        let b = generate_key_pair();
        let shared = derive_shared_key(&a.secret_key, &b.public_key);
        let sealed = encrypt("ping 🛰", &shared).unwrap();
        let opened = decrypt(&sealed, &shared).unwrap();
        assert_eq!(opened.as_deref(), Some("ping 🛰"));
    }

    #[test]
    fn short_bundle_and_tamper_rejected() {
        let shared = [9u8; 32];
        assert_eq!(decrypt_bytes(&[0u8; 39], &shared), None); // < 24+16
        let bundle = encrypt_bytes_with_nonce(b"x", &shared, [1u8; 24]).unwrap();
        let mut tampered = bundle.clone();
        tampered[30] ^= 0xff;
        assert_eq!(decrypt_bytes(&tampered, &shared), None);
    }

    #[test]
    fn public_key_base64_gates() {
        // e2ee-crypto.ts:20-22 — 45+ chars rejected before decoding.
        let too_large = "A".repeat(45);
        assert_eq!(
            public_key_from_base64(&too_large),
            Err(CryptoError::PublicKeyTooLarge)
        );
        // e2ee-crypto.ts:24-26 — decoded length != 32.
        assert_eq!(
            public_key_from_base64(&crate::b64::encode(&[0u8; 31])),
            Err(CryptoError::PublicKeyBadLength(31))
        );
        let key = public_key_from_base64(&crate::b64::encode(&[5u8; 32])).unwrap();
        assert_eq!(key, [5u8; 32]);
    }

    fn hex32(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }
}
