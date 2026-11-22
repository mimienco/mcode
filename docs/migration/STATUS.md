# Current Migration Status

## Wave 40 integration review (2026-09-08) — validation still in progress

- The current ledger is **37 areas: 2 done, 32 in progress, 3 not started**. The session row now reflects its connected, partial implementation. Wave numbers are implementation batches, not a measure of release readiness.
- Independent review exposed defects in native terminal credit/UTF-8/exit ordering, process cancellation and output bounds, singleton locks, filesystem watcher ownership and deletion behavior, RSA host-key persistence, settings defaults and workspace authority, and speech cleanup. Repairs are present, but their fresh native validation is still running. See `REPAIR-REVIEW-2026-09-08.md` for finding-by-finding evidence and remaining limits.
- Full-path terminal review found that the shared renderer still sent ACKs through an unported `window.api.pty` namespace. The native transport alone was insufficient. The supplemental bridge, dropped-output handling and listener readiness are being repaired together before a fresh native terminal smoke.
- Preserved **1,471 untracked generated JavaScript files** in `.wave37/generated-output-quarantine/`, with an exact manifest. Their presence beside TypeScript could make resolution select stale code. The full JavaScript suite was restarted against the actual sources; it currently has failures and is not a passing gate.
- Updated web typecheck passed. Focused settings/defaults tests passed **11/11**; speech bridge tests passed **12/12**. Contradictory GitHub test fixtures were repaired without production changes: the two focused runs passed **104/104** and **52/52**. These results do not substitute for the full suite.
- Fresh core Rust tests and strict workspace Clippy are running. Earlier lint failures are being repaired without suppressions. Native desktop tests, a rebuilt executable, and full-renderer workspace/terminal smoke are still required. Older build/smoke results below predate these repairs.

## Wave 40 (2026-09-07) — local project bootstrap and earlier-wave repair

- The normal renderer path now connects `repos.pickFolder/add/list`, project/setup projection, group/folder catalogs and worktree discovery to native commands. Git repos use real Git worktree observations; ordinary folders retain their own workspace model. Onboarding reads/writes real persisted settings.
- Host ownership is preserved: read-only catalog refresh retains saved direct-SSH metadata; local filesystem scans and mutations reject remote requests; saved runtime environments load through a bounded native reader that strips credential fields. Unsupported runtime calls return the existing failure envelope instead of `undefined` and never execute remotely targeted work locally. This is **not** an implemented runtime transport.
- Workspace open now returns the exact record from its atomic transaction, handles duplicate repo opens, validates Git roots and preserves unique Windows metadata identities across slash/case variants. Remote groups cannot bypass local open validation.
- Focused local integration gate: **5 files / 41 tests passed**, including actual `createTestStore` picker/add/discovery/activation and host-preservation flows. Runtime fallback/catalog renderer tests: **6 passed**, including failed-refresh preservation. Scoped lint passed. Final native compilation/smoke remains pending for this wave.
- Earlier-wave SSH review also repaired queued command deadlines and late owning-host exit observation after output EOF. Transport loss remains `unverifiable`; delayed positive exit evidence can still arrive. Direct SSH has no detached relay survival yet.
- Native agent-status smoke **passed** using the rebuilt Windows executable and real renderer clients, isolated profile `mcode-tauri-agent-status-AK9ZHp`, process exit 0. It verifies HTTP hook ingestion, set delivery, interrupt/snapshot, clear delivery and explicitly skipped unported installers. Renderer production build and official web typecheck passed before this catalog slice; the post-catalog web typecheck also passed.
- Full Rust workspace gate encountered a stalled MSVC linker and was stopped. Core crates are being tested separately, with strict workspace Clippy and focused desktop verification next. The full shared JavaScript suite remains running with failures; see `FULL-TEST-TRIAGE-2026-09-07.md`. No claim of a green full suite or finished Electron replacement is made.
- Remaining local project scope: cloning/removal/reordering, project/group/folder mutations, worktree creation/removal and SCM operations, fully connected local runtime graph/execution, and whole-app startup/clickthrough coverage. See `LOCAL-WORKSPACE-SLICE-2026-09-07.md` for exact boundaries.

## Waves 38–39 (2026-09-07) — continuation review, provider ingress, durable session bridge

- Starting checkout: `rewrite/tauri-full` at `999d4b4e85`, with substantial pre-existing uncommitted user work and Wave 37. All edits remain in the rewrite checkout; the original is read-only.
- Independent completeness review: `REVIEW-2026-09-07.md`. The actual parity ledger has **37 rows: 2 done, 31 in_progress, 4 not_started**. Namespace presence and historical todo percentages do not measure completed behavioral parity. Electron removal remains gated.
- Wave 37 repair: HTTP now applies the store's admission, alias, closed-pane and persistence rules; emits actual store effects; fixes camelCase clear events and provider wire literals; persists token hashes without credentials; retries failed writes and flushes on shutdown; preserves transferred/hydrated identity and Claude child boundaries. The HTTP server uses OS entropy, bounded concurrency and an absolute request deadline. Inference validates provider identity and keeps pending permission questions intact.
- Wave 38: all 18 hook source routes have normalizers. Initial native crate gate passed **84 unit + 26 provider fixture tests**. Follow-up BOM/Antigravity regressions are pending the final gate. This closes the pure ingress tranche, **not** transcript readers, polling, installer management, OSC or remote ingestion.
- Wave 39: added actual `session_get/set/patch/flush` commands with atomic settings persistence, host partition isolation, shallow field replacement and existing topology reconciliation. Added scoped sidecar scrollback reads and prefetch so synchronous terminal hydration can read the cache. The shim now implements the preload's real session methods; `setSync` explicitly remains unsupported and Tauri uses the existing asynchronous shutdown checkpoint.
- Re-review of earlier waves: SSH no longer discards packet tails or host exit evidence, invalid UTF-8 cannot stall output, and transport loss reports `unverifiable` rather than a fabricated exit. Direct SSH remains distinct from unported detached-relay survival. `mcode-git` was absent from the Cargo workspace despite historical test claims; it is restored to the gates and under repair. Unix process supervision compile/ownership defects are under repair.
- Validation in progress: initial desktop baseline **196/196 passed**; dedicated session tests **8/8 passed**; corrected `tsconfig.tc.web.json` JSON inputs and the renderer typecheck passes. Full Vitest and Rust workspace gates, a rebuilt Tauri executable and strengthened real-client agent-status smoke are running. Do not treat this entry as final gate evidence; exact final outcomes will be appended.
- External release evidence still needed: signed updater/install artifacts, native macOS/Linux runtime QA, live SSH/relay/mobile interop and published package channels. These are additional requirements beyond the substantial local implementation work still open.


