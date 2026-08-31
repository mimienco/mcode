//! Pairing offer encode/decode — byte-compatible port of
//! `src/shared/pairing.ts` + `src/shared/mobile-relay-pairing-offer.ts`.
//!
//! The wire format is frozen: Android camera intents deep-link
//! `mcode://pair?code=<base64url(json)>` into the mobile client, and
//! independently updated mobile/hosts must interoperate. Golden vectors
//! cover encode, decode, and the rejection reasons.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD as BASE64URL;
use serde::{Deserialize, Serialize};

/// mobile-relay-pairing-offer.ts — `PAIRING_OFFER_VERSION`.
pub const PAIRING_OFFER_VERSION: u32 = 2;
/// mobile-pairing-protocol-limits.ts.
pub const PAIRING_ENDPOINT_MAX_CHARACTERS: usize = 16 * 1024;
/// mobile-pairing-protocol-limits.ts.
pub const PAIRING_DEVICE_TOKEN_MAX_CHARACTERS: usize = 64 * 1024;
/// mobile-pairing-protocol-limits.ts.
pub const PAIRING_PUBLIC_KEY_MAX_CHARACTERS: usize = 4 * 1024;
/// pairing.ts — deep-link scheme+host (only these carry runtime auth).
pub const PAIRING_URL_PREFIX: &str = "mcode://pair?code=";
/// Invite TTL and clock-skew leeway (mobile-relay-pairing-offer.ts).
pub const MAX_INVITE_TTL_MS: i64 = 10 * 60 * 1000;
pub const INVITE_EXPIRY_CLOCK_SKEW_MS: i64 = 30 * 1000;

/// Typed mirror of the zod `PairingOfferSchema` — every field the mobile
/// client reads, with the same presence rules. `relay` is mobile-scope only.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingOffer {
    pub v: u32,
    pub endpoint: String,
    pub device_token: String,
    /// Desktop's Curve25519 public key, pinned by the offer (standard base64
    /// with padding, 32 bytes — `isCanonicalBase64Key` in the TS schema).
    pub public_key_b64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paired_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<RelayV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayV1 {
    pub v: u32,
    pub director_url: String,
    pub invite: RelayInvite,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayInvite {
    pub token: String,
    /// Host-proof freshness: expiry stamped by the cell's own clock.
    pub expiry_ms: i64,
    pub relay_host_id_b64: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingError {
    /// pairing.ts — size gate on the encoded offer.
    OfferTooLarge,
    /// pairing.ts — URL shape violations.
    InvalidUrl,
    /// Base64url decode failure.
    BadEncoding,
    /// JSON shape/limit violations from the zod schema.
    Schema(&'static str),
}

impl core::fmt::Display for PairingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OfferTooLarge => write!(f, "Pairing offer exceeds safe size"),
            Self::InvalidUrl => {
                write!(
                    f,
                    "Invalid pairing URL: must start with mcode://pair and include a pairing code"
                )
            }
            Self::BadEncoding => write!(f, "Invalid pairing code encoding"),
            Self::Schema(detail) => write!(f, "Invalid pairing offer: {detail}"),
        }
    }
}

impl std::error::Error for PairingError {}

fn is_canonical_base64_key(value: &str) -> bool {
    // mobile-relay-pairing-offer.ts `isCanonicalBase64Key`: exactly 43 chars +
    // '=', decoding to exactly 32 bytes, and re-encoding is byte-identical.
    if value.len() != 44 || !value.ends_with('=') {
        return false;
    }
    if !value[..43]
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
    {
        return false;
    }
    use base64::Engine as _;
    match base64::engine::general_purpose::STANDARD.decode(value) {
        Ok(decoded) if decoded.len() == 32 => {
            base64::engine::general_purpose::STANDARD.encode(&decoded) == value
        }
        _ => false,
    }
}

fn is_canonical_https_origin(value: &str) -> bool {
    // Byte-length limit uses UTF-8 length, mirroring TextEncoder.
    if value.len() > 512 && value.len() > PAIRING_ENDPOINT_MAX_CHARACTERS {
        return false;
    }
    match url::Url::parse(value) {
        Ok(parsed) => {
            parsed.scheme() == "https"
                && parsed.as_str().trim_end_matches('/') == value.trim_end_matches('/')
                && value == parsed.origin().ascii_serialization()
        }
        Err(_) => false,
    }
}

