# SSH and Remote Workspace Behavior

## Current contract to preserve

The current relay/main implementation provides SSH-backed execution, remote PTYs, remote workspaces/worktrees, watchers, reconnect, parking, and remote subscriber state. Existing Docker and fault-harness scripts are behavioral specifications.

## Target contract

- Strict host-key verification and known-host management.
- Private-key/passphrase handling outside the renderer; SSH agent support where currently available.
- Proxy/jump host, keepalive, connection/operation timeouts, reconnect, and explicit cancellation.
- Remote exec with structured stdout/stderr/exit state and remote PTY with the same stream semantics as local PTY.
- Remote filesystem/workspace scoping and worktree recovery where the current product supports them.
- Stable error codes that distinguish authentication, host key, transport, timeout, remote process, and verification failures.
- No secret material in logs, Tauri payloads, crash reports, or exported diagnostics.

## Required evidence

Reproducible Docker hosts cover password/key/agent paths, unknown/changed keys, reconnect, watcher isolation, terminal parking, bulk open, interruption, and remote worktrees. Platform-specific agent and key-store behavior must be tested on Windows, macOS, and Linux before parity is green.
