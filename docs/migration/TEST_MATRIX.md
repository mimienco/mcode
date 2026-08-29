# Test Matrix

| Lane | Windows | macOS | Linux | Current state |
| --- | --- | --- | --- | --- |
| Rust fmt/clippy/unit/integration | required | required | required | Windows checkpoint green: fmt, workspace/all-features clippy `-D warnings`, 22 settings contracts including host topology, tombstone, identity-guard, split-retirement, and revision-boundary cases, 3 Tauri contracts, 1 lifecycle contract |
| TypeScript typecheck, oxlint, oxfmt, Vitest | required | required | required | lifecycle/window/session-contract Vitest green: 12 files, 75 tests; baseline typecheck has 10 errors outside migration-owned files |
| Tauri lifecycle/workspace golden E2E | WebView2 | WKWebView | WebKitGTK | Prior Windows debug smoke green; manual-close/checkpoint unit contracts green; current package smoke memory-blocked and real behavioral E2E not started |
| Monaco/xterm/IME/clipboard/drag/keyboard | required | required | required | Electron terminal perf: 9 pass, 4 fail, 1 skip, 7 not run |
| PTY/process load, recovery, kill tree | ConPTY/Job Objects | Unix PTY/process groups | Unix PTY/process groups | not started |
| Git 2.25/worktrees/long paths/WSL | required + WSL | required | required | not started |
| SSH Docker and native agent behavior | required | required | required | current harnesses inventoried |
| Wire/pairing cross-version goldens | required | required | required | not started |
| Installer/signing/update/crossgrade | required | required | required | not started |
| glibc floor | n/a | n/a | Ubuntu 20.04 floor | not started |
| terminal/startup/RAM/package benchmarks | same machine | same machine | same machine | Electron terminal baseline partial/red; startup/RAM/package pending |
| localization/chunk/max-lines/dead-code/reliability gates | required | required | required | migration wiring pending |

Core automated journey: install/start, open workspace, detect repository, create worktree, launch agent, stream/input terminal, observe file/watch/diff, stop cleanly, restart/restore, connect SSH, pair mobile, stream remotely, and apply a controlled update.

No platform is marked green from compilation alone, and no test may pass solely by skipping the relevant code path.
