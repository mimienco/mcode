//! Golden vector tests. Fixtures live beside this file (README.md documents
//! how every expectation maps to the TypeScript source). Fixtures are
//! committed; expectations were computed by this Rust implementation on first
//! authoring and cross-checked against the TS sources line-by-line.

use std::path::Path;

use crate::crypto::{self, CryptoError};
use crate::frame::{
    DecodedFrame, FrameDecoder, FrameDecoderError, HEADER_LENGTH, TurnOutcome, encode_frame,
};
use crate::handshake::{
    ClientHandshakeDecision, ServerHandshakeDecision, client_evaluate_reply,
    client_handshake_frame, read_and_check_sentinel, server_handle_handshake_frame,
};
use crate::protocol::message_type;
use crate::v2::{self, Direction, PayloadKind};

fn fixture(name: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/vectors")
        .join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {name} unreadable: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("fixture {name} invalid: {e}"))
}

fn hex(s: &str) -> Vec<u8> {
    let s = s.trim();
    assert_eq!(s.len() % 2, 0, "odd hex in fixture");
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("bad hex digit"))
        .collect()
}

fn hex_fixed<const N: usize>(s: &str) -> [u8; N] {
    let v = hex(s);
    v.try_into().expect("fixture hex length mismatch")
}

fn decode_all(bytes: &[u8]) -> (Vec<DecodedFrame>, Vec<FrameDecoderError>) {
    let mut decoder = FrameDecoder::new(None, None, None);
    let mut frames = Vec::new();
    let mut errors = Vec::new();
    decoder.feed(bytes).expect("within retained limit");
    loop {
        let outcome = decoder.decode_turn(&mut |f| frames.push(f), &mut |e| errors.push(e));
        match outcome {
            TurnOutcome::Idle => break,
            // Continuations drain synchronously in tests until idle.
            _ => continue,
        }
    }
    (frames, errors)
}

/// (a) frame encode/decode roundtrip + continuation split at every header boundary.
#[test]
fn vector_frame_roundtrip_and_splits() {
    let v = fixture("frame-roundtrip.json");
    assert_eq!(v["header_length"].as_u64(), Some(HEADER_LENGTH as u64));

    for case in v["cases"].as_array().unwrap() {
        let frame_bytes = hex(case["frame_hex"].as_str().unwrap());
        // Decode the documented frame bytes: fields must match the case.
        let (frames, errors) = decode_all(&frame_bytes);
        assert!(errors.is_empty(), "case {}", case["name"]);
        assert_eq!(frames.len(), 1, "case {}", case["name"]);
        assert_eq!(
            frames[0].frame_type,
            case["frame_type"].as_u64().unwrap() as u8
        );
        assert_eq!(frames[0].id, case["id"].as_u64().unwrap() as u32);
        assert_eq!(frames[0].ack, case["ack"].as_u64().unwrap() as u32);
        assert_eq!(
            frames[0].payload,
            hex(case["payload_hex"].as_str().unwrap())
        );
        // Encode from parts reproduces the fixture bytes.
        let encoded = encode_frame(
            case["frame_type"].as_u64().unwrap() as u8,
            case["id"].as_u64().unwrap() as u32,
            case["ack"].as_u64().unwrap() as u32,
            &hex(case["payload_hex"].as_str().unwrap()),
        );
        assert_eq!(encoded, frame_bytes, "case {}", case["name"]);
        // Continuation split at EVERY byte boundary of the frame: same decode.
        for split in 1..frame_bytes.len() {
            let (frames, errors) =
                decode_all(&[&frame_bytes[..split], &frame_bytes[split..]].concat());
            assert!(errors.is_empty(), "split {split} case {}", case["name"]);
            assert_eq!(frames.len(), 1, "split {split} case {}", case["name"]);
            assert_eq!(
                frames[0].payload,
                hex(case["payload_hex"].as_str().unwrap())
            );
        }
    }
}

