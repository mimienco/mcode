//! Skill-share deep links (`mcode://skills/share/<id>`), ported from the
//! Electron `open-url`/second-instance flow (src/main/index.ts). The plugin
//! hands us every URL; we filter to skill-share, keep the latest id, and
//! emit `skill-share://pending` so the renderer can consume it on mount
//! (mirrors `ui:consumePendingSkillShare` one-shot pull semantics).

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_deep_link::DeepLinkExt;

pub const PENDING_SKILL_SHARE_EVENT: &str = "skill-share://pending";

const SHARE_ID_PATTERN_BYTES: usize = 128;

#[derive(Default)]
struct PendingSkillShare(Mutex<Option<String>>);

/// Mirrors `parseSkillShareId`'s `mcode:` branch (shared/skill-share-link.ts):
/// `mcode://skills/share/<id>` with `[A-Za-z0-9_-]{1,128}`.
pub fn skill_share_id_from_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("mcode://")?;
    let path = rest.strip_prefix("skills/share/")?;
    let id = path.trim_end_matches('/');
    let valid = !id.is_empty()
        && id.len() <= SHARE_ID_PATTERN_BYTES
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    valid.then(|| id.to_string())
}

/// Registers the `mcode` scheme and forwards deep links to the renderer.
/// Startup URLs (cold launch) are replayed via the plugin's `on_open_url`
/// exactly like Electron's `open-url` + argv `capture` pair.
pub fn register<R: Runtime>(app: AppHandle<R>) {
    app.manage(PendingSkillShare::default());
    // Why: single-instance is enforced separately; the plugin routes
    // second-instance URLs here, so one subscription covers both paths.
    app.deep_link().on_open_url({
        let app = app.clone();
        move |event| {
            let latest = event
                .urls()
                .iter()
                .filter_map(|url| skill_share_id_from_url(url.as_str()))
                .next_back();
            if let Some(id) = latest {
                publish(&app, id);
            }
        }
    });
}

fn publish<R: Runtime>(app: &AppHandle<R>, id: String) {
    if let Some(state) = app.try_state::<PendingSkillShare>() {
        *state.0.lock().expect("pending skill-share poisoned") = Some(id.clone());
    }
    // Fire-and-forget: the mounted renderer consumes immediately; a cold
    // renderer re-pulls the pending state via the companion command.
    let _ = app.emit(PENDING_SKILL_SHARE_EVENT, id);
}

/// One-shot consume mirroring `ui:consumePendingSkillShare`.
pub fn consume<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let state = app.try_state::<PendingSkillShare>()?;
    state.0.lock().expect("pending skill-share poisoned").take()
}

#[cfg(test)]
mod tests {
    use super::skill_share_id_from_url;

    #[test]
    fn parses_canonical_skill_share_url() {
        assert_eq!(
            skill_share_id_from_url("mcode://skills/share/abc_123-XYZ"),
            Some("abc_123-XYZ".to_string())
        );
    }

    #[test]
    fn tolerates_trailing_slash() {
        assert_eq!(
            skill_share_id_from_url("mcode://skills/share/abc/"),
            Some("abc".to_string())
        );
    }

    #[test]
    fn rejects_wrong_path_or_bad_id() {
        assert_eq!(skill_share_id_from_url("mcode://skills/other/abc"), None);
        assert_eq!(
            skill_share_id_from_url("mcode://skills/share/has space"),
            None
        );
        assert_eq!(skill_share_id_from_url("mcode://skills/share/"), None);
        let long = "a".repeat(129);
        assert_eq!(
            skill_share_id_from_url(&format!("mcode://skills/share/{long}")),
            None
        );
    }

    #[test]
    fn rejects_non_scheme_urls() {
        assert_eq!(
            skill_share_id_from_url("https://app.mcode.dev/skills/share/abc"),
            None
        );
    }
}
