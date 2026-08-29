# Current Migration Status

## Current phase

Phase 3 – typed custom-window controls routed through Electron/Tauri runtime clients on Windows.

## Last green commit

`46e1837066` (`feat(tauri): bootstrap typed desktop lifecycle shell`), following preservation checkpoint `3b21fb44b7`.

## Completed in this session

- Resolved source, backup, and rewrite paths.
- Captured source HEAD, branch, remotes, stashes, status, diffs, and SHA-256 manifest.
- Created independent backup and rewrite copies without overwriting existing directories.
- Verified both copies: 16,748 present files matched, three tracked deletions were expected, zero hashes mismatched.
- Created and checked out `rewrite/tauri-full` only in the rewrite copy.
- Read repository instructions, design rules, package/workspace configuration, Electron main/preload entry points, and the primary release pipeline.
- Added a test-driven source-integrity checker and confirmed the original source remains unchanged.
- Initialized the migration documentation and parity ledger.
- Restored the Electron dependency graph in the backup with the pinned pnpm version.
- Captured targeted unit/configuration results, typecheck/build viability, and the real Electron terminal-performance suite.
- Re-verified the original after the baseline phase.
- Installed and pinned Rust 1.98.0 with rustfmt/clippy for the MSVC target.
- Added the Cargo workspace, Tauri shell/configuration/capability, typed renderer client, and the first lifecycle command.
- Built the real 10,642-module React renderer through the Tauri-specific Vite configuration.
- Compiled, linted, contract-tested, smoke-launched, and CLI-built the initial Windows Tauri shell.
- Marked all 16,748 present manifest files read-only in the Electron backup after baseline capture.
- Created local, hook-validated checkpoint commits on `rewrite/tauri-full`; nothing was pushed.
- Replaced every direct preload call in `WindowControls` with the typed desktop window client.
- Added lazy Electron/Tauri runtime selection so Electron does not load Tauri window code and Tauri does not call the preload facade.
- Implemented Tauri minimize, maximize/restore, maximize-state query/subscription, and interceptable close-request behavior.
- Moved migration-owned renderer tests into the repository's normal Vitest/typecheck lane.
- Added only the three non-default Tauri window mutation permissions required by this slice and locked the set with a Rust contract test.

## Tests executed

- `node --test config/scripts/verify-original-integrity.node-test.mjs`: PASS (1 test).
- `node config/scripts/verify-original-integrity.mjs --source D:\ADE\orca --snapshot D:\ADE\_migration-snapshot --json`: PASS.
- Full SHA-256 verification, backup: PASS (16,748 checked, 3 expected deletions, 0 mismatches).
- Full SHA-256 verification, rewrite: PASS (16,748 checked, 3 expected deletions, 0 mismatches).
- `corepack pnpm install --frozen-lockfile` in backup: PASS in 13m02s; Node 22 versus required Node 24 warning recorded.
- Targeted Vitest release/runtime lane: PASS (4 files, 58 passed, 4 skipped).
- `corepack pnpm typecheck` in backup: FAIL on pre-existing node/CLI/web TypeScript errors; see `BENCHMARK_BASELINE.md`.
- `corepack pnpm app:prod:build` in backup: FAIL because it gates on the same typecheck errors.
- `corepack pnpm run build:electron-vite` in backup: PASS (3,151 main and 10,642 renderer modules).
- `corepack pnpm test:e2e:terminal-perf` in backup: FAIL (9 passed, 4 failed, 1 skipped, 7 not run; 9.1m).
- Post-baseline original integrity: PASS (16,751 manifest entries).
- Focused typed window/client Vitest: PASS (3 files, 13 tests).
- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace`: PASS (3 contract tests, including the least-privilege window capability set).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PASS.
- `corepack pnpm install --frozen-lockfile` in rewrite: PASS in 17m19s with pnpm 10.24.0.
- `corepack pnpm run build:tauri-renderer`: PASS (10,642 modules, 1m47s first run).
- Debug WebView2 smoke launch: PASS, exit 0 in 6.4s with isolated data and no forced stop.
- `corepack pnpm exec tauri build --debug --no-bundle --ci`: PASS; native executable produced after a 2m28s native build.
- Post-Tauri-bootstrap original integrity: PASS (16,748 present hashes, 3 expected deletions, 0 mismatches).
- Unbounded `corepack pnpm run typecheck:web`: infrastructure FAIL; TypeScript-Go exhausted memory after spawning roughly 180 workers.
- `GOMAXPROCS=4 corepack pnpm run typecheck:web`: expected baseline FAIL only (`mission` view, nullable AddRepo test nodes, unused terminal handler, optional Jira signal); no migration-owned diagnostic.
- Permission-reviewed custom-window renderer build: PASS (10,651 modules, 3m09s inside the Tauri CLI build).
- Permission-reviewed `tauri build --debug --no-bundle --ci`: PASS; native build completed in 6m38s after the frontend hook.
- Permission-reviewed isolated WebView2 smoke: PASS, exit 0 in 8.9s, no forced stop.

## Current blockers

- The copied Electron baseline is not fully green: production build is blocked by existing type errors and four terminal-performance E2E scenarios fail.
- The Tauri shell exposes the lifecycle context command and typed custom-window controls. The rest of the renderer still depends broadly on `window.api`, so the shell is not feature-complete or parity-ready.
- Tauri close currently uses the interceptable native close event, but Electron-equivalent running-terminal confirmation, minimize-to-tray, quit acknowledgement, and session checkpoint semantics are not yet connected.
- macOS signing/notarization, Linux WebKitGTK/glibc-floor, and cross-platform installer evidence require their respective CI hosts and credentials.

## Parity summary

- done: 0
- in progress: 2
- not started: 34
- blocked: 0

## Exact next actions

1. Finish the machine-readable Electron/Node surface inventory and map dynamic IPC registrations.
2. Port close-request confirmation, minimize-to-tray, and session checkpoint behavior onto the typed window client.
3. Route renderer startup services through typed lifecycle clients, then design scoped filesystem/process contracts.
4. Add real Windows WebView2 window-behavior E2E assertions; the debug-only self-exit remains smoke evidence only.
