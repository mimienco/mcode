# Tauri Migration Plan

The migration is a sequence of independently verifiable vertical slices. Electron remains the behavioral oracle until every parity entry has implementation and evidence.

1. Preserve the source and establish integrity guardrails.
2. Inventory Electron, preload, Node-native, renderer, release, data, and protocol surfaces.
3. Capture Electron functional and performance baselines on the same hardware.
4. Bootstrap a Rust workspace and minimal Tauri shell while keeping the React renderer and Test Bench design system intact.
5. Replace preload with generated TypeScript contracts over narrow Tauri commands and channels.
6. Port lifecycle, configuration, logging, identifiers, settings migration, and window capabilities.
7. Port scoped filesystem, watcher reconciliation, Git, worktrees, path rules, and WSL behavior.
8. Port process supervision, PTY streaming/recovery, agent execution, and terminal UI contracts.
9. Port SSH, remote worktrees, relay, pairing, mobile compatibility, CLI, and headless serve.
10. Port updater/channels, packaging, crossgrade, telemetry, integrations, voice, computer use, and OS permissions.
11. Replace platform/release/e2e gates, run security and license reviews, and produce installable cross-platform artifacts.
12. Remove Electron and Node-native desktop dependencies only after every parity row is green or explicitly accepted.

Each slice requires tests, parity evidence, a status update, an original-integrity check, and a small commit. No push is authorized.
