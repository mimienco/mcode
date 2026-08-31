# mcode-wire golden vectors

Each fixture is consumed by `src/vectors/mod.rs` (`cargo test -p mcode-wire`).
Expectations were computed from the Rust implementation on first authoring
and are cross-checked against the TypeScript source line-by-line; the mapping
for every expectation is listed below. TS paths are relative to the oracle
repo (`D:/ADE/orca`).

## a. `frame-roundtrip.json` — frame encode/decode + continuation splits

- Header layout: `relay/protocol.ts:146-151` (`encodeFrame`): byte 0 = type,
  bytes 1-4 = id BE u32, bytes 5-8 = ack BE u32, bytes 9-12 = payload length
  BE u32, then payload.
- `HEADER_LENGTH = 13`: `shared/relay-frame-decoder.ts:14`.
- `MAX_MESSAGE_SIZE = 16 * 1024 * 1024`: `shared/relay-frame-decoder.ts:15`.
- Split-at-every-header-boundary: the decoder buffers bytes until
  `buffer.length >= HEADER_LENGTH` (decoder.ts:121) and
  `>= HEADER_LENGTH + length` (decoder.ts:137), so a frame split at ANY byte
  boundary decodes identically once all bytes arrive — vectors encode full
  frames and the test re-splits at every offset 1..len-1 and asserts identical
  decode results (continuation splits, decoder.ts:97-165 turn loop).
- Multi-frame ordering: decoder.ts:108-152 — frames decode in feed order.

## b. `box-seal-open.json` — NaCl box legacy framing

- Bundle layout: `shared/e2ee-crypto.ts:55-59` — `nonce(24) || ciphertext`
  where `nacl.box.after` ciphertext = `MAC(16) || encrypted`
  (e2ee-crypto.ts:53). Total overhead beyond plaintext = 40 bytes.
- Fixed keys/nonce (hex, documented so both TS and Rust can reproduce):
  - alice secret: `0badcafe0badcafe0badcafe0badcafe0badcafe0badcafe0badcafe0badcafe`
  - bob public:   `5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1ce5ea1`
  - nonce:        `07070707070707070707070707070707070707070707070707070707070707` (24 bytes of 0x07)
  - plaintext:    `hello`
  - shared key = `nacl.box.before(bobPublic, aliceSecret)` — the Rust fixture
    value was computed via `crypto_box_beforenm`; tweetnacl's `box.after` is
    `crypto_secretbox_xsalsa20poly1305` under the same derived key, so the
    MAC-ciphertext bytes are the standard XSalsa20-Poly1305 output
    (e2ee-crypto.ts:53,69).
- 4 MiB cap: e2ee-crypto.ts:6 (`MAX_E2EE_TEXT_PLAINTEXT_BYTES`).
- Base64 cap formula: e2ee-crypto.ts:8-9 —
  `Math.ceil((4194304 + 24 + 16) / 3) * 4 = 5592448`.
- Base64 framing: e2ee-crypto.ts:36 (`Buffer.toString('base64')`).
- Short-bundle rejection: e2ee-crypto.ts:63-65 — `< nonceLength +
  overheadLength` → null.
- Public-key gates: e2ee-crypto.ts:20-22 (base64 > 44 chars),
  e2ee-crypto.ts:24-26 (decoded != 32 bytes).

## c. `v2-framing.json` — secretbox v2 framing

- Byte layout: `shared/mobile-e2ee-v2-framing.ts:12-26` —
  `nonce(24) || secretbox(header(42) || payload, nonce, key)`.
- Header: framing.ts:62-74 — `sessionId(32) || direction(1) || kind(1) ||
  counter(8 BE)`; `HEADER_LENGTH = 32+1+1+8 = 42` (framing.ts:8).
- Nonce: framing.ts:76-92 — `sessionId[0..12] || 0x02 || directionByte ||
  payloadKindByte || 0x00 || counter(8 BE)`.