/// (b) box seal→open with fixed keys/nonce.
#[test]
fn vector_box_seal_open() {
    let v = fixture("box-seal-open.json");
    assert_eq!(
        v["max_e2ee_text_plaintext_bytes"].as_u64(),
        Some(crypto::MAX_E2EE_TEXT_PLAINTEXT_BYTES as u64)
    );
    assert_eq!(
        v["legacy_frame_overhead_bytes"].as_u64(),
        Some(crypto::LEGACY_E2EE_FRAME_OVERHEAD_BYTES as u64)
    );
    assert_eq!(
        v["max_e2ee_encrypted_base64_characters"].as_u64(),
        Some(crypto::MAX_E2EE_ENCRYPTED_BASE64_CHARACTERS as u64)
    );

    let alice_secret: [u8; 32] = hex_fixed(v["alice_secret_hex"].as_str().unwrap());
    let bob_public: [u8; 32] = hex_fixed(v["bob_public_hex"].as_str().unwrap());
    let nonce: [u8; 24] = hex_fixed(v["nonce_hex"].as_str().unwrap());
    let plaintext = v["plaintext_utf8"].as_str().unwrap().as_bytes();

    let shared = crypto::derive_shared_key(&alice_secret, &bob_public);
    let bundle = crypto::encrypt_bytes_with_nonce(plaintext, &shared, nonce).unwrap();
    assert_eq!(bundle.len(), v["bundle_len"].as_u64().unwrap() as usize);
    assert_eq!(
        &bundle[..24],
        &nonce,
        "nonce first per e2ee-crypto.ts:55-59"
    );
    // MAC(16) then ciphertext; opening recovers plaintext.
    assert_eq!(
        crypto::decrypt_bytes(&bundle, &shared).as_deref(),
        Some(plaintext)
    );
    // Deterministic: same inputs → same bundle.
    let again = crypto::encrypt_bytes_with_nonce(plaintext, &shared, nonce).unwrap();
    assert_eq!(bundle, again);

    // Short bundle → None (e2ee-crypto.ts:63-65).
    let short = vec![
        0u8;
        v["tamper_expectations"]["short_bundle_len"]
            .as_u64()
            .unwrap() as usize
    ];
    assert_eq!(crypto::decrypt_bytes(&short, &shared), None);
    // Tampered MAC → None.
    let mut tampered = bundle.clone();
    let mac_index = 24; // first MAC byte
    tampered[mac_index] ^= 0x01;
    assert_eq!(crypto::decrypt_bytes(&tampered, &shared), None);

    // Public-key gates (e2ee-crypto.ts:19-28).
    for case in v["public_key_cases"].as_array().unwrap() {
        let result = crypto::public_key_from_base64(case["input_base64"].as_str().unwrap());
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "case {case}"
        );
    }

    // Base64 framing end-to-end (e2ee-crypto.ts:34-46).
    let sealed = crypto::encrypt("🛰 ping", &shared).unwrap();
    assert_eq!(
        crypto::decrypt(&sealed, &shared).unwrap().as_deref(),
        Some("🛰 ping")
    );
}

/// (c) v2 framing seal/open.
#[test]
fn vector_v2_framing() {
    let v = fixture("v2-framing.json");
    let key: [u8; 32] = hex_fixed(v["key_hex"].as_str().unwrap());
    let session_id: [u8; 32] = hex_fixed(v["session_id_hex"].as_str().unwrap());
    assert_eq!(v["minimum_frame_len"].as_u64(), Some(82));

    for case in v["cases"].as_array().unwrap() {
        let direction = match case["direction"].as_str().unwrap() {
            "mobile-to-desktop" => Direction::MobileToDesktop,
            _ => Direction::DesktopToMobile,
        };
        let kind = match case["payload_kind"].as_str().unwrap() {
            "text" => PayloadKind::Text,
            _ => PayloadKind::Binary,
        };
        let counter = case["counter"].as_u64().unwrap();
        let payload = case["payload_utf8"].as_str().unwrap().as_bytes();

        let frame =
            v2::seal_v2_frame(payload, &key, &session_id, direction, kind, counter).unwrap();
        let expected_nonce = hex(case["expected_nonce_hex"].as_str().unwrap());
        assert_eq!(&frame[..24], &expected_nonce[..], "case {}", case["name"]);
        assert_eq!(frame.len(), 24 + 16 + 42 + payload.len());
        let opened =
            v2::open_v2_frame(&frame, &key, &session_id, direction, kind, counter).unwrap();
        assert_eq!(opened.as_deref(), Some(payload), "case {}", case["name"]);

        // Deterministic: resealing reproduces byte-identical frame.
        let again =
            v2::seal_v2_frame(payload, &key, &session_id, direction, kind, counter).unwrap();
        assert_eq!(frame, again, "case {}", case["name"]);
    }

    // Rejections.
    let counter = v["cases"][0]["counter"].as_u64().unwrap();
    let frame = v2::seal_v2_frame(
        b"mcode-v2",
        &key,
        &session_id,
        Direction::DesktopToMobile,
        PayloadKind::Binary,
        counter,
    )
    .unwrap();
    let short = vec![0u8; 81];
    assert_eq!(
        v2::open_v2_frame(
            &short,
            &key,
            &session_id,
            Direction::DesktopToMobile,
            PayloadKind::Binary,
            counter
        )
        .unwrap(),
        None
    );
    assert_eq!(
        v2::open_v2_frame(
            &frame,
            &key,
            &session_id,
            Direction::MobileToDesktop,
            PayloadKind::Binary,
            counter
        )
        .unwrap(),
        None
    );
    assert_eq!(
        v2::open_v2_frame(
            &frame,
            &key,
            &session_id,
            Direction::DesktopToMobile,
            PayloadKind::Text,
            counter
        )
        .unwrap(),
        None
    );
    assert_eq!(
        v2::open_v2_frame(
            &frame,
            &key,
            &session_id,
            Direction::DesktopToMobile,
            PayloadKind::Binary,
            counter + 1
        )
        .unwrap(),
        None
    );
    let mut tampered = frame.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert_eq!(
        v2::open_v2_frame(
            &tampered,
            &key,
            &session_id,
            Direction::DesktopToMobile,
            PayloadKind::Binary,
            counter
        )
        .unwrap(),
        None
    );

    // Validation errors (framing.ts:104,107): the typed Rust entry points take
    // [u8; 32], so length violations are exercised against the shared
    // validate_frame_inputs seam with slices.
    for err in v["validation_errors"].as_array().unwrap() {
        if let Some(key_len) = err["key_len"].as_u64() {
            assert_eq!(
                v2::validate_frame_inputs(&vec![0u8; key_len as usize], &session_id, 0),
                Err(CryptoError::V2KeyBadLength(key_len as usize))
            );
        }
        if let Some(sid_len) = err["session_id_len"].as_u64() {
            assert_eq!(
                v2::validate_frame_inputs(&key, &vec![0u8; sid_len as usize], 0),
                Err(CryptoError::V2SessionIdBadLength(sid_len as usize))
            );
        }
    }
}

