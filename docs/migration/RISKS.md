# Migration Risks

| Risk | Impact | Mitigation / evidence required |
| --- | --- | --- |
| Existing source is heavily dirty | Accidental loss or false attribution | Immutable original, two verified copies, manifest guard after every phase, explicit staging only |
| PTY/process behavior differs by OS | Lost output, orphaned agents, shutdown failures | Behavioral inventory, golden streams, resize/load/kill-tree tests, same-machine benchmarks |
| System WebViews differ | Monaco/xterm/IME/clipboard regressions | Real WebView2, WKWebView, and WebKitGTK E2E matrix |
| IPC inventory misses dynamic channels | Silent feature loss | Deterministic AST inventory plus an explicit call-site resolution ledger; generation fails on stale/unknown resolutions, and parity cannot turn green from regex alone |
| Wire protocol drifts | Existing mobile clients break | Golden vectors, explicit versions/capabilities, old-client/new-host and inverse compatibility tests |
| Updater identity changes | Crossgrade or rollback failure | Signed channel matrix and a bridge release if direct replacement is unreliable |
| Git/WSL/path semantics drift | Worktree corruption or inaccessible files | Git 2.25 capability tests, Windows/WSL path corpus, interruption recovery tests |
| Rust is absent from the inherited process PATH | Tauri CLI reports a false missing-toolchain error | Prepend `%USERPROFILE%\.cargo\bin` in migration commands/CI; pinned Rust 1.98.0 is installed and verified |
| Signing/notarization credentials unavailable | Cannot prove production artifact trust | Implement all credential-independent gates; document exact external secret checks without simulating success |
| New dependency licensing | Distribution incompatibility | License allowlist and generated notices before adoption |
| Scope is larger than one session | Incomplete handoff | Status, parity JSON, decisions, risks, test evidence, and small commits are mandatory checkpoints |
| Renderer receives a native quit but never acknowledges it | Native exit can hang forever | Exact request IDs plus a 10-second timeout force only an unreceived request; a receipt ACK disarms the timeout while the user decides |
| Renderer or WebView disappears during shutdown delivery/execution | A prevented native exit or confirmed request can be stranded | Delivery failure creates a fresh executing Quit; confirm-and-execute is one backend command; confirmed requests remain force-claimable |
| Full renderer/package builds exhaust Windows pagefile under concurrent work | Current artifact evidence can be unavailable despite green focused contracts | Keep resource limits explicit, retain targeted real WebView2 protocol smoke evidence, and rerun full production build on a quiet/CI host before parity completion |
