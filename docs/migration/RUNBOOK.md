# Release Runbook — MCode Tauri (rewrite)

Audience: the agent or engineer producing the first Tauri-based MCode release.
State at writing: branch `rewrite/tauri-full` @ `883e3ae669` — 34/40 parity todos done; the five
open items and their blockers are listed at the bottom.

## 1. Prerequisites

| Requirement | Where | Status |
| --- | --- | --- |
| Rust 1.98+ toolchain (rustup, clippy, rustfmt) | `C:\Users\Milan M\.cargo\bin` | installed |
| Node 22 + pnpm 10 | system | installed |
| WebView2 runtime | OS component | present on Win10/11 |
| sherpa-onnx DLLs (voice) | `node_modules/sherpa-onnx-win-x64/` | present; bundled as resources follow-up (BUNDLE_PARITY.md) |
| Updater minisign key | `tauri signer generate` → CI secret | **MISSING — external blocker** |
| Signing cert (Authenticode/SignPath) | org policy | **MISSING — external blocker** |

## 2. Local verification ladder (run top-to-bottom before any build)

```sh
# 1. Rust gates
export PATH="/c/Users/Milan M/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --workspace 2>&1 | grep -c "warning: "   # expect 0
cargo test --workspace                                  # 38+ suites green

# 2. Web gates
npx oxlint                                              # 0 errors
npx tsc --noEmit -p config/tsconfig.web.json            # 19 known baseline errors, no NEW
npx vitest run src/renderer/src/desktop --config config/vitest.config.ts --maxWorkers=2

# 3. Renderer suite (known flaky at full parallelism — use the worker cap)
npx vitest run src/renderer/src/components/terminal-pane/ --config config/vitest.config.ts --maxWorkers=2
# expect: 385 files / 3741 passed | 1 skipped
```

## 3. Build

```sh
pnpm tauri build
# Output: src-tauri/target/release/bundle/{nsis,msi}/...
```

- Updater artifacts (`createUpdaterArtifacts: true`) are produced but **unsigned** until the
  minisign key is provisioned (`plugins.updater.pubkey` is intentionally empty — D-010,
  runtime injection at build time; updater fails closed on an empty key).
- Bundle resources: notification-sounds, plugins/launch, onboarding/feature-wall, skills
  (`tauri.conf.json > bundle > resources`, map form — see BUNDLE_PARITY.md for why).
- Voice: sherpa DLLs are NOT yet bundled (decision pending voice-port step 8); the app falls back
  gracefully (`speech://error`, session marked dead).

## 4. Smoke checklist (manual, Windows)

1. Launch → main window opens; tray icon appears (Open MCode / Settings… / Quit).
2. `mcode://skills/share/<id>` deep link (register `mcode` scheme on a debug build first) →
   renderer opens the share.
3. Tray Settings… → window reveals + Settings page opens (also verifies the pending one-shot on
   a cold renderer: launch minimized, click Settings before mount completes).
4. Clipboard: copy from Monaco, paste into xterm (verified write-then-read path).
5. Model download: Settings → Voice → download zipformer-zh-14m (57 MB; sha256 verified; progress
   events); interrupt and resume (Range header).
6. Dictation with the downloaded model (needs DLLs present — see §3 voice note).
7. PDF export: settings → export markdown → expect the structured unsupported error (wry has no
   programmatic print-to-pdf) — NOT a crash.

## 5. Known deviations & open items

| Item | Deviation | Tracked |
| --- | --- | --- |
| PDF export | returns structured `unsupported` (wry gap) | SECURITY_PERF_REVIEW.md, export_commands.rs doc |
| Notification dismiss | returns false (Action Center COM needed) | notification_commands.rs doc |
| Notification tag replace | independent toasts | notification_commands.rs doc |
| runtime metadata ACL | 0600 on POSIX only; Windows ACL deviation documented | runtime_metadata.rs |
| feedAudio JSON overhead | Channel/ArrayBuffer upgrade flagged | tauri-speech-client.ts:130 |
| Dictation session timeouts | caller-layer reaper not implemented | session.rs doc |
| Sherpa DLL bundling | pending voice step 8 | BUNDLE_PARITY.md |

## 6. Open todos and their blockers (as of this runbook)

- **Signing pipeline, crossgrade** — needs real signing credentials; simulated evidence prohibited.
- **Homebrew/AUR, update survival** — needs published artifacts + CI hosts.
- **Remove Electron dependencies** — gated on all parity rows green + the E2E lane live
  (CUTOVER_INVENTORY.md §8 defines the 14-step order; steps 4–11 parallelize).
- **Build all platforms, full tests** — needs macOS/Linux CI hosts (Windows validated locally).
- Mobile companion interop — needs a device session + relay director host (separate blocked row).

## 7. Rollback

The Electron app is untouched: original repo `D:\ADE\orca` (HEAD `5c13ecce88`, user-active) and
read-only backup `D:\ADE\orca-electron-backup-20260829-095236` (manifest-verified). The rewrite
is fully additive on its own branch; shipping a Tauri build requires no cutover of the original
until the parity gates in CUTOVER_INVENTORY.md §8 pass.
