use minisign_verify::{PublicKey, Signature};

const SIGNED_FIXTURE: &[u8] = b"mcode-tauri-updater-signature-fixture-v1";

fn test_only_signature() -> (PublicKey, Signature) {
    let public_key = PublicKey::decode(include_str!("fixtures/test-only-updater.pub")).unwrap();
    let signature = Signature::decode(include_str!("fixtures/test-only-artifact.sig")).unwrap();
    (public_key, signature)
}

#[test]
fn tauri_generated_minisign_fixture_verifies_unchanged_artifact_bytes() {
    let (public_key, signature) = test_only_signature();

    public_key
        .verify(SIGNED_FIXTURE, &signature, true)
        .expect("Tauri-generated updater signature must verify");
}

#[test]
fn tauri_generated_minisign_fixture_rejects_modified_artifact_bytes() {
    let (public_key, signature) = test_only_signature();
    let mut modified = SIGNED_FIXTURE.to_vec();
    modified.push(b'!');

    assert!(public_key.verify(&modified, &signature, true).is_err());
}
