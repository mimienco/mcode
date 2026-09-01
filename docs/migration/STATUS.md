# Current Migration Status

## Wave 6 progress (transport seam implementation)

- Renderer seam landed: `pty-transport-factory.ts` (`createLocalPtyTransport` selecting by `isTauri()`), `pty-transport-tauri.ts` (full (a)-row `PtyTransport` over the typed pty-client), `pty-transport-tauri-dedupe.ts` (`applyPtySeqDedupe` + `installDispatcherEventPort`), `pty-dispatcher.ts` (`PtyEventPort` + `setPtyDispatcherEventPort`, async-tolerant attach, Electron-only resync/ready guards), `pty-client.ts` (`ackData`/`getSize`), `pty-connection.ts:4026` swapped to `createLocalPtyTransport`, `terminal-pty-ack-gate.ts` `window.api?.pty?.ackData?.` guard.
- Rust command exposures landed: `pty_ack_data` + `pty_get_size` (src-tauri/src/pty_commands.rs with `*_for` seams; registered in lib.rs `generate_handler`, build.rs manifest, capabilities/pty-commands.json). Contract tests: src-tauri/tests/pty_command_contract.rs (5 tests, green).
- ConPTY exit fix (uncommitted): reader-EOF alone is not a death signal on Windows (conhost holds the output pipe open after child exit). crates/mcode-pty/src/session.rs:205 `child_exited()`, session.rs:58 `SharedMaster::close` (Option-backed; dropping the portable-pty master terminates the pseudoconsole and releases the reader), session.rs:221 `drain_after_child_exit_and_cause()`; src-tauri/src/pty_commands.rs:478 `spawn_pty_forwarder` polls child exit (pty_commands.rs:489), closes the master and joins the reader before the final drain (pty_commands.rs:519), then emits one final flush + a single `pty://exit`.
- Test evidence: vitest — factory 3, tauri-transport 8, ack-gate 10 (across files), pty-client 9, dispatcher spot checks 52 (all green); cargo test -p mcode-pty 37 green. Orchestrator full gates (workspace/clippy/fmt/tsc) pending.
- In flight: real `SmokeTestMode::Pty` run (lib.rs `Pty` variant, pty-smoke.html + pty-smoke.ts vite entry landed earlier).

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
- in progress: 15 (lifecycle, windows, shell-ui, fs.scoped, fs.watcher, git.repositories, git.worktrees, process.supervisor, pty.local, pty.streaming, ssh.transport, wire.protocol, updater.channels, packaging.release, terminal.ui)
- not started: 19
- terminal.ui moved to in_progress this wave: design + full window.api.pty.* inventory in TERMINAL_UI_INTEGRATION.md; implementation pending
- blocked: 0

## Exact next actions

1. Transport seam pass: implement pty-transport-factory.ts + pty-transport-tauri.ts per TERMINAL_UI_INTEGRATION.md §2/§4 (factory-selection + seq-dedupe unit tests, then wire pty-connection.ts:4026), plus the (b) command exposures pty_ack_data / pty_get_size and the SmokeTestMode::Pty E2E.
2. SSH command exposure: wire ssh_connect/ssh_exec/ssh_open_pty/ssh_disconnect commands over mcode-ssh, then the pty://replay channel and `remote:`-prefix guards (PTY_CONTRACT_SPEC §9.3).
3. Relay host daemon (mcode-relay host RPC) and wire-protocol v2 handshake against mobile pairing.
4. Crossgrade: settings migration path from Electron data dir; updater channel endpoints need real credentials for signed-artifact evidence.

## Session progress (2026-09-01, wave 8 — menus/tray/dialogs slice done)

