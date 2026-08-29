# Data and Settings Migration

## Inventory to resolve

- App identifiers and platform data/config/cache/log locations.
- Settings schema, defaults, opt-in/opt-out state, profiles, accounts, workspace history, layout/session state, and onboarding state.
- Pairing/device records and secret storage boundaries.
- CLI registration, custom protocols, file associations, shortcuts, Dock/Start Menu/Desktop entries, and uninstall behavior.

## Migration invariants

- The first Tauri release must discover the last Electron data location without changing or deleting it before a verified migration completes.
- Migration is versioned, idempotent, resumable, and records only non-sensitive progress.
- Secrets are moved through the platform secret store or re-authenticated; plaintext copying is prohibited.
- Telemetry consent is preserved exactly and never defaults to broader collection.
- Workspace history and session state preserve canonical paths, Windows/WSL translation, and missing-workspace behavior.
- Rollback either reads compatible data or uses an explicit snapshot/bridge strategy.

Direct crossgrade versus an Electron bridge release remains pending signed installer/updater experiments. A simulated signature result cannot close this item.
