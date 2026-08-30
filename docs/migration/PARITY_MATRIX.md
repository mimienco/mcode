# Feature Parity Matrix

The machine-readable source of truth is `parity-matrix.json`. A row becomes `done` only after current behavior is documented, the production Rust/Tauri path exists, required tests pass, and platform evidence is linked. “Implemented locally” is not equivalent to cross-platform parity.

Current summary:

| Status      | Count |
| ----------- | ----: |
| done        |     0 |
| in progress |     3 |
| not started |    33 |
| blocked     |     0 |

The initial 36 rows cover lifecycle/windowing, workspaces/settings, filesystem/watcher, Git/worktrees/WSL, process/PTY/agents, SSH/remote, relay/mobile/wire, CLI/headless, updater/packaging, diagnostics/permissions, editor/terminal/rich UI, integrations, voice/computer use, i18n/theme, extensions, and session recovery.

Current Windows lifecycle evidence now includes real WebView2 Quit and Restart acknowledgement, durable checkpoint persistence, child relaunch, delivery-failure recovery, atomic confirm-and-execute, and a production-source gate that routes every full-renderer restart/relaunch action through the runtime lifecycle client. The lifecycle row remains `in_progress` because the Tauri updater apply path, single-instance/headless behavior, tray handling, and macOS/Linux evidence are still missing.

The Electron/Node removal gate now also has a source-authoritative renderer import ledger: all three captured production entries resolve through 4,716 renderer/shared modules with zero reachable Node/Electron imports. Four Node imports remain only in two exact, unreachable test-support modules; this inventory is evidence for the removal gate but does not by itself advance a feature row.

Evidence rules:

- `currentFiles` must identify the Electron/Node implementation or behavioral tests.
- `targetRustModule` and `targetCommand` must name production code, not a planned placeholder.
- `tests` must include unit/contract coverage and E2E/platform evidence where applicable.
- Platform states are tracked independently; unavailable local platforms remain `not_started`, not assumed green.
- Any accepted exception requires a decision record and explicit user acceptance.
