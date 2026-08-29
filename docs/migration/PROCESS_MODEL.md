# Process Model

## Current Electron model

`src/main/index.ts` is the composition root for Electron lifecycle, windows, tray/menu, single-instance handling, PTY daemon ownership, relay/RPC, plugins, updater, headless serve, telemetry, and shutdown. `src/preload/index.ts` exposes a very broad context bridge. Node subprocesses and native modules implement PTY, file watching, SSH/relay helpers, agents, and packaging-time utilities.

## Target model

```text
React renderer
  -> typed desktop client
  -> narrow Tauri commands / bounded channels
  -> Rust application services
       -> scoped filesystem + watcher
       -> Git + worktrees
       -> process supervisor + PTY
       -> SSH + remote workspace
       -> relay/wire + pairing
       -> settings, updater, telemetry, permissions
```

The Tauri process owns lifecycle and native services. Long-running work is represented by opaque process/session handles, cancellation tokens, and structured events. The renderer never receives secrets, raw host capabilities, or an unrestricted command executor.

## Required invariants

- One authoritative owner per PTY/process; renderer reload does not implicitly kill it.
- Every stream has ordering, bounded buffering, ACK/backpressure, resync, cancellation, and terminal-state semantics.
- Shutdown performs graceful cancellation before bounded forced termination and leaves no child tree behind.
- Workspace identity and scope accompany filesystem, Git, process, and SSH operations.
- CLI, headless serve, relay, and desktop reuse Rust core services rather than spawning a hidden Node backend.