/// (d) handshake accept/reject/mismatch + sentinel.
#[test]
fn vector_handshake() {
    let v = fixture("handshake.json");
    // Sentinel bytes (protocol.ts:28).
    let sentinel = v["sentinel"].as_str().unwrap();
    assert_eq!(sentinel.len(), v["sentinel_len"].as_u64().unwrap() as usize);
    assert_eq!(crate::protocol::RELAY_SENTINEL, sentinel.as_bytes());
    assert_eq!(read_and_check_sentinel(sentinel.as_bytes()), Some(Ok(())));
    assert_eq!(read_and_check_sentinel(&sentinel.as_bytes()[..10]), None);
    assert!(matches!(
        read_and_check_sentinel(b"NOPE-NOPE"),
        Some(Err(_))
    ));
    // Version + message types (protocol.ts:27, 30-34).
    assert_eq!(
        crate::protocol::RELAY_VERSION,
        v["relay_version"].as_str().unwrap()
    );
    let types = &v["message_types"];
    assert_eq!(
        message_type::REGULAR,
        types["Regular"].as_u64().unwrap() as u8
    );
    assert_eq!(
        message_type::HANDSHAKE,
        types["Handshake"].as_u64().unwrap() as u8
    );
    assert_eq!(
        message_type::KEEPALIVE,
        types["KeepAlive"].as_u64().unwrap() as u8
    );

    let launch = crate::protocol::RELAY_VERSION;

    // accept
    let a = &v["accept"];
    let frame = make_frame(a["client_json"].as_str().unwrap());
    let decision = server_handle_handshake_frame(&frame, launch, None);
    let ServerHandshakeDecision::Accept { version, reply } = decision else {
        panic!("expected accept, got {decision:?}");
    };
    assert_eq!(version, launch);
    let reply_json = json_payload(&reply);
    assert_eq!(reply_json, a["reply_json"].as_str().unwrap());

    // accept with matching credential
    let ac = &v["accept_with_credential"];
    let frame = make_frame(ac["client_json"].as_str().unwrap());
    assert!(matches!(
        server_handle_handshake_frame(
            &frame,
            launch,
            Some(ac["expected_credential"].as_str().unwrap())
        ),
        ServerHandshakeDecision::Accept { .. }
    ));

    // version mismatch
    let m = &v["version_mismatch"];
    let frame = make_frame(m["client_json"].as_str().unwrap());
    let decision = server_handle_handshake_frame(&frame, launch, None);
    let ServerHandshakeDecision::Mismatch {
        expected,
        got,
        reply,
    } = decision
    else {
        panic!("expected mismatch, got {decision:?}");
    };
    assert_eq!(expected, m["expected"].as_str().unwrap());
    assert_eq!(got, m["got"].as_str().unwrap());
    assert_eq!(json_payload(&reply), m["reply_json"].as_str().unwrap());

    // wrong frame type
    let w = &v["wrong_frame_type"];
    let frame = DecodedFrame {
        frame_type: w["frame_type"].as_u64().unwrap() as u8,
        id: 0,
        ack: 0,
        payload: b"{}".to_vec(),
    };
    assert_eq!(
        server_handle_handshake_frame(&frame, launch, None),
        ServerHandshakeDecision::WrongFrameType(w["frame_type"].as_u64().unwrap() as u8)
    );

    // unknown type
    let u = &v["unknown_type"];
    let frame = make_frame(u["client_json"].as_str().unwrap());
    let decision = server_handle_handshake_frame(&frame, launch, None);
    let ServerHandshakeDecision::BadHandshake(msg) = decision else {
        panic!("expected bad handshake, got {decision:?}");
    };
    assert!(
        msg.contains(u["error_contains"].as_str().unwrap()),
        "got: {msg}"
    );

    // credential mismatch + absent-vs-expected
    for key in ["credential_mismatch", "credential_absent_vs_expected"] {
        let c = &v[key];
        let frame = make_frame(c["client_json"].as_str().unwrap());
        assert_eq!(
            server_handle_handshake_frame(
                &frame,
                launch,
                Some(c["expected_credential"].as_str().unwrap())
            ),
            ServerHandshakeDecision::CredentialMismatch,
            "{key}"
        );
    }

    // client replies
    let ok = &v["client_reply_ok"];
    let reply = make_frame(ok["reply_json"].as_str().unwrap());
    assert!(matches!(
        client_evaluate_reply(Some(&reply), b"leftover-bytes"),
        ClientHandshakeDecision::Accepted { leftover } if leftover == b"leftover-bytes"
    ));
    let mm = &v["client_reply_mismatch"];
    let reply = make_frame(mm["reply_json"].as_str().unwrap());
    assert_eq!(
        client_evaluate_reply(Some(&reply), &[]),
        ClientHandshakeDecision::Mismatch {
            expected: mm["expected"].as_str().unwrap().to_string(),
            got: mm["got"].as_str().unwrap().to_string()
        }
    );

    // Connect-side first frame JSON shape (relay-handshake.ts:247-253).
    let frame = client_handshake_frame(launch, Some("cred"));
    assert_eq!(
        json_payload(&frame),
        "{\"type\":\"mcode-relay-handshake\",\"version\":\"0.1.0\",\"endpointCredential\":\"cred\"}"
    );
    let frame = client_handshake_frame(launch, None);
    assert_eq!(
        json_payload(&frame),
        "{\"type\":\"mcode-relay-handshake\",\"version\":\"0.1.0\"}"
    );
}

