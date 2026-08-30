# Feature Parity Matrix

The machine-readable source of truth is `parity-matrix.json`. A row becomes `done` only after current behavior is documented, the production Rust/Tauri path exists, required tests pass, and platform evidence is linked. “Implemented locally” is not equivalent to cross-platform parity.

Current summary:

| Status      | Count |
| ----------- | ----: |
| done        |     0 |
| in progress |     7 |
| not started |    29 |
| blocked     |     0 |

The initial 36 rows cover lifecycle/windowing, workspaces/settings, filesystem/watcher, Git/worktrees/WSL, process/PTY/agents, SSH/remote, relay/mobile/wire, CLI/headless, updater/packaging, diagnostics/permissions, editor/terminal/rich UI, integrations, voice/computer use, i18n/theme, extensions, and session recovery.

Current Windows lifecycle evidence now includes real WebView2 Quit and Restart acknowledgement, durable checkpoint persistence, child relaunch, delivery-failure recovery, atomic confirm-and-execute, close-to-tray suppression during every shutdown phase, and a production-source gate that routes every full-renderer restart/relaunch action through the runtime lifecycle client. Native update application now has a distinct lifecycle intent and backend install path. The lifecycle row remains `in_progress` because single-instance/headless/crash-recovery behavior and macOS/Linux evidence are still missing.

Windows windowing now restores validated on-screen inner bounds and maximized state, persists normal placement with Electron-compatible guards, updates the close-to-tray preference transactionally, and has a real WebView2 close/hide smoke with a confirmed installed tray. Windows shell UI is also `in_progress`: immediate tray installation plus page-ready and 12-second retries, Open/left-click restore, and lifecycle-aware Quit paths exist, while real tray-menu interaction evidence, the one-time hidden-window notification, attention/activity icon state, menus/dialogs/deep links, and non-Windows shell evidence remain open.

Updater and packaging are now `in_progress`: Rust owns strict five-channel feed configuration, native signature verification, state/progress, verified bytes, and lifecycle-authorized install; Tauri updater artifacts are enabled; production renderer callers route through the runtime client; and an isolated Windows WebView2 smoke proves an unconfigured build fails explicitly. No production feed/key or signed release artifact exists yet, so real download/install, crossgrade, rollback, release publication, and macOS/Linux evidence remain open.

Scoped filesystem is now `in_progress`: local listing, stat, and existence probes run through capability-directory handles with atomic live-root replacement/revocation; renderer ownership preserves direct SSH and remote-runtime routes; and a Windows WebView2 smoke proves authorized reads and outside-root denial. Bounded content reads, writes, linked-worktree verification, external grants, watchers, native SSH, and macOS/Linux evidence remain open.

The Electron/Node removal gate now also has a source-authoritative renderer import ledger: all three captured production entries resolve through 4,716 renderer/shared modules with zero reachable Node/Electron imports. Four Node imports remain only in two exact, unreachable test-support modules; this inventory is evidence for the removal gate but does not by itself advance a feature row.

Evidence rules:

- `currentFiles` must identify the Electron/Node implementation or behavioral tests.
- `targetRustModule` and `targetCommand` must name production code, not a planned placeholder.
- `tests` must include unit/contract coverage and E2E/platform evidence where applicable.
- Platform states are tracked independently; unavailable local platforms remain `not_started`, not assumed green.
- Any accepted exception requires a decision record and explicit user acceptance.
