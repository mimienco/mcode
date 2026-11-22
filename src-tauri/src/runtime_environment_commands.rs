//! Read-only public runtime catalog; pairing/remote transports remain unported.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde_json::{Map, Value};
use tauri::Manager;

use crate::DesktopCommandError;

const MAX_STORE_BYTES: u64 = 1024 * 1024;
const STORE_FILENAME: &str = "mcode-environments.json";

fn store_error() -> DesktopCommandError {
    DesktopCommandError::new(
        "runtime_error",
        "runtime_environments_list",
        "Could not read the MCode environments catalog; it is invalid or unavailable.".into(),
        "Your saved servers could not be loaded. Their configuration has not been changed.",
        None,
        None,
    )
}

fn nonempty_string(fields: &Map<String, Value>, key: &str) -> bool {
    fields
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
}

fn finite_number(fields: &Map<String, Value>, key: &str) -> bool {
    fields
        .get(key)
        .and_then(Value::as_f64)
        .is_some_and(f64::is_finite)
}

fn public_environment(value: Value) -> Option<Value> {
    let fields = value.as_object()?;
    for key in ["id", "name", "preferredEndpointId"] {
        if !nonempty_string(fields, key) {
            return None;
        }
    }
    for key in ["createdAt", "updatedAt"] {
        if !finite_number(fields, key) {
            return None;
        }
    }
    if fields.get("lastUsedAt")? != &Value::Null && !finite_number(fields, "lastUsedAt") {
        return None;
    }
    if fields.get("runtimeId")? != &Value::Null && !nonempty_string(fields, "runtimeId") {
        return None;
    }
    if fields.contains_key("pairingRevision") && !finite_number(fields, "pairingRevision") {
        return None;
    }
    if fields.contains_key("pairedDeviceId") && !nonempty_string(fields, "pairedDeviceId") {
        return None;
    }
    if fields.contains_key("source")
        && !matches!(fields["source"].as_str(), Some("manual" | "ephemeral-vm"))
    {
        return None;
    }
    if fields.contains_key("connectionDependency")
        && fields["connectionDependency"].as_str() != Some("ssh-tunnel")
    {
        return None;
    }
    let endpoints = fields.get("endpoints")?.as_array()?;
    if endpoints.is_empty() {
        return None;
    }
    let mut public_endpoints = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let endpoint = endpoint.as_object()?;
        if endpoint.get("kind")?.as_str()? != "websocket" {
            return None;
        }
        for key in ["id", "label", "endpoint", "deviceToken", "publicKeyB64"] {
            if !nonempty_string(endpoint, key) {
                return None;
            }
        }
        let mut public = Map::new();
        for key in ["id", "kind", "label", "endpoint"] {
            public.insert(key.into(), endpoint.get(key)?.clone());
        }
        public_endpoints.push(Value::Object(public));
    }
    let mut public = Map::new();
    for key in [
        "id",
        "name",
        "createdAt",
        "updatedAt",
        "pairingRevision",
        "pairedDeviceId",
        "lastUsedAt",
        "runtimeId",
        "source",
        "connectionDependency",
        "preferredEndpointId",
    ] {
        if let Some(value) = fields.get(key) {
            public.insert(key.into(), value.clone());
        }
    }
    public.insert("endpoints".into(), Value::Array(public_endpoints));
    Some(Value::Object(public))
}

fn parse_public_catalog(bytes: &[u8]) -> Result<Vec<Value>, DesktopCommandError> {
    if bytes.len() as u64 > MAX_STORE_BYTES {
        return Err(store_error());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| store_error())?;
    if value.get("version").and_then(Value::as_f64) != Some(1.0) {
        return Err(store_error());
    }
    value
        .get("environments")
        .and_then(Value::as_array)
        .ok_or_else(store_error)?
        .iter()
        .cloned()
        .map(|value| public_environment(value).ok_or_else(store_error))
        .collect()
}

fn read_public_catalog(directory: &Path) -> Result<Vec<Value>, DesktopCommandError> {
    let path = directory.join(STORE_FILENAME);
    let before_open = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(store_error()),
    };
    if !before_open.is_file() || before_open.len() > MAX_STORE_BYTES {
        return Err(store_error());
    }
    let file = File::open(path).map_err(|_| store_error())?;
    let metadata = file.metadata().map_err(|_| store_error())?;
    if !metadata.is_file() || metadata.len() > MAX_STORE_BYTES {
        return Err(store_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| store_error())?;
    }
    let mut bytes = Vec::new();
    file.take(MAX_STORE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| store_error())?;
    parse_public_catalog(&bytes)
}

