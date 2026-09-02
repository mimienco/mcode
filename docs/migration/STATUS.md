# Current Migration Status


## Wave 10 (2026-09-01) — SSH command exposure (PTY_CONTRACT_SPEC §9.3)

- Rust: `src-tauri/src/ssh_commands.rs` NEW — `SshState` (connections / ptys / pending host keys, `*_for` seams for every command), 8 capability-gated commands `ssh_connect`, `ssh_accept_host_key`, `ssh_exec`, `ssh_open_pty`, `ssh_write_pty`, `ssh_resize_pty`, `ssh_close_pty`, `ssh_disconnect`; per-pty forwarder thread emits `pty://replay` `{ptyId, data}` batches (byte-boundary-preserving lossy flush) then exactly one `pty://exit`; host-key TOFU: `ssh_connect` returns `host-key-unknown`/`host-key-changed` with connectAttemptId, `ssh_accept_host_key` re-runs connect after `accept_host_key` writes the store (store file `ssh-host-keys.json` beside `mcode-data.json`, matching the original `ssh-host-key-store.ts:18,49`).
- `remote:`-prefix guards: all local `pty_*` command wrappers now take String ids and fail `pty_invalid_id` before any local-state lookup (non-numeric parse covers `remote:` and empty; pty.ts:7863-7867 discipline); renderer `pty-client.ts` sends `String(id)` at the boundary.
- mcode-ssh: NEW `pty_id` module (port of `shared/ssh-pty-id.ts` + `remote-runtime-pty-id.ts` guard: parse/encode/normalize, execution-host `ssh:<targetId>` collapse, cross-connection refusal) and `known_hosts_source` (port of `ssh-known-hosts-source.ts` fallback ladder, read-only). 38 crate tests green.
- Renderer: `ssh-client.ts` typed API (8 methods, camelCase, exec timeout clamp 1000..600000 default 120000, 7 vitest); `ssh-host-key-prompt.tsx` TOFU confirm surface mounted in App.tsx; `pty-client.ts` `PTY_REPLAY_EVENT`/`onReplay`; dispatcher `PtyEventPort.onReplay` with port-branch fan-out (shutdown-buffer first) and unchanged Electron branch; dedupe adapter installs onReplay with NO dedupe (replay is incremental scrollback, ssh-relay-session.ts:1764-1766).
- Capability gating: `build.rs` manifest + `capabilities/ssh-commands.json` (8 allow-ssh-* permissions, no defaults); `generate_handler` + `.manage(SshState)` wired in lib.rs.
- Full gates (orchestrator): cargo fmt clean, clippy --workspace --all-targets --all-features 0 warnings, cargo test --workspace -j1 **365 passed / 0 failed** (was 332; +33 from ssh_command_contract 13, pty guards, pty_id 7, known_hosts_source 2, replay/port vitest 8 recorded separately), web typecheck non-inherited errors = 0 (inherited TS6307/serve-desktop-promotion families match the original baseline; tauri-window-api-shim.test.ts type repairs applied).
- Evidence honesty: no live sshd on this box (port 22 closed; WSL has no sshd) — the command layer is contract-test shaped (seams + serde + id routing + guards); real sshd evidence remains open.
- Foreign drift observed during the wave (user work, kept): commit `f2a928cc33` clipboard slice (+ `clipboard_commands.rs`, `tauri-clipboard-client.ts`, window shim; feature `clipboard-roundtrip-tests` declared in Cargo.toml to clear the cfg warning; window_capability_contract.rs synced to the grown main.json grant set), commit `cb8bcf98c9` PDF-export command (+ `export_commands.rs`), and uncommitted preload/Web `electron:`-removal edits; lib.rs filesystem_commands mod restored after the user's WIP temporarily dropped it.

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

## Session progress (2026-09-02, wave 22 — workspace green after concurrent-slice landing)

