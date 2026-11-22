//! MCode's own accepted-host-key store: `ssh-host-keys.json` beside the profile data.
//!
//! Mirrors `ssh-host-key-store.ts`: records scoped to host + port + key type,
//! validated on load (a fingerprint that disagrees with its key is dropped),
//! versioned, and replaced atomically so a crash mid-write can never publish a
//! half-written trust list.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SshError;

pub const STORE_VERSION: u32 = 1;
const MAX_STORE_BYTES: u64 = 1024 * 1024;
const MAX_PORT: u32 = 65535;

/// One accepted host key. `key_base64` is the raw wire blob exactly as the
/// server presented it; `fingerprint` is the SHA-256 OpenSSH fingerprint that
/// must always agree with it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrustedHostKeyRecord {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub key_base64: String,
    pub fingerprint: String,
    pub accepted_at: String,
}

#[derive(Serialize, Deserialize)]
struct HostKeyStoreFile {
    version: u32,
    hosts: HashMap<String, TrustedHostKeyRecord>,
}

/// Endpoint key inside the store. Normalized host: lowercase host, port always
/// included — the store only ever records the endpoint actually connected to,
/// so unlike `known_hosts` there is no bare-host fallback.
pub fn endpoint_key(host: &str, port: u16) -> String {
    format!("{}:{}", host.trim().to_lowercase(), port)
}

/// The `algorithm` field used by the store. One `ssh-rsa` key is negotiated as
/// `rsa-sha2-512`, `rsa-sha2-256` or `ssh-rsa`; the stored algorithm is the
/// key's own type so all three signature variants match the same record.
pub fn store_algorithm_name(key_algorithm: &str) -> String {
    if key_algorithm.starts_with("rsa-sha2-") {
        "ssh-rsa".to_string()
    } else {
        key_algorithm.to_string()
    }
}

pub struct HostKeyStore {
    path: PathBuf,
}

/// What one lookup against store records found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreOutcome {
    Match,
    Mismatch,
    /// We hold a key for this host, just not of the presented type.
    UnknownTypeKnownHost,
    Unknown,
}

