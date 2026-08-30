# Current Migration Status

## Current phase

Phase 3 – native updater and Windows lifecycle implemented; capability-scoped filesystem discovery, bounded content, atomic local saves, and native-drop external claims integrated and verified on Windows.

## Last green commit

`5f6d922429` (`fix(tauri): harden native drop capabilities`).

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
- Added a deterministic production-entry import graph for the verified read-only source backup.
- Traced 4,716 runtime-reachable renderer/shared modules and 25,964 import sites across the captured main, popout, and paired-web entries.
- Inventoried 72 external package roots with exact importing-file sets and proved zero reachable Node built-ins or Electron imports.
- Added an exact stale-checked allowlist for four Node imports in two production-named test-support modules; neither module is reachable from a production entry.
- Made unresolved local imports, non-literal dynamic imports, newly reachable exceptions, and stale allowlist entries hard failures.
- Re-ran the two-axis review at `463f002bc2`: Standards PASS and Spec PASS with no remaining renderer-inventory findings.
- Routed all four remaining production renderer Restart/Relaunch callers through the runtime desktop lifecycle client.
- Added a production-source contract that rejects any new direct `window.api.app.restart()` or `relaunch()` caller outside the Electron compatibility adapter.
- Kept update application separate from ordinary restart: the existing Electron `quitAndInstall` checkpoint contracts remain green, while a real Tauri updater apply path is still required.
- Re-ran the two-axis review at `2834a9574e`: Standards PASS and Spec PASS with no remaining restart-routing findings.
- Added `mcode-updater` with strict five-channel HTTPS feed configuration, a serialized native updater state machine, bounded progress, transition guards, and retryable install failures.
- Added real Minisign fixtures that prove unchanged payloads verify and modified payloads are rejected; no private signing key is retained in the repository.
- Exposed narrow Rust-owned updater status/check/download/install commands and status events without granting raw updater-plugin renderer permissions.
- Routed production updater callers through the lazy runtime desktop client while preserving Electron compatibility and explicitly rejecting unported pinned/local/performance build modes on Tauri.
- Added the distinct `UpdateInstall` lifecycle intent so a verified update can execute only after the existing renderer acknowledgement, durable checkpoint, dirty-file/process decision, and atomic backend confirmation.
- Enabled Tauri updater artifacts and made absent release key/endpoints fail as `updater_not_configured` rather than using the empty initialization bootstrap.
- Ran an isolated real Windows WebView2 updater-check smoke: it recorded `updater_not_configured`, wrote the shutdown checkpoint, exited 0 in 6.74 seconds, and left zero processes.
- Added a pure `mcode-platform` window policy with Electron-compatible minimum-size, visible-overlap, persistence, and shutdown-aware close-to-tray contracts.
- Restored valid persisted position/inner-size and maximized state at Tauri startup; invalid/off-screen bounds fall back to an outer size fitted within the primary work area.
- Debounced move/resize persistence by 500 ms, retained normal bounds while maximized, skipped fullscreen and near-minimum teardown bounds, and synchronized the final persistence write with the authorized-exit freeze.
- Added immediate Windows tray installation with page-load and 12-second idempotent retries, Open/left-click restore-focus behavior, and Quit routed through the existing renderer acknowledgement protocol.
- Added transactional live Tauri synchronization for the Windows “Minimize to Tray on Close” switch, including renderer rollback and visible failure feedback, without granting raw tray or notification permissions to the renderer.
- Ran a real visible Windows WebView2 close-to-tray smoke: saved inner size restored, tray installation was confirmed, the preference was atomically persisted, native close hid the window, direct app-exit was renderer-acknowledged, exit code was 0, and zero processes remained. Tray Open/left-click/Quit interaction remains untested.

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
- `node config/scripts/generate-renderer-import-inventory.node-test.mjs`: PASS (15 tests).
- Captured-source renderer import generation plus `--check`: PASS (4,716 reachable files, 25,964 import sites, 72 packages, 0 reachable Node/Electron imports, 4 exact unreachable exceptions).
- Focused renderer inventory oxlint: PASS; oxfmt check: PASS after formatting.
- Post-inventory original integrity guard: EXTERNAL DELTA detected; HEAD, branch, and staged diff unchanged, while `AGENTS.md` and three Settings files changed in the original between 14:07 and 14:25. Migration evidence was regenerated from the verified read-only backup.
- Full-renderer restart routing plus runtime lifecycle client: PASS (2 files, 4 tests).
- Existing updater/app-restart preparation and durability contracts: PASS (3 files, 11 tests).
- Affected startup, UpdateCard, AdvancedPane, and Sidebar restart paths: PASS (84 full-suite tests plus 2 focused Sidebar restart tests). The full Sidebar suite retains two unrelated pre-existing Discord assertions against concurrently changed source.
- `cargo test --workspace -j1`: PASS (including 16 lifecycle, 12 native shutdown, 9 updater configuration/state, and all existing workspace contracts).
- `cargo test -p mcode-updater --test signature_contract`: PASS (2 tests; unchanged payload accepted, modified payload rejected).
- `cargo clippy --workspace --all-targets --all-features -j1 -- -D warnings`: PASS.
- Updater-focused renderer Vitest: PASS (8 files, 113 tests).
- Final affected-file oxlint: PASS; oxfmt applied to migration-owned files.
- `GOMAXPROCS=4 corepack pnpm run typecheck:web`: expected baseline FAIL with exactly 10 diagnostics outside updater-owned files.
- `corepack pnpm run build:tauri-renderer`: PASS (10,662 modules).
- `corepack pnpm exec tauri build --debug --no-bundle --ci`: PASS; updater plugin initialized and native debug executable completed.
- Isolated WebView2 updater smoke: PASS (`updater_not_configured`, shutdown checkpoint present, exit 0 in 6.74s, zero remaining processes).
- Post-updater original-integrity guard: EXTERNAL DELTA remains; HEAD, branch, and staged diff are unchanged, while 26 original files now differ from the snapshot. The expanded external set includes dev-start, watcher/repos/preload, window, palette/toolbar/status, Settings, and localization files. None were copied into the rewrite.
- Two-axis review found and drove fixes for duplicate channel URLs and a latched renderer restart state after update-install timeout; Standards review also caught the missing native launch check and unhandled Electron-only modifier checks.
- Five channel endpoints are now URL-unique after parsing; timeout, user veto, and install failure all release renderer restart preparation; the native launch check subscribes first and is marked non-user-initiated; unsupported manual modes surface a visible error.
- Post-review `cargo test --workspace -j1`: PASS; post-review clippy `-D warnings`: PASS.
- Post-review updater renderer lane: PASS (9 files, 134 tests).
- Corrected staged re-review: Standards PASS and Spec PASS with no remaining native-updater findings.
- `cargo test -p mcode-app`: PASS (18 lifecycle contracts including every shutdown phase).
- Window policy and desktop settings contracts: PASS (10 new Rust tests).
- Window/close-to-tray renderer contracts: PASS (5 files, 55 tests, including serialized overlapping updates, consecutive-failure rollback, and combined close/lifecycle listener behavior).
- `cargo check -p mcode-desktop`: PASS with the Tauri `tray-icon` feature.
- Tauri-specific renderer build: PASS (10,663 modules; inherited CSS Highlight and chunk-size warnings only).
- Direct native debug build: PASS.
- Visible Windows WebView2 close-to-tray/restore smoke: PASS (`1200x800` inner size restored, tray ready, setting persisted, marker `hidden`, renderer ACK, exit 0, zero remaining processes).
- Isolated committed-worktree `cargo test --workspace -j1`: PASS; debug workspace clippy and release `mcode-desktop` clippy both PASS with `-D warnings`.
- Final two-axis re-review: Standards PASS and Spec PASS after tray-install state separation, serialized effective-value preference transactions, release-CFG correction, and persistence-freeze deadlock prevention.
- Post-windowing original-integrity guard: EXTERNAL DELTA remains exactly 26 hash mismatches; original HEAD, branch, and staged diff remain unchanged, and none of those files were copied into the rewrite.

