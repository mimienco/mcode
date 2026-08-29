# Relay and Mobile Wire Protocol

Tauri IPC is not the relay protocol. Existing mobile clients must continue to communicate with the Rust host independently of desktop shell changes.

## Compatibility rules

- Preserve message types, identifiers, serialization, crypto primitives, pairing semantics, and ordering rules until versioned negotiation says otherwise.
- Introduce explicit protocol versions and capability negotiation before adding incompatible fields or behavior.
- Unknown optional capabilities are ignored safely; unsupported required capabilities fail with a stable reason.
- Pairing and session keys never enter renderer state or logs.
- Terminal streams carry sequence and recovery semantics compatible with the existing clients.

## Required evidence

- Golden vectors for QR/pairing payloads, handshake, key derivation/encryption, message serialization, invalid input, and replay handling.
- Existing mobile client against the new host and updated mobile client against the previous Electron host.
- Disconnect/reconnect, independently updated clients, version skew, corrupted messages, and capability downgrade tests.

The current protocol schema and crypto call sites still require line-by-line extraction from `src/relay`, `src/shared`, and `mobile`; no protocol redesign is authorized before those vectors exist.
