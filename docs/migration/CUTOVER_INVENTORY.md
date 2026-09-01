# Cutover Dead-Code Inventory (Electron → Tauri)

Repo: `D:/ADE/orca-tauri-rewrite-20260829-095236`, branch `rewrite/tauri-full`, HEAD `fee656bd13823bbcd96bbd53e98592d367112a49` (2026-09-02).

Purpose: inventory everything that becomes dead once the Tauri renderer ships, mark each group
**DELETE** / **KEEP** / **MIGRATE**, and fix a safe deletion order. **Nothing is deleted by this
document** — per `PLAN.md` step 12 and the removal gate in `ELECTRON_SURFACE.md`, no Electron or
Node-native production dependency is removed until its responsibility has a green row in
`parity-matrix.json` and an equivalent test has replaced each Electron-specific assertion.

All numbers below were produced by the commands in the appendix on this working tree (2026-09-02;
working tree carries unrelated user WIP, which does not affect the counted surfaces).

---

## 1. Volume summary

| Surface                                            | Files   | LOC (incl. blank)  | Disposition |
| -------------------------------------------------- | ------: | -----------------: | ----------- |
| `src/main/**` total                                |   4,932 |          1,176,353 | mixed (see §2) |
| — of which imports `electron` (production, no `*.test.*`) | 296 |         150,654 | DELETE/MIGRATE per row |
| — of which imports `electron` (`*.test.*` only)    |      33 |           61,162 | DELETE with their units |
| `src/main` test files total (all)                  |   2,320 |          690,223   | mostly MIGRATE-then-DELETE |
| `src/preload/**` total (all reference the Electron surface) | 116 | 15,322          | DELETE last (types reusable) |
| `electron.vite.config.ts`                          |       1 |               331 | DELETE |
| `config/electron-builder.config.cjs`               |       1 |               703 | DELETE |
| `package.json` scripts referencing electron        |  43 / 133 | —               | DELETE / replace |
| `config/scripts/*` referencing electron-builder    |      22 | —                 | DELETE / replace |

Headline dead volume at full parity: **~227k LOC** (150.7k main production + 61.2k main
electron-coupled tests + 15.3k preload) plus the config/script shell. The remaining ~1.0M LOC of
`src/main` is logic that must first **migrate** (or is already shared) — see §2/§5.

---

## 2. `src/main/**` — files importing `electron`

`grep -rlE "from ['\"]electron['\"]|require\(['\"]electron['\"]\)" src/main --include="*.ts"` → **329 files**
(296 production + 33 test files). Production LOC by top-level directory:

| Dir            | Prod files | Prod LOC | Notes / disposition |
| -------------- | ---------: | -------: | ------------------- |
| `runtime`      |          4 |   43,400 | `mcode-runtime.ts` alone is 41,074 LOC and hosts **headless serve** as well as desktop — split, do not bulk-delete (§5) |
| `ipc`          |        121 |   41,742 | one handler file per IPC family (679 `.ts` files / 174,262 LOC in the dir incl. non-electron helpers); delete family-by-family as parity rows go green |
| `browser`      |         39 |   14,349 | embedded-browser (BrowserView/webContents) — MIGRATE to `browser.emulator` target; `psl` dep dies here |
| *(top level)*  |         21 |    9,181 | `index.ts` (3,493), `updater.*` family, `app-icon.ts`, `app-relaunch.ts`, `serve-update-handoff.ts`, etc. |
| `ssh`          |          4 |    5,398 | Electron-side glue only; transport already ported (`crates/mcode-ssh`), commands in `src-tauri/src/ssh_commands.rs` |
| `rate-limits`  |          9 |    5,167 | provider usage fetchers — MIGRATE under `integrations.providers` (in_progress) |
| `codex-accounts` |        2 |    4,389 | OAuth window flow → Rust |
| `persistence`  |          2 |    4,242 | Electron `safeStorage`/userData store → `crates/mcode-settings` + crossgrade |
| `window`       |         20 |    3,599 | BrowserWindow services → `desktop.windows` / `desktop.shell-ui` (mcode-platform, `desktop_window.rs`, `windows_tray.rs`) |
| `claude-accounts` |       3 |    2,136 | `runtime-auth-service.ts` etc.; **`keychain.ts` is in the CLI closure — KEEP that file** |
| `daemon`       |          2 |    1,777 | only 2 of 284 files; daemon core is Electron-free |
| `speech`       |          3 |    1,585 | `stt-worker.ts`, `openai-api-key-store.ts`, + `ipc/speech.ts` — MIGRATE to `crates/mcode-voice` |
| `crash-reporting` |       6 |    1,300 | `crashReporter` API → Rust/tauri |
| remaining 27 dirs (≤1.3k LOC each) | ~53 | ~11,000 | tray, menu, dock, star-nag, emulator glue, warp-themes, plugins, skills, startup, mcode-profiles, telemetry, jira, ai-vault, agent-hooks, computer, pi, stats, ports, memory, network, i18n, hang-watchdog, pty, lib, minimax, observability, artifacts |
| **Total**      |  **296**   | **150,654** | |

