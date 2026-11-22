//! Renderer settings-document surface (GlobalSettings/PersistedUIState boot
//! reads) — the document the SettingsService owns (`settings_path()`,
//! mcode-data.json). `settings_get_document` returns the whole persisted
//! document; `settings_set_document` applies a JSON merge patch through the
//! service's own update_persisted primitive (same write lock + atomic-file
//! discipline as every other writer).
//!
//! The Electron preload's `settings.get/set` mapped onto main-process store
//! reads/writes of this same document (global-settings-types GlobalSettings
//! lives under the `settings` key, persisted UI state under `ui`).

use crate::DesktopCommandError;
use mcode_settings::{SettingsService, SettingsServiceError};
use serde_json::Value;

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn settings_get_document(
    service: tauri::State<'_, SettingsService>,
) -> Result<Value, DesktopCommandError> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || read_document(&service))
        .await
        .map_err(|error| {
            DesktopCommandError::settings(
                SettingsServiceError::Read(error.to_string()),
                "settings_get_document",
            )
        })?
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn settings_set_document(
    service: tauri::State<'_, SettingsService>,
    patch: Value,
) -> Result<Value, DesktopCommandError> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || set_document_for(&service, patch))
        .await
        .map_err(|error| {
            DesktopCommandError::settings(
                SettingsServiceError::Read(error.to_string()),
                "settings_set_document",
            )
        })?
}

fn set_document_for(service: &SettingsService, patch: Value) -> Result<Value, DesktopCommandError> {
    if !patch.is_object() {
        return Err(DesktopCommandError::settings(
            SettingsServiceError::Parse("merge patch must be an object".into()),
            "settings_set_document",
        ));
    }
    service
        .update_persisted_public(|root| {
            validate_renderer_patch(root, &patch)?;
            let mut document = Value::Object(root.clone());
            merge_patch(&mut document, &patch);
            if let Value::Object(merged) = document {
                *root = merged;
            }
            Ok(())
        })
        .map_err(|error| DesktopCommandError::settings(error, "settings_set_document"))?;
    read_document(service)
}

fn validate_renderer_patch(
    root: &serde_json::Map<String, Value>,
    patch: &Value,
) -> Result<(), SettingsServiceError> {
    let forbidden = || {
        SettingsServiceError::Parse(
            "settings patch changes native-owned workspace authority".into(),
        )
    };
    for (key, value) in patch.as_object().expect("validated object") {
        if !matches!(
            key.as_str(),
            "settings" | "ui" | "keybindings" | "onboarding"
        ) {
            return Err(forbidden());
        }
        if key == "settings" {
            let fields = value.as_object().ok_or_else(forbidden)?;
            if let Some(directory) = fields.get("workspaceDir")
                && root
                    .get("settings")
                    .and_then(|settings| settings.get("workspaceDir"))
                    != Some(directory)
                {
                    return Err(forbidden());
                }
        }
    }
    Ok(())
}

fn read_document(service: &SettingsService) -> Result<Value, DesktopCommandError> {
    service
        .read_persisted_public()
        .map_err(|error| DesktopCommandError::settings(error, "settings_get_document"))
}

/// RFC 7386 merge patch (the subset the renderer needs: set/replace, nested
/// objects merge, null deletes).
fn merge_patch(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                if value.is_null() {
                    target_map.remove(key);
                } else {
                    merge_patch(
                        target_map
                            .entry(key.clone())
                            .or_insert_with(|| Value::Object(serde_json::Map::new())),
                        value,
                    );
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renderer_cannot_inject_roots_or_relabel_remote_catalogs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = json!({"settings":{"workspaceDir":dir.path()}, "repos":[
            {"id":"remote", "path":"/server", "executionHostId":"runtime-a"}
        ]});
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let service = SettingsService::new(path.clone());
        for patch in [
            json!({"repos":[{"path":"/outside"}]}),
            json!({"projectGroups":[{"id":"injected", "parentPath":"/outside"}]}),
            json!({"folderWorkspaces":[{"id":"injected", "folderPath":"/outside"}]}),
            json!({"settings":{"workspaceDir":"/outside"}}),
            json!({"settings":null}),
            json!({"workspaceSession":{"terminalLayoutsByTabId":{}}}),
        ] {
            assert!(set_document_for(&service, patch).is_err());
            assert_eq!(service.read_persisted_public().unwrap(), original);
        }
    }

    #[test]
    fn ordinary_preferences_and_onboarding_preserve_native_owned_sections() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(dir.path().join("settings.json"));
        service
            .update_persisted_public(|root| {
                root.insert("repos".into(), json!([{"id":"saved","path":dir.path()}]));
                root.insert(
                    "settings".into(),
                    json!({"workspaceDir":dir.path(),"notifications":{"enabled":true}}),
                );
                Ok(())
            })
            .unwrap();
        let result = set_document_for(
            &service,
            json!({"settings":{"theme":"light", "workspaceDir":dir.path()},
            "onboarding":{"checklist":{"addedRepo":true}},"ui":{"sidebarWidth":260}}),
        )
        .unwrap();
        assert_eq!(result["repos"][0]["id"], "saved");
        assert_eq!(result["settings"]["notifications"]["enabled"], true);
        assert_eq!(result["settings"]["theme"], "light");
        assert_eq!(result["onboarding"]["checklist"]["addedRepo"], true);
    }

    #[test]
    fn merge_patch_sets_nested_and_deletes_with_null() {
        let mut target = json!({
            "settings": { "theme": "dark", "keep": 1 },
            "ui": { "windowBounds": { "width": 100 } }
        });
        merge_patch(
            &mut target,
            &json!({
                "settings": { "theme": "light", "extra": true },
                "ui": { "windowBounds": null }
            }),
        );
        assert_eq!(target["settings"]["theme"], json!("light"));
        assert_eq!(target["settings"]["extra"], json!(true));
        assert_eq!(target["settings"]["keep"], json!(1));
        assert!(target["ui"].get("windowBounds").is_none());
    }

    #[test]
    fn merge_patch_replaces_arrays_and_scalars_verbatim() {
        let mut target = json!({ "repos": ["a"], "n": 1 });
        merge_patch(&mut target, &json!({ "repos": ["b", "c"], "n": null }));
        assert_eq!(target["repos"], json!(["b", "c"]));
        assert!(target.get("n").is_none());
    }
}