impl HostKeyStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Load and validate every record. A corrupt file degrades to empty — that
    /// costs a first-contact prompt, where refusing to load would wedge the
    /// store; a *newer* version on disk is left intact and reported as
    /// [`StoreError::Withheld`] so callers refuse to overwrite it.
    pub fn load(&self) -> Result<Vec<TrustedHostKeyRecord>, StoreError> {
        let contents = match fs::read(&self.path) {
            Ok(bytes) => {
                if bytes.len() as u64 > MAX_STORE_BYTES {
                    return Err(StoreError::Withheld("it exceeds the size limit".into()));
                }
                bytes
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(StoreError::Withheld(format!(
                    "it could not be read: {error}"
                )));
            }
        };

        let parsed: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_slice(&contents);
        let value = match parsed {
            // Unparseable records are unrecoverable by any version, so there is
            // nothing left for a rewrite to destroy.
            Err(_) => return Ok(Vec::new()),
            Ok(value) => value,
        };
        if let Some(version) = value.get("version").and_then(serde_json::Value::as_u64)
            && version > u64::from(STORE_VERSION)
        {
            return Err(StoreError::Withheld(format!("it is version {version}")));
        }
        let Some(hosts) = value.get("hosts").and_then(serde_json::Value::as_object) else {
            return Ok(Vec::new());
        };
        let mut records = Vec::with_capacity(hosts.len());
        for record in hosts.values() {
            if let Ok(record) = serde_json::from_value::<TrustedHostKeyRecord>(record.clone())
                && let Some(valid) = validate_record(record)
            {
                records.push(valid);
            }
        }
        Ok(records)
    }

    /// Look up the presented key. Scoped to host + port + key type, exact —
    /// matching the store algorithm name, so an `rsa-sha2-512` offer matches a
    /// record saved from an `ssh-rsa` negotiation and vice versa.
    pub fn lookup(
        records: &[TrustedHostKeyRecord],
        host: &str,
        port: u16,
        key_algorithm: &str,
        key_bytes: &[u8],
    ) -> StoreOutcome {
        let wanted = endpoint_key(host, port);
        let wanted_algorithm = store_algorithm_name(key_algorithm);
        let mut saw_same_type = false;
        let mut saw_other_type = false;
        for record in records {
            if endpoint_key(&record.host, record.port) != wanted {
                continue;
            }
            if record.algorithm != wanted_algorithm {
                saw_other_type = true;
                continue;
            }
            match decode_record_key(record) {
                Some(bytes) if bytes == key_bytes => return StoreOutcome::Match,
                _ => saw_same_type = true,
            }
        }
        if saw_same_type {
            return StoreOutcome::Mismatch;
        }
        if saw_other_type {
            return StoreOutcome::UnknownTypeKnownHost;
        }
        StoreOutcome::Unknown
    }

    /// Record an accepted key, superseding any earlier record for the same
    /// host + port + key type. Read-modify-write inside one call; the caller
    /// serializes concurrent accepts.
    pub fn accept(
        &self,
        host: &str,
        port: u16,
        algorithm: &str,
        key_base64: &str,
        fingerprint: &str,
        accepted_at: String,
    ) -> Result<TrustedHostKeyRecord, SshError> {
        if host.trim().is_empty() {
            return Err(SshError::InvalidArgument("host"));
        }
        if port == 0 || port as u32 > MAX_PORT {
            return Err(SshError::InvalidArgument("port"));
        }
        let record = TrustedHostKeyRecord {
            host: host.trim().to_lowercase(),
            port,
            algorithm: algorithm.to_string(),
            key_base64: key_base64.to_string(),
            fingerprint: fingerprint.to_string(),
            accepted_at,
        };
        if validate_record(record.clone()).is_none() {
            return Err(SshError::Protocol(
                "refusing to store a host key record that fails validation".into(),
            ));
        }

        let mut records = match self.load() {
            Ok(records) => records,
            Err(StoreError::Corrupt) => Vec::new(),
            // Not a failure of the accept itself: the key verified, we simply
            // decline to rewrite a file whose current contents we cannot see.
            Err(StoreError::Withheld(reason)) => {
                return Err(SshError::Protocol(format!(
                    "host key store left untouched because {reason}"
                )));
            }
        };
        let wanted = endpoint_key(host, port);
        let wanted_algorithm = store_algorithm_name(algorithm);
        records.retain(|existing| {
            !(endpoint_key(&existing.host, existing.port) == wanted
                && store_algorithm_name(&existing.algorithm) == wanted_algorithm)
        });
        records.push(record.clone());
        self.persist(&records)?;
        Ok(record)
    }

    /// Temp file + fsync + rename, so a crash mid-write can never publish a
    /// half-written trust list (same shape as mcode-fs `write_file_atomic`).
    fn persist(&self, records: &[TrustedHostKeyRecord]) -> Result<(), SshError> {
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(SshError::InvalidArgument("store path has no parent"))?;
        fs::create_dir_all(parent).map_err(|error| {
            SshError::Protocol(format!(
                "could not create host key store directory: {error}"
            ))
        })?;

        let mut file: HashMap<String, &TrustedHostKeyRecord> = HashMap::new();
        for record in records {
            file.insert(endpoint_key(&record.host, record.port), record);
        }
        let serialized = serde_json::to_string_pretty(&HostKeyStoreFile {
            version: STORE_VERSION,
            hosts: file
                .into_iter()
                .map(|(key, record)| (key, record.clone()))
                .collect(),
        })
        .map_err(|error| {
            SshError::Protocol(format!("could not serialize host key store: {error}"))
        })?;

        let temp_path = parent.join(format!(
            "{}.tmp-{}",
            self.path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "ssh-host-keys.json".into()),
            std::process::id()
        ));
        {
            let mut temp = fs::File::create(&temp_path).map_err(|error| {
                SshError::Protocol(format!("could not open host key store temp file: {error}"))
            })?;
            temp.write_all(serialized.as_bytes()).map_err(|error| {
                SshError::Protocol(format!("could not write host key store temp file: {error}"))
            })?;
            temp.sync_all().map_err(|error| {
                SshError::Protocol(format!("could not flush host key store temp file: {error}"))
            })?;
        }
        match fs::rename(&temp_path, &self.path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                Err(SshError::Protocol(format!(
                    "could not replace host key store: {error}"
                )))
            }
        }
    }
}

/// Why a store read could not produce trustworthy contents.
#[derive(Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The file parsed but was unusable; treated as empty on read.
    Corrupt,
    /// The file exists and holds records we could not see, so a rewrite would
    /// delete keys rather than supersede them.
    Withheld(String),
}

/// A record whose fingerprint disagrees with its key was corrupted or
/// hand-edited; it is dropped rather than trusted.
pub fn validate_record(record: TrustedHostKeyRecord) -> Option<TrustedHostKeyRecord> {
    if record.host.is_empty()
        || record.port == 0
        || record.port as u32 > MAX_PORT
        || record.algorithm.is_empty()
        || record.key_base64.is_empty()
        || record.fingerprint.is_empty()
    {
        return None;
    }
    let key = decode_record_key(&record)?;
    if crate::fingerprint::fingerprint_of(&key) != record.fingerprint {
        return None;
    }
    Some(record)
}