Test files importing `electron`: 33 files / 61,162 LOC, concentrated in `window` (11), `browser`
(11), `ipc` (7), `startup` (2), plus one each in `ssh`/`runtime`. These die with their production
units; the other 2,287 main test files (≈629k LOC) exercise migratable logic and move/re-target to
the Rust contract tests or the Tauri E2E lane.

### Entry point

`src/main/index.ts` wires everything: `app, BrowserWindow, dialog, ipcMain, nativeTheme,
powerMonitor` from `electron` plus 300+ local imports. It is the last file to go (order step 11).

---

## 3. `src/preload/**` — the contextBridge surface

- 116 files / 15,322 LOC; 94 API definition files under `src/preload/api/`.
- 5 files import `electron` directly: `index.ts` (5,173 lines — `contextBridge, ipcRenderer,
  webFrame, webUtils`), `gitlab.ts`, `browser-window-close.ts`, `renderer-restart-wiring.ts`,
  `usage-provider-api.ts`. The rest are type modules consumed by both preload and renderer.
- Renderer coupling: **737 renderer files** reference `window.api` across **77 distinct top-level
  namespaces** (`pty` 446 sites, `ui` 394, `shell` 167, `gh` 148, `runtimeEnvironments` 103, …).
- Replacement already in progress: `tauri-window-api-shim.ts` (clipboard wave) installed first in
  `main.tsx`; typed clients (`pty-client.ts`, `fs-client.ts`, `ssh-client.ts`) route via
  `isTauri()`.

Disposition: **DELETE — last of all cutover work** (order step 10). The `api/*.d.ts` type
contracts stay valuable as the shim's type source; delete the files only when the shim owns the
full 77-namespace surface and `grep -rl "window\.api\." src/renderer` returns 0 files outside the
shim.

---

## 4. Build configs, scripts, package.json

| Item | Detail | Action |
| ---- | ------ | ------ |
| `electron.vite.config.ts` (331 LOC) | main/preload/renderer bundling; also carries renderer chunk-budget plugin used by `check-renderer-chunk-budgets.mjs` | **DELETE** once `config/vite.tauri.config.ts` (41 LOC) + `vite.web.config.ts` (31 LOC) are the only build lanes; port the chunk-budget check if still wanted |
| `config/electron-builder.config.cjs` (703 LOC) + `electron-builder-config.test.mjs` | NSIS/dmg/AppImage, channels, sherpa dylib pruning, signing | **DELETE** after `packaging.release` row green (BUNDLE_PARITY.md records the mapping) |
| `packaged-runtime-node-modules.cjs` | node_modules packaging contract (node-pty, sherpa-onnx) | **DELETE** when the deps go |
| package.json deps | `@electron-toolkit/preload` (**0 source usages** — only a vite `exclude` entry), `@electron-toolkit/utils` (7 prod main files), `electron-updater` (`src/main/electron-updater-loader.ts` + ~15 `updater*.ts` modules) | first two **DELETE** (utils after order step 6); updater after `updater.channels` green |
| package.json devDeps | `electron ^43`, `electron-builder`, `electron-vite`, `app-builder-lib`, `@electron/asar`, `@electron/rebuild`, `@electron-toolkit/tsconfig` | **DELETE** with steps 1–3 (tsconfig base can be inlined first) |
| package.json scripts | 43 of 133 mention electron (`ensure:electron-runtime`, `build:electron-vite*`, `rebuild:electron`, all `test:e2e*` electron projects, 11 `bench:*` lanes) | **DELETE/replace** — E2E must move to the Tauri/WebDriver lane first (separate cutover slice), benches re-anchored on the Tauri build |
| `tests/playwright.config.ts` | `electron-headless` / `electron-headful` projects; 14 e2e/tool files use `_electron.launch`; 299 spec files total, 143 touch `window.api` | **MIGRATE** (E2E replacement inventory is its own cutover slice); specs die with the Electron lane |
| `config/tsconfig.node.json` | `include: ../src/main/**/*` | retarget to the surviving main subset, then **DELETE** with step 11 |

