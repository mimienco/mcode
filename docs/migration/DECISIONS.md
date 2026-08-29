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

## Pending decisions

- PTY crate/FFI strategy after behavior and throughput spikes on all platforms.
- Rust SSH implementation after host-key, agent, jump-host, and remote-PTY compatibility tests.
- Direct crossgrade versus an Electron bridge release after updater identity/signature experiments.
