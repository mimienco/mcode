# Current Migration Status

## Current phase

Phase 3 – confirmed manual close plus app-wide Quit/Restart acknowledgement with a typed, durable Tauri shutdown checkpoint on Windows.

## Last green commit

`7284f6377f` (`fix(tauri): make shutdown execution atomic`), following renderer-ack checkpoints `11399f6cc0`, `eb8c0f4b55`, and `36d1b85d50`.

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
- Routed native close requests and confirmations through the runtime-selected typed desktop client, including late-subscription cleanup and rejected-handler containment.
- Preserved Electron's synchronous beforeunload checkpoint while adding a Tauri finalizer that runs only after Save/Discard and child-process decisions.
- Added `mcode-settings`, a serialized settings service that merges versioned shutdown snapshots without replacing unrelated or host-partitioned state.
- Made the Tauri checkpoint durable with a same-directory atomic replacement, file sync, bounded input/output, and exact preservation on validation or parse failure.
- Moved blocking checkpoint I/O and serialization onto Tauri's blocking worker lane.
- Rejected unknown UI checkpoint fields, invalid execution-host IDs, untyped session roots, and invalid nested workspace-session records at the Rust DTO boundary.
- Reconciled stale renderer snapshots in local and runtime-host partitions against host-owned topology, applied prior or incoming retirement tombstones before binding preservation, and prevented terminated SSH leases from being resurrected.
- Added a shared Rust/TypeScript v1 fixture that covers every workspace-session field, top-level view, mirrored nested enum, TUI-agent variant, resumable-agent variant, and representative execution-host identity.
- Kept the window open and displayed distinct localized errors for checkpoint durability failure and native close-confirmation failure.
- Completed the required two-axis standards/spec review and committed its fixes separately; broader quit/tray/restore requirements remain open.
- Added a versioned app-wide Quit/Restart protocol with exact request IDs, immediate renderer receipt acknowledgement, durable checkpoint confirmation, cancellation, and a bounded 10-second pre-receipt fallback.
- Made confirm-and-execute one backend command so a renderer failure cannot strand a confirmed request between two IPC calls; confirmed requests also remain force-claimable as a final safety net.
- Added a forced native Quit recovery when the main window is missing or event delivery fails after Tauri has already prevented the original exit.
- Normalized structured Rust command failures into a validated TypeScript `DesktopCommandError` with typed code/context and a safe user-facing message.
- Routed dirty-file Cancel back through the native request cancellation path and moved the native lifecycle subscription ahead of later renderer bootstrap work.
- Ran isolated real Windows WebView2 Quit and Restart flows: both wrote the shutdown checkpoint; Restart relaunched a child that wrote `relaunched`; no native process or dev port remained.
- Re-ran the two-axis review at `7284f6377f`: Standards PASS and Spec PASS with no remaining shutdown-slice findings.
- Replaced the preliminary IPC regex counts with a deterministic Babel-TypeScript AST inventory of the immutable original.
- Correlated 1,935 operations and 978 channels; reconciled all 20 dynamic factories against explicit call sites with zero unresolved entries or parse failures.
- Proved complete command pairing (no registrations without callers and no callers without registrations) and recorded the two subscription-only legacy event channels.

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
- Focused lifecycle/window/client/session-contract Vitest: PASS (12 files, 80 tests).
- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace`: PASS after checkpoint reconciliation (22 settings, 3 Tauri command/capability, and 1 lifecycle contract test).
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
- Current lifecycle-checkpoint source integrity: PASS (16,751 entries; 16,748 present hashes, 3 expected deletions, 0 mismatches).
- Current Tauri renderer rebuild: infrastructure FAIL on three attempts because unrelated host processes left insufficient memory (latest failure while allocating 196 KiB); prior checkpoint output was not reused as current evidence.
- Current renderer typecheck: baseline FAIL with 10 errors outside migration-owned files and none in this slice.
- `cargo test -p mcode-app -j1 --test lifecycle_contract`: PASS (13 tests).
- `cargo test -p mcode-desktop --test app_shutdown_command_contract -j1`: PASS (8 tests).
- `cargo test -p mcode-desktop --lib -j1`: PASS (1 test).
- `cargo check -p mcode-desktop --tests -j1`: PASS.
- `cargo clippy -p mcode-desktop --all-targets --all-features -j1 -- -D warnings`: PASS.
- Current migration-owned desktop Vitest: PASS (8 files, 34 tests); oxlint and oxfmt checks PASS.
- Isolated WebView2 Quit smoke: PASS; receipt ACK, durability confirmation, exit 0, and checkpoint verified.
- Isolated WebView2 Restart smoke: PASS; receipt ACK, durability confirmation, child relaunch marker `relaunched`, checkpoint, exit 0, and no remaining process verified.
- Final original-integrity guard: PASS (`unchanged: true`; 16,748 checked, 3 expected missing, 0 unexpected or mismatched).
- `node --test config/scripts/generate-electron-ipc-inventory.node-test.mjs`: PASS (3 tests).
- Full original IPC generation plus `--check`: PASS (7,763 production files, 1,935 operations, 978 channels, 20 reconciled factories, 0 dynamic, 0 parse failures).

## Current blockers

- The copied Electron baseline is not fully green: production build is blocked by existing type errors and four terminal-performance E2E scenarios fail.
- The Tauri shell exposes the lifecycle context command and typed custom-window controls. The rest of the renderer still depends broadly on `window.api`, so the shell is not feature-complete or parity-ready.
- Tauri manual close and app-wide native Quit/Restart now use the existing decision flow and await an atomic shutdown checkpoint. Updater-triggered restart integration in the full production renderer, minimize-to-tray, and startup restore remain incomplete.
- macOS signing/notarization, Linux WebKitGTK/glibc-floor, and cross-platform installer evidence require their respective CI hosts and credentials.

## Parity summary

- done: 0
- in progress: 3
- not started: 33
- blocked: 0

## Exact next actions

1. Finish the separate production renderer Node-built-in/import inventory using an exact allowlist.
2. Connect updater-triggered restart and the full production renderer to the proven acknowledgement/durability protocol.
3. Port Windows minimize-to-tray and startup restore, then expand the real WebView2 harness beyond the targeted lifecycle page.
4. Use the correlated IPC ledger to prioritize and specify scoped filesystem/process contracts.