---

## 5. Node-only runtime deps — require-site mapping

Method: per-dep import scan across `src/{main,preload,relay,cli,renderer,shared}`, `tools`,
`config/scripts`, `tests` (commands in appendix). Counts are production (non-test) files.

| Dependency | main | preload | relay | cli | renderer | shared | Verdict |
| ---------- | ---: | ------: | ----: | --: | -------: | -----: | ------- |
| `@electron-toolkit/preload` | 0 | 0 | 0 | 0 | 0 | 0 | **DELETE now** — zero require sites anywhere in source |
| `@electron-toolkit/utils` | 7 | 0 | 0 | 0 | 0 | 0 | **DELETE** with main (order step 6) |
| `electron-updater` | 1 (+15 updater modules) | 0 | 0 | 0 | 0 | 0 | **DELETE** after `updater.channels` green; replaced by `crates/mcode-updater` + `src-tauri/src/updater_commands.rs` |
| `ssh2` | 36 | 0 | 0 | 0 | 0 | 0 | **DELETE** after `ssh.workspaces` green — transport already `russh` (`mcode-ssh`), commands `ssh_*` |
| `@parcel/watcher` | 7 | 0 | 0 | 0 | 0 | 0 | **DELETE** after `filesystem.watcher` green — replaced by `crates/mcode-fs` watcher |
| `posthog-node` | 2 | 0 | 0 | 0 | 0 | 0 | **MIGRATE** — telemetry consent already ported (`mcode-app::telemetry_consent`); client goes with `telemetry.diagnostics` |
| `@linear/sdk` | 7 | 0 | 0 | 0 | 0 | 0 | **MIGRATE/DELETE** with `integrations.providers` (in_progress) — provider-neutral command contracts per IPC_MAP |
| `psl` | 1 (`browser/browser-cookie-import-policy.ts`) | 0 | 0 | 0 | 0 | 0 | **DELETE** with the browser subtree |
| `sherpa-onnx` (+5 platform optionals) | 0 static — `stt-worker.ts` does `require(workerData.sherpaModulePath)` at runtime; DLLs (`sherpa-onnx-c-api.dll`, `onnxruntime.dll`) are today's evidence source for `crates/mcode-voice` | 0 | 0 | 0 | 0 | 0 | **MIGRATE** → `crates/mcode-voice/src/sherpa` (FFI live, runtime GO); **DELETE** only after voice steps 2–8 land (order step 8) |
| `windows-native-registry` (optional) | 1 (`src/main/windows-native-registry.ts`) + 5 config scripts | 0 | 0 | 0 | 0 | 0 | **MIGRATE** — check each consumer against `crates/mcode-platform` (WSL/registry helpers) before deleting |
| `node-pty` | 8 | 0 | **1** (`src/relay/pty-handler.ts`, `pty-shell-utils.ts`) | 0 | 0 | 0 | **KEEP** — the Node relay host needs it; only the Electron main copies die (`crates/mcode-pty` covers desktop) |
| `serve-sim` | 2 (emulator) | 0 | 0 | **1** (`src/cli/help.ts`) | 0 | 0 | **KEEP** (CLI references it); Electron emulator glue deletes separately |
| `agent-browser` | 1 (`browser/agent-browser-bridge.ts`) | 0 | 0 | 0 | 0 | 0 | **MIGRATE/DELETE** with `browser.emulator` (not_started) |
| `ws` | 26 | 0 | relay/shared/cli use | 1 | 2 | 31 | **KEEP** — wire protocol / relay transport |
| `zod` | 93 | 0 | 0 | 0 | 0 | 127 | **KEEP** — shared validation |
| `yaml` | 4 | 0 | 0 | 0 | 0 | 2 (+24 scripts) | **KEEP** |
| `jsonc-parser` | 1 (`devin/hook-config-json.ts`) | 0 | 0 | via CLI closure | 0 | 0 | **KEEP** — the devin hook service is in the CLI closure (§6) |
| `@xterm/headless` / `@xterm/addon-serialize` | 7 / 2 | 0 | 0 | 0 | yes | 5 / 3 | **KEEP** — renderer terminal stack |
| `tweetnacl` | 4 | 0 | 0 | 0 | 3 | 5 | **KEEP** — E2EE (mcode-wire parity) |
| `qrcode` | 0 | 0 | 0 | 0 | 2 (+1 script) | 0 | **KEEP** — mobile pairing QR |
| `i18next` / `react-i18next` | 1 / 0 | 0 | 0 | 0 | 3 / 51 | 0 | **KEEP** — renderer i18n |

