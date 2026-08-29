# Electron and Node Surface Inventory

## Entry points read

- `src/main/index.ts` (3,493 lines): lifecycle and application composition.
- `src/preload/index.ts` (5,172 lines): renderer bridge, event forwarding, PTY buffering/ACK/resync, and service namespaces.
- `src/preload/api-types.ts`: public renderer contract types.
- `electron.vite.config.ts` and `config/electron-vite-target.config.ts`: main/preload/renderer bundling.
- `config/electron-builder.config.cjs`: platform packaging, channels, signing, helpers, assets, and update identity.
- `.github/workflows/release-cut.yml` plus macOS/dev-channel/Homebrew workflows: release orchestration and gates.

## Current responsibility groups

- Application lifecycle, crash recovery, single instance, windows, tray, menus, shortcuts, dialogs, deep links.
- Profiles, accounts, settings, onboarding, notifications, permissions, themes, locale, diagnostics, telemetry, updates.
- Projects, folders, repositories, worktrees, sparse checkout, cleanup, disk usage, ports, sessions, dashboards.
- PTY/process supervision, terminals, shells, agent runtimes, restoration, foreground processes, terminal previews.
- Filesystem, watchers, editor/notebook content, Git/source control, browser/emulator, plugins/skills/pets.
- SSH and remote workspaces, relay/RPC, mobile pairing, automations, voice/speech, native chat, AI vault, computer use.
- CLI, headless serve, runtime environments, rate limits, hosted review providers and issue integrations.

## Authoritative IPC surface scan

`electron-ipc-inventory.json` is generated from the immutable original with `generate-electron-ipc-inventory.mjs`. The Babel TypeScript AST scan covers production files under `src/main`, `src/preload`, `src/renderer/src`, and `src/shared`; tests, fixtures, mocks, harnesses, build output, and dependencies are excluded by a locked rule.

Current result:

- 7,763 production source files parsed; zero parse failures.
- 1,935 IPC operations across 135 files.
- 762 Main registrations, 787 Preload callers, 208 Main emitters, and 178 Preload subscriptions.
- 978 distinct channels.
- 20 parameterized factory sites reconciled against explicit production call-site evidence in `electron-ipc-dynamic-resolutions.json`.
- Zero unresolved dynamic entries.
- Zero command registrations without callers and zero callers without registrations.
- Zero emitters without subscriptions. Two subscription-only legacy channels remain: `export:requestPdf` and `ui:closeSessionTab`; neither has a production emitter anywhere in the captured original.

The checked artifact records every operation with process area, direction, transport, receiver, expression, channel set, relative file, line, and column. `--check` regenerates the entire ledger and fails on any byte difference.

Production renderer imports still require a separate exact Node-built-in allowlist scan. Broad substring results are not accepted as evidence.

## Removal gate

No Electron or Node-native production dependency is removed until its responsibility has a green row in `parity-matrix.json` and an equivalent test has replaced each Electron-specific assertion.
