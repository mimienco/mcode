# IPC Migration Map

The preload API is the compatibility surface, not the target architecture. Each namespace is mapped to a Rust service and a typed TypeScript client; literal channel inventory is supplemented with manual registration/call-site tracing.

## Machine-readable source of truth

- `electron-ipc-inventory.json`: 1,935 located operations and a 978-channel correlation index from the immutable original.
- `electron-ipc-dynamic-resolutions.json`: evidence for all 20 parameterized registration/caller/emitter factories.
- `config/scripts/generate-electron-ipc-inventory.mjs`: deterministic AST generator and stale-artifact check.
- `config/scripts/generate-electron-ipc-inventory.node-test.mjs`: imported/template channel resolution, alias/optional-call detection, exclusion, ordering, reconciliation, and correlation contracts.

All 762 registrations correlate to at least one of 787 callers, and every caller correlates to a registration. All 208 emitters correlate to a subscription. Of 178 subscriptions, only `export:requestPdf` and `ui:closeSessionTab` lack any production emitter in the captured original and are retained as explicit subscription-only compatibility findings.

| Current namespace                                   | Target service             | Transport shape                             | State       |
| --------------------------------------------------- | -------------------------- | ------------------------------------------- | ----------- |
| app, platform, ui, shell                            | lifecycle/platform         | commands plus lifecycle events              | inventoried |
| settings, accounts, profiles, onboarding            | configuration/identity     | commands plus change channel                | inventoried |
| repos, projects, groups, folders, sparse, worktrees | git/workspace              | commands plus progress channel              | inventoried |
| fs, notebook, git                                   | scoped workspace data      | commands plus watcher channel               | inventoried |
| pty, terminal preview, runtime, agentStatus         | process/PTY/agents         | commands plus sequenced binary channels     | inventoried |
| ssh, remote workspace                               | SSH/remote                 | commands plus status/stream channels        | inventoried |
| mobile, relay/RPC, automations                      | wire/relay                 | versioned protocol independent of Tauri IPC | inventoried |
| updater                                             | release/update             | commands plus state/progress channel        | inventoried |
| browser, emulator, computer use, permissions        | platform capabilities      | capability-scoped commands/events           | inventoried |
| speech, nativeChat, aiVault                         | voice/AI services          | commands and bounded streams                | inventoried |
| telemetry, crash, feedback, export                  | diagnostics                | redacted commands/events                    | inventoried |
| GitHub, GitLab, Bitbucket, Linear, Jira             | provider integrations      | provider-neutral command contracts          | inventoried |
| skills, plugins, pet, cache, session                | extension/session services | commands and change events                  | inventoried |

First migrated renderer surface:

| Electron channel/API      | Tauri/runtime target                                                                      | Evidence                                                                                                                    | Remaining gap                                                                                  |
| ------------------------- | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `window:minimize`         | typed client → Tauri core window `minimize`                                               | client/component tests plus least-privilege capability contract                                                             | cross-platform UI E2E                                                                          |
| `window:maximize`         | typed client → Tauri core window `toggleMaximize`                                         | client/component tests plus least-privilege capability contract                                                             | cross-platform UI E2E                                                                          |
| `window:isMaximized`      | typed client → Tauri core window `isMaximized`                                            | client/component tests plus startup/persistence Rust contracts                                                              | cross-platform restore evidence                                                                |
| `window:maximize-changed` | typed client → Tauri resize event plus ordered maximize query                             | subscription/cleanup tests                                                                                                  | platform event E2E                                                                             |
| `window:request-close`    | typed client → interceptable Tauri core window `close` → narrow native tray policy        | client/component tests, capability contract, native build, and visible Windows WebView2 close/hide smoke                    | one-time tray notice and non-Windows evidence                                                  |
| tray Open/Quit            | Rust-owned Tauri tray → restore/focus or app-wide lifecycle Quit                          | native build and source contracts; close smoke confirms the tray is installed before hiding                                 | real Open/left-click/Quit interaction, attention/activity states, and non-Windows shell UI     |
| app-wide Quit/Restart     | typed lifecycle client → versioned request/receipt/cancel/atomic-execute commands         | Rust contracts, full-renderer routing contract, updater checkpoint contracts, real WebView2 Quit/Restart smoke              | Tauri update-apply capability and non-Windows evidence                                         |
| updater status/check      | runtime updater client → `update_get_status` / `update_check`                             | strict Rust feed configuration, state-machine tests, renderer validation/routing tests, unconfigured-build WebView2 smoke   | production feed key/endpoints and real signed feed                                             |
| updater download/apply    | runtime updater client → `update_download` / `update_install` → `UpdateInstall` lifecycle | native signature verification, progress/state events, lifecycle contracts, install retry/error handling                     | published signed artifact install, rollback, and non-Windows evidence                          |
| `fs:readDir`              | runtime filesystem client → `fs_read_dir` → capability-relative Rust filesystem           | atomic live-root refresh/revocation, traversal/symlink/race protection, sorting, command/renderer contracts, WebView2 smoke | verified linked worktrees, native SSH replacement, watcher events, and cross-platform evidence |
| `fs:stat`                 | runtime filesystem client → `fs_stat` → capability-relative Rust filesystem               | capability containment, metadata serialization, command/renderer contracts, WebView2 smoke                                  | external-path grants, native SSH replacement, and cross-platform evidence                      |
| `fs:pathExists`           | runtime filesystem client → `fs_path_exists` → capability-relative Rust filesystem        | missing-leaf/outside denial, live revocation, command/renderer contracts, WebView2 smoke                                    | external-path grants, native SSH replacement, and cross-platform evidence                      |

The Tauri updater events are `mcode://updater/status` and the exact-request `mcode://updater/install-aborted` recovery signal. Renderer code has no raw updater-plugin capability; the Rust command boundary owns feed selection, signature trust, downloaded bytes, lifecycle authorization, and native installation.

For every command, the final map will record the old registration and caller, request/response schema, Rust handler, capability, error codes, cancellation behavior, tests, and parity evidence. Generic `invoke(command, unknown)` wrappers are prohibited in product code.

Regenerate and verify from the rewrite repository:

```powershell
node --max-old-space-size=4096 config/scripts/generate-electron-ipc-inventory.mjs --source D:\ADE\orca --resolutions docs\migration\electron-ipc-dynamic-resolutions.json --output docs\migration\electron-ipc-inventory.json --check
```
