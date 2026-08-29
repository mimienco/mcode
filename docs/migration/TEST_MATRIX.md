# Test Matrix

| Lane | Windows | macOS | Linux | Current state |
| --- | --- | --- | --- | --- |
| Rust fmt/clippy/unit/integration | required | required | required | Windows checkpoint green: fmt, desktop all-target/all-feature clippy `-D warnings`, 22 settings contracts, 13 lifecycle contracts, 8 native shutdown-command contracts, and 1 desktop error-contract unit test |
| TypeScript typecheck, oxlint, oxfmt, Vitest | required | required | required | migration-owned desktop Vitest green: 8 files, 34 tests; oxlint/oxfmt green; baseline typecheck has the same 10 errors outside migration-owned files |
| Tauri lifecycle/workspace golden E2E | WebView2 | WKWebView | WebKitGTK | Targeted real Windows Quit and Restart protocol smoke green: receipt ACK, durable checkpoint, atomic execution, child relaunch marker, clean process/port exit; full production-renderer journey and macOS/Linux lanes remain open |
| Monaco/xterm/IME/clipboard/drag/keyboard | required | required | required | Electron terminal perf: 9 pass, 4 fail, 1 skip, 7 not run |
| PTY/process load, recovery, kill tree | ConPTY/Job Objects | Unix PTY/process groups | Unix PTY/process groups | not started |
| Git 2.25/worktrees/long paths/WSL | required + WSL | required | required | not started |
| SSH Docker and native agent behavior | required | required | required | current harnesses inventoried |
| Wire/pairing cross-version goldens | required | required | required | not started |
| Installer/signing/update/crossgrade | required | required | required | not started |
| glibc floor | n/a | n/a | Ubuntu 20.04 floor | not started |
| terminal/startup/RAM/package benchmarks | same machine | same machine | same machine | Electron terminal baseline partial/red; startup/RAM/package pending |
| localization/chunk/max-lines/dead-code/reliability gates | required | required | required | migration wiring pending |
| Electron/Node surface inventory | source-authoritative | source-authoritative | source-authoritative | AST generator tests and deterministic full-original `--check` green: 7,763 production files, 1,935 operations, 978 channels, 20 reconciled factories, 0 unresolved dynamic entries, 0 parse failures |

Core automated journey: install/start, open workspace, detect repository, create worktree, launch agent, stream/input terminal, observe file/watch/diff, stop cleanly, restart/restore, connect SSH, pair mobile, stream remotely, and apply a controlled update.

No platform is marked green from compilation alone, and no test may pass solely by skipping the relevant code path.
