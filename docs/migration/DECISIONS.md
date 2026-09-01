# Migration Decisions

## D-001: Side-by-side rewrite

Status: accepted.

The Electron source is an immutable behavioral reference. All migration edits occur on `rewrite/tauri-full` in the timestamped rewrite copy. The separate backup is reserved for controlled baseline execution.

## D-002: Manifest is the binary integrity authority

Status: accepted.

Git status and patches preserve repository intent, while `manifest.sha256` proves byte identity for present files and records expected tracked deletions. This avoids mistaking Git binary-patch formatting for source drift.

## D-003: Domain contracts replace the preload facade

Status: accepted in principle; command-level design is incremental.

React code will depend on a typed, platform-neutral desktop client. Tauri commands are narrow request/response operations; channels carry bounded streams with sequence, backpressure, cancellation, and resynchronization semantics. No generic shell or filesystem bridge is allowed.

## D-004: Compatibility precedes dependency removal

Status: accepted.

Electron, main/preload code, Node-native dependencies, and existing tests remain until the corresponding parity rows have Rust/Tauri implementation plus unit, contract, E2E, and platform evidence.

## D-005: Rust module seams follow current responsibilities

Status: accepted for the bootstrap.

The initial workspace separates application composition, contracts, platform services, process/PTY, Git/worktrees, scoped filesystem/watcher, SSH, and wire/relay concerns. Exact crate boundaries may deepen as behavior is ported, but renderer-facing contracts remain small.

## D-006: Pin the verified Tauri bootstrap stack

Status: accepted for the bootstrap.

The initial shell pins Tauri core 2.11.5, Tauri CLI 2.11.4, `@tauri-apps/api` 2.11.1, `tauri-build` 2.6.3, and Rust 1.98.0. These versions were checked against the official Tauri release channel and then verified together by `tauri info`, Cargo tests, clippy, a Windows WebView2 smoke launch, and a CLI-driven debug build. Transitive `wry` and `tao` versions remain lockfile-controlled; an “outdated” advisory from `tauri info` is not overridden independently of Tauri core.

## D-007: Two-stage renderer acknowledgement with atomic execution

Status: accepted for the Windows lifecycle slice.

A native Quit/Restart request first requires an immediate receipt ACK. The existing 10-second Electron bound applies only before that receipt, so an unresponsive or absent renderer cannot hang shutdown while a responsive renderer may leave a real dirty-file or running-process decision open indefinitely. After the renderer has staged the durable checkpoint and the user has approved, one Rust command confirms, claims, and starts native execution. Delivery failure after `prevent_exit()` forces a fresh executing Quit. Manual last-window close uses the same app-wide protocol rather than a separate native-close shortcut.

## D-008: Production-entry graph defines the renderer import boundary

Renderer Node/Electron evidence is derived from the verified captured source backup's actual production entries, not repository-wide substring matches or filename guesses. The generator traces runtime local imports through shared source, records external packages, and rejects unresolved dynamic/local edges. Node imports may be allowlisted only when their exact source modules are unreachable from every production entry; stale exceptions and newly reachable exceptions are errors.

## D-009: Renderer restart and update application remain separate capabilities

Every production renderer action that restarts or relaunches the app routes through the runtime desktop lifecycle client. Tauri therefore uses the versioned request/receipt/cancel/atomic-execute protocol, while Electron retains its compatibility implementation behind the same client. Applying a downloaded update remains a distinct updater capability: a plain restart must never masquerade as a successful Tauri update install.

## D-010: The Rust updater owns trust, state, and install authorization

The renderer can call only the narrow `update_get_status`, `update_check`, `update_download`, and `update_install` commands and receive the validated `mcode://updater/status` event; raw `updater:*` plugin permissions are not granted. Rust requires a non-empty Minisign public key and distinct HTTPS endpoints for stable, RC, hourly, daily, and adhoc feeds at compile time. The empty static plugin bootstrap exists only because the plugin requires a configuration object; every real check constructs a runtime updater with the validated key and exactly one selected endpoint, and an unconfigured build returns `updater_not_configured` rather than simulating success.

Downloaded bytes are accepted only after the Tauri updater verifies their signature, then remain backend-owned until install. Update application uses the existing shutdown checkpoint protocol with the distinct `UpdateInstall` intent. An unacknowledged request times out back to the running app and emits an exact abort event that releases renderer restart preparation; a user veto does the same. An acknowledged request may wait for the user's dirty-file/process decision, and only atomic confirmation can execute the installer. A failed install restores the verified bytes for retry, records a structured error, and leaves the app open.

## D-011 (2026-09-01): ConPTY exit — reader-EOF alone is not a death signal on Windows

Status: accepted.

On Windows, conhost keeps the output pipe open after the child process exits, so treating reader-EOF as process death exits early and drops the tail of output (and the true exit status). The pty forwarder therefore polls `PtySession::child_exited()` as the authoritative death signal: when the child has exited without EOF, it closes the master (`SharedMaster::close` drops the Option-backed portable-pty master, terminating the pseudoconsole and releasing the reader) and joins the reader before the final drain — the node-pty close→drain→exit discipline. Reader-EOF with a live child remains a pure drain signal.

Evidence: crates/mcode-pty/src/session.rs (`child_exited`, `drain_after_child_exit_and_cause`, `SharedMaster::close`), src-tauri/src/pty_commands.rs (`spawn_pty_forwarder` child-exit poll + drain-after-close branch).

## Pending decisions

- PTY crate/FFI strategy after behavior and throughput spikes on all platforms.
- Rust SSH implementation after host-key, agent, jump-host, and remote-PTY compatibility tests.
- Direct crossgrade versus an Electron bridge release after updater identity/signature experiments.