/// Validate against the zod schema semantics (mobile-relay-pairing-offer.ts).
fn validate(offer: &PairingOffer, now_ms: i64) -> Result<(), PairingError> {
    if offer.v != PAIRING_OFFER_VERSION {
        return Err(PairingError::Schema("v must be 2"));
    }
    if offer.endpoint.is_empty() || offer.endpoint.len() > PAIRING_ENDPOINT_MAX_CHARACTERS {
        return Err(PairingError::Schema("endpoint length out of range"));
    }
    if offer.device_token.is_empty()
        || offer.device_token.len() > PAIRING_DEVICE_TOKEN_MAX_CHARACTERS
    {
        return Err(PairingError::Schema("deviceToken length out of range"));
    }
    if !is_canonical_base64_key(&offer.public_key_b64) {
        return Err(PairingError::Schema(
            "publicKeyB64 must be canonical base64 of 32 bytes",
        ));
    }
    if let Some(scope) = &offer.scope
        && scope != "mobile"
        && scope != "runtime"
    {
        return Err(PairingError::Schema("scope must be mobile or runtime"));
    }
    if let Some(relay) = &offer.relay {
        if offer.scope.as_deref() == Some("runtime") {
            // relay v1 is mobile-only (superRefine in the schema).
            return Err(PairingError::Schema("relay is mobile-only"));
        }
        if relay.v != 1 {
            return Err(PairingError::Schema("relay.v must be 1"));
        }
        if !is_canonical_https_origin(&relay.director_url) {
            return Err(PairingError::Schema(
                "relay.directorUrl must be a canonical https origin",
            ));
        }
        if relay.invite.expiry_ms <= now_ms - INVITE_EXPIRY_CLOCK_SKEW_MS {
            return Err(PairingError::Schema("relay invite expired"));
        }
        if relay.invite.expiry_ms > now_ms + MAX_INVITE_TTL_MS + INVITE_EXPIRY_CLOCK_SKEW_MS {
            return Err(PairingError::Schema(
                "relay invite expiry too far in the future",
            ));
        }
        if relay.invite.relay_host_id_b64.is_empty()
            || relay.invite.relay_host_id_b64.len() > PAIRING_PUBLIC_KEY_MAX_CHARACTERS
        {
            return Err(PairingError::Schema(
                "relay.relayHostIdB64 length out of range",
            ));
        }
    }
    Ok(())
}

/// pairing.ts:14-29 — `encodePairingOffer`: JSON → base64url (unpadded) →
/// `mcode://pair?code=…`, with the size gate.
pub fn encode_pairing_offer(offer: &PairingOffer, now_ms: i64) -> Result<String, PairingError> {
    validate(offer, now_ms)?;
    let json = serde_json::to_vec(offer).map_err(|_| PairingError::Schema("serialize failed"))?;
    let code = BASE64URL.encode(&json);
    if code.len() > 128 * 1024 {
        return Err(PairingError::OfferTooLarge);
    }
    Ok(format!("{PAIRING_URL_PREFIX}{code}"))
}

/// pairing.ts:31-37 — `decodePairingOffer`.
pub fn decode_pairing_offer(url: &str) -> Result<PairingOffer, PairingError> {
    if url.len() > (128 * 1024) + 1024 {
        return Err(PairingError::InvalidUrl);
    }
    let code = extract_pairing_code_from_url(url).ok_or(PairingError::InvalidUrl)?;
    let json = BASE64URL
        .decode(code.as_bytes())
        .map_err(|_| PairingError::BadEncoding)?;
    let offer: PairingOffer =
        serde_json::from_slice(&json).map_err(|_| PairingError::BadEncoding)?;
    Ok(offer)
}