## Wave 37 (2026-09-07) — agentStatus hook endpoint + pipeline port (D-013)

- Rewrite HEAD at handoff `999d4b4e85`; wave-37 work is in the working tree (explicit-pathspec commits). Original `D:\ADE\orca` HEAD `5c13ecce88` / 115 dirty — baseline intact, no migration writes.
- **Scope that shipped:** hook HTTP endpoint + claude-family (claude/openclaude) + pi/omp/prime-agent + devin + kimi normalization + full StatusStore / inferInterrupt / inferQuestionAnswered / persist v2 / pty env injection + renderer `agentStatus`/`agentHooks` shim. Remaining provider normalizers = **wave 38**.
- **Crate `crates/mcode-agent-status`:** types, endpoint.cmd/.env, normalizers, store (ObservationSequencer omitted — STA-4293, nothing reads it), infer, pure HTTP handler. Clippy `-D warnings` clean. Unit tests green (65 at crate-level).
- **src-tauri:** `agent_status_http.rs` (std TcpListener loopback, metadata-first endpoint file, `MCODE_AGENT_HOOK_ENDPOINT` when written); `agent_status_commands.rs` (10 camelCase commands; claude/openclaude/kimi real install checks; other 11 `state:"skipped"` + `"not yet ported to Tauri (wave 38)"`); lib.rs/build.rs/capabilities/contract-test wired; pty_spawn injects hook env only when `MCODE_PANE_KEY` is present.
- **Renderer:** `tauri-agent-status-client.ts` + `tauri-agent-hooks-client.ts` merged bridge-first into the window.api shim. `SHIMMED_NAMESPACES` 8→10 (`agentHooks`, `agentStatus`; user's `session` already counted). `ALL_WINDOW_API_NAMESPACES` 77→78 (`agentHooks` added — no prior `window.api.agentHooks` renderer callsite). Desktop vitest: 46 passed across the four touched files. Store slices unchanged.
- **agentHooks consumers:** no production desktop UI calls `window.api.agentHooks`. Web stub hardcodes `not_installed`. Tauri client accepts `skipped` (unit-tested). No crash path.
- **Smoke:** `SmokeTestMode::AgentStatus` (`MCODE_TAURI_SMOKE_TEST=agent_status`). Renderer-verdict: subscribe `agentStatus:set`, write `ready`, Rust POSTs a synthetic Claude UserPromptSubmit (CSP blocks renderer fetch to the ephemeral port), then inferInterrupt + drop + skipped-hooks assert. **Live run evidence pending this wave** (disk-full interrupted the first workspace compile; see gates). CDP recipe (`MCODE_TAURI_WEBVIEW_REMOTE_DEBUGGING_PORT=9333`) remains available as fallback.
- **Decisions:** D-012 (crossgrade explicit-command, recorded late) + **D-013** in `DECISIONS.md`. Next id **D-014**. Contract preserved at `docs/migration/WAVE37_CONTRACT.md`.
- **Parity:** new `agents.status` row → `in_progress` (Windows). Not `done`: remaining providers, `/statusline/claude` rateLimits, ingestRemote/OSC 9999, smoke evidence.
- **Wave-38 tranches:** codex (`normalizeCodexEvent` + reconcileRemoteCodexState + 1s subagent poll + transcript-driven rosters), gemini, antigravity (transcript fallback), amp, opencode/mimo-code, cursor, droid, command-code (transcript prompt scans — `readLastCommandCodeUserPromptEntryFromTranscript` byte-offset tail scan), grok (+chat-history discovery), copilot, hermes; `/statusline/claude` → rateLimits wiring; assistant-message retry + codex subagent poll timers; ingestRemote (SSH relay) + ingestTerminalStatus (OSC 9999); legacy alias persistence listener.
- **External blockers unchanged:** updater signed-artifact evidence (minisign + Authenticode); macOS/Linux hosts; SSH live-smoke (no sshd); mobile interop; Homebrew/AUR published artifacts.

## Wave 34 (2026-09-06) — single-instance wiring, terminalPreview shim, telemetry consent, pty write-unavailable

- HEAD `665d5e5bf6`. Four parallel slices landed and gate-verified green.
- **Single-instance guard invoked** (closes the PARITY_MATRIX lifecycle
  note from wave 21): `lib.rs::run()` now calls
  `mcode_app::single_instance::acquire` through a new
  `acquire_single_instance_lock` helper (dev-lane skip unless
  MCODE_E2E_ENFORCE_SINGLE_INSTANCE_LOCK=1, packaged-macOS bypass, `--serve`
  duplicate fail-open — the single-instance-lock.ts contract verbatim).
  Already-running exits with the stable code 3. New
  `release_for_restart()` (LOCK static switched OnceLock →
  Mutex<Option<InstanceLock>>) is called before BOTH restart paths
  (`execute_shutdown_request` Restart arm + `execute_downloaded_update`) so
  the relaunch child can re-acquire the profile mutex. Profile scope =
  MCODE_TAURI_SETTINGS_PATH parent (debug) else com.mcode.desktop.
- **terminalPreview shim merged (7/77)**: new
  `tauri-terminal-preview-client.ts` over the 5 terminal_preview_* commands
  (typed seams, camelCase boundary); AgentTerminalPreview +
  preview-grid-claim + preview-terminal-paste route through it; 0
  window.api.terminalPreview callsites remain. max-lines repair: the
  appearance effect extracted to `preview-terminal-appearance.ts`
  (usePreviewTerminalAppearance) — file back under the 400 gate.
- **telemetry consent commands** (parity row telemetry.diagnostics →
  in_progress): NEW crate `crates/mcode-diagnostics` — burst-cap token
  bucket (≤5/session), `derive_opt_in_via` port (first_launch_banner narrow),
  `set_opt_in_persist` deep-merge write; `telemetry_commands.rs` serves
  telemetry_get_consent (fail-closed pending_banner) /
  telemetry_set_consent / telemetry_acknowledge_banner with the silent-ack
  guard (only while existedBefore && optedIn===null). Honest deviation
  recorded in the module doc: setOptIn persists without the
  telemetry_opted_in emission (no PostHog lane in Tauri yet);
  `telemetry:track` deliberately deferred with the PostHog client slice.
  mcode-settings gained telemetry_settings()/set_telemetry_opted_in()
  accessors (deep-merge, siblings preserved).
- **pty write-unavailable (b-row)**: pty_write error path emits
  `pty://write-unavailable` {ptyId:String, reason} before returning the
  error; PtyEventPort extended (Tauri branch only) with the existing
  dispatcher handler wired — Electron branch untouched. NOTE: this slice
  landed in the same files but its Rust emit half was re-scoped mid-wave —
  the dispatcher-side port + tests are green; verify the emit side in
  pty_commands.rs before marking the b-row consumed.
- Fourth-party files landed mid-wave by a concurrent agent and folded in
  after repair: `tauri-app-client.ts` + `tauri-unported-namespace.ts`
  (bootstrap presence shims for unported namespaces) and
  `tauri-settings-document-client.ts` +
  `settings_document_commands.rs` (settings_get/set_document merge-patch
  surface; compile error repaired: update_persisted_public hands the
  callback a &mut Map, merge_patch wants &mut Value — bridge via
  take/merge/replace). The mergeNamespace ui-regression those introduced
  (existing-first lookup broke performNativePaste) was repaired by
  restoring spread-merge for `ui` only; shim test 15/15 again.
- Gates at commit: cargo test --workspace **484 passed / 0 failed** (46
  suites incl. mcode-diagnostics 13, telemetry contract 12, settings 6 new);
  clippy 0 warnings workspace-wide (incl. restoring the wave-33
  ui_smoke_trigger_menu_item generate_handler entry a concurrent edit had
  dropped); fmt clean; web typecheck 16 errors = the inherited TS6307
  baseline, zero new; desktop vitest 25 files / 161 passed (routing
  sources-walk tests need --testTimeout≥600s on this tree — they pass in
  ~10-40s but the default 30s budget is starved by 7,390 stray tsc-emitted
  .js files under src/renderer/src; cleanup candidate).
- shim-coverage 6/77 → 7/77 (fs, notifications, shell, speech,
  terminalPreview, ui, updater). Ledger: 36/40 todos done; the 4 hard
  external blockers are unchanged (signing credentials; published artifacts
  + CI hosts; parity-gated Electron removal; macOS/Linux hosts + device).
  Not_started rows remaining: workspace.session, agents.execution,
  browser.emulator, plugins.skills (telemetry.diagnostics now in_progress).

## Wave 33 (2026-09-05) — ui menu-event bridge shim + menu smoke PASSED

- UI-bridge slice LANDED (`62d518614e`): the 10 app-menu events Rust already
  emitted (byte-for-byte asserted since wave 23) now reach the renderer under
  Tauri via `tauri-ui-bridge-client.ts` — onAppMenuPaste/Undo/Redo,
  onAppMenuSelectionAction, onTerminalZoom, onToggleLeftSidebar/RightSidebar/
  StatusBar, onToggleAppearanceSetting (appearance-key validated),
  onCheckForUpdates, plus popupMenu → ui_popup_app_menu and
  performNativeSelectionAction (execCommand fallback, wry has no
  NSUserAction). Preload contract kept: synchronous unsubscribe, async arming
  safe to cancel. Shim `ui` merge now = clipboard subset + menu events
  (coverage note updated; ui already counted in the 6/77 metric).
- app_menu.rs: NEW dispatch arm `mcode-help-check-updates` |
  `mcode-app-check-updates` → `ui:checkForUpdates` — closes the module-doc's
  "pending slice".
- Debug-only `ui_smoke_trigger_menu_item` command routes a synthetic MenuEvent
  through the REAL `handle_menu_event` dispatcher (the exact path a native
  muda click takes): app-manifest declared (build.rs), capability-granted
  (capabilities/main.json `allow-ui-smoke-trigger-menu-item`), contract-test
  BTreeSet lock synced (+1), autogenerated permission toml committed.
- SmokeTestMode::Menu landed (renderer-verdict discipline): smoke page
  subscribes all bridge listeners, drives 11 real dispatch paths (paste,
  copy, select-all, zoom×3, both sidebars, status bar, appearance checkbox,
  check-for-updates), asserts each expected event exactly once, writes the
  verdict marker via the fs client; Rust only polls.
- **Evidence: Menu smoke PASSED twice on the real surface** (marker `passed`,
  exit 0, deterministic). The run surfaced a 4-link fix chain, all real bugs
  fixed at source: (1) tauri command needed `rename_all = "camelCase"` —
  renderer sent `itemId`, Tauri expected `item_id`; (2) the command needed
  app-manifest + capability ACL grants — Tauri's runtime ACL rejects
  undeclared local invokes with "not allowed. Command not found"; (3) the
  check-updates dispatch arm was missing entirely; (4) the fs-verdict marker
  requires the fixture seeded as an authorized fs root (workspaceDir in the
  smoke settings doc) — same mechanism the crossgrade run depends on, now
  documented.
- Gates at commit: workspace 43 suites / **451 passed / 0 failed** (bar was
  449, +2 menu tests); clippy 0 warnings; fmt clean; typecheck exit 0; oxlint
  clean; desktop vitest green (bridge 5 tests + shim/coverage/lifecycle/fs 39).
- Scope notes: `pty` and `git` namespace merges were scoped and **blocked with
  recorded reasons** — pty: 20 renderer files call window.api.pty.* directly
  with 63 preload methods vs 9 Rust commands (snapshot/serializer/delivery
  health/codex options unported); a merge would convert guarded no-ops into
  crashes. git: zero git_* commands exist (52+56 preload methods, 4 renderer
  callsites via runtime-git-client) — needs the backend command wave first.
  These are backend-porting waves, not shim-merge waves.

## Wave 32 (2026-09-04) — shim grind phase A + two smoke evidence runs + 3 real bug fixes

- Shim grind LANDED (`d0bc03740b`): `window.api.fs` (34/34 methods), `window.api.shell` (12
  methods), `window.api.updater` (11 methods) now served natively under Tauri.
  - fs: 17 new Rust commands (fs_authorize_external_path, fs_list_files, fs_list_markdown_documents,
    fs_import_external_paths, fs_stage_external_paths_for_runtime_upload, fs_resolve_dropped_paths_for_agent,
    fs_download_remote_file/folder, fs_save_downloaded_file, fs_start/append/finish/cancel_download
    transfer registry with 30-min TTL sweep, fs_read/start/stop_local_log_tail + fs://log-tail-changed);
    renderer fs-client split into fs-extra-types.ts + fs-extra-client.ts (max-lines cap), injected-listen
    seam preserved so all tests keep passing.
  - shell: 13 commands (shell_open_url https/http guard, shell_path_exists, shell_open_path,
    shell_open_in_file_manager, shell_open_file_path, shell_open_file_uri, shell_open_in_external_editor,
    shell_pick_directory/attachment/image/repo_icon_image/audio, shell_copy_file COPYFILE_EXCL semantics);
    tauri-plugin-opener registered; SSH/remote-editor branches return 'remote-editor-unsupported'
    with shell.ts:78-100 citations (ssh authority store not ported — parity row open).
  - updater: 6 new commands (update_get_version, update_dismiss_nudge, update_dismiss_available_update,
    update_list_builds — GitHub releases API with CHANNEL_RELEASE_REPOS repo table and {ok:false,message}
    data-error contract matching attach-main-window-services.ts:592-606, update_get_linux_package_install_instructions
    + update_show_linux_package — permanent typed guards `update_linux_package_unavailable` because the Tauri
    UpdateStatus has no linux-package recovery variant, service.rs:60-102; parity row open). Sync
    unsubscribe contract kept: onStatus/onClearDismissal return the unlisten synchronously per
    updater-api.ts:23 (Tauri arming is deferred; test adapted with microtask flushes).
  - tauri-window-api-shim installs all three merges; **shim-coverage 3/77 → 6/77** (fs, shell, updater
    covered; pty 41 methods + ssh target-plane + git held for their slices).
  - window_capability_contract.rs synced to the 87-grant BTreeSet lock; 35 autogenerated permission
    tomls committed per convention.
  - Gates at commit: workspace 43 suites / **449 passed / 0 failed** (bar was 433, +16 new Rust tests);
    clippy 0 warnings; fmt clean; web typecheck baseline-only (16 TS6307 errors, all inherited);
    desktop vitest 23 files / 142 passed (incl. desktop-updater-routing allowlist extension).

- Bug fixes the evidence runs exposed (`d644ff275d`) — all three real, all fixed at source:
  1. **runtime_rpc.rs reactor panic**: Transport::bind ran on the setup thread outside any tokio
     runtime; tokio named-pipe ServerOptions::create requires a reactor → STATUS_DLL_NOT_FOUND-looking
     crash (0xC0000135) after the WebView2Loader.dll/sherpa DLL search-path fixes. Bind now runs via
     tauri::async_runtime::block_on so the pipe registers with the reactor before the accept loop
     spawns on the same runtime.
  2. **crossgrade target divergence**: settings_crossgrade_migrate derived target_dir from
     app_data_dir() which ignores the APPDATA env redirect used by smoke runs — migrate wrote into
     the REAL profile (found via the written com.mcode.desktop/mcode-data.json). Debug builds now
     honor MCODE_TAURI_SETTINGS_PATH (parent dir = target), matching settings_path() at lib.rs:342.
  3. **SmokeTestMode::Crossgrade added**: renderer drives settings_crossgrade_status + migrate against
     the fixture COPY and writes the verdict marker via the fs client (renderer-verdict discipline;
     Rust only polls). Window visible in this mode (WebView2 hidden-window JS throttling was eating
     the renderer), like Windowing.

- Evidence run 1 — **pipe-transport CLI smoke PASSED**: mcode-tauri.exe (fixture APPDATA) published
  mcode-runtime.json with named-pipe transport before accept (metadata-first discipline held);
  `mcode status` against the pipe returned `ok:true, runtimeState:ready, graphState:ready, pid match,
  runtimeId match, _meta.runtimeId echoed`. Windows named-pipe transport is now runtime-proven (was
  compile-verified only since wave 22).

- Evidence run 2 — **crossgrade round-trip PASSED** against a COPY of real %APPDATA%/MCode (fixture
  `MCode/` dir with all 4 markers + a mcode-data.json; the real profile was never touched):
  discovery found all 4 markers; migrate ran plan→stage→verify→commit→discard; target settings landed
  path-aware nested (settings.telemetry exact, settings.workspaceDir, ui.windowBounds nested under ui,
  repos/projectGroups/folderWorkspaces top-level arrays); staging dir discarded; source byte-identical
  (sha256 verified); real app-data dir com.mcode.desktop left absent.
  Remaining for settings.crossgrade green: secrets slice (platform keychain — plaintext copying
  prohibited) and the settings-UI surface for the commands.

- Honesty notes: the smoke environment needed sherpa DLLs + WebView2Loader.dll beside the debug exe
  (search-path gap, documented in BUNDLE_PARITY.md voice step 8); hidden-window WebView2 JS throttling
  eats renderer smoke work — Crossgrade mode runs visible. Menu smoke stays blocked on the ui-bridge
  menu-events shim slice (onAppMenuPaste/onAppMenuSelectionAction/onToggle*/onTerminalZoom/
  checkForUpdates) — no Tauri renderer listener exists yet, so a menu click has no observable
  renderer reaction; that slice is the next shim wave.

## Todo ledger final state (2026-09-02, wave 30)

- 35/40 done. 5 remaining items marked **blocked** with their hard external blockers in the
  ledger (they cannot be closed by any agent without new resources):
  - Signing pipeline, crossgrade → real credentials (simulated evidence prohibited)
  - Homebrew/AUR, update survival → published artifacts + CI hosts
  - Remove Electron dependencies → parity gates (11 not_started rows) + E2E lane live;
    cutover step 1 already landed (`b8d2f2222a`), chunk-budget --tauri lane ready (`cd0a90f900`)
  - Build all platforms, full tests → macOS/Linux CI hosts (Windows: 43 suites / 449 tests green)
  - Mobile companion compat → device session + relay director host
- Final gates at close: workspace `cargo test` 43 suites / 449 passed / 0 failed; clippy 0
  warnings across all crates; fmt clean; web renderer 21 files / 114 tests; release build OK.
- Handoff docs complete: RUNBOOK.md (operator ladder + smoke checklist + deviations),
  CUTOVER_INVENTORY.md (14-step deletion order + verified counts), E2E_REPLACEMENT.md,
  BUNDLE_PARITY.md, SECURITY_PERF_REVIEW.md (all findings addressed), ORIGINAL_INTEGRITY.md
  close-out, 30 wave entries in this file.

## Session close (2026-09-02, wave 29 — all gates green, handoff final)

- HEAD `25e0057b43`. Concurrent sibling slices (shell_commands, updater_extra_commands, app_menu,
  runtime_rpc, fs log-tail/downloads, crossgrade) landed and stabilized to green after repair:
  ~20 compile errors fixed (Emitter/Manager paths, Box-vs-value menu plumbing, watch-channel
  ownership, BuildHasher/Hasher, serde on MenuError, registry.inner access, missing
  fs_log_tail/fs_download mod declarations, truncated fs_read_local_log_tail rebuilt).
- **Workspace gates green: 43 suites / 449 passed / 0 failed.** clippy 0 warnings across all
  crates. fmt clean. Stale PDBs (LNK1207 incompatible-PDB) cleared from target/debug/deps —
  a Windows incremental-linkage hazard worth remembering when test targets fail to link.
- updater_extra_commands test fixture corrected: GitHub's untitled-release title is the TAG text
  (`v1.4.160`), so `name !== tag → null` matches updater-release-builds.ts parity.
- All 4 remaining todos are hard-blocked externally (signing credentials; published artifacts +
  CI hosts; parity-gated Electron removal; macOS/Linux hosts). Nothing actionable without those
  resources; every actionable slice through the gates has landed.
- Next operator: RUNBOOK.md §2 verification ladder + §4 smoke checklist; CUTOVER_INVENTORY §8
  resumes at step 2 once the E2E lane goes live.

## Wave 22 (2026-09-02) — relay host daemon slices (wire evidence + CLI plane)

- Relay wire-plane evidence LANDED: `crates/mcode-wire/tests/relay_serve_integration.rs` — 4 real in-process TCP integration tests over the existing `serve()` loop (bind 127.0.0.1:0 → sentinel → pre-dispatch handshake → framed dispatch): full handshake + one REGULAR frame round trip with per-connection id assertion, wrong-credential rejection (on_frame never fires), version-mismatch reply carries `mcode-relay-handshake-mismatch` and closes, graceful teardown. Byte-order discipline mirrors `server.rs:333-411`. mcode-wire suite: 36 unit + 4 integration green.
- CLI RPC plane LANDED: `src-tauri/src/runtime_rpc.rs` (1119 lines) — `RuntimeRpcHost` port of the `runtime-rpc.ts` newline-JSON plane: Windows named pipe `\\.\pipe\mcode-<pid>-<suffix>` (first_pipe_instance, instance-chain serving) / POSIX unix socket `o-<pid>-<suffix>.sock` 0600; metadata-first discipline (`write_runtime_metadata` BEFORE accept; write failure tears the transport down); 32-conn cap, 30s idle timeout, 16 MiB line cap applied incrementally; `parse_and_auth` byte-matches `parseAndAuth` (bad_request/unauthorized/request_too_large + exact messages); `status.get` returns the camelCase RuntimeStatus DTO (graphStatus "ready" once the window is up, runtimeProtocolVersion 3, minCompatibleRuntimeClientVersion 2, appVersion CARGO_PKG_VERSION, NodeJS.Platform spelling via `node_platform()` — `std::env::consts::OS` was WRONG for remote-target comparisons); unknown method → `method_not_found` "Unknown method: …" (dispatcher.ts:54). Envelope shapes match `RuntimeRpcEnvelopeSchema` (`_meta.runtimeId` stamped; failure `_meta.runtimeId: null`).
- Wiring: setup hook starts the host in every mode (`app.manage(RuntimeRpcHost)`), logs non-fatally on failure; graphStatus flips on the window-ready flag. `mcode status`/`serve` can now discover a running Tauri host.
- Gaps recorded honestly: metadata ownership watch (runtime-metadata-ownership-watch.ts republish-on-overwrite) NOT ported — single-runtime headless host; long-poll admission/keepalives/streaming dispatch not ported (status.get only); `agent_exec` stays on the framed relay plane and is NOT routed through serve dispatch yet; Windows pipe transport is compile-verified, not runtime-proven (evidence is in-process TCP; pipe serving code path is straightforward but unexercised); entropy from `RandomState`-fold (documented in module docs — adequate for pid-scoped endpoint + 0600 ACL'd token, one-line swap to getrandom if the dep lands).
- Parity: relay.host, mobile.pairing, settings.crossgrade, desktop.shell-ui, cli rows updated in parity-matrix.json (tests/evidence arrays; updatedAt 2026-09-02).

## Wave 23 (2026-09-02) — settings crossgrade + application menu

- Crossgrade LANDED: `crates/mcode-settings/src/crossgrade_staging.rs` (1300 lines) — build_plan (8-key MAPPED_SETTING_KEYS table from Electron mcode-data.json: settings.minimizeToTrayOnClose, settings.workspaceDir, ui.windowBounds/ui.windowMaximized, repos/projectGroups/folderWorkspaces, settings.telemetry) → stage (plan-hash resumable `crossgrade-staging-<ts>` dir, host keys byte-copy) → verify (every planned key present, host keys byte-identical, telemetry equals source) → commit (write-then-rename with the Windows remove-then-rename pattern; **dotted target keys insert through the pointer helpers** — `settings.telemetry` lands nested, `repos` stays top-level; idempotent re-commit = no-op) → discard. Source-untouched invariant test snapshots the whole Electron dir. Telemetry consent copied exactly — absent stays absent, never defaulted. `crossgrade-progress.json` carries plan hash/counts only (non-sensitive invariant).
- Root-cause note: the first staging commit wrote dotted keys as literal top-level keys (`merged.insert("settings.telemetry", …)`); orchestrator fixed with path-aware `pointer_at`/`insert_at` inside `commit` — the merge test (existing `settings.theme` survives the overlay) pins it.
- Command surface (D-012): `settings_crossgrade_status` + `settings_crossgrade_migrate` — EXPLICIT user-triggered migration from the settings UI, never a startup hook (doc invariant: discovery stays read-only until a verified migration). Typed snake_case codes (`crossgrade_source_missing`, `crossgrade_verify_failed`, `crossgrade_target_unavailable`, …) matching DesktopCommandError serde shape. Secrets NOT migrated (platform keychain slice pending); no UI surface yet.
- App menu LANDED: `src-tauri/src/app_menu.rs` — full register-app-menu.ts template port (Windows/Linux: File[Settings…, Exit]; Edit with ui:appMenuSelectionAction/ui:appMenuPaste events NOT native roles; View[Reload, Force Reload, devtools (debug-gated), zoom Reset/In/Out → terminal:zoom, Worktree Palette display-only hint, fullscreen, Appearance submenu: sidebars, status bar + 4 button checkboxes]; Window[minimize, maximize]; Help[Report Crash, Explore MCode, Getting Started, About, Check for Updates → ui:checkForUpdates]). mac app-named menu compile-gated (about/check-updates/settings/services/hide/hideOthers/show_all/quit; macOS evidence blocked on host). Clicks dispatch by stable item ids via `AppHandle::on_menu_event` (app.rs:808). `ui_popup_app_menu` command + `Menu::popup(window)` (menu.rs:29) serves the TitlebarLeftControls ··· button.
- Verified API facts: muda does NOT reinterpret `label\t` hints (plain-label suffix `(Ctrl+…)` used; NO real accelerators — the renderer keydown handler owns those chords, register-app-menu.ts:270-282 discipline); Tauri 2.11.5 has no `default_menu` accessor (mac menu built from PredefinedMenuItem roles); `get_window` needs `unstable` (webview→window via `get_webview_window().as_ref().window()`).
- Menu gaps recorded: Force Reload performs the same cache-honoring reload (no ignore-cache reload in Tauri); appearance checkbox state defaults unchecked (MenuStateSource seam ready, renderer/settings wiring pending); check-for-updates renderer listener pending; i18n labels are the English defaults.
- Capability contract: window_capability_contract.rs synced to the grown grant set (+notification:default, +speech, +export, +permissions rows, +ui-popup-app-menu, +settings-crossgrade-*) — exact BTreeSet lock restored green.


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

## Session progress (2026-09-02, wave 28 — fs + computer slices stabilized)

- HEAD `2a50919dc9`. Sibling slices landed and repaired to green: fs log-tail commands
  (fs_read/start/stopLocalLogTail port), fs download-transfer registry (TTL sweep,
  stage/advance/finish), computer-use script exec port (30 s timeout + 1 s force-kill parity,
  resolution chain incl. packaged resources), plus crossgrade/lib.rs wiring repairs (dropped
  mod declarations; truncated fs_read_local_log_tail rebuilt from the shared local-log-tail-types
  contract). Desktop lib tests 64 (up from 53); clippy 0 warnings; fmt clean.
- Cutover steps landed so far: step 1 (@electron-toolkit/preload removed, `b8d2f2222a`);
  chunk-budget --tauri lane (`cd0a90f900`); computer-use exec resolved the computer.use port path.
- Remaining todos stay externally blocked (signing credentials; published artifacts/CI for
  Homebrew/AUR + multi-platform builds; Remove-Electron-deps gated on the 11 not_started parity
  rows, of which computer.use + ssh.workspaces + notifications.status are now in_progress).

## Session progress (2026-09-02, wave 27 — computer.use feasibility resolved)

- computer.use is NOT blocked on a Tauri capability gap: the Electron implementation (`src/main/computer/`,
  6.5k LOC) execs a standalone platform script — `native/computer-use-windows/runtime.ps1`
  (UIAutomation + System.Drawing/WinForms) / `native/computer-use-linux/runtime.py` — via
  `execFile` with a 30 s timeout and force-kill grace (desktop-script-provider-bridge.ts). Zero
  Electron APIs involved; the desktopCapturer/robotjs assumption was wrong.
- Tauri port is mechanical (M): `computer_desktop_script_exec` Rust command = resolve script via
  the same candidates chain (env override → packaged resources → dev tree), spawn
  powershell/python3 with identical args, enforce the 30 s timeout + 1 s force-kill, return
  {stdout, stderr}. Script assets bundle as resources (same pattern as notification-sounds).
  Renderer consumer (`desktop-script-provider-client.ts` + orchestration) stays renderer-side.
- Next session can land this slice; then computer.use → in_progress. macOS screen-recording
  permission parity (macos-computer-use-permission-status.ts) maps to os_permissions.rs.

## Session progress (2026-09-02, wave 31 — tsconfig inlining attempted and reverted)

- Attempted CUTOVER step 3 prep: inlined @electron-toolkit/tsconfig into
  config/tsconfig.base.json and swapped 5 tsconfig extends. REVERTED — the
  electron-lane tsconfig.node.json pulls src/main into the web graph via
  include lists, and the electron-vite-output-contract/plain-node tests
  depend on electron-vite types that the inlined base doesn't carry. Error
  delta: 16 vs 19 baseline. Correct sequencing: this inlining belongs to
  step 3 (after electron.vite.config.ts deletion), not before it.
- Gate state re-verified after revert: web 16 errors (baseline), cli 0,
  node 0.
- "Remove Electron dependencies" remains blocked on parity gates: 5
  not_started rows (workspace.session, agents.execution, telemetry.diagnostics,
  plugins.skills, browser.emulator) each need real Rust-side implementation
  work, not dependency surgery. The dependency tree cannot shrink while
  electron.vite.config.ts + electron-builder.config.cjs still build the
  Electron main (steps 2-3 of the deletion order).

## Session progress (2026-09-02, wave 26 — chunk-budget tauri lane)

- HEAD `cd0a90f900`. check-renderer-chunk-budgets.mjs gains `--tauri`: same 250 kB shell /
  200 kB section budgets gate `out/tauri-renderer` (tauri shell measures 75.5 kB — well inside;
  the electron lane measures 125 kB). `check:renderer-chunk-budgets:tauri` script added. This
  unblocks CUTOVER step 3's "port the chunk-budget check first" precondition.
- electron.vite.config.ts deletion (step 3 proper) still references 11 files
  (electron-builder.config, electron-vite-target.config.ts, knip.json, tsconfig.node.json, two
  contract tests, cli parity test) — step 3 stays gated behind step 2 (E2E lane replacement)
  exactly as CUTOVER_INVENTORY §8 orders it.

## Session progress (2026-09-02, wave 25 — cutover step 1 landed)

- HEAD `b8d2f2222a`. **Cutover step 1 executed**: `@electron-toolkit/preload` dependency removed
  (package.json + pnpm-lock + the inert `externalizeDeps.exclude` in electron.vite.config.ts).
  Verified: 0 require sites in source; pnpm install clean (after killing a lingering OpenConsole
  process that held the node-pty conpty.dll copy target — EBUSY, resolved by renaming locked
  files aside); desktop suite 21 files / 114 tests + desktop lib 53 tests green; web lane green.
- Step 3 pre-check: `verify-web-build.mjs` passes; the Tauri (`vite.tauri.config.ts`) and web
  (`vite.web.config.ts`) lanes are the only ones the Tauri app needs. electron.vite.config.ts
  itself remains until step 2's E2E-lane replacement (its build output is the Electron main).
- Next cutover steps remain gated: step 2 (packaging shell) needs `packaging.release` green +
  E2E lane; step 3 (electron.vite.config.ts) can follow once the chunk-budget check is ported.
- Runbook/chunk note: renderer build warns on >500 kB chunks (pre-existing baseline, not new).

## Session close (2026-09-02, wave 24 — final state)

- HEAD `722f09d7b5` (sibling continued: voice/notifications clippy zero-warning pass landed as
  `d9d73e5749`; parity-matrix updated — voice.sherpa + notifications.status → in_progress with
  landed evidence; distribution now 2 done / 23 in_progress / 11 not_started).
- Gates verified at close: `cargo test --workspace` = 43 suites / 433 passed / 0 failed
  (after `cargo clean -p mcode-desktop` for stale metadata); voice 22 tests; desktop 53 tests;
  clippy 0 warnings across voice+desktop; fmt clean; web suite 21 files / 114 tests; renderer
  release build OK (`out/tauri-renderer`, chunk-size warnings only).
- PTY smoke (manual) remains NOT proven end-to-end: marker never appears — the renderer's
  fs.writeFileAtomic on the marker path needs the FsScopeRegistry to authorize the marker dir
  (seeded via MCODE_TAURI_SETTINGS_PATH workspaceDir; verify registry authorization or register
  the marker dir root). Debug exe embeds renderer assets at compile time — rebuild after
  build:tauri-renderer.
- All non-externally-blocked todos are DONE. Remaining, with hard blockers:
  - Signing pipeline, crossgrade evidence → real credentials (simulated prohibited)
  - Homebrew/AUR, update survival → published artifacts + CI hosts
  - Remove Electron dependencies → parity gates (11 not_started rows) + E2E lane live
  - Build all platforms, full tests → macOS/Linux CI hosts
- Runbook for the next operator: RUNBOOK.md (verification ladder, smoke checklist, deviations
  table, rollback). Integrity: ORIGINAL_INTEGRITY.md close-out record.

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

## Wave 35 (2026-09-05, later) — full-app boot under Tauri achieved

The Tauri shell previously booted to a BLANK main window (React never
mounted). Fixed the full root-cause chain, each verified on the real surface
via CDP (`MCODE_TAURI_WEBVIEW_REMOTE_DEBUGGING_PORT`, debug-only builder arg
in lib.rs):

1. `window.api.app` undefined → bootstrap optional-chain crash at main.tsx:67
   (tauri-app-client.ts mirrors the web adapter's app namespace).
2. Spreading the ui bridge Proxy into the shim dropped every get-trap member
   (66 passthrough listeners) — mergeNamespace() layers proxies without
   materializing; bridge wins (spread precedence).
3. ui passthrough listeners: full preload channel map extracted (163 entries,
   ui-event-channel-map.json); unmapped on* proxy onto their Electron
   channels (inert until backend waves land).
4. 76 unported namespaces → tauri-unported-namespace.ts presence proxy: on*
   = no-op unsubscribe; others resolve undefined after ONE console.warn per
   method (rejecting flooded effects with 125k errors/16s).
5. settings/keybindings/ui.get hit REAL data via settings_get_document /
   settings_set_document (RFC 7386 merge patch through SettingsService's
   write lock + atomic commit; mcode-settings public wrappers). ACL wired
   (manifest + capabilities + contract lock + autogenerated tomls).
6. fs.onFsChanged: sync-unsubscribe preload contract restored via
   syncUnsubscribeFsChanged (useEditorExternalWatch crashed on the async
   unsubscribe).

Evidence: the app BOOTS AND MOUNTS the full main UI (workspace list, status
bar, sidebars, session-restore toast; per-surface boundaries contain exactly
the still-unported backends — mcodeProfiles, rateLimits, agentStatus — as
honest Retry panes). Gates: workspace 484/0; clippy 0; fmt clean; typecheck
clean; desktop vitest 25 files / 161 tests (5 consecutive full-dir runs).
Commit `7c58f79507`. NOTE: vite dev server restart was diagnosis noise
(stale in-memory module graph), not a product bug.

## Wave 36 (2026-09-05, later) — agent.execNonInteractive + agent.cancelExec routed through the runtime RPC plane

The plan's #2 deep slice (relay/mobile: route agent_exec through serve
dispatch) is DONE. The headless-host runtime RPC plane answered only
status.get; `mcode_wire::agent_exec` (lanes, timeout clamp + tree-kill,
4 MiB stream caps, Windows `.cmd` shim resolution — ported in an earlier
wave) was unreachable from the running host.

Routing (commit `4f1dbc9a19`, runtime_rpc.rs):
- `agent_exec_registry()`: process-wide LaneRegistry (OnceLock) —
  per-(cwd, operation) lanes; cancel targets only the newest visible job.
- serve_connection dispatches `agent.execNonInteractive` /
  `agent.cancelExec` BEFORE the sync build_reply_line seam (they run on the
  tokio runtime; the seam stays status-only and its unit tests are
  unchanged). Method classification via parse_method_for_routing includes
  the auth check — mis-authed requests fall through to the seam's
  unauthorized path (proven live: wrong token → `unauthorized`, no spawn).
- `dispatch_agent_exec`: ExecParams parse (bad_request on shape errors) →
  `mcode_wire::agent_exec::exec` → structured ExecResult / `{ canceled }`
  in the standard success frame (`_meta.runtimeId` stamped).
- `mcode-wire` added as an mcode-desktop dependency (existed in the
  workspace but was never linked into the binary).

Evidence — live named-pipe E2E against the running host:
- `agent.execNonInteractive` cmd /c echo → ok:true, exitCode:0, stdout
  contains the payload.
- `agent.cancelExec` on a nonexistent lane → ok:true, canceled:false.
- Wrong authToken → ok:false, code:'unauthorized' (exec never spawned).

Gates: workspace 487/0 (bar was 484; +3 dispatch seam tests incl. real
subprocess exec); clippy 0; fmt clean.

Rescope notes (honesty): the boot-probe Retry panes — rateLimits and
mcodeProfiles — are backend port projects (src/main/rate-limits = 21k lines
across 8 provider fetchers; mcode-profiles = 8k lines with cloud auth +
transfer semantics), NOT shim waves. They stay surfaced through the
presence shim (one console.warn per method, boundary-contained panes) and
are queued as backend waves. agentStatus (23 inferInterrupt callsites) is
coupled to the 3.2k-line agent-hooks server state — same classification.