#[tauri::command]
pub(crate) async fn runtime_environments_list(
    app: tauri::AppHandle,
) -> Result<Vec<Value>, DesktopCommandError> {
    let directory = app.path().app_data_dir().map_err(|_| store_error())?;
    tauri::async_runtime::spawn_blocking(move || read_public_catalog(&directory))
        .await
        .map_err(|_| store_error())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry() -> Value {
        json!({"id":"persisted-owner", "name":"Server", "createdAt":1, "updatedAt":2,
            "pairingRevision":3, "lastUsedAt":null, "runtimeId":"runtime-id", "pairedDeviceId":"device-id",
            "source":"manual", "connectionDependency":"ssh-tunnel", "preferredEndpointId":"ep",
            "unknownSecret":"must-not-leak", "endpoints":[{"id":"ep", "kind":"websocket", "label":"Remote",
                "endpoint":"wss://host/ws", "deviceToken":"token-secret", "publicKeyB64":"key-secret", "extra":"extra-secret"}]})
    }

    #[test]
    fn valid_catalog_preserves_identity_and_strips_secrets_and_unknown_fields() {
        let result = parse_public_catalog(
            &serde_json::to_vec(&json!({"version":1,"environments":[entry()]})).unwrap(),
        )
        .unwrap();
        assert_eq!(result[0]["id"], "persisted-owner");
        assert_eq!(result[0]["pairingRevision"], 3);
        assert_eq!(result[0]["pairedDeviceId"], "device-id");
        assert_eq!(result[0]["endpoints"][0]["endpoint"], "wss://host/ws");
        let wire = serde_json::to_string(&result).unwrap();
        for secret in [
            "token-secret",
            "key-secret",
            "extra-secret",
            "must-not-leak",
            "deviceToken",
            "publicKeyB64",
        ] {
            assert!(!wire.contains(secret));
        }
    }

    #[test]
    fn malformed_catalog_or_any_bad_entry_is_error_not_an_empty_success() {
        for value in [
            json!(null),
            json!({"version":2,"environments":[]}),
            json!({"version":1,"environments":[entry(), {}]}),
        ] {
            assert!(parse_public_catalog(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        for key in [
            "id",
            "name",
            "lastUsedAt",
            "runtimeId",
            "createdAt",
            "endpoints",
        ] {
            let mut value = entry();
            value.as_object_mut().unwrap().remove(key);
            assert!(public_environment(value).is_none(), "missing {key}");
        }
        let mut value = entry();
        value["pairingRevision"] = Value::Null;
        assert!(public_environment(value).is_none());
        let mut value = entry();
        value["endpoints"][0]["deviceToken"] = json!("");
        assert!(public_environment(value).is_none());
    }

    #[test]
    fn bounded_parser_rejects_oversize_and_invalid_utf8_without_payload_in_error() {
        assert!(parse_public_catalog(&vec![b' '; MAX_STORE_BYTES as usize + 1]).is_err());
        assert!(parse_public_catalog(&[0xff]).is_err());
        assert!(parse_public_catalog(b"not-json").is_err());
        assert!(
            parse_public_catalog("\u{feff}{\"version\":1,\"environments\":[]}".as_bytes()).is_err()
        );
    }

    #[test]
    fn public_endpoint_preserves_oracle_url_semantics() {
        let mut value = entry();
        value["endpoints"][0]["endpoint"] = json!("wss://name:password@host/ws?endpoint=one");
        let public = public_environment(value).unwrap();
        assert_eq!(
            public["endpoints"][0]["endpoint"],
            "wss://name:password@host/ws?endpoint=one"
        );
        assert!(public["endpoints"][0].get("deviceToken").is_none());
    }

    #[test]
    fn catalog_command_is_registered_and_granted_to_main_window() {
        assert!(
            include_str!("lib.rs")
                .contains("runtime_environment_commands::runtime_environments_list")
        );
        assert!(include_str!("../build.rs").contains("\"runtime_environments_list\""));
        let capability: Value = serde_json::from_str(include_str!(
            "../capabilities/runtime-environment-commands.json"
        ))
        .unwrap();
        assert!(
            capability["windows"]
                .as_array()
                .unwrap()
                .contains(&json!("main"))
        );
        assert!(
            capability["permissions"]
                .as_array()
                .unwrap()
                .contains(&json!("allow-runtime-environments-list"))
        );
    }

    #[test]
    fn absent_catalog_is_empty_but_existing_directory_is_error() {
        let directory = std::env::temp_dir().join(format!(
            "mcode-env-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        assert!(read_public_catalog(&directory).unwrap().is_empty());
        std::fs::create_dir(directory.join(STORE_FILENAME)).unwrap();
        assert!(read_public_catalog(&directory).is_err());
        std::fs::remove_dir(directory.join(STORE_FILENAME)).unwrap();
        std::fs::write(
            directory.join(STORE_FILENAME),
            serde_json::to_vec(&json!({"version":1,"environments":[entry()]})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            read_public_catalog(&directory).unwrap()[0]["id"],
            "persisted-owner"
        );
        std::fs::remove_file(directory.join(STORE_FILENAME)).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
