# Current Migration Status

## Current phase

Phase 3 – initial Rust/Tauri lifecycle and window bootstrap verified on Windows.

## Last green commit

`6740f9c538120796b687ffe2b6f798b488cdaf76` (copied source HEAD; no migration commit yet)

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
- `node --test --experimental-strip-types src/renderer/src/desktop/desktop-client.node-test.ts`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace`: PASS (2 contract tests).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PASS.
- `corepack pnpm install --frozen-lockfile` in rewrite: PASS in 17m19s with pnpm 10.24.0.
- `corepack pnpm run build:tauri-renderer`: PASS (10,642 modules, 1m47s first run).
- Debug WebView2 smoke launch: PASS, exit 0 in 6.4s with isolated data and no forced stop.
- `corepack pnpm exec tauri build --debug --no-bundle --ci`: PASS; native executable produced after a 2m28s native build.
- Post-Tauri-bootstrap original integrity: PASS (16,748 present hashes, 3 expected deletions, 0 mismatches).

## Current blockers

- The copied Electron baseline is not fully green: production build is blocked by existing type errors and four terminal-performance E2E scenarios fail.
- The Tauri shell currently exposes only the lifecycle context command. The existing renderer still depends broadly on `window.api`, so the shell is not feature-complete or parity-ready.
- macOS signing/notarization, Linux WebKitGTK/glibc-floor, and cross-platform installer evidence require their respective CI hosts and credentials.

## Parity summary

- done: 0
- in progress: 2
- not started: 34
- blocked: 0

## Exact next actions

1. Finish the machine-readable Electron/Node surface inventory and map dynamic IPC registrations.
2. Route renderer startup through the typed desktop client and port one complete user-visible workflow.
3. Design and test scoped filesystem and process supervision contracts before adding commands.
4. Add Windows lifecycle E2E assertions without using the debug-only self-exit harness as parity evidence.