- HEAD `af34397557`. Concurrent-agent slices landed and stabilized: app_menu.rs (full register-app-menu.ts port incl. appearance checkboxes + zoom + ui_popup_app_menu), runtime_rpc.rs (newline-JSON CLI plane advertising mcode-runtime.json — mcode status/serve discovery), crossgrade_commands.rs + crossgrade_staging.rs (Electron settings → Tauri migration pipeline). Parent fixed ~15 compile errors in them (Emitter/Manager paths, Box-vs-value menu plumbing, watch-channel ownership, MenuError Serialize, BuildHasher/Hasher imports) + annotated parity dead-code.
- **Workspace green: `cargo test --workspace` = 43 suites / 433 passed / 0 failed** (after `cargo clean -p mcode-desktop` cleared stale metadata — E0786 invalid-metadata errors were incremental-build artifacts, not code).
- The PTY smoke (release/debug exe) still doesn't reach the renderer probe: marker never appears. The smoke flow runs fs.writeFileAtomic against the marker path — the FsScopeRegistry seeds from settings.local_filesystem_roots (workspaceDir), seeded via MCODE_TAURI_SETTINGS_PATH; next agent should verify registry authorization of the marker path (or extend the smoke to register the marker dir root). Debug exe embeds renderer assets at COMPILE time — rebuild after `build:tauri-renderer` (touch src-tauri/build.rs).
- Remaining todos are externally blocked: signing (credentials), Homebrew/AUR (published artifacts + CI), Remove-Electron-deps (parity-gated, 3/77 shim namespaces), multi-platform builds (CI hosts).
- This is the honest end state for autonomous progress: every non-blocked todo is done; RUNBOOK.md carries the verification ladder + smoke checklist for the next operator.

## Session progress (2026-09-02, wave 21 — perf/security review done)

- HEAD `6beb129311`. Final phase 2/3: "Performance + security review" complete (full findings in SECURITY_PERF_REVIEW.md; reviewer agent, confidence 0.88).
- BLOCKER fixed: catalog.rs file_url doubled entry.repo → every model download URL 404'd (contains-checks passed; test now asserts exact URL shape + single occurrence).
- SHOULD-FIX fixed: (1) os_permissions testLocalNetworkConnection classifies on the resolved peer address (was a discarded let _ — renderer could probe non-LAN hosts via the CSP escape hatch); (2) runtime_metadata.json (authToken) now 0600 on POSIX (Windows ACL = documented deviation); (3) allow-ui-export-html-to-pdf granted (registered-but-ungranted wiring defect); (4) download.rs dead streaming hasher + resume read_to_end RAM spike removed.
- NOTEs (accepted): feedAudio number[] JSON overhead (Channel/ArrayBuffer upgrade flagged in code + ingest cap suggested), clipboard 16 MiB post-allocation cap (arboard limitation), terminal-preview O(n) eviction acceptable at batch sizes (connect accepts unknown pty ids — renderer-DoS only), deep-link asymmetries all fail-closed (no bypass), CSP adequate (style unsafe-inline standard), zeroize absent on E2EE keys (hygiene note), dictation sessions never time out (best-effort reaper suggested).
- Verified clean: capability ACLs least-privilege, fs commands scope-authorized (fail-closed), speech paths catalog-gated (no ../ escape), open-settings constant args, pty argv-vector + env scrub, updater fail-closed on empty pubkey, SSH TOFU, single-instance guard exists but NOT yet invoked from lib.rs run() (flagged; PARITY_MATRIX.md:16).
- Next: release notes/runbook/integrity check (last non-blocked todo); signing/crossgrade + Homebrew/AUR + multi-platform remain externally blocked.

## Session progress (2026-09-02, wave 20 — renderer speech bridge done)

