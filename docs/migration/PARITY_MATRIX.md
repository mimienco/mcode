# Feature Parity Matrix

The machine-readable source of truth is `parity-matrix.json`. A row becomes `done` only after current behavior is documented, the production Rust/Tauri path exists, required tests pass, and platform evidence is linked. “Implemented locally” is not equivalent to cross-platform parity.

Current summary:

| Status | Count |
| --- | ---: |
| done | 0 |
| in progress | 3 |
| not started | 33 |
| blocked | 0 |

The initial 36 rows cover lifecycle/windowing, workspaces/settings, filesystem/watcher, Git/worktrees/WSL, process/PTY/agents, SSH/remote, relay/mobile/wire, CLI/headless, updater/packaging, diagnostics/permissions, editor/terminal/rich UI, integrations, voice/computer use, i18n/theme, extensions, and session recovery.

Current Windows lifecycle evidence now includes real WebView2 Quit and Restart acknowledgement, durable checkpoint persistence, child relaunch, delivery-failure recovery, and atomic confirm-and-execute. The lifecycle row remains `in_progress` because full renderer integration, single-instance/headless behavior, tray handling, and macOS/Linux evidence are still missing.

Evidence rules:

- `currentFiles` must identify the Electron/Node implementation or behavioral tests.
- `targetRustModule` and `targetCommand` must name production code, not a planned placeholder.
- `tests` must include unit/contract coverage and E2E/platform evidence where applicable.
- Platform states are tracked independently; unavailable local platforms remain `not_started`, not assumed green.
- Any accepted exception requires a decision record and explicit user acceptance.