/// pairing.ts:41-66 — only `mcode://pair` with `?code=` (or `#code`) carries
/// runtime auth material; the query-param form wins.
fn extract_pairing_code_from_url(url: &str) -> Option<&str> {
    // The fast path: PAIRING_URL_PREFIX already consumes `mcode://pair?code=`,
    // so the remainder IS the code (base64url can never contain `#`).
    if let Some(code) = url.strip_prefix(PAIRING_URL_PREFIX) {
        if code.is_empty() {
            return None;
        }
        return Some(code.split('#').next().unwrap_or(code));
    }
    // Hash form: `mcode://pair#code` (no query).
    let (scheme_host, fragment) = url.split_once('#')?;
    if scheme_host != "mcode://pair" {
        return None;
    }
    let code = fragment.strip_prefix("code=").unwrap_or(fragment);
    if code.is_empty() {
        return None;
    }
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    fn base_offer() -> PairingOffer {
        PairingOffer {
            v: 2,
            endpoint: "ws://127.0.0.1:45678".into(),
            device_token: "tok-123".into(),
            // 44-char canonical standard base64 of 32 arbitrary bytes.
            public_key_b64: base64::engine::general_purpose::STANDARD.encode([7u8; 32]),
            paired_device_id: None,
            scope: Some("mobile".into()),
            relay: None,
        }
    }

    #[test]
    fn encode_produces_the_deep_link_shape() {
        let url = encode_pairing_offer(&base_offer(), NOW).unwrap();
        assert!(url.starts_with("mcode://pair?code="));
        assert!(!url[18..].contains('='), "base64url is unpadded");
    }

    #[test]
    fn decode_roundtrips_encode() {
        let offer = base_offer();
        let url = encode_pairing_offer(&offer, NOW).unwrap();
        assert_eq!(decode_pairing_offer(&url).unwrap(), offer);
    }

    #[test]
    fn rejects_wrong_scheme_and_missing_code() {
        assert_eq!(
            decode_pairing_offer("mcode://pairing?code=abc").unwrap_err(),
            PairingError::InvalidUrl
        );
        assert_eq!(
            decode_pairing_offer("mcode://pair").unwrap_err(),
            PairingError::InvalidUrl
        );
        assert_eq!(
            decode_pairing_offer("https://pair?code=abc").unwrap_err(),
            PairingError::InvalidUrl
        );
    }

    #[test]
    fn rejects_noncanonical_public_key() {
        let mut offer = base_offer();
        offer.public_key_b64 = "not-canonical".into();
        assert!(matches!(
            encode_pairing_offer(&offer, NOW),
            Err(PairingError::Schema(_))
        ));
    }

    #[test]
    fn rejects_relay_on_runtime_scope() {
        let mut offer = base_offer();
        offer.scope = Some("runtime".into());
        offer.relay = Some(RelayV1 {
            v: 1,
            director_url: "https://relay.example.com".into(),
            invite: RelayInvite {
                token: "t".into(),
                expiry_ms: NOW + 60_000,
                relay_host_id_b64: "aGVsbG8".into(),
            },
        });
        assert_eq!(
            encode_pairing_offer(&offer, NOW).unwrap_err(),
            PairingError::Schema("relay is mobile-only")
        );
    }

    #[test]
    fn accepts_mobile_relay_with_fresh_invite() {
        let mut offer = base_offer();
        offer.relay = Some(RelayV1 {
            v: 1,
            director_url: "https://relay.example.com".into(),
            invite: RelayInvite {
                token: "t".into(),
                expiry_ms: NOW + 60_000,
                relay_host_id_b64: "aGVsbG8".into(),
            },
        });
        assert!(encode_pairing_offer(&offer, NOW).is_ok());
    }

    #[test]
    fn rejects_stale_invite_within_skew_window() {
        let mut offer = base_offer();
        offer.relay = Some(RelayV1 {
            v: 1,
            director_url: "https://relay.example.com".into(),
            invite: RelayInvite {
                token: "t".into(),
                // Older than the 30s skew allowance → expired.
                expiry_ms: NOW - INVITE_EXPIRY_CLOCK_SKEW_MS - 1,
                relay_host_id_b64: "aGVsbG8".into(),
            },
        });
        assert_eq!(
            encode_pairing_offer(&offer, NOW).unwrap_err(),
            PairingError::Schema("relay invite expired")
        );
    }

    #[test]
    fn deep_link_hash_form_decodes() {
        // The original also accepts `mcode://pair#code`; keep parity.
        let offer = base_offer();
        let json = serde_json::to_vec(&offer).unwrap();
        let code = BASE64URL.encode(&json);
        let url = format!("mcode://pair#{code}");
        assert_eq!(decode_pairing_offer(&url).unwrap(), offer);
    }
}