fn decode_record_key(record: &TrustedHostKeyRecord) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    let key = engine
        .decode(record.key_base64.trim())
        .ok()
        .filter(|key| !key.is_empty())?;
    if read_wire_algorithm(&key)?.as_str() != record.algorithm {
        return None;
    }
    Some(key)
}

/// Read the algorithm name from a key blob's own length-prefixed header — the
/// same check `readHostKeyType` does in the original.
pub fn read_wire_algorithm(key: &[u8]) -> Option<crate::fingerprint::AlgorithmName> {
    let mut cursor = 0usize;
    let read_u32 = |cursor: &mut usize| -> Option<u32> {
        let slice = key.get(*cursor..*cursor + 4)?;
        *cursor += 4;
        Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
    };
    let name_length = read_u32(&mut cursor)? as usize;
    let name = key.get(cursor..cursor.checked_add(name_length)?)?;
    cursor += name_length;
    if cursor > key.len() {
        return None;
    }
    // An SSH key blob is `string alg-name || alg-specific body`; the body
    // layout differs per algorithm (certificates add fields), so the rest is
    // opaque here. Require a non-empty body and no dangling length prefix.
    if cursor == key.len() {
        return None;
    }
    Some(crate::fingerprint::AlgorithmName(
        std::str::from_utf8(name).ok()?.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use base64::Engine as _;

    fn store_in_dir(dir: &Path) -> HostKeyStore {
        HostKeyStore::open(dir.join("ssh-host-keys.json"))
    }

    #[test]
    fn roundtrips_a_record_through_load_and_accept() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in_dir(dir.path());
        let key = crate::fingerprint::tests::ED25519_A_BYTES.to_vec();
        let fingerprint = crate::fingerprint::fingerprint_of(&key);
        let key_base64 = base64::engine::general_purpose::STANDARD.encode(&key);

        store
            .accept(
                "Build-01",
                22,
                "ssh-ed25519",
                &key_base64,
                &fingerprint,
                "2026-08-31T00:00:00Z".into(),
            )
            .unwrap();

        let records = store.load().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].host, "build-01");
        assert_eq!(
            HostKeyStore::lookup(&records, "BUILD-01", 22, "ssh-ed25519", &key),
            StoreOutcome::Match
        );
    }

    #[test]
    fn rsa_wire_key_roundtrips_and_matches_each_signature_variant() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in_dir(dir.path());
        // Public key from a generated 2048-bit RSA pair; no private key is retained.
        let key = base64::engine::general_purpose::STANDARD.decode(
            "AAAAB3NzaC1yc2EAAAADAQABAAABAQDi2tLdz2sZU51gcBl8mdgyA5tg1UNiC8jJYZQMRuLvsdD6uXgDjxfmLQAPeBfaG0HrnjiqizjnWeMiQsklFPbwiZfgY9Uz7vjepITJHhHqlmlNhuZ3hzG88EtdXj3L+jiimKb7uo7O24OkS9Gqo4ZBfGis6FLYiCGUJtauH86cMYmjwBkAXjxL5qfoWSe6xPnPXkiygN3Z+fjtphFTnduX/FA099BHOxu2R4m1EfzWykYNbDDGWEC04pwdwvoMVojva3EICnYhFusfzuAbEiGL03c/xTlBNCo4VmeG3naoLkyUMorXXKilHQcIIe+JNjcoKMOwVG+sanVllIKqLc4L"
        ).unwrap();
        let fingerprint = crate::fingerprint::fingerprint_of(&key);
        let encoded = base64::engine::general_purpose::STANDARD.encode(&key);
        let record = store
            .accept(
                "RSA-HOST",
                2222,
                "ssh-rsa",
                &encoded,
                &fingerprint,
                "2026-09-07T00:00:00Z".into(),
            )
            .unwrap();
        assert_eq!(record.algorithm, "ssh-rsa");
        let records = store_in_dir(dir.path()).load().unwrap();
        assert_eq!(records, vec![record.clone()]);
        for algorithm in ["ssh-rsa", "rsa-sha2-256", "rsa-sha2-512"] {
            assert_eq!(
                HostKeyStore::lookup(&records, "rsa-host", 2222, algorithm, &key),
                StoreOutcome::Match
            );
        }
        let mut wrong_type = record.clone();
        wrong_type.algorithm = "ssh-ed25519".into();
        assert!(validate_record(wrong_type).is_none());
        let mut wrong_fingerprint = record;
        wrong_fingerprint.fingerprint = "SHA256:tampered".into();
        assert!(validate_record(wrong_fingerprint).is_none());
    }

    #[test]
    fn supersedes_only_the_same_host_port_and_type() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in_dir(dir.path());
        let key_a = crate::fingerprint::tests::ED25519_A_BYTES.to_vec();
        let key_b = crate::fingerprint::tests::ED25519_B_BYTES.to_vec();
        let encode = |bytes: &[u8]| {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        };
        store
            .accept(
                "host",
                22,
                "ssh-ed25519",
                &encode(&key_a),
                &crate::fingerprint::fingerprint_of(&key_a),
                "t1".into(),
            )
            .unwrap();
        store
            .accept(
                "host",
                22,
                "ssh-ed25519",
                &encode(&key_b),
                &crate::fingerprint::fingerprint_of(&key_b),
                "t2".into(),
            )
            .unwrap();

        let records = store.load().unwrap();
        assert_eq!(records.len(), 1, "the earlier key is superseded");
        assert_eq!(
            HostKeyStore::lookup(&records, "host", 22, "ssh-ed25519", &key_b),
            StoreOutcome::Match
        );
        assert_eq!(
            HostKeyStore::lookup(&records, "host", 22, "ssh-ed25519", &key_a),
            StoreOutcome::Mismatch
        );
    }

    #[test]
    fn replaces_atomically_across_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in_dir(dir.path());
        let encode = |bytes: &[u8]| {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        };
        let key_a = crate::fingerprint::tests::ED25519_A_BYTES.to_vec();
        let key_b = crate::fingerprint::tests::ED25519_B_BYTES.to_vec();
        store
            .accept(
                "h",
                2200,
                "ssh-ed25519",
                &encode(&key_a),
                &crate::fingerprint::fingerprint_of(&key_a),
                "t1".into(),
            )
            .unwrap();
        let before = fs::read(store_in_dir(dir.path()).path).unwrap();
        store
            .accept(
                "h",
                2201,
                "ssh-ed25519",
                &encode(&key_b),
                &crate::fingerprint::fingerprint_of(&key_b),
                "t2".into(),
            )
            .unwrap();
        let after = fs::read(store_in_dir(dir.path()).path).unwrap();
        assert_ne!(before, after);
        assert_eq!(store.load().unwrap().len(), 2);
        // No temp residue: every intermediate file is gone after the rename.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn rejects_a_tampered_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in_dir(dir.path());
        let encode = |bytes: &[u8]| {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        };
        let key = crate::fingerprint::tests::ED25519_A_BYTES.to_vec();
        let mut record = store
            .accept(
                "h",
                22,
                "ssh-ed25519",
                &encode(&key),
                &crate::fingerprint::fingerprint_of(&key),
                "t".into(),
            )
            .unwrap();
        record.fingerprint = "SHA256:AAAA".into();
        let serialized = serde_json::to_string_pretty(&HostKeyStoreFile {
            version: STORE_VERSION,
            hosts: HashMap::from([(endpoint_key("h", 22), record)]),
        })
        .unwrap();
        fs::write(store_in_dir(dir.path()).path, serialized).unwrap();

        assert!(
            store.load().unwrap().is_empty(),
            "a fingerprint that disagrees with its key is dropped"
        );
    }

    #[test]
    fn treats_a_newer_store_version_as_withheld() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in_dir(dir.path());
        fs::write(
            store_in_dir(dir.path()).path,
            r#"{"version":99,"hosts":{}}"#,
        )
        .unwrap();
        assert_eq!(
            store.load().unwrap_err(),
            StoreError::Withheld("it is version 99".into())
        );
    }

    #[test]
    fn treats_unparseable_store_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in_dir(dir.path());
        fs::write(store_in_dir(dir.path()).path, "{not json").unwrap();
        assert_eq!(store.load().unwrap(), Vec::new());
    }

    #[test]
    fn never_promotes_an_unrecorded_type_to_first_contact() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in_dir(dir.path());
        let encode = |bytes: &[u8]| {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        };
        let key = crate::fingerprint::tests::ED25519_A_BYTES.to_vec();
        store
            .accept(
                "h",
                22,
                "ssh-ed25519",
                &encode(&key),
                &crate::fingerprint::fingerprint_of(&key),
                "t".into(),
            )
            .unwrap();
        let records = store.load().unwrap();
        assert_eq!(
            HostKeyStore::lookup(&records, "h", 22, "rsa-sha2-512", b"other"),
            StoreOutcome::UnknownTypeKnownHost
        );
    }
}