- directionByte: framing.ts:94-96 (mobile-to-desktop=0, desktop-to-mobile=1);
  payloadKindByte: framing.ts:98-100 (text=0, binary=1).
- Fixed keys/session (hex, documented):
  - key:        `0101010101010101010101010101010101010101010101010101010101010101`
  - session id: `0202020202020202020202020202020202020202020202020202020202020202`
  - counter: 7, direction `desktop-to-mobile`, kind `binary`, payload `mcode-v2`
- Minimum-frame rejection: framing.ts:37 — `< 24 + 16 + 42 = 82` bytes → null.
- Nonce-mismatch rejection: framing.ts:47-49; header-mismatch: framing.ts:55-58;
  auth failure: framing.ts:51-54. All map to `null`.
- Validation errors: framing.ts:102-112 messages
  (`Invalid E2EE v2 key length: N`, `Invalid E2EE v2 session ID length: N`,
  `Invalid E2EE v2 counter: N`).

## d. `handshake.json` — accept/reject/mismatch

- Sentinels: `relay/protocol.ts:27-28` — `RELAY_VERSION = '0.1.0'`,
  `RELAY_SENTINEL = 'MCODE-RELAY v0.1.0 READY\n'` (23 bytes incl. newline).
- Handshake frame = `encodeFrame(MessageType.Handshake=2, 0, 0, JSON)`
  (protocol.ts:45-48). JSON shapes (protocol.ts:40-43):
  - `{"type":"mcode-relay-handshake","version":"0.1.0"[,"endpointCredential":"..."]}`
    — credential field omitted when absent (relay-handshake.ts:251 spread).
  - `{"type":"mcode-relay-handshake-ok","version":"0.1.0"}`
  - `{"type":"mcode-relay-handshake-mismatch","expected":"0.1.0","got":"9.9.9"}`
- Accept path: relay-handshake.ts:152-154. Version-mismatch reply:
  relay-handshake.ts:130-142 (mismatch frame written best-effort, then close).
- Wrong frame type pre-handshake: relay-handshake.ts:106-112.
- Unknown type parse failure: protocol.ts:53-59 —
  `` `Unknown handshake type: ${t}` ``.
- Credential mismatch: relay-handshake.ts:144-151 (undefined/absent credential
  vs expected → reject). Rust replaces the TS `!==` with subtle
  ConstantTimeEq over UTF-8 bytes — same accept/reject decision, timing-safe.
- `EXIT_CODE_VERSION_MISMATCH = 42`: relay-handshake.ts:17.
- MessageType ids: protocol.ts:30-34 (Regular=1, Handshake=2, KeepAlive=9).

## e. `oversized-frame.json` — oversized frame rejection

- Threshold: decoder.ts:126 — `length > MAX_MESSAGE_SIZE` (strictly greater).
- Behavior: decoder.ts:127-134 — header discarded, oversized counter set,
  `Frame payload too large: ${length} bytes — discarded` error published
  immediately; payload bytes then discarded in bounded slices via
  `discardOversizedPayload` (decoder.ts:167-179): min(remaining, buffered,
  max(1, maxBytesPerTurn - bytes)).
- Retained-input limit: decoder.ts:66-74 — `retained > MAX_RETAINED_BYTES`
  (decoder.ts:19) → reset + `Frame decoder retained-input limit exceeded: N`.
  The fixture uses `MAX_MESSAGE_SIZE + 1` as the declared length so the test
  never allocates an oversized payload.
- Error recovery: after all oversized bytes are discarded, the decoder resumes
  parsing the NEXT frame (decoder.ts:116-119 loop + hasRunnableWork 181-190).

## f. `credential-compare.json` — timing-safe credential compare

- TS decision: relay-handshake.ts:144-151 — reject when expected credential is
  configured and the message credential differs or is absent.
- Rust implements the same decision matrix via `subtle::ConstantTimeEq`
  (brief requirement); vectors enumerate: equal (accept), differing value
  (reject), absent-vs-expected (reject), expected-absent (accept regardless,
  TS:145 gate), unicode/empty values, length differences.
