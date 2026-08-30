# Security Model

## Trust boundaries

- Repository/workspace content is untrusted, including symlinks, Git config/hooks, terminal escape sequences, agent output, and generated files.
- Renderer/web content is less privileged than Rust services and receives only capability-scoped commands.
- SSH peers, relay clients, deep links, update feeds, provider responses, and native helpers cross explicit validation boundaries.
- Agent processes are powerful local actors but are not allowed to expand workspace or secret scope implicitly.

## Required controls

- Minimal Tauri capabilities per window/webview and a restrictive CSP.
- No remote content in privileged webviews; validated deep links and external URLs.
- Canonical workspace roots, traversal/symlink escape protection, atomic files, and safe temporary paths.
- Filesystem operations are relative to open capability-directory handles whose identity is checked against the canonical requested root at capture time. Trusted-root refresh serializes exact snapshot comparison and atomic replacement; slow platform discovery releases the settings-write lock, memoizes failures per extraction, then revalidates the snapshot so stale authority cannot be returned or reinstalled. Temporarily unavailable roots are retried, absolute symlink targets with parent components fail closed, and WSL UNC containment preserves the Linux tail's case sensitivity.
- File reads use one already-authorized open handle for metadata and payload, enforce a byte limit before and after reading with a one-byte growth sentinel, and never hand an oversized partial buffer to the renderer. File saves resolve contained symlinks through capability handles, use a cryptographically random `create_new` temporary in the target directory, apply the target permission policy before payload bytes, flush the complete temporary, then atomically rename it; returned failures remove the temporary and Unix commits also sync the containing directory. Dynamic external grants, crash-stale temp recovery, and Windows DACL/extended-metadata preservation remain explicit release gaps.
- Validated argument vectors, bounded environment forwarding, process-tree ownership, and terminal control-sequence handling.
- Host-key verification, protected credentials, established cryptography, protocol versioning, and replay/downgrade tests.
- Signed updates, artifact provenance, draft gates, and channel allowlists.
- Structured logs with correlation/workspace/agent/session/process/platform/duration/result identifiers and content/secret redaction.

## STRIDE scenarios to test

Malicious repositories and hooks; manipulated workspace files; terminal/escape injection; malicious deep links; relay/pairing spoofing or replay; compromised update feeds; SSH MITM; symlink/path races; unsafe native helpers; renderer compromise; and diagnostic/telemetry leakage.

A formal threat-model artifact and focused security review are required before release. This document defines initial boundaries, not a completed review.