## Scoped filesystem discovery slice

- Added the `mcode-fs` Rust boundary with identity-verified open capability-directory handles, handle-relative traversal, atomic root replacement/revocation, deterministic directory listing, stat, and existence probes.
- Native `fs_read_dir`, `fs_stat`, and `fs_path_exists` commands use typed Electron-compatible payloads, structured errors, and `spawn_blocking`; an accidental SSH-owned invocation fails explicitly instead of touching local disk.
- Native authority refresh serializes settings snapshots, exact root-set comparison, and capability replacement before every operation. Slow WSL probes run outside the settings-write lock, memoize each distro per extraction, and revalidate the snapshot before returning. WSL worktree roots use the distro's probed actual `$HOME`, fall back to the configured desktop root like Electron when unavailable, keep distribution-qualified UNC identity, and compare the Linux tail case-sensitively. Remote/runtime records, imported external paths, and unverified worktree metadata remain excluded, and removed roots are revoked atomically.
- The renderer's local directory/stat/exists routes now use a lazy typed desktop filesystem client and reapply the existing pinned English numeric collator for Unicode-compatible Electron ordering. Direct SSH continues through the existing Electron SSH provider, and remote runtime RPC routes remain unchanged.
- Focused evidence: 15 core Rust contracts, 1 WSL-home platform contract, 8 settings-root contracts, 12 Tauri adapter unit tests, 4 command/wiring source contracts, and 33 renderer contracts. Full workspace Rust tests, workspace clippy with `-D warnings`, focused oxlint/oxfmt, and the 10,649-module Tauri renderer production build pass.
- Real Windows Tauri/WebView2 filesystem smoke: PASS for authorized directory listing, file metadata, missing-path behavior, and an outside-root `fs_path_denied`; lifecycle ACK, exit 0, marker `passed`, and zero remaining processes were confirmed.
- Post-filesystem original-integrity guard: EXTERNAL DELTA is now 45 hash mismatches; original HEAD, branch, and staged diff remain unchanged. None of the external edits were copied into the rewrite.
- Remaining discovery work: independently verified linked-worktree registration, native non-drop external-path producers, search/ignore behavior, watchers, native SSH replacement, and macOS/Linux evidence.

