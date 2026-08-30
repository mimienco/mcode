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

## Pending decisions

- PTY crate/FFI strategy after behavior and throughput spikes on all platforms.
- Rust SSH implementation after host-key, agent, jump-host, and remote-PTY compatibility tests.
- Direct crossgrade versus an Electron bridge release after updater identity/signature experiments.
