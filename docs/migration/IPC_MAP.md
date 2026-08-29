# IPC Migration Map

The preload API is the compatibility surface, not the target architecture. Each namespace is mapped to a Rust service and a typed TypeScript client; literal channel inventory is supplemented with manual registration/call-site tracing.

| Current namespace | Target service | Transport shape | State |
| --- | --- | --- | --- |
| app, platform, ui, shell | lifecycle/platform | commands plus lifecycle events | inventoried |
| settings, accounts, profiles, onboarding | configuration/identity | commands plus change channel | inventoried |
| repos, projects, groups, folders, sparse, worktrees | git/workspace | commands plus progress channel | inventoried |
| fs, notebook, git | scoped workspace data | commands plus watcher channel | inventoried |
| pty, terminal preview, runtime, agentStatus | process/PTY/agents | commands plus sequenced binary channels | inventoried |
| ssh, remote workspace | SSH/remote | commands plus status/stream channels | inventoried |
| mobile, relay/RPC, automations | wire/relay | versioned protocol independent of Tauri IPC | inventoried |
| updater | release/update | commands plus state/progress channel | inventoried |
| browser, emulator, computer use, permissions | platform capabilities | capability-scoped commands/events | inventoried |
| speech, nativeChat, aiVault | voice/AI services | commands and bounded streams | inventoried |
| telemetry, crash, feedback, export | diagnostics | redacted commands/events | inventoried |
| GitHub, GitLab, Bitbucket, Linear, Jira | provider integrations | provider-neutral command contracts | inventoried |
| skills, plugins, pet, cache, session | extension/session services | commands and change events | inventoried |

First migrated renderer surface:

| Electron channel/API | Tauri/runtime target | Evidence | Remaining gap |
| --- | --- | --- | --- |
| `window:minimize` | typed client → Tauri core window `minimize` | client and component tests | cross-platform UI E2E |
| `window:maximize` | typed client → Tauri core window `toggleMaximize` | client and component tests | cross-platform UI E2E |
| `window:isMaximized` | typed client → Tauri core window `isMaximized` | client and component tests | persisted initial bounds/restore |
| `window:maximize-changed` | typed client → Tauri resize event plus ordered maximize query | subscription/cleanup tests | platform event E2E |
| `window:request-close` | typed client → interceptable Tauri core window `close` | client/component test and native build | terminal confirmation, tray, quit/session handshake |

For every command, the final map will record the old registration and caller, request/response schema, Rust handler, capability, error codes, cancellation behavior, tests, and parity evidence. Generic `invoke(command, unknown)` wrappers are prohibited in product code.
