# Tauri Bootstrap Evidence

## Scope

This checkpoint proves that the copied React renderer can be built by a Tauri-specific frontend lane and embedded in a native Tauri 2 shell on Windows. It does **not** prove functional parity: almost all renderer operations still target the Electron `window.api` facade and must be moved behind typed domain clients.

## Pinned stack

- Rust 1.98.0, `x86_64-pc-windows-msvc`, with rustfmt and clippy.
- Tauri core 2.11.5 and `tauri-build` 2.6.3.
- Tauri CLI 2.11.4 and JavaScript API 2.11.1.
- WebView2 151.0.4129.107 and Visual Studio Build Tools 2022 on the validation host.

The toolchain is selected by `rust-toolchain.toml`. This host requires `%USERPROFILE%\.cargo\bin` to be prepended to `PATH` for JavaScript-spawned Tauri CLI processes.

## Implemented slice

- `mcode-app::LifecycleService` returns a serialized launch context with explicit development/production lane, headless, and safe-mode fields.
- `app_get_launch_context` is a narrow Tauri command backed by managed application state.
- The renderer has a runtime-validating `DesktopClient` abstraction plus a Tauri invoke adapter.
- Renderer-drawn window controls use a typed runtime-selected window client. Electron retains its existing preload behavior; Tauri lazily binds its core window capability for minimize, maximize/restore, state query/subscription, and close request.
- Native manual-close requests are intercepted once, run the existing dirty-editor and running-child decisions, then await a versioned Rust shutdown checkpoint before the single confirmed native close.
- `mcode-settings::SettingsService` validates the checkpoint envelope, merges local/host session partitions and the active-view patch, reconciles stale local and runtime-host snapshots with host-owned terminal topology, bounds the profile file to 64 MiB before reading, and commits with an atomic same-directory replacement plus `sync_all`.
- Blocking profile work runs on Tauri's blocking worker lane. Validation or durability failure leaves the window open and produces a safe renderer toast.
- Prior and incoming terminal-retirement tombstones are applied before local binding preservation. Terminated matching SSH leases are not restored while live local/app-scoped bindings, missing buffers, scrollback references, titles, and newer host topology survive a stale renderer checkpoint.
- The Rust boundary validates nested session records and uses a typed execution-host identifier; a shared v1 fixture guards Rust/TypeScript field and mirrored nested-enum parity.
- The native shell creates one constrained main WebView window and embeds the real React build.
- Windows startup restores only meaningful on-screen inner bounds, restores maximized state, fits first-launch outer dimensions within the primary work area, and persists normal placement through a 500 ms debounced atomic merge.
- Windows attempts tray installation immediately after native window creation, again after the first page finishes loading, and through a 12-second fallback. Left click/Open restores and focuses the main window; Quit restores it before entering the app-wide acknowledgement path.
- The live close-to-tray preference is owned by a narrow Rust command and native state. The renderer update is rolled back if native synchronization fails; native close hides only on Windows while the lifecycle is idle and a tray is confirmed available.
- A debug-only smoke mode uses isolated WebView data, hides the test window, and exits normally after startup. Release builds ignore both smoke environment variables.
- The capability file grants Tauri's core default plus only `close`, `minimize`, and renderer-callable `toggle_maximize` mutations to the main window; a Rust contract locks that exact set. No shell or broad filesystem plugin is enabled.

## Verification

| Command or check | Result |
| --- | --- |
| renderer lifecycle/window/session contracts and WindowControls | PASS, 12 Vitest files / 80 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS, including 22 settings contracts, 3 Tauri command/capability contracts, and 1 lifecycle contract |
| Cargo clippy with all targets/features and warnings denied | PASS |
| direct Rust debug build | PASS |
| isolated hidden WebView2 smoke launch | PASS, exit 0 in 6.4s |
| frozen target dependency install | PASS, 17m19s |
| initial Tauri-specific React build | PASS, 10,642 modules in 1m47s |
| permission-reviewed custom-window Tauri-specific React build | PASS, 10,651 modules in 3m09s inside CLI build |
| `tauri info` with explicit Cargo path | PASS; all required Windows components detected |
| `tauri build --debug --no-bundle --ci` | PASS after custom-window migration; `target/debug/mcode-tauri.exe` produced |
| post-permission-review isolated WebView2 smoke | PASS, exit 0 in 8.9s without forced stop |
| visible Windows close-to-tray WebView2 smoke | PASS, saved inner size restored, tray installation confirmed, setting persisted, marker `hidden`, renderer-acknowledged direct exit 0, zero processes; tray menu interactions remain untested |

Known non-fatal build warnings are the inherited CSS minifier warning for the CSS Custom Highlight API, chunks over 500 kB, and Node 22 running below the repository's declared Node 24 engine. The migration will not relax production gates to hide those warnings.

The current lifecycle checkpoint could not reproduce the full renderer/package smoke on this host: three builds exhausted memory while unrelated applications were consuming it, with the narrowest attempt failing on a 196 KiB allocation. Earlier green package/smoke evidence remains historical only and is not treated as verification of the new checkpoint.

## Next acceptance boundary

The custom titlebar, confirmed close checkpoint, Windows startup restore, and core tray path are now moved end to end. Windowing remains `in_progress`: the one-time hidden-window notice, attention/activity state, navigation containment, popouts, broader real-renderer interaction assertions, and macOS/Linux evidence are still required.