/// (e) oversized frame rejection.
#[test]
fn vector_oversized_frame() {
    let v = fixture("oversized-frame.json");
    assert_eq!(v["max_message_size"].as_u64(), Some(16 * 1024 * 1024));
    assert_eq!(
        v["max_retained_bytes"].as_u64(),
        Some((16 * 1024 * 1024 + 13 + 1024 * 1024) as u64)
    );

    // Case 1: oversized header, then a valid frame after the discard loop.
    let c1 = &v["cases"][0];
    let mut bytes = hex(c1["oversized_frame_prefix_hex"].as_str().unwrap());
    bytes.extend_from_slice(&hex(c1["trailing_valid_frame_hex"].as_str().unwrap()));
    let mut decoder = FrameDecoder::new(None, None, None);
    decoder.feed(&bytes).unwrap();
    let mut frames = Vec::new();
    let mut errors = Vec::new();
    loop {
        let outcome = decoder.decode_turn(&mut |f| frames.push(f), &mut |e| errors.push(e));
        if outcome == TurnOutcome::Idle {
            break;
        }
    }
    let msg = errors.first().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains(c1["expected_error_contains"].as_str().unwrap()),
        "got: {msg}"
    );
    assert_eq!(errors.len(), 1);
    // TS oracle (simulated against relay-frame-decoder.ts): after the oversized
    // header is discarded, the trailing bytes are consumed as oversized-payload
    // garbage — no frame survives this turn.
    assert_eq!(
        frames.len(),
        c1["expected_final_frames"].as_array().unwrap().len()
    );

    // Case 2: feed oversized header then the (never-materialized) payload bytes
    // in bounded chunks — exactly one error, discard proceeds across turns.
    let c2 = &v["cases"][1];
    let mut decoder = FrameDecoder::new(None, None, None);
    let mut errors = Vec::new();
    let mut frames = 0usize;
    let chunk = c2["chunk_size"].as_u64().unwrap() as usize;
    decoder
        .feed(&hex(c2["oversized_frame_prefix_hex"].as_str().unwrap()))
        .unwrap();
    // declared length 16777217 → only feed a bounded slice of "payload" bytes
    // (the fixture guarantees no oversized allocation is needed).
    let declared = c2["oversized_length"].as_u64().unwrap() as usize;
    assert!(declared == 16 * 1024 * 1024 + 1);
    for _ in 0..4 {
        decoder.feed(&vec![0u8; chunk]).unwrap();
        loop {
            let outcome = decoder.decode_turn(&mut |_| frames += 1, &mut |e| errors.push(e));
            if outcome == TurnOutcome::Idle {
                break;
            }
        }
    }
    assert_eq!(
        errors.len(),
        c2["expected_error_count"].as_u64().unwrap() as usize
    );
    assert!(
        errors[0]
            .to_string()
            .contains(c2["expected_error_contains"].as_str().unwrap())
    );

    // Case 3: retained-input limit → reset + error, buffer drained.
    let c3 = &v["cases"][2];
    let oversized_chunk = vec![0u8; c3["chunk_hex_of_zeros_len"].as_u64().unwrap() as usize];
    let mut decoder = FrameDecoder::new(None, None, None);
    let err = decoder.feed(&oversized_chunk).unwrap_err();
    assert!(
        err.to_string()
            .contains(c3["expected_error_contains"].as_str().unwrap())
    );
    assert!(decoder.drain().is_empty(), "reset cleared the buffer");

    // Case 4: boundary — declared length == MAX_MESSAGE_SIZE is NOT oversized
    // (decoder.ts:126 strict >). Feed header declaring exactly 16MiB-13 payload
    // bytes; only feed a small prefix, expect NO oversized error (just an
    // incomplete frame waiting for more bytes).
    let declared = 16 * 1024 * 1024;
    let mut header = encode_frame(message_type::REGULAR, 0, 0, &[]);
    header[9..13].copy_from_slice(&((declared - HEADER_LENGTH) as u32).to_be_bytes());
    let mut decoder = FrameDecoder::new(None, None, None);
    decoder.feed(&header).unwrap();
    let mut errors = Vec::new();
    let outcome = decoder.decode_turn(&mut |_| {}, &mut |e| errors.push(e));
    assert!(errors.is_empty(), "boundary frame must not be oversized");
    assert_eq!(outcome, TurnOutcome::Idle, "incomplete frame waits");
}