- HEAD `e549fcfc2d` (adds `9c46c0781c`..`e549fcfc2d` on top of wave 7). Shell phase complete: "Menus, tray, dialogs, deep links" marked done.
- Tray: Windows tray now has Settings… (Open MCode / Settings… / — / Quit); Settings reveals the window AND pushes `ui:openSettings` + pending one-shot via `app_menu_events::open_settings`.
- Dialogs: `src-tauri/src/app_dialogs.rs` ports the Electron floating-doc pickers as `ui_pickFloatingMarkdownDocument` / `ui_pickFloatingWorkspaceDirectory` (markdown ext gate incl. trailing-separator defense, canonicalize, default floating-workspace dir; 1 test); tauri-plugin-dialog wired; commands registered (generate_handler/build.rs/main.json).
- Menu-parity survey (MenuParity scout, full JSON at agent://MenuParity): rewrite has NO Tauri application-menu builder yet — only the tray. Electron register-app-menu.ts structure documented (File/Edit/View/Window/Help; platform split; label-embedded shortcut hints, not accelerators). Remaining app-menu rows all missing (zoom, reload, appearance checkboxes, edit roles, about/updates-in-help). `ui_open_feature_tour`/`ui_open_crash_report` = Rust seam ready, renderer consumer pending. `TitlebarLeftControls` still calls `window.api.ui.popupMenu()` — no Tauri popup equivalent.
- Verification: desktop lib 16 tests, clippy 0 warnings, fmt clean; hooks suite 26 files/122 tests green (useIpcEvents changes). useAutomationDispatchEvents 6 failures = inherited from original repo (verified against D:/ADE/orca) — do not fix blind.
- PTY WIP audit (PtyWipAudit scout): uncommitted `session.rs` child-exit teardown + `pty_ack_data`/`pty_get_size` commands are fully wired (define/register/manifest/capability/permissions) and compile clean — concurrent agent's slice, committed later by them; I left it uncommitted.
- Next agent: (1) Tauri application-menu builder (port register-app-menu.ts tree; Windows/Linux: File[Settings, Exit] Edit View Window Help with autoHideMenuBar equivalent; macOS: app-named menu) fed by app_menu_events seam; add zoom/reload/appearance events; (2) popupMenu equivalent for TitlebarLeftControls; (3) real PTY smoke evidence run (still outstanding since wave 3); (4) editor content parity (Monaco/TextMate) next phase.

## Session progress (2026-09-01, wave 7 — deep links + menu intents)

- HEAD `9c781c1c43` (two commits: `7890c181b6` skill-share deep links, `9c781c1c43` menu intents + dialog plugin).
- Deep links: `tauri-plugin-deep-link` + `mcode` scheme (tauri.conf.json plugins.deep-link.desktop.schemes); `src-tauri/src/skill_share_deep_link.rs` ports `parseSkillShareId`'s mcode branch (4 unit tests), keeps a one-shot pending slot, emits `skill-share://pending`; `ui_consumePendingSkillShare` command mirrors the Electron pull. Renderer: `useIpcEvents` isTauri() branch listens + consumes on mount.
- Menu/tray intents: `src-tauri/src/app_menu_events.rs` — settings push + untimed one-shot (`ui_openSettings`, `ui_consumePendingOpenSettings`), setup-guide/feature-tour/crash-report emitters; five `ui_*` commands added to build.rs app manifest + main.json capability. `tauri-plugin-dialog` wired (native open/save for later fs slices).
- Known inherited failure: `useAutomationDispatchEvents.test.ts` 6 failures reproduce identically in the original repo (D:/ADE/orca, user WIP baseline) — not caused by these slices; do not "fix" blind. Hooks suite otherwise green (26 files / 122 tests). Desktop lib 15 tests, clippy 0 warnings, fmt clean.
- Rust gotchas hit: `tauri_plugin_deep_link::init()` (Builder is private); `.on_open_url` closure needs `app.clone()` moved in (E0505); MutexGuard has no `swap` — use `std::mem::take`.
- Next agent: (1) tray "Settings…" item should call `app_menu_events::open_settings` (windows_tray.rs currently only Open/Quit); (2) native-file open/save commands via tauri-plugin-dialog for the fs slices that used Electron dialog (floating-doc picker, SSH download save); (3) full app menu (register-app-menu.ts port — zoom/appearance/sidebar toggles) once settings store is reachable Rust-side; (4) real PTY smoke evidence run still outstanding.

## Session end handoff (2026-08-31, wave 6 — terminal transport seam)

- HEAD `94ab62cea7`: terminal-pane suite 385 files / **3741 passed | 1 skipped** (`--maxWorkers=2`; two known flaky remote-runtime stream tests time out at full parallelism — see wave-5 notes), workspace/clippy/fmt green at last full run.
- Wave-6 slice: **Tauri PTY transport seam committed** —
  - `pty-transport-factory.ts`: runtime-conditional selection (§2.1);
  - `pty-transport-tauri.ts`: Tauri command/event transport (a-row; capability optionals absent = not negotiated; `getConnectionId` returns null);
  - `pty-transport-tauri-dedupe.ts`: §2.3 seq-dedupe with restore-marker window;
  - `pty-transport-tauri-owned.ts`: owned-handler registry (ownership rule) + `unregisterPtyDataAndStatusHandlers`;
  - `pty-dispatcher.ts` split: eager pre-attach buffering extracted to `pty-dispatcher-eager-buffer.ts` (max-lines 300 gate; dispatcher re-exports the surface);
  - transport tests green (selection/dedupe/capability absence); full pane suite green.
- Gotcha for future sessions: oxlint `max-lines` is configured `skipBlankLines+skipComments` (`.oxlintrc.json:141`) — `wc -l` overcounts; check `oxlint` directly. `curly` requires braces on all ifs (one-liners rejected).
- Next agent: (1) real PTY smoke evidence run (unchanged from wave 5); (2) bind mcode-wire server to runtime id + write `mcode-runtime.json` at startup; (3) menus/tray/dialogs/deep-links slice; (4) editor content parity (Monaco/TextMate/xterm specials).

## Session end handoff (2026-08-31, wave 5)

- HEAD: workspace green (38 suites / 321+ tests), clippy 0 warnings, fmt clean, tree committed.
- Wave-5 addition: mcode-app `runtime_metadata` — the `mcode-runtime.json` CLI discovery contract (atomic write at startup, idempotent remove on exit, camelCase disk shape, authToken host-local only; 3 tests). This unblocks `mcode status`/serve wiring: the headless host writes this file, the CLI reads it.
- Next agent: (1) real PTY smoke evidence run; (2) bind mcode-wire `server` to a runtime id + write runtime metadata at startup (contract now available in mcode-app::runtime_metadata); (3) terminal-UI transport seam; (4) menus/deep-links.

## Session end handoff (2026-08-31, wave 4 — final)

- HEAD `a9976b30ac`: workspace 38 suites / 318 tests green, clippy 0 warnings, fmt clean, tree fully committed.
- Wave-4 additions: mcode-wire `agent_exec` (non-interactive relay exec: lanes, timeout clamp+tree kill, 4MiB/stream cap, CREATE_NO_WINDOW; 5 tests) and mcode-app `telemetry_consent` (full precedence port: DO_NOT_TRACK > MCODE_TELEMETRY_DISABLED > CI presence > tri-state optIn; warn-once for unrecognized kill-switch values; fail-closed pending_banner; 5 tests).
- Remaining items and their hard blockers, all external: menus/deep-links + terminal-UI (transport-seam pass over 42 call sites, next session's first slice), voice/sherpa + computer-use (native FFI + macOS/Windows entitlement work), installer/signing/crossgrade (needs real signing credentials — no simulated evidence permitted), Homebrew/AUR (needs published artifacts), CI matrix (needs the CI hosts). Each is fully specified in its contract doc.

## Session end handoff (2026-08-31, wave 3 — final for this session)

- HEAD `8d3807880a`: workspace 38 suites / 308 tests green, clippy 0 warnings, fmt clean, zero non-inherited working-tree deltas. Working tree is fully committed.
- Wave-3 additions: mcode-wire `server` (headless relay accept path), mcode-wire `pairing` (deep-link port), mcode-settings `crossgrade_discovery`, PTY smoke harness + vite entry.
- Terminal-UI, menus/deep-links, relay/agent-exec, and the release/cutover phases are specified and ready for follow-on sessions: each has its contract doc (PTY_CONTRACT_SPEC.md, TERMINAL_UI_INTEGRATION.md, DATA_AND_SETTINGS_MIGRATION.md) and the handoff notes above name the exact next commands and environment variables.

## Session end handoff (2026-08-31, wave 2)

- `1ccdfbdf7f` @ workspace: 38 suites / 308 tests green, clippy 0 warnings, fmt clean.
- New: mcode-wire `pairing` (deep-link encode/decode + schema validation, 8 tests), mcode-wire `server` (headless relay accept: sentinel → pre-dispatch handshake → framed dispatch; handshake decoder capped at 1 frame/turn so the post-handshake tail stays buffered for drain — the borrow-safe equivalent of the TS callback drain), `mcode-settings::crossgrade_discovery` (Electron %APPDATA%/MCode location via marker files, read-only, 4 tests), PTY smoke harness (pty-smoke.html + renderer driver; Rust only polls the renderer-written verdict marker).
- Next agent: (1) run the PTY smoke for real (`MCODE_TAURI_SMOKE_PTY_MARKER`/`_ECHO` env + tauri debug build) and record evidence; (2) wire runtime-metadata advertising into mcode-wire/server (mcode-runtime.json contract in shared/runtime-bootstrap.ts); (3) terminal UI seq-dedupe consume pass per TERMINAL_UI_INTEGRATION.md.

## Session end handoff (2026-08-31)

- Branch `rewrite/tauri-full` @ `a256b6f643`: workspace 294 tests / 0 failed across 38 suites, clippy 0 warnings, fmt clean, web typecheck = original baseline superset (21 vs 33).
- New this session: `mcode-wire` (frame codec, E2EE crypto_box/secretbox, v2 framing, handshake, golden vectors, 21 tests), `mcode-ssh` (russh transport, host-key verifier/store/known_hosts, 29 tests), `mcode-app::single_instance` (named-mutex guard, 4 tests), typed `pty-client.ts` + 5 `pty_*` commands + capability file, fs-client routing consolidation, inherited typecheck repairs, TERMINAL_UI_INTEGRATION.md (terminal.ui seam design + full window.api.pty.* inventory).
- Original `D:/ADE/orca` integrity re-verified: HEAD moved externally (user work) but all migration work stayed in the rewrite; manifest/snapshot preserved under `docs/migration/source-snapshot/`.
- Next agent: execute the **transport seam pass** per TERMINAL_UI_INTEGRATION.md — factory + Tauri transport + unit tests, wire `pty-connection.ts:4026`, (b) command exposures, pty smoke E2E. Pane serializers/delivery stay deferred (doc §3). SSH command exposure and relay host follow.
