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

## Preliminary static scan

A broad literal scan found 731 distinct main-process channel registration strings and 915 distinct preload channel strings. These numbers are intentionally preliminary: factories, computed names, event-only channels, and tests make simple regex both under- and over-inclusive. The IPC map must be reconciled against registration helpers and renderer call sites before it is considered complete.

Production renderer imports will be rescanned using an exact Node-built-in allowlist and with tests/fixtures excluded. Earlier broad substring results are not accepted as evidence.

## Removal gate

No Electron or Node-native production dependency is removed until its responsibility has a green row in `parity-matrix.json` and an equivalent test has replaced each Electron-specific assertion.