/// (f) credential compare decision matrix (timing-safe ConstantTimeEq path).
#[test]
fn vector_credential_compare() {
    let v = fixture("credential-compare.json");
    for case in v["cases"].as_array().unwrap() {
        let expected = case["expected"].as_str();
        let provided = case["provided"].as_str();
        let frame = client_handshake_frame(crate::protocol::RELAY_VERSION, provided);
        let decoded = {
            let mut d = FrameDecoder::new(None, None, None);
            d.feed(&frame).unwrap();
            let mut fs = Vec::new();
            loop {
                let o = d.decode_turn(&mut |f| fs.push(f), &mut |_| {});
                if o == TurnOutcome::Idle {
                    break;
                }
            }
            fs.remove(0)
        };
        let decision =
            server_handle_handshake_frame(&decoded, crate::protocol::RELAY_VERSION, expected);
        let accepted = matches!(decision, ServerHandshakeDecision::Accept { .. });
        assert_eq!(
            accepted,
            case["decision"].as_str().unwrap() == "accept",
            "case {}: {}",
            case["provided"],
            case["why"]
        );
    }
}

fn make_frame(json: &str) -> DecodedFrame {
    let bytes = encode_frame(message_type::HANDSHAKE, 0, 0, json.as_bytes());
    let mut d = FrameDecoder::new(None, None, None);
    d.feed(&bytes).unwrap();
    let mut fs = Vec::new();
    loop {
        let o = d.decode_turn(&mut |f| fs.push(f), &mut |_| {});
        if o == TurnOutcome::Idle {
            break;
        }
    }
    assert_eq!(fs.len(), 1);
    fs.remove(0)
}

fn json_payload(frame: &[u8]) -> String {
    String::from_utf8(frame[HEADER_LENGTH..].to_vec()).unwrap()
}
