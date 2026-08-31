# Current Migration Status

## Current phase

Phase 6 in progress — wire protocol (mcode-wire) and SSH transport (mcode-ssh) ported with green suites; inherited typecheck baseline repaired to match the immutable original exactly.

## Last green commit

`ea2b757749` (feat(ssh): port SSH transport to mcode-ssh on russh).

## Completed in this session

- Recovered prior migration state; verified toolchain (rustc 1.98.0, pnpm 10.24.0) and original integrity (HEAD/branch/diffs/manifest unchanged; benign user-driven working-tree drift documented in ORIGINAL_INTEGRITY.md).
- Full IPC surface enumerated: 1,935 operations, 978 channels, 88 families; NODE_BUILTIN_INVENTORY.md documents renderer Node coupling (90 production files with hard coupling; 713 files reference window.api) with exact allowlist.
- FS_CONTRACT_SPEC.md (29 fs channels, security invariants, watcher must-carry vs Electron glue) and PTY_CONTRACT_SPEC.md (40+ pty channels, binary streaming design: seq/bounded queues/backpressure planes/2ms batcher/salvage/reload recovery) written with file:line evidence from the original.
- crates/mcode-platform: tray (idempotent install, event seam, 7 tests) + window placement (bounds persistence, close-to-tray gating, 5 tests) + WSL home helpers (restored).
- crates/mcode-app: startup restore service (build_restore_plan over persisted root + checkpoint; tombstone/lease precedence; 10 tests; 18 lifecycle tests stay green).
- crates/mcode-fs: scoped FS per spec — authorization registry (LeafPolicy, LRU 4096, traversal/symlink invariants), atomic writes, search (rg + cancellation tokens), watcher (150ms/500ms debounce, coalescing, 5000-cap overflow, ignore rules, WSL-ready) — 62 tests.
- crates/mcode-git: controlled git-CLI execution — capability cache, porcelain v2 status, stage/commit/discard/diff/history/fetch/pull/push, worktree create/list/remove/prune/lock/repair with safety gates, long-path + WSL path rules — 59 tests.
- crates/mcode-process: supervisor with Job Objects (Windows) / process-group SIGKILL (POSIX), graceful-then-force ladder, env scrub list, descendant scan, ExitCause normalization — 25 tests.
- crates/mcode-pty: local PTY via portable-pty (ConPTY), byte owner (u64 char sequence, bounded pending max(2MiB, rows*120), query salvage, restore markers, cumulative ACK credit with per-pty 512KiB/global 8MiB gates), 2ms/16KiB batcher with interactive carve-out, DSR auto-answer for ConPTY handshake, shell selection chain — 37 tests including 2 real ConPTY integration tests.
- src-tauri: 17 fs_* commands wired with capability-gated ACL (dedicated fs-commands.json, no defaults), NativeFilesystemState seeded from settings scope roots, fs://changed event forwarding with per-window teardown, smoke-mode filesystem probe.
- Renderer: typed fs-client (17 methods, DTO mirror, runtime selection, 7 vitest tests); updater state machine (mcode-updater crate, 11 tests) with UpdateInstall routed through the shutdown protocol.
- Disk incident: 10 GB stale target-migration-lifecycle dir removed (drive was 100% full); build verified after cleanup.

## Tests executed

- cargo test --workspace: PASS (38 suites, 290 tests, 0 failed) including new mcode-wire (29 golden-vector tests) and mcode-ssh (29 tests) suites.
- cargo clippy --workspace --all-targets --all-features: PASS (0 warnings) after fixing SshError From<russh::Error>, pass-scoped known_hosts decision, and unused imports.
- cargo fmt --all -- --check: PASS.
- GOMAXPROCS=4 tsc -p config/tsconfig.web.json: 21 errors — every one byte-identical to the immutable original's baseline (verified by running the same command in D:/ADE/orca: 33 errors, superset of ours).
- vitest runtime-file-client suites: PASS (22 + 46 + 43 tests across the five files); fs-client: PASS (7); lifecycle-smoke-client: PASS (2); native-chat MessageList: PASS (2).
- Full SHA-256 backup verification: PowerShell Get-FileHash sweep (Git-Bash sha256sum -c reads CRLF files in text mode and false-fails; documented in DECISIONS).

## Current blockers

- Terminal UI (xterm/Monaco) not yet connected to the Rust streaming contract — the byte-owner API exists; renderer consume pass is next.
- SSH, relay, mobile pairing, updater crossgrade, telemetry, voice/computer-use, i18n gates, and cross-platform (macOS/Linux) evidence remain unported.
- The copied Electron baseline still has 4 failing terminal-perf E2E scenarios (pre-existing, recorded in BENCHMARK_BASELINE.md).

## Parity summary

- done: 2 (session.recovery, git.wsl-paths)
- in progress: 9 (lifecycle, windows, fs.scoped, fs.watcher, git.repositories, git.worktrees, updater.channels, packaging.release, + terminal-adjacent)
- not started: 25
- blocked: 0

## Exact next actions

1. Wire pty_spawn/pty_write/pty_resize/pty://data commands + typed terminal client into src-tauri (byte-owner and batcher are ready in mcode-pty).
2. Port SSH transport (ssh2 -> russh decision per ADR) and remote worktrees.
3. Port relay host + wire protocol with golden test vectors.
4. Connect terminal UI (xterm.js) to pty://data with seq-based dedupe.
5. Crossgrade: settings migration path from Electron data dir; updater channel endpoints need real credentials for signed-artifact evidence.
