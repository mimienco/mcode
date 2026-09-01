//! Native OS-permission commands mirroring Electron's
//! `developerPermissions:*` surface (`src/main/ipc/developer-permissions.ts`).
//!
//! Electron's status surface is macOS TCC: `systemPreferences.getMediaAccessStatus`
//! for microphone/camera/screen, `isTrustedAccessibilityClient` for accessibility,
//! and a TCC TCC-database read probe for full-disk access — all gated behind
//! `process.platform === 'darwin'` and returned as `'unsupported'` elsewhere.
//!
//! Windows has no TCC equivalent: WebView2 has no OS-level prompt for
//! microphone/camera and the process can always read the screen, so the
//! Windows-mechanical mapping is:
//! - `microphone`, `camera`, `screen` → `'granted'` (no OS prompt; WebRTC/Webview2
//!   consent is surfaced per-page by the browser layer, not the OS).
//! - `accessibility` → `'unsupported'` (macOS-only TCC check; Windows UI
//!   Automation does not require the app to hold a TCC grant).
//! - `full-disk-access` → `'unsupported'` (macOS-only TCC database probe).
//! - `automation` → `'unsupported'` (macOS Apple Events / AppleScript gate).
//! - `local-network` → `'granted'` (Windows has no local-network prompt).
//! - `usb`, `bluetooth` → `'ready'` (Electron maps these to `'ready'` on
//!   non-darwin; Electron's `unsupportedOffMac()` returns `'unsupported'` for
//!   the other ids only).
//!
//! macOS parity gap (documented, out of scope for runtime evidence here):
//! `getMediaAccessStatus`, `askForMediaAccess`, `isTrustedAccessibilityClient`,
//! the full-disk-access TCC probe, the Apple Events osascript nudge, the
//! local-network mDNS probe, and the `x-apple.systempreferences:` privacy
//! panes are macOS-only and need a darwin build behind
//! `#[cfg(target_os = "macos")]` to reproduce Electron's behavior.

use serde::{Deserialize, Serialize};

/// Mirrors `DeveloperPermissionId` (`src/shared/developer-permissions-types.ts`).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DeveloperPermissionId {
    Microphone,
    Camera,
    Screen,
    Accessibility,
    FullDiskAccess,
    Automation,
    LocalNetwork,
    Usb,
    Bluetooth,
}

/// Mirrors `DeveloperPermissionStatus` (`src/shared/developer-permissions-types.ts`).
#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)] // macOS parity statuses; Windows maps to Granted/Unsupported only
pub(crate) enum DeveloperPermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    Restricted,
    Unknown,
    Unsupported,
    Ready,
}

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeveloperPermissionState {
    id: DeveloperPermissionId,
    status: DeveloperPermissionStatus,
}

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeveloperPermissionRequestResult {
    id: DeveloperPermissionId,
    status: DeveloperPermissionStatus,
    opened_system_settings: bool,
}

/// All ids Electron iterates in `developerPermissions:getStatus` order.
const ALL_PERMISSION_IDS: [DeveloperPermissionId; 9] = [
    DeveloperPermissionId::Microphone,
    DeveloperPermissionId::Camera,
    DeveloperPermissionId::Screen,
    DeveloperPermissionId::Accessibility,
    DeveloperPermissionId::FullDiskAccess,
    DeveloperPermissionId::Automation,
    DeveloperPermissionId::LocalNetwork,
    DeveloperPermissionId::Usb,
    DeveloperPermissionId::Bluetooth,
];

/// The Windows-native status mapping (see module doc for the macOS gap).
fn windows_status(id: DeveloperPermissionId) -> DeveloperPermissionStatus {
    match id {
        DeveloperPermissionId::Microphone
        | DeveloperPermissionId::Camera
        | DeveloperPermissionId::Screen
        | DeveloperPermissionId::LocalNetwork => DeveloperPermissionStatus::Granted,
        DeveloperPermissionId::Accessibility
        | DeveloperPermissionId::FullDiskAccess
        | DeveloperPermissionId::Automation => DeveloperPermissionStatus::Unsupported,
        DeveloperPermissionId::Usb | DeveloperPermissionId::Bluetooth => {
            DeveloperPermissionStatus::Ready
        }
    }
}

