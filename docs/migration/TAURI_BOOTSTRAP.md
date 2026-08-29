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
- The native shell creates one constrained main WebView window and embeds the real React build.
- A debug-only smoke mode uses isolated WebView data, hides the test window, and exits normally after startup. Release builds ignore both smoke environment variables.
- The capability file grants Tauri's core default plus only `close`, `minimize`, and renderer-callable `toggle_maximize` mutations to the main window; a Rust contract locks that exact set. No shell or broad filesystem plugin is enabled.

## Verification

| Command or check | Result |
| --- | --- |
| renderer desktop/window clients and WindowControls | PASS, 3 Vitest files / 13 tests |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS, 3 contract tests including exact main-window capability grants |
| Cargo clippy with all targets/features and warnings denied | PASS |
| direct Rust debug build | PASS |
| isolated hidden WebView2 smoke launch | PASS, exit 0 in 6.4s |
| frozen target dependency install | PASS, 17m19s |
| initial Tauri-specific React build | PASS, 10,642 modules in 1m47s |
| permission-reviewed custom-window Tauri-specific React build | PASS, 10,651 modules in 3m09s inside CLI build |
| `tauri info` with explicit Cargo path | PASS; all required Windows components detected |
| `tauri build --debug --no-bundle --ci` | PASS after custom-window migration; `target/debug/mcode-tauri.exe` produced |
| post-permission-review isolated WebView2 smoke | PASS, exit 0 in 8.9s without forced stop |

Known non-fatal build warnings are the inherited CSS minifier warning for the CSS Custom Highlight API, chunks over 500 kB, and Node 22 running below the repository's declared Node 24 engine. The migration will not relax production gates to hide those warnings.

## Next acceptance boundary

The custom titlebar is the first renderer-used `window.api` path moved end to end. It remains `in_progress`: close confirmation, tray behavior, quit/session handshakes, bounds restore, focus/navigation, popouts, real WebView2 interaction assertions, and macOS/Linux evidence are still required. The next slice ports that close lifecycle rather than treating a successful raw close as parity.