---

## 6. KEEP-with-care — src/main code still needed without Electron

Direct imports of `src/main` from `src/relay` / `src/cli` source: **zero** (the separation is via
tsconfig file lists, not import paths).

### 6.1 CLI closure (`config/tsconfig.cli.json`)

95 `src/main` files / **16,756 LOC** compile into the headless CLI. Transitive-closure check
confirmed **0 of them import `electron`** (one comment-only false positive:
`startup/shell-path-hydration-cache.ts:9` explicitly documents *why* it avoids electron). Breakdown:
`codex` 39, `agent-hooks` 12, provider `hook-service`s (`copilot` 4, `hermes` 4, `antigravity` 4,
`amp` 3, `claude` 3, `devin` 3, `kimi` 2, plus gemini/grok/droid/cursor/command-code/openclaude),
`startup` 4 (`hydrate-shell-path`, `serve-mode-argv`, `shell-path-hydration-cache`,
`windows-shell-path-ownership`), `sqlite/sync-database.ts`, `claude-accounts/keychain.ts`,
`runtime/runtime-metadata.ts`, `win32-utils.ts`, `rolling-file-backup.ts`.

**KEEP all 95** (or relocate, but do not delete) regardless of cutover progress. The `startup/*`
four are also candidates for the Tauri host to reuse as-is during crossgrade.

### 6.2 Relay closure (`config/tsconfig.relay.json`)

Exactly 2 files: `src/main/pty/overlay-mirror.ts` + `src/main/pty/shell-startup-env.ts`
(**456 LOC**, no electron imports). **KEEP** — `relay.host` is `not_started`; the relay host stays
a Node process (`build:relay`) and still needs `node-pty` + `ws`.

### 6.3 Headless serve path (Electron-coupled, do NOT bulk-delete)

`mcode serve` mode runs through the same `src/main/index.ts` + `runtime/mcode-runtime.ts`
composition as the desktop app (`serve-mode-argv.ts`, `server/serve-readiness.ts`,
`serve-stdout-boundary.ts`, `serve-update-handoff.ts`, `automations/headless-*`,
`daemon/headless-emulator`, 163 `headless` mentions in `mcode-runtime.ts`). The Rust side already
has `mcode-wire::server` (headless relay accept) and `mcode-app::runtime_metadata`, but the
behavioral host is still the TS one. **Disposition: MIGRATE** the serve composition to the Tauri
Rust host (or a standalone Rust daemon under `headless.serve`, currently `in_progress`) *before*
any deletion touches `runtime/` or top-level `index.ts`. Until then these files are
DELETE-blocked even though they import `electron`.

### 6.4 Mobile companion

The mobile app (`mobile/`, Expo) pairs through the relay/wire protocol and shares nothing from
`src/main` directly; its coupling is the relay closure (§6.2) and `src/shared` (which is not part
of this inventory). No additional `src/main` file is mobile-pinned beyond §6.2.

---

## 7. MIGRATE targets by parity row (from `parity-matrix.json`)

