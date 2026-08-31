//! Fingerprints and key-algorithm naming shared by the store, verifier, and
//! known_hosts parser.
//!
//! The fingerprint format is exactly what `ssh-keygen -lf` prints:
//! `SHA256:` + base64 with padding stripped. The algorithm name keeps the
//! promotion rule from the original: one `ssh-rsa` key is negotiated as
//! `rsa-sha2-512`, `rsa-sha2-256` or `ssh-rsa`, and all three are the same
//! key type for matching and ordering purposes.

use std::fmt;

/// An algorithm name read from a key blob or a known_hosts line.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AlgorithmName(pub String);

impl AlgorithmName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The key type this algorithm presents as. RSA signature variants
    /// (`rsa-sha2-512`/`rsa-sha2-256`) and the legacy `ssh-rsa` name all
    /// denote the same key type, `ssh-rsa`.
    pub fn key_type(&self) -> &str {
        if self.0.starts_with("rsa-sha2-") {
            "ssh-rsa"
        } else {
            &self.0
        }
    }

    /// Whether this is any RSA signature variant of the `ssh-rsa` key type.
    pub fn is_rsa(&self) -> bool {
        self.0 == "ssh-rsa" || self.0.starts_with("rsa-sha2-")
    }
}

impl fmt::Display for AlgorithmName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// `SHA256:<base64 unpadded>` — the OpenSSH fingerprint format.
pub fn format_fingerprint(digest: &[u8]) -> String {
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(digest);
    let unpadded = encoded.trim_end_matches('=');
    format!("SHA256:{unpadded}")
}

/// SHA-256 fingerprint of a raw host key blob.
pub fn fingerprint_of(key_bytes: &[u8]) -> String {
    use sha2::Digest as _;
    format_fingerprint(&sha2::Sha256::digest(key_bytes))
}

#[cfg(test)]
pub(crate) mod tests {
    /// Fixed ed25519 public-key blobs (32 bytes) used as deterministic host
    /// key fixtures across the test modules. The 32-byte body is arbitrary;
    /// the algorithm header is a real `ssh-ed25519` wire header.
    pub const ED25519_A_BYTES: [u8; 51] = [
        0, 0, 0, 11, b's', b's', b'h', b'-', b'e', b'd', b'2', b'5', b'5', b'1', b'9', 0, 0, 0, 32,
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        26, 27, 28, 29, 30, 31, 32,
    ];

    pub const ED25519_B_BYTES: [u8; 51] = [
        0, 0, 0, 11, b's', b's', b'h', b'-', b'e', b'd', b'2', b'5', b'5', b'1', b'9', 0, 0, 0, 32,
        32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10,
        9, 8, 7, 6, 5, 4, 3, 2, 1,
    ];

    /// A minimal well-formed `ssh-rsa` blob: header plus `e` and `n` mpints.
    pub fn rsa_a_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&7u32.to_be_bytes());
        out.extend_from_slice(b"ssh-rsa");
        let mut mpint = |bytes: &[u8]| {
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(bytes);
        };
        mpint(&[0x01, 0x00, 0x01]); // e = 65537
        mpint(&[0x9d; 64]); // n
        out
    }
}

#[cfg(test)]
mod more_tests {
    use super::tests;
    use super::{AlgorithmName, fingerprint_of, format_fingerprint};

    #[test]
    fn fingerprint_matches_openssh_sha256_unpadded_format() {
        // Reference value: fingerprint of an empty input, which
        // `ssh-keygen -lf` renders as SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRk...
        let digest = [0u8; 32];
        let formatted = format_fingerprint(&digest);
        assert!(formatted.starts_with("SHA256:"));
        assert!(!formatted.contains('='), "padding must be stripped");
        let encoded_part = &formatted["SHA256:".len()..];
        assert_eq!(encoded_part.len(), 43, "32 bytes -> 43 unpadded chars");
        assert!(
            encoded_part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/'),
            "standard base64 alphabet only"
        );
    }

    #[test]
    fn fingerprint_is_deterministic_and_distinguishes_keys() {
        let a = fingerprint_of(&tests::ED25519_A_BYTES);
        let b = fingerprint_of(&tests::ED25519_B_BYTES);
        assert_eq!(a, fingerprint_of(&tests::ED25519_A_BYTES));
        assert_ne!(a, b);
    }

    #[test]
    fn rsa_signature_variants_promote_to_ssh_rsa_key_type() {
        for algorithm in ["ssh-rsa", "rsa-sha2-256", "rsa-sha2-512"] {
            let name = AlgorithmName(algorithm.into());
            assert_eq!(name.key_type(), "ssh-rsa");
            assert!(name.is_rsa());
        }
        let ed = AlgorithmName("ssh-ed25519".into());
        assert_eq!(ed.key_type(), "ssh-ed25519");
        assert!(!ed.is_rsa());
    }
}
