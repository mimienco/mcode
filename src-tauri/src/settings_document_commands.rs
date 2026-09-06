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
pub(crate) fn settings_get_document(
    service: tauri::State<'_, SettingsService>,
) -> Result<Value, DesktopCommandError> {
    read_document(&service)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn settings_set_document(
    service: tauri::State<'_, SettingsService>,
    patch: Value,
) -> Result<Value, DesktopCommandError> {
    if !patch.is_object() {
        return Err(DesktopCommandError::settings(
            SettingsServiceError::Parse("merge patch must be an object".into()),
            "settings_set_document",
        ));
    }
    service
        .update_persisted_public(|root| {
            let mut document = Value::Object(std::mem::take(root));
            merge_patch(&mut document, &patch);
            if let Value::Object(merged) = document {
                *root = merged;
            }
            Ok(())
        })
        .map_err(|error| DesktopCommandError::settings(error, "settings_set_document"))?;
    read_document(&service)
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