| src/main surface | Parity row (status) | Target |
| ---------------- | ------------------- | ------ |
| `speech/` (27 files, 5,495 LOC) + `ipc/speech.ts` | `voice.sherpa` (not_started; FFI spike + live runtime GO done) | `crates/mcode-voice/src/sherpa`; commands `speech_start`, `speech_stop`, `speech_models`; remaining: model catalog/download manager, session service, renderer client + shim, cloud/keystore, macOS TCC, mobile RPC, packaging (STATUS.md wave-14 steps 2–8) |
| `ssh/` glue + remote worktrees | `ssh.workspaces` (not_started) | `crates/mcode-ssh` + `src-tauri/src/ssh_commands.rs` (8 commands landed) |
| `browser/` + `emulator/` + `computer/` | `browser.emulator`, `computer.use` (not_started) | Rust capability commands; `agent-browser` bridge decision pending |
| `updater*.ts` family (top level, ~15 files) + `electron-updater-loader.ts` | `updater.channels` (in_progress) | `crates/mcode-updater` + `src-tauri/src/updater_commands.rs` |
| `window/` (20), `tray/` (3), `menu/` (2), `dock/` (2), `startup` window services | `desktop.windows`, `desktop.shell-ui` (in_progress) | `crates/mcode-platform` + `desktop_window.rs`, `windows_tray.rs`, `app_menu_events.rs` |
| `persistence/loading-store/` (Electron `safeStorage`, userData paths) | `settings.crossgrade` (not_started; `mcode-settings::crossgrade_discovery` landed) | `crates/mcode-settings` |
| `rate-limits/` (9 files) | `integrations.providers` (in_progress) | provider-neutral Rust commands |
| `telemetry/` (posthog-node), `crash-reporting/` (6) | `telemetry.diagnostics` (not_started) | Rust telemetry/crash commands |
| `ipc/notifications*`, `notification-sound-*` | `notifications.status` (not_started; dispatch command landed) | `notification_commands.rs` + renderer shim merge |
| `runtime/mcode-runtime.ts` serve paths, `server/`, `automations/headless-*` | `headless.serve` (in_progress), `relay.host` (not_started) | Rust host daemon over `mcode-wire::server` (§6.3) |
| `agent-hooks/server.ts` + per-agent hook services | `agents.execution` (not_started) — but CLI closure keeps the pure files | split: CLI-kept pure logic stays; Electron IPC/notify parts migrate |
| `ai-vault/`, `daemon/` scanning layers | no dedicated row yet | attach to `agents.execution` / `workspace.session` rows before deleting anything |

---

## 8. Safe deletion order

Each step is gated on the named parity rows being green **and** the replacement test lane passing.
"Tests survive" = the step does not break any remaining suite.

1. **`@electron-toolkit/preload` dependency** — zero require sites in source (only a vite
   `externalizeDeps.exclude` entry). Anytime; tests unaffected.
2. **Electron packaging shell**: `config/electron-builder.config.cjs` +
   `electron-builder-config.test.mjs` + `packaged-runtime-node-modules.cjs` +
   `ensure:electron-runtime` / `build:unpack` / `build:win|mac|linux` scripts + devDeps
   `electron`, `electron-builder`, `app-builder-lib`, `@electron/asar`, `@electron/rebuild`.
   *Gate: `packaging.release` green, E2E replaced by the Tauri driver lane (no remaining
   `test:e2e*` consumer), BUNDLE_PARITY rows closed.*
3. **`electron.vite.config.ts` + `run-electron-vite-*.mjs` / `run-electron-vite-dev` scripts +
   `electron-vite` / `@electron-toolkit/tsconfig`**. *Gate: `config/vite.tauri.config.ts` and
   `vite.web.config.ts` are the only build lanes; `verify-web-build` green. Port the
   chunk-budget check first.*
4. **Desktop-window-only main subtrees**: `window/` (20), `tray/`, `menu/`, `dock/`,
   `startup` window-lifecycle files, top-level `app-icon.ts` / `app-relaunch.ts`.
   *Gate: `desktop.windows` + `desktop.shell-ui` green; their 22 electron-importing test files
   replaced by Rust contract tests + WebView2 smokes.*
5. **Electron-only IPC families** in `src/main/ipc/` whose namespaces are fully covered by typed
   Tauri clients + shim (clipboard, dialogs, deep links, menus, fs, git, pty command surface).
   Family-by-family, each gated on its IPC_MAP row; keep families listed in §6 out of scope.
6. **Browser/emulator/computer** subtrees + `psl` + `agent-browser`. *Gate: `browser.emulator` /
   `computer.use` decided (ported or explicitly dropped).*
7. **SSH remote-workspace glue** + `ssh2` dependency. *Gate: `ssh.workspaces` green with real
   sshd evidence (wave-10 honesty note still open).*
8. **Speech**: `src/main/speech/` + `ipc/speech.ts` + `sherpa-onnx` + platform optionals —
   **only after voice steps 2–8 land** (model catalog/download manager, session service, renderer
   client + shim merge, cloud/keystore, macOS TCC, mobile RPC, packaging per STATUS wave-14) and
   `voice.sherpa` row green. Until then the sherpa DLLs also remain the `mcode-voice` test
   evidence source.
9. **Updater family** (`updater*.ts`, `electron-updater-loader.ts`, `update-install-exit-watchdog.ts`,
   linux package recovery) + `electron-updater` dep. *Gate: `updater.channels` green with signed
   feed evidence.*
