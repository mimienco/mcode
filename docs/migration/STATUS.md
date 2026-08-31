# Current Migration Status

## Current phase

Wave 4 complete — wire protocol (mcode-wire, 21 tests), SSH transport (mcode-ssh, 29 tests), the pty command layer (src-tauri pty_spawn/pty_write/pty_resize/pty_kill/pty_has_pty + pty://data / pty://exit events over mcode-pty), and the single-instance guard (crates/mcode-app/src/single_instance.rs, 4 unit tests) are landed; TERMINAL_UI_INTEGRATION.md maps the renderer terminal stack onto the typed pty-client (`src/renderer/src/desktop/pty-client.ts`) and defines the transport-seam pass.

## Last green commit

`a256b6f643` (feat(tauri): add ssh transport, wire protocol, and pty command layer).

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
- Wave 4: mcode-ssh transport (russh, 29 tests), mcode-wire protocol (21 tests incl. golden vectors), mcode-app single_instance guard (4 tests), src-tauri pty command layer (pty_commands.rs: 5 commands + pty://data chunked streaming + pty://exit, camelCase DTOs, bounded 5 s kill flush) with the typed renderer pty-client (7 vitest tests).
- TERMINAL_UI_INTEGRATION.md: full inventory of 24 distinct window.api.pty.* methods (58 call sites) across pty-transport.ts / pty-connection.ts / pty-dispatcher.ts with per-method disposition — 8 methods ride the new pty-client seam now, 5 are thin command exposures over existing mcode-pty/mcode-process capabilities, 11 (35 call sites: pane serializers, delivery resync, hidden-delivery gate, viewport authority) stay on Electron wiring until their Rust slices; transport seam = one factory swap at pty-connection.ts:4026 preserving PtyTransport; seq-dedupe rule (drop startSeq < processedChars) mapped onto terminal-pty-ack-gate cumulative ACK; verification plan (factory-selection + seq-dedupe unit tests, pty smoke E2E spawning cmd /c echo).

## Tests executed

- cargo test --workspace: PASS (294 tests, 0 failed) including mcode-wire (21) and mcode-ssh (29) suites.
- cargo clippy --workspace --all-targets --all-features: PASS (0 warnings) after fixing SshError From<russh::Error>, pass-scoped known_hosts decision, and unused imports.
- cargo fmt --all -- --check: PASS.
- GOMAXPROCS=4 tsc -p config/tsconfig.web.json: 21 errors — every one byte-identical to the immutable original's baseline (verified by running the same command in D:/ADE/orca: 33 errors, superset of ours).
- vitest runtime-file-client suites: PASS (22 + 46 + 43 tests across the five files); fs-client: PASS (7); lifecycle-smoke-client: PASS (2); native-chat MessageList: PASS (2).
- Full SHA-256 backup verification: PowerShell Get-FileHash sweep (Git-Bash sha256sum -c reads CRLF files in text mode and false-fails; documented in DECISIONS).
- GOMAXPROCS=4 tsc -p config/tsconfig.web.json re-verified after wave 4: 21 errors, unchanged from the immutable original's baseline.

## Current blockers

- Terminal UI (xterm) not yet connected to the Rust streaming contract — seam design in TERMINAL_UI_INTEGRATION.md; the transport-factory + Tauri-transport implementation pass is next (pane-serializer/delivery surfaces deliberately deferred there).
- SSH, relay, mobile pairing, updater crossgrade, telemetry, voice/computer-use, i18n gates, and cross-platform (macOS/Linux) evidence remain unported.
- The copied Electron baseline still has 4 failing terminal-perf E2E scenarios (pre-existing, recorded in BENCHMARK_BASELINE.md).

## Parity summary

- done: 2 (session.recovery, git.wsl-paths)
- in progress: 10 (lifecycle, windows, fs.scoped, fs.watcher, git.repositories, git.worktrees, ssh.transport, wire.protocol, updater.channels, packaging.release)
- not started: 25
- terminal.ui: in_progress (design + inventory in TERMINAL_UI_INTEGRATION.md; implementation pending)
- blocked: 0

## Exact next actions

1. Transport seam pass: implement pty-transport-factory.ts + pty-transport-tauri.ts per TERMINAL_UI_INTEGRATION.md §2/§4 (factory-selection + seq-dedupe unit tests, then wire pty-connection.ts:4026), plus the (b) command exposures pty_ack_data / pty_get_size and the SmokeTestMode::Pty E2E.
2. SSH command exposure: wire ssh_connect/ssh_exec/ssh_open_pty/ssh_disconnect commands over mcode-ssh, then the pty://replay channel and `remote:`-prefix guards (PTY_CONTRACT_SPEC §9.3).
3. Relay host daemon (mcode-relay host RPC) and wire-protocol v2 handshake against mobile pairing.
4. Crossgrade: settings migration path from Electron data dir; updater channel endpoints need real credentials for signed-artifact evidence.
1. Wire pty_spawn/pty_write/pty_resize/pty://data commands + typed terminal client into src-tauri (byte-owner and batcher are ready in mcode-pty).
2. Port SSH transport (ssh2 -> russh decision per ADR) and remote worktrees.
3. Port relay host + wire protocol with golden test vectors.
4. Connect terminal UI (xterm.js) to pty://data with seq-based dedupe.
5. Crossgrade: settings migration path from Electron data dir; updater channel endpoints need real credentials for signed-artifact evidence.

## Session end handoff (2026-08-31)

- Branch `rewrite/tauri-full` @ `a256b6f643`: workspace 38 suites / 290+ tests green, clippy 0 warnings, fmt clean, web typecheck = original baseline superset (21 vs 33).
- New this session: `mcode-wire` (frame codec, E2EE crypto_box/secretbox, v2 framing, handshake, 29 golden vectors), `mcode-ssh` (russh transport, host-key verifier/store/known_hosts, 29 tests), `mcode-app::single_instance` (named-mutex guard, 4 tests), typed `pty-client.ts` + 5 `pty_*` commands + capability file, fs-client routing consolidation, inherited typecheck repairs.
- Original `D:/ADE/orca` integrity re-verified: HEAD moved externally (user work) but all migration work stayed in the rewrite; manifest/snapshot preserved under `docs/migration/source-snapshot/`.
- Next agent: start at **Terminal UI wiring** — attach xterm.js to `pty://data` with seq-based dedupe (PTY_CONTRACT_SPEC §5), then route the 42 `window.api.pty.*` renderer call sites through `createTauriPtyClient`. Menus/deep-links and settings-crossgrade follow.
