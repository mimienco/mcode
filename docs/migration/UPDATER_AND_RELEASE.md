# Updater and Release Migration

## Existing release contract

The current workflows cut stable/RC releases, keep drafts private until all artifact gates pass, dispatch signed/notarized macOS builds, sign Windows through the existing infrastructure, build Linux x64/arm64 artifacts, verify manifests and telemetry identity, exercise crash-isolation and golden E2E gates, and update Homebrew. Separate hourly, daily, and adhoc repositories preserve low-latency channels without polluting the main release feed.

## Target requirements

- Tauri bundler/updater artifacts for macOS Apple Silicon and Intel where retained, Windows x64 plus any confirmed existing architectures, and Linux at the current glibc floor.
- Stable, RC, hourly, daily, and adhoc identities, repositories, signing keys, manifest selection, and channel switching.
- Apple signing/notarization/entitlements and existing Windows signing trust chain.
- Draft-first publication, artifact completeness checks, signature verification, failure cleanup, rollback policy, and update survival under active/idle sessions.
- Homebrew and AUR outputs derived from verified release artifacts.
- Explicit Electron-to-Tauri crossgrade, uninstall, downgrade/rollback, and file-association tests.

Credential-independent CI will be executable locally/CI. Credential-required jobs must name exact secrets, formats, scopes, and verification steps; success is never faked.

## Implemented native capability

- `mcode-updater` defines the serialized check/download/install state machine and validates a non-empty Minisign public key plus unique HTTPS endpoints for all five release channels.
- The Tauri backend exposes only `update_get_status`, `update_check`, `update_download`, and `update_install`, and emits validated state/progress on `mcode://updater/status`.
- Renderer production callers use a lazy Electron/Tauri runtime client. Electron retains its compatibility path; Tauri never calls the preload updater facade.
- Tauri downloads through the native updater, which verifies the signed payload before returning its bytes. Verified bytes remain backend-owned and a failed install restores them for retry.
- Applying an update uses the `UpdateInstall` shutdown intent. The normal checkpoint and dirty-file/process decisions run before atomic install authorization; unacknowledged requests time out safely, while acknowledged decisions are not force-closed.
- The native launch path subscribes before hydration, then checks the running release channel with `userInitiated: false`. Manual checks remain user-initiated and rejected Electron-only modes surface an explicit UI error.
- Timeout, user veto, and installer error each release renderer restart preparation, so an aborted install cannot suppress later unsaved-change guards.
- `bundle.createUpdaterArtifacts` is enabled. Renderer capabilities deliberately contain no raw `updater:*` permission.

Release builds must provide all of these compile-time values:

- `MCODE_TAURI_UPDATER_PUBLIC_KEY`
- `MCODE_TAURI_UPDATER_STABLE_ENDPOINT`
- `MCODE_TAURI_UPDATER_RC_ENDPOINT`
- `MCODE_TAURI_UPDATER_HOURLY_ENDPOINT`
- `MCODE_TAURI_UPDATER_DAILY_ENDPOINT`
- `MCODE_TAURI_UPDATER_ADHOC_ENDPOINT`

The checked-in empty updater plugin configuration is a nonfunctional bootstrap required by plugin initialization. Runtime checks always overwrite it with the validated key and one selected endpoint. Missing configuration produces the structured `updater_not_configured` error; the isolated Windows WebView2 smoke proves that path exits cleanly without claiming an available update.

## Remaining release work

- Supply production endpoint values and the public verification key through release CI; none are checked in or inferred.
- Build and publish signed Tauri artifacts plus channel-specific `latest.json` feeds for stable, RC, hourly, daily, and adhoc.
- Port pinned-build discovery, local-build/performance-prerelease modes, downgrade/rollback policy, and any channel-retention behavior still owned by Electron.
- Prove a real network check, verified download, installer application, active-session survival, Electron-to-Tauri crossgrade, downgrade/rollback, and failure cleanup.
- Add Windows signed-installer evidence and macOS signing/notarization plus Linux glibc-floor/package evidence. No platform updater row is complete until its real artifact journey is green.