10. **`src/preload/**`** — after every one of the 77 `window.api` namespaces is served by
    `tauri-window-api-shim.ts`/typed clients and `grep -rl "window\.api\." src/renderer` is empty
    outside the shim; `api/*.d.ts` types are folded into the shim's contract types first. This is
    the last renderer-visible removal (737 files reference the surface today).
11. **Telemetry/crash/persistence leftovers** (`telemetry/`, `crash-reporting/`,
    `persistence/loading-store/`) + `posthog-node`. *Gate: `telemetry.diagnostics`,
    `settings.crossgrade` green.*
12. **Headless serve composition** — `runtime/mcode-runtime.ts` (after splitting the 41k-line
    file), `server/`, `automations/headless-*`, remaining `runtime/` electron glue.
    *Gate: `headless.serve` green on the Rust host AND relay/mobile unaffected.*
13. **`src/main/index.ts` + `config/tsconfig.node.json` retarget + `@electron-toolkit/utils`** —
    the app composition goes when nothing above remains.
14. **Final sweep**: remaining `@parcel/watcher`, `@linear/sdk` (if integrations moved), and the
    43 electron scripts. Re-run the appendix greps; every command must return 0 outside
    the KEEP set of §6 (CLI closure 95 files, relay closure 2 files, `node-pty`/`ws` for relay).

Steps 4–11 are mutually independent once steps 1–3 are done and can proceed in parallel per owner,
with the §6 KEEP-with-care list as a hard exclusion set for every deletion batch.

---

## Appendix — verification commands used

All counts from this working tree, 2026-09-02 (Git Bash; `\` path separators normalized in the doc):

```sh
# Electron importers in src/main (files):
grep -rlE "from ['\"]electron['\"]|require\(['\"]electron['\"]\)" src/main --include="*.ts" | wc -l
# → 329  (296 production after filtering out *.test.*, 33 test files)

# Production LOC of electron-importing main files, grouped (node walk matching the same regex):
# per-dir table in §2 — script sums fs.readFileSync(...).split('\n').length per matched file
# → TOTAL|296|150654   (test files: 33 files / 61162 LOC)

# src/main totals:
find src/main -type f \( -name "*.ts" -o -name "*.js" \) | wc -l          # → 4932
find src/main -type f -name "*.test.ts" | wc -l                           # → 2320
cat of non-test *.ts | wc -l                                              # → 486130
cat of test *.ts    | wc -l                                               # → ~690223 (LF-split)

# Preload:
find src/preload -type f \( -name "*.ts" -o -name "*.d.ts" \) | wc -l     # → 116
cat src/preload/** | wc -l                                                # → 15322
grep -rlE "from ['\"](electron|electron-updater)['\"]" src/preload        # → 5 files

# Renderer surface coupling:
grep -rl "window\.api" src/renderer --include="*.ts" --include="*.tsx" | wc -l   # → 737
grep -rEoh "window\.api\.[a-zA-Z_]+" src/renderer … | cut -d. -f1 | sort -u | wc -l  # → 77 namespaces

# Dep require-site mapping (per dep, per area) — node script, counts in §5.
# Invisible-dynamic deps checked repo-wide (src, tools, config/scripts, tests, native, mobile…):
#   sherpa-onnx → 0 static imports (dynamic require(modulePath) in stt-worker.ts)
#   windows-native-registry → src/main/windows-native-registry.ts + 5 config scripts
#   serve-sim → src/main/emulator ×2 + src/cli/help.ts
#   agent-browser → src/main/browser/agent-browser-bridge.ts

# CLI / relay closures:
#   closure computed over relative imports from the tsconfig.cli.json / tsconfig.relay.json
#   file lists: CLI → 95 files / 16,756 LOC / 0 electron imports; relay → 2 files / 456 LOC.

# package.json / scripts:
#   133 scripts, 43 mention electron; deps: @electron-toolkit/preload|utils, electron-updater;
#   devDeps: electron, electron-builder, electron-vite, app-builder-lib,
#            @electron/asar, @electron/rebuild, @electron-toolkit/tsconfig
#   optionalDeps: sherpa-onnx-{darwin,linux,windows}-* ×5, windows-native-registry

# Configs:
wc -l electron.vite.config.ts        # → 331
wc -l config/electron-builder.config.cjs  # → 703
wc -l config/vite.tauri.config.ts vite.web.config.ts  # → 41 / 31

# E2E lane:
ls tests/e2e/*.spec.ts | wc -l       # → 299 (143 reference window.api; 14 files use _electron)
```