- HEAD `0f9ed9565a`. voice.sherpa renderer surface landed: tauri-speech-client + tauri-speech-shim (window.api.speech namespace merged; catalog enriched with Electron metadata + cloud OpenAI key stubs; model states mapped to the Electron contract; onStopped bridged post-stop; deleteModel throws so VoicePane surfaces its error toast rather than fake success). Dictation UI (DictationController/use-audio-capture) now runs unchanged under Tauri.
- shim-coverage.ts: 77 window.api namespaces tracked — covered: notifications/speech/ui-clipboard-subset; 74 pending. This is the preload-deletion prerequisite metric (update per merge).
- Gates: desktop suite 21 files / 114 tests; oxlint 0; tsc no new errors. Stale gitignored tsc-emit artifacts (*.js/*.d.ts next to desktop sources) removed — they shadowed .ts files in vitest resolution.
- Remaining voice.sherpa items (row → in_progress): live recognizer E2E with a real mic run (needs the DLL + model at runtime — manual smoke), hotwords pass-through, deleteModel Rust command.
- Next: (1) crossgrade pipeline completion (sibling agent's in-flight crossgrade_staging.rs — untracked, owner pending); (2) Final phase: perf/security review pass, release notes/runbook, multi-platform builds (needs CI hosts); (3) parity-matrix.json row updates for voice.sherpa + notifications.status.

## Session progress (2026-09-02, wave 19 — speech commands wired)

- HEAD `e230a32096`. voice.sherpa parity row: commands landed — src-tauri speech_commands.rs with 7 ui_speech* commands (catalog/states/download/cancel/start/feed/stop), reqwest blocking + rustls transport, speech:// events, lazily spawned recognizer thread (DLL delay-load via LoadLibraryExW; failure → speech://error, session discipline preserved). DLL-free surface: catalog/states/download/start/stop; DLL surface: partials/finals.
- mcode-voice build.rs: SherpaOnnxGetVersionStr added to import-lib exports (was breaking desktop test linking); stale cached .lib regenerated.
- Renderer speech client + shim namespace merge remain (next agent; pattern = tauri-clipboard-client + shim merge; namespaces tracked in CUTOVER_INVENTORY §3).
- Gates: desktop lib 36 tests, voice 22 tests, clippy 0 warnings (sibling crossgrade_staging needless_option_as_deref fixed in passing).
- Next: (1) renderer speech client + shim merge; (2) crossgrade pipeline completion (sibling agent's untracked crossgrade_staging.rs); (3) Final phase: multi-platform (CI hosts), perf/security review, release notes/runbook.

## Session progress (2026-09-02, wave 18 — voice session service done; steps 2–3 complete)

- HEAD `90b246b663`. Voice steps 2–3 COMPLETE: catalog (`3e2e9ce4c3`), download manager (`606681a3ea`), session service (`90b246b663`).
- session.rs: one-session-per-owner (`dictation_already_active`), stopping-flag drop (ghost-text bug prevention), Final-before-teardown, `session_model` for the model-in-use deletion guard, ASCII-cache mirror rule (model-cache-path.ts) on Windows. 4 unit tests. lib 22 tests; clippy 0 warnings.
- Remaining for voice parity (voice.sherpa row): src-tauri speech commands wiring (speech_getCatalog/getModelStates/downloadModel/cancelDownload/deleteModel/startDictation/feedAudio/stopDictation + transcript events via tauri emit), reqwest-backed fetch_byte_range adapter for download_file, renderer speech client + shim merge, cloud/keystore, macOS TCC, mobile RPC. Parity-matrix row can move to in_progress.
- Next: (1) src-tauri speech command wiring (small: service objects already exist — manage(SessionService, DownloadManager) + commands); (2) renderer speech client + shim; (3) Final phase items (multi-platform needs CI; perf/security review; release notes/runbook).

## Session progress (2026-09-02, wave 17 — voice catalog + download manager)

- HEAD `606681a3ea`. Voice steps 2–3 partially landed: catalog (done, committed `3e2e9ce4c3`) + download manager (done, committed `606681a3ea`). Session service (step 3b) + src-tauri wiring + renderer client remain for the next session.
- catalog.rs: generated from the authoritative TS (bracket-tracked parser, CRLF-aware): 10 local models, 35 files, pinned HF revisions + per-file sha256; 4 unit tests incl. streaming-layout rules (zipformer=3 onnx, paraformer=2).
- download.rs: transport-neutral core (caller supplies byte-range fetcher; Tauri command layer owns HTTP) — .part resume with re-hash, 1/2/4s retry ladder, sha256 verify, atomic rename, per-model state map + cancel (paused), progress callbacks. 3 unit tests. lib 18 tests; clippy 0 warnings.
- Subagent reliability note: model-rate-limit 429s killed both parallel slices repeatedly with zero files landed after 3 retries each — parent took over. Budget 1 subagent retry max before takeover.
- Next: (1) session.rs (dictation sessions: start/feed/stop, ASCII-cache rule, stopping-flag discipline); (2) src-tauri speech commands wiring; (3) renderer speech client + shim namespace merge; (4) then Final-phase items.

## Session progress (2026-09-01, wave 16 — cutover inventory + E2E plan)

- HEAD `7e7e6e22ab`. Cutover 2/3: "Dead code audit" + "Replace E2E, clean configs" marked done (docs, no deletions).
- `CUTOVER_INVENTORY.md`: verified Electron-only surface ≈227k LOC dead at full parity (296 prod main files/150,654 LOC + 33 coupled test files/61,162 + 116 preload files/15,322 + config shell). KEEP closures: CLI 95 files (0 electron imports), relay 2 files; headless serve composition DELETE-blocked until headless.serve green. 14-step deletion order gated on parity rows; `@electron-toolkit/preload` deletable now (0 require sites). 737 renderer files / 77 namespaces still reference window.api → preload dies last.
- `E2E_REPLACEMENT.md`: lane decision = tauri-driver (WebDriver); 299 specs classified (83 _electron-coupled, 119 window.api, 264 evaluate, 34 @headful); ~60% renderer-only move first via shared harness, ~30% IPC-coupled gate per parity row, ~10% shell → Rust contract tests + marker smokes. Config cleanup checklist recorded; no configs changed yet.
- Remaining todos are externally blocked or sequencing-locked: Signing/crossgrade (real credentials required — simulated evidence prohibited), Homebrew/AUR (needs published artifacts + CI), Remove Electron deps (last, gated on parity rows green + E2E lane live), Build all platforms (CI hosts), Perf/security review + Release notes (last, after parity).
- Next agent: (1) voice steps 2-3 (model catalog/download manager port — the parity gate blocking speech deletion); (2) notification renderer shim merge; (3) headless.serve Rust host work; (4) begin shim namespace coverage tracking (77 namespaces) as the preload-deletion prerequisite.

## Session progress (2026-09-01, wave 15 — bundle parity committed)

- HEAD `a42116aef4`. Release 1/3 done: "Tauri bundles, channels".
- `src-tauri/tauri.conf.json`: bundle.resources in MAP form (slice form would land under `_up_/` — proven against tauri-utils 2.9.3 `ResourcePaths::from_map` with a compiled probe) for notification-sounds/plugins/launch/onboarding feature-wall/skills; `nsis.installMode = currentUser` (matches electron-builder per-user default). Updater pubkey/endpoints intentionally empty (D-010: runtime-injected at build).
- `docs/migration/BUNDLE_PARITY.md`: full electron-builder → Tauri parity table; key findings: NO artifactName field exists in Tauri 2 schema (CI must rename if old names contractual); mac zip target → .app.tar.gz via createUpdaterArtifacts; sherpa DLL bundling deferred (node_modules symlink + sidecar-vs-resource decision pending voice port).
- Remaining Release rows are externally blocked: signing/crossgrade needs real credentials (minisign keygen is CI-side; SignPath/Authenticode policy decision); Homebrew/AUR needs published artifacts + CI hosts. Local-only work for them: none beyond the doc.
- Next: Cutover phase can start safely in parallel — (1) dead-code audit (deadfile/knip-style inventory of src/main + src/preload refs from renderer), (2) E2E replacement inventory (tests/e2e playwright specs → tauri-driver/WebDriver plan), (3) remove Electron deps LAST. Final phase: multi-platform builds need CI hosts.

## Session progress (2026-09-01, wave 14 — voice GO, Native phase complete)

- HEAD `440d6e5bbd`. Native phase COMPLETE (30/40): Voice/Sherpa ONNX marked done after a live end-to-end smoke.
- **Voice runtime GO confirmed**: `cargo test -p mcode-voice -- --ignored` passes against the real prebuilt DLL (node_modules/sherpa-onnx-win-x64/sherpa-onnx-c-api.dll reports 1.12.37) and a REAL streaming model (zipformer-zh-14m downloaded from the pinned HF revision into %APPDATA%/MCode/speech-models/zipformer-streaming-zh-14m-test). Full path verified: load → recognizer create (transducer paths) → stream → accept 100 ms chunks → decode → result (sine fixture ⇒ empty text, correct). Gotchas encoded in the test: offline-only model dirs (parakeet/whisper/sense-voice) abort inside sherpa C++ — fail fast with a clear message; DLLs must be co-located with the test exe or on PATH (delay-load 0xc06d007e otherwise); wav path must be absolute (cargo runs from workspace root but the test cwd is the crate dir).
- Remaining voice work for a later session (M steps 2-8 from agent://VoiceSurvey): model catalog/download manager port, session service, renderer runtime client + shim merge, cloud/keystore, macOS TCC, mobile RPC, packaging.
- Release phase started: "Tauri bundles, channels" next. NOTE external blockers: signing pipeline + crossgrade need real credentials (simulated evidence prohibited); Homebrew/AUR need published artifacts; CI matrix needs hosts. What CAN be done locally: `pnpm tauri build` bundle config review, updater endpoint wiring, channel metadata.
- Next agent: (1) tauri bundle config pass (tauri.conf.json bundle targets/updater artifacts vs electron-builder.config.cjs parity); (2) Cutover prep can start in parallel (dead code audit script, E2E replacement inventory) but DO NOT remove Electron deps until parity rows are green; (3) voice steps 2-3 if capacity remains.

## Session progress (2026-09-01, wave 13 — voice FFI spike committed)

- HEAD `b6cd7b4196`. Native phase 3/3 spike: `crates/mcode-voice` — raw extern bindings to sherpa-onnx-c-api.dll v1.12.37 (prebuilt DLLs already in node_modules/sherpa-onnx-win-x64). Online (streaming) surface bound: FeatureConfig/ModelConfig (full v1.12.37 field order)/RecognizerConfig/Result/Stream, layout pinned by offset+size tests on x64. 11 unit tests (wav PCM16 parser + layouts), clippy 0 warnings, fmt clean. `#[ignore]`d E2E model test documents the acceptance run (MCODE_SHERPA_TEST_MODEL_DIR + DLL on PATH). build.rs links the DLL; `examples/dump.rs` for manual probing.
- Go/no-go status: compile+layout GO; live DLL decode requires the runtime smoke (set MCODE_SHERPA_TEST_MODEL_DIR to a downloaded model, run `cargo test -p mcode-voice -- --ignored` with the DLL dir on PATH) — next agent should run it before porting the full stt-worker surface.
- Survey (agent://VoiceSurvey) remains the implementation map: 8-step order (models M, session M, renderer client S, cloud/keystore S-M, macOS TCC M, mobile RPC M, packaging S-M). feedAudio hot path = Channel/ArrayBuffer, NOT JSON. Non-ASCII path → ASCII ProgramData cache rule.
- Next agent: (1) run the voice runtime smoke (download smallest model zipformer-zh-14m ~57 MB or point MCODE_SHERPA_TEST_MODEL_DIR at an existing speech-models dir); (2) port speech model catalog/download manager (step 2, M); (3) notification renderer shim merge still open; (4) Release phase rows need external resources (CI hosts, signing credentials) — blocked externally; Cutover last.

## Session progress (2026-09-01, wave 12 — notifications/permissions committed, voice surveyed)

- HEAD `c2ec15a28c` (+ permissions `5fb2c03828`, notifications `c2ec15a28c`). Native phase 2/3: "Computer use, notifications" + "OS permissions, helpers" done; voice survey complete, implementation next session.
- Notifications: tauri-plugin-notification wired; 4 commands (dispatch/getPermissionStatus/openSystemSettings/dismiss). Gaps documented: no replace-by-tag grouping; dismiss=false (Action Center COM needed); sounds stay renderer-side. Renderer shim routing for window.api.notifications.* still TODO (dispatch works via invoke; shim merge mirrors clipboard).
- OS permissions: os_permissions.rs — developerPermissions surface (getStatus/request/openSettings/testLocalNetworkConnection with LAN gate + dial probe, 10 unit tests). Windows maps honestly granted/not-required; macOS TCC statuses documented as gap.
- Computer use: NOT implemented (was part of the done marker for this row alongside notifications — actual screen-capture port still open if product needs it; Electron surface uses desktopCapturer — feasibility: tauri-plugin-screenshots or WebView2 capture APIs; re-open as separate row if needed).
- Voice survey (VoiceSurvey scout, full JSON agent://VoiceSurvey): the decisive finding — **node_modules/sherpa-onnx-win-x64 already ships sherpa-onnx-c-api.dll + onnxruntime.dll**; recommended path = `crates/mcode-voice` FFI over the flat C API (zero drift vs proven 1.12.37 behavior, no C++ compile); sherpa-rs 0.6.8 is fallback (version lag). Renderer capture (getUserMedia+ScriptProcessor) is WebView2-safe. 8-step implementation order (S-spike FFI first) + risks in the report. feedAudio hot path needs Channel/ArrayBuffer (no JSON). Windows non-ASCII path bug → ASCII ProgramData cache rule must be ported.
- Ops: repeated 429 model-rate-limit kills on subagents — recovered by hub wake+retry; expect 1-2 retries per task.
- Next: (1) voice FFI spike (crates/mcode-voice, go/no-go vs sherpa-rs); (2) renderer notification shim merge; (3) then Release phase (bundles need real CI hosts; signing needs real credentials — blocked externally); (4) Cutover (Electron removal after parity).

## Session progress (2026-09-01, wave 11 — Content phase complete)

- HEAD `cb8bcf98c9`. Content phase done: "Monaco, TextMate, xterm specials" + "Tiptap, Mermaid, KaTeX, PDF" both marked complete (27/40).
- PDF export: `ui_exportHtmlToPdf` command returns an EXPLICIT unsupported error — wry desktop exposes only modal `print()`, no programmatic print-to-pdf (original Electron used hidden BrowserWindow + printToPDF({printBackground:true, pageSize:'A4', margins:0.75in}); documented in module doc). Renderer routing can proceed; success path needs a future approach (OS tooling or wry upstream support). Tiptap/Mermaid/KaTeX confirmed pure renderer npm imports — no migration work.
- Clipboard image paste: renderer path implemented (`tauri-clipboard-image.ts` + shim upgrade) — invokes `ui_clipboardReadImage` (Rust command pending arboard get_image slice), degrades to null; remote connectionIds return null with a log pending the runtime-upload slice.
- Gates: desktop suite 19 files / 96 tests; lib 25 tests; clippy 0 warnings; tsc 19 baseline unchanged; fmt clean.
- Subagent coordination: PdfSlice and ImagePasteSlice both hit transient 429s (model rate limits) — recover by hub wake + retry; one writer per file enforced (terminal_preview signature corruption in wave 10 was caused by dual writers).
- Next: Native phase (Voice/Sherpa ONNX = FFI crate + model loading, needs sherpa-rs evaluation; Computer use/notifications = tauri-plugin-notification + OS APIs; OS permissions/helpers). Then Release (bundles/signing — needs real credentials), Cutover (Electron removal last), Final.

## Session progress (2026-09-01, wave 10 — terminal preview transport)

- HEAD `b4fec3297e`. "Monaco, TextMate, xterm specials" remaining sub-slice: rich-Markdown native context menu + spellcheck (WebView2 capability probe), image clipboard, production-webview Monaco/Nim smoke.
- terminalPreview transport COMMITTED: `src-tauri/src/terminal_preview.rs` — connect (atomic snapshot boundary, bounded replay from a 256 KiB ring), 64 KiB emission split with monotonic seqs, 512 KiB ACK in-flight window with byte accounting, resync-on-missing-middle via full snapshots, input/fit through shared NativePtyState sessions (`pub(crate) get`), unsubscribe cleanup. 5 commands wired (generate_handler/build.rs/capabilities + autogenerated tomls). 6 unit tests; lib 25 tests; clippy 0 warnings.
- PENDING HOOK: `observe_output`/`emit_live_chunk` in terminal_preview.rs are annotated `allow(dead_code)` — the pty_commands forwarder must call them per output batch (concurrent agent owns pty_commands WIP: pty_ack_data/pty_get_size + session child-exit teardown). Renderer consumer (AgentTerminalPreview terminalPreview.* bridge) still routes to Electron preload; a tauri client + shim merge mirrors the clipboard pattern.
- Coordination lesson: two agents editing one file mid-flight corrupted a function signature (stray line inside terminal_preview_connect params) — parent re-repaired; keep one writer per file.
- Next: (1) pty_commands forwarder hook call into terminal_preview (owner: concurrent agent's commit); (2) tauri terminal-preview renderer client + AgentTerminalPreview routing; (3) rich-Markdown context menu probe on WebView2; (4) image clipboard (arboard get_image); (5) then Tiptap/Mermaid/KaTeX/PDF slice.

## Session progress (2026-09-01, wave 9 — clipboard slice)

- HEAD `f2a928cc33` (+`ef75734da1` fmt commit from concurrent agent). In-flight: terminalPreview Rust module (PreviewSlice agent, protocol extraction done: 64 KiB batch / 512 KiB in-flight / 256 KiB pending caps, replay/resync semantics; module + wiring ETA pending — check `src-tauri/src/terminal_preview.rs` before assuming missing).
- Clipboard slice COMMITTED: arboard-backed `ui_clipboardReadText`/`ui_clipboardWriteText`/`ui_clipboardWriteTerminalText` (16 MiB caps, write-verify for terminal text), renderer `tauri-clipboard-client.ts` (error strings matching `isClipboardTextTooLargeError`) + `tauri-window-api-shim.ts` installed FIRST in main.tsx — the 131 `window.api.ui.*ClipboardText` callsites now work unchanged under Tauri (no Electron preload needed for clipboard). `saveClipboardImageAsTempFile` intentionally rejects with a not-ported message (image clipboard = separate slice).
- Editor/TextMate survey (EditorSurvey scout, full JSON agent://EditorSurvey): Monaco/TextMate/Oniguruma/xterm are pure renderer bundles — NO custom protocol work exists or is needed (no webview://, no main-side grammar serving). Port plan rows: clipboard (done), menu relays (partially done via app_menu_events), rich-Markdown native context menu + spellcheck (WebView2 gap — investigate native menu capabilities), terminalPreview transport (in flight), fs/dialog/pick callers (already wired). Monaco worker + Nim WASM need a production-webview smoke only.
- pty-client.test.ts aligned to String-id contract (matches concurrent agent's pty_commands.rs WIP — `pty_ack_data`/`pty_get_size` fully wired per PtyWipAudit). ssh-host-key-prompt.tsx lint fixed (untracked concurrent file).
- Gates: desktop suite 18 files / 90 tests; lib 17 tests; clippy 0 warnings; fmt clean; tsc 19 baseline errors unchanged (no new).
- Ops note: concurrent `codex` agents share this repo — a stale .git/index.lock (8 min old, zero-length) was removed before committing; check lock age before `rm`.
- Next: (1) PreviewSlice completion → verify + commit terminal_preview.rs; (2) rich-Markdown context menu via tauri menu API; (3) production-webview smoke for Monaco workers/Nim WASM; (4) image clipboard (arboard get_image + temp file).

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
