# Test Matrix

| Lane | Windows | macOS | Linux | Current state |
| --- | --- | --- | --- | --- |
| Rust fmt/clippy/unit/integration | required | required | required | Windows bootstrap green: fmt, clippy `-D warnings`, 2 contract tests |
| TypeScript typecheck, oxlint, oxfmt, Vitest | required | required | required | targeted Vitest green; baseline typecheck red |
| Tauri lifecycle/workspace golden E2E | WebView2 | WKWebView | WebKitGTK | Windows debug smoke and typed window-control tests green; real behavioral E2E not started |
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