## Scoped filesystem content and atomic-save slice

- Added bounded same-handle reads with a 50 MiB byte cap, one-byte growth sentinel, early binary-prefix detection, Electron-compatible text/image/PDF payloads, and a stable file identity for the initial live-log snapshot.
- Added capability-relative same-directory atomic saves with cryptographically random `create_new` temporary names, target-permission carry-forward before payload bytes, contained-symlink resolution, full temporary flush, atomic rename, returned-error cleanup, and Unix parent-directory sync. Concurrent readers observe only complete old or new versions.
- Added native `fs_read_file` and `fs_write_file_atomic` commands, strict renderer response validation, local editor/MCP routing through the typed desktop client, explicit direct-SSH ownership retention, and unchanged remote-runtime RPC ownership. Independently authorized external previews/uploads remain on the existing provider until native grants exist.
- Evidence: the full Rust workspace test suite passes, including 19 core filesystem contracts, 15 Tauri adapter tests, and 4 command/wiring contracts; 4 renderer files/47 tests and scoped all-target clippy pass with `-D warnings`. Full web typecheck reports only the known unrelated baseline errors. The production Tauri renderer built 10,649 modules, and `tauri build --debug --no-bundle --ci` produced the embedded debug application.
- Real Windows Tauri/WebView2 filesystem smoke: PASS in 7.24 seconds. It verified bounded text read, non-empty log identity, atomic create (`atomic save`), atomic replacement (`atomic replacement`), missing behavior, and outside-root read/write denial; lifecycle ACK, exit 0, marker `passed`, and zero remaining processes were confirmed.
- The original guard still shows unchanged HEAD, branch, and staged diff. External user changes in the original now account for 45 manifest hash mismatches; none were copied into the rewrite.
- Remaining content work: native chooser/import authorities, writable external-file save semantics, watcher/local-tail service, crash-stale temporary recovery, Windows DACL/extended-metadata preservation, linked-worktree evidence, native SSH, and macOS/Linux validation.

## Native-drop external capability slice