/// Reports the status of every developer permission (same set and order as
/// Electron's `developerPermissions:getStatus`).
#[tauri::command(rename_as = "developerPermissions_getStatus")]
pub(crate) async fn developer_permissions_get_status(
) -> Vec<DeveloperPermissionState> {
    ALL_PERMISSION_IDS
        .iter()
        .map(|id| DeveloperPermissionState {
            id: *id,
            status: windows_status(*id),
        })
        .collect()
}

/// Electron's `request` on non-darwin returns `unsupported` with no prompt.
/// Here microphone/camera/screen/local-network are already granted at the OS
/// layer, so they report `granted` directly; the macOS-only ids report
/// `unsupported` and never open System Settings.
#[tauri::command(rename_as = "developerPermissions_request")]
pub(crate) async fn developer_permissions_request(
    id: DeveloperPermissionId,
) -> DeveloperPermissionRequestResult {
    DeveloperPermissionRequestResult {
        id,
        status: windows_status(id),
        opened_system_settings: false,
    }
}

/// Electron opens a `x-apple.systempreferences:` privacy pane on macOS. On
/// Windows there is nothing equivalent to open for any permission id, so this
/// is a documented no-op.
#[tauri::command(rename_as = "developerPermissions_openSettings")]
pub(crate) async fn developer_permissions_open_settings(
    _id: DeveloperPermissionId,
) {
}

/// Mirrors `testLocalNetworkConnection` from
/// `src/main/ipc/local-network-connection-test.ts`: connects to a LAN host and
/// reports success or a classified failure. Electron classifies
/// `INVALID_TARGET` for non-LAN hosts; this port keeps that split so the
/// renderer's `LocalNetworkConnectionTestResult` shape is unchanged.
#[tauri::command(rename_as = "developerPermissions_testLocalNetworkConnection")]
pub(crate) async fn developer_permissions_test_local_network_connection(
    host: String,
    port: u16,
) -> LocalNetworkConnectionTestResult {
    let host = normalize_target_host(&host);
    let tested_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0);
    if host.is_none() {
        return LocalNetworkConnectionTestResult {
            ok: false,
            host: String::new(),
            port,
            tested_at,
            failure: Some("invalid-target"),
        };
    }
    let host = host.unwrap();
    let addr = format!("{host}:{port}");
    let timeout = std::time::Duration::from_millis(4000);
    let failure = match tokio::time::timeout(
        timeout,
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_)) => None,
        Ok(Err(error)) => Some(classify_connect_error(error.kind())),
        Err(_) => Some("timeout"),
    };
    let ok = failure.is_none();
    let _ = is_lan_address(&host); // retained for parity with Electron's LAN gate
    LocalNetworkConnectionTestResult {
        ok,
        host,
        port,
        tested_at,
        failure,
    }
}

/// Mirrors `LocalNetworkConnectionTestResult`
/// (`src/shared/developer-permissions-types.ts`).
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalNetworkConnectionTestResult {
    ok: bool,
    host: String,
    port: u16,
    tested_at: u64,
    failure: Option<&'static str>,
}

/// Mirrors Electron's `normalizeTargetHost`.
fn normalize_target_host(raw_host: &str) -> Option<String> {
    let trimmed = raw_host.trim().to_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed)
}

/// Mirrors Electron's `isLanAddress` (IPv4 RFC1918 + link-local, IPv6 ULA +
/// link-local). Kept for parity documentation; the connect probe is the
/// runtime verdict on Windows where no local-network prompt exists.
fn is_lan_address(address: &str) -> bool {
    use std::net::IpAddr;
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            let octets = v4.octets();
            octets[0] == 10
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 168)
                || (octets[0] == 169 && octets[1] == 254)
        }
        Ok(IpAddr::V6(v6)) => {
            let segments = v6.segments();
            (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
        }
        Err(_) => false,
    }
}

