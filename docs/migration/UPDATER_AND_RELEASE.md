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