- Added a native-origin grant registry: only capabilities synchronously captured by the first app-owned Tauri `DragDrop::Drop` listener for the same window can be claimed. Claims never reopen renderer-supplied paths, batches above 256 paths are rejected before capture, and exact-root deduplication preserves broader directory grants. Pending and active handles share one process-wide 256-grant LRU; trusted-root authorization discards its redundant pending capture; explicit revoke removes pending and active state; navigation and destroy remove window state; and globally unique generations reject stale captures even when a window label is reused.
- Added pinned read-only external capabilities. A dropped file is opened once, so replacing its leaf or retargeting an alias never exposes a sibling. A dropped directory grants read/list/stat access to its complete captured subtree through the existing capability-relative filesystem while retaining symlink containment. Writes through either kind fail with stable `fs_external_write_unsupported`; trusted workspace roots retain atomic writes.
- Added typed Tauri authorize/revoke commands and a runtime renderer drop adapter. Tauri uses `WebviewWindow.onDragDropEvent`; Electron keeps the existing preload subscription. All six production drop subscribers now share the adapter, including CSS-coordinate conversion, Shadow DOM target discovery, and race-safe asynchronous unsubscription.
- Security evidence: 3 core unit tests plus 26 filesystem capability contracts; 36 desktop adapter/registry tests; 6 command/wiring contracts; and the 5 directly affected renderer files/45 tests pass. The full Rust workspace suite, scoped clippy with `-D warnings`, changed-file oxfmt/oxlint, `git diff --check`, and the 10,652-module production Tauri renderer build pass.
- The existing real Windows WebView2 filesystem smoke was rebuilt through the Tauri CLI and rerun successfully: marker `passed`, atomic create/replacement exact, renderer shutdown acknowledgement present, exit 0, and no remaining process. It does not synthesize a native OS drop.
- This is intentionally not full external-path parity. Tauri emits its built-in raw renderer drop event before the later app-owned listener captures handles, so a coordinated renderer/local-process retarget race is not yet closed; eliminating it requires an app-owned post-capture event or an OS-handle-bearing drop source. Synchronous capture in that listener can also block window dispatch for an unbounded duration on a slow or offline network path. Non-drop producers, writable external saves, a real OS-level drop smoke, and macOS/Linux evidence remain open.

## Current blockers

- The copied Electron baseline is not fully green: production build is blocked by existing type errors and four terminal-performance E2E scenarios fail.
- The Tauri shell exposes the lifecycle context command and typed custom-window controls. The rest of the renderer still depends broadly on `window.api`, so the shell is not feature-complete or parity-ready.
- Tauri manual close, close-to-tray, every direct full-renderer Restart/Relaunch action, tray Quit, and native update application now share the existing decision flow and atomic shutdown checkpoint. Real tray Open/left-click/Quit interaction, the one-time tray notice, tray attention/activity icon states, popout restoration, and non-Windows windowing evidence remain incomplete.
- The native updater capability has no production public key or channel endpoint values, and no workflow publishes signed Tauri artifacts/feeds yet. Real network download/install, pinned/local/performance builds, crossgrade, rollback, session survival, and non-Windows evidence remain incomplete.
- macOS signing/notarization, Linux WebKitGTK/glibc-floor, and cross-platform installer evidence require their respective CI hosts and credentials.
- The original repository received concurrent edits after the migration snapshot. Forty-five files now differ while its HEAD, branch, and staged diff remain unchanged. They are preserved and were not copied into the rewrite; final integrity cannot return to the captured byte state unless the external owner resolves or accepts that delta.

## Parity summary

- done: 0
- in progress: 7
- not started: 29
- blocked: 0

## Exact next actions

1. Close the remaining Windows shell/window gaps: one-time tray notice, attention/activity icon states, popout restoration, navigation containment, and focused E2E assertions.
2. Add release-workflow artifact/manifest gates before attempting production key injection or a live updater install.
3. Continue scoped filesystem parity with verified folder/linked-worktree scopes, native watcher/local-tail recovery, native chooser/import authorities, writable external-file save semantics, and crash-stale temporary cleanup before process/PTY migration.