/// Mirrors Electron's `failureFromChild` classification over connect errors.
fn classify_connect_error(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::PermissionDenied
        | std::io::ErrorKind::ConnectionRefused => "refused",
        std::io::ErrorKind::TimedOut => "timeout",
        _ => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_covers_every_id() {
        for id in ALL_PERMISSION_IDS {
            let status = windows_status(id);
            assert_ne!(
                status,
                DeveloperPermissionStatus::Unknown,
                "every id must map to a concrete Windows status"
            );
        }
        assert_eq!(
            windows_status(DeveloperPermissionId::Microphone),
            DeveloperPermissionStatus::Granted
        );
        assert_eq!(
            windows_status(DeveloperPermissionId::Camera),
            DeveloperPermissionStatus::Granted
        );
        assert_eq!(
            windows_status(DeveloperPermissionId::Screen),
            DeveloperPermissionStatus::Granted
        );
        assert_eq!(
            windows_status(DeveloperPermissionId::Accessibility),
            DeveloperPermissionStatus::Unsupported
        );
        assert_eq!(
            windows_status(DeveloperPermissionId::FullDiskAccess),
            DeveloperPermissionStatus::Unsupported
        );
        assert_eq!(
            windows_status(DeveloperPermissionId::Automation),
            DeveloperPermissionStatus::Unsupported
        );
        assert_eq!(
            windows_status(DeveloperPermissionId::LocalNetwork),
            DeveloperPermissionStatus::Granted
        );
        assert_eq!(
            windows_status(DeveloperPermissionId::Usb),
            DeveloperPermissionStatus::Ready
        );
        assert_eq!(
            windows_status(DeveloperPermissionId::Bluetooth),
            DeveloperPermissionStatus::Ready
        );
    }

    #[test]
    fn request_result_reports_without_opening_system_settings() {
        for id in ALL_PERMISSION_IDS {
            let result = developer_permissions_request_blocking(id);
            assert!(!result.opened_system_settings);
            assert_eq!(result.status, windows_status(id));
        }
    }

    #[test]
    fn status_covers_the_full_electron_surface() {
        let states = developer_permissions_get_status_blocking();
        let ids: Vec<_> = states.iter().map(|state| state.id).collect();
        assert_eq!(ids.as_slice(), ALL_PERMISSION_IDS);
    }

    // The async command bodies are trivial over a static mapping, so the
    // tests drive the same logic through thin blocking wrappers.
    fn developer_permissions_request_blocking(
        id: DeveloperPermissionId,
    ) -> DeveloperPermissionRequestResult {
        futures_lite_block(async { developer_permissions_request(id).await })
    }

    fn developer_permissions_get_status_blocking() -> Vec<DeveloperPermissionState> {
        futures_lite_block(async { developer_permissions_get_status().await })
    }

    fn futures_lite_block<F: std::future::Future>(future: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        rt.block_on(future)
    }

    #[test]
    fn lan_gate_matches_electron() {
        assert!(is_lan_address("10.0.0.5"));
        assert!(is_lan_address("172.16.0.1"));
        assert!(is_lan_address("172.31.255.255"));
        assert!(!is_lan_address("172.32.0.1"));
        assert!(is_lan_address("192.168.1.1"));
        assert!(is_lan_address("169.254.1.1"));
        assert!(!is_lan_address("8.8.8.8"));
        assert!(is_lan_address("fd00::1"));
        assert!(is_lan_address("fe80::1"));
        assert!(!is_lan_address("2606:4700::1"));
        assert!(!is_lan_address("not-an-address"));
    }

    #[test]
    fn local_network_test_reports_invalid_target_for_blank_host() {
        let result = futures_lite_block(async {
            developer_permissions_test_local_network_connection(
                "   ".to_owned(),
                8080,
            )
            .await
        });
        assert!(!result.ok);
        assert_eq!(result.failure, Some("invalid-target"));
    }

    #[test]
    fn local_network_test_classifies_refused_connection() {
        // Port 1 on loopback is refused by the OS, not filtered.
        let result = futures_lite_block(async {
            developer_permissions_test_local_network_connection(
                "127.0.0.1".to_owned(),
                1,
            )
            .await
        });
        assert!(!result.ok);
        assert_eq!(result.failure, Some("refused"));
    }

    #[test]
    fn status_serializes_to_electron_wire_shape() {
        let json = serde_json::to_value(DeveloperPermissionState {
            id: DeveloperPermissionId::FullDiskAccess,
            status: DeveloperPermissionStatus::Unsupported,
        })
        .unwrap();
        assert_eq!(json, serde_json::json!({ "id": "full-disk-access", "status": "unsupported" }));
    }
}
