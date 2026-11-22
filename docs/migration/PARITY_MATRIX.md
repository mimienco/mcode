# Feature Parity Matrix

The machine-readable source of truth is `parity-matrix.json`. A row becomes `done` only after current behavior is documented, the production Rust/Tauri path exists, required tests pass, and platform evidence is linked. “Implemented locally” is not equivalent to cross-platform parity.

Current summary:

| Status      | Count |
| ----------- | ----: |
| done        |     2 |
| in progress |    32 |
| not started |     3 |
| blocked     |     0 |

The current 37 rows cover lifecycle/windowing, workspaces/settings, filesystem/watcher, Git/worktrees/WSL, process/PTY/agents, SSH/remote, relay/mobile/wire, CLI/headless, updater/packaging, diagnostics/permissions, editor/terminal/rich UI, integrations, voice/computer use, i18n/theme, extensions, and session recovery. Counts reflect the machine-readable ledger; they are not a percentage of working product behavior or release approval.

As of 2026-09-08, `workspace.session` is `in_progress`: native local catalog/picker/open, Git/folder discovery, project projection and async host-partitioned session persistence are connected to renderer adapters. New roots require an actual native-picker grant or existing persisted authorization; remote execution is explicitly gated. See [LOCAL-WORKSPACE-SLICE-2026-09-07.md](LOCAL-WORKSPACE-SLICE-2026-09-07.md) and [REPAIR-REVIEW-2026-09-08.md](REPAIR-REVIEW-2026-09-08.md). Fresh Rust/native gates, a complete restart/restore workflow, remaining mutations and macOS/Linux evidence are pending. No other feature-row status changed in this update.

Current Windows lifecycle evidence now includes real WebView2 Quit and Restart acknowledgement, durable checkpoint persistence, child relaunch, delivery-failure recovery, atomic confirm-and-execute, close-to-tray suppression during every shutdown phase, and a production-source gate that routes every full-renderer restart/relaunch action through the runtime lifecycle client. Native update application now has a distinct lifecycle intent and backend install path. The lifecycle row remains `in_progress` because single-instance/headless/crash-recovery behavior and macOS/Linux evidence are still missing.

Windows windowing now restores validated on-screen inner bounds and maximized state, persists normal placement with Electron-compatible guards, updates the close-to-tray preference transactionally, and has a real WebView2 close/hide smoke with a confirmed installed tray. Windows shell UI is also `in_progress`: immediate tray installation plus page-ready and 12-second retries, Open/left-click restore, and lifecycle-aware Quit paths exist, while real tray-menu interaction evidence, the one-time hidden-window notification, attention/activity icon state, menus/dialogs/deep links, and non-Windows shell evidence remain open.

Updater and packaging are now `in_progress`: Rust owns strict five-channel feed configuration, native signature verification, state/progress, verified bytes, and lifecycle-authorized install; Tauri updater artifacts are enabled; production renderer callers route through the runtime client; and an isolated Windows WebView2 smoke proves an unconfigured build fails explicitly. No production feed/key or signed release artifact exists yet, so real download/install, crossgrade, rollback, release publication, and macOS/Linux evidence remain open.

The Electron/Node removal gate now also has a source-authoritative renderer import ledger: all three captured production entries resolve through 4,716 renderer/shared modules with zero reachable Node/Electron imports. Four Node imports remain only in two exact, unreachable test-support modules; this inventory is evidence for the removal gate but does not by itself advance a feature row.

Evidence rules:

- `currentFiles` must identify the Electron/Node implementation or behavioral tests.
- `targetRustModule` and `targetCommand` must name production code, not a planned placeholder.
- `tests` must include unit/contract coverage and E2E/platform evidence where applicable.
- Platform states are tracked independently; unavailable local platforms remain `not_started`, not assumed green.
- Any accepted exception requires a decision record and explicit user acceptance.
