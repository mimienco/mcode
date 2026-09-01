# E2E Replacement Plan (Electron Playwright → Tauri)

Repo: `D:/ADE/orca-tauri-rewrite-20260829-095236`, branch `rewrite/tauri-full`.
Companion to `CUTOVER_INVENTORY.md` (§4 config cleanup) and `BUNDLE_PARITY.md`.
**No configs are changed by this document** — it is the migration plan; edits land per-step
below once the lane is proven.

## 1. Current lane (Electron)

`tests/playwright.config.ts` (@stablyai/playwright-test wrapper):

- `testDir: ./e2e`, `globalSetup` builds the Electron app + seeds a scratch git repo,
  `globalTeardown` cleans up.
- 2 projects: `electron-headless` (all specs minus `@headful`) and `electron-headful`
  (`@headful` grep only — 34 spec files tagged).
- Tests start the app via `_electron.launch()`; **83 spec files** reference `electronApp`/`_electron`
  directly; **299 spec files** total; **119** touch `window.api` (accesses the preload bridge from
  the page); **264** use `page.evaluate` (renderer-side probes, most are lane-agnostic).
- Per-test userData isolation; `workers: CI ? 1 : undefined`; 120 s per-test budget
  (cold start dominates); trace/screenshot on failure.

## 2. Spec classification (counted signals)

| Signal | Files | Meaning for migration |
| --- | ---: | --- |
| `electronApp`/`_electron` direct use | 83 | Launch/plumbing code must swap to the new lane's app-starter; assertion bodies often survive unchanged |
| `window.api` access from tests | 119 | Under Tauri these reads must go through the real bridge (invoke) or the shim — tests stay valid if they assert behavior, not the bridge object |
| `page.evaluate` probes | 264 | Lane-agnostic; only the page-url/bootstrap changes |
| `@headful` tagged | 34 | Need a visible window under the new lane too (WebView2 headful works; CI needs interactive sessions or drop to smoke-marker equivalents) |
| remaining (~180) | ~180 | Pure renderer assertions → move first, nearly verbatim |

Extrapolated split: **~60% renderer-only** (move first), **~30% IPC-coupled** (move per-parity-row;
the assertion target becomes the Rust command's observable effect), **~10% shell behavior**
(tray/window/updater → Rust contract tests + manual smokes, not browser E2E).

## 3. Recommended lane: tauri-driver (WebDriver) for E2E, keep marker-smokes for the shell

Decision: **tauri-driver + WebDriver client**, reusing Playwright only as the test runner where
practical (`playwright` can drive a raw CDP/WebDriver endpoint, but its `_electron` helper is
Electron-only). Concretely:

- **Primary:** `tauri-driver` (ships with the Tauri CLI) exposes WebDriver for the WebView2
  app; specs connect via a WebDriver client (`selenium-webdriver` or `webdriverio`) instead of
  `_electron.launch`. CI-feasible on Windows runners: WebView2 headful works on VMs with
  `--headless`-equivalent disabled but remote-debugging enabled; tauri-driver needs the
  `WebView2-runtime` present on the runner (pre-installed on `windows-latest`).
- **Why not CDP-attach:** WebView2 supports `--remote-debugging-port`, and the repo already has a
  CDP harness; but tauri-driver gives window-management + WebView2 capability parity without
  maintaining a custom relay, and it is the Tauri-blessed path that survives runtime upgrades.
- **Shell behaviors that WebDriver cannot see** (tray icon state, native menu contents, updater
  feed side effects) stay covered by the existing marker-file smoke harness
  (`MCODE_TAURI_SMOKE_*` in `src-tauri/src/lib.rs`) plus Rust contract tests — this is the
  ~10% shell bucket.
- **Auth/token setup:** the app's test mode already exists (smoke modes + seeded repo in
  globalSetup); port `global-setup.ts` to (1) build via `tauri build --debug` (or `tauri dev`
  for fast iteration), (2) seed the same scratch repo, (3) start `tauri-driver`, (4) point the
  WebDriver client at it.

### Per-test bootstrap diff

| Electron today | Tauri lane |
| --- | --- |
| `_electron.launch({ args: [userDataDir, ...] })` | spawn `tauri-driver` → `webdriverio.remote({ capabilities: { 'ms:webview2': … } })` → attach; app arguments passed via `tauri.conf.json > app > windows` env or CLI arg parsed in `lib.rs` (smoke-mode plumbing already reads env vars) |
| `electronApp.evaluate(...)` for main-process asserts | gone: main-process asserts become Rust contract tests, or marker-file checks against the smoke harness |
| `window.api` page reads | real invoke calls through the app's own bridge (works because the shim serves the surface) |
| `userDataDir` isolation per test | per-test `MCODE_USER_DATA_DIR`-style env (already how smoke modes isolate) |

## 4. Migration order

1. **Lane bootstrap (S/M):** `tauri-driver` + webdriverio harness; run 3 renderer-only specs
   (candidates: `app-navigation-latency.spec.ts`, `expand-collapse*`-adjacent E2E,
   `edge-wrapped-terminal-http-links.spec.ts`) to prove build→launch→assert→teardown.
2. **Renderer-only bulk (~180 specs):** move project-by-project; keep the spec bodies,
   replace the launch helper via a shared `tauri-e2e-harness.ts` (mirrors `tests/e2e/helpers/*`
   pattern). Delete the electron project entries for each moved batch.
3. **IPC-coupled specs (~90):** gate each on its parity row; assertion target switches from
   "main process state" to "command effect observable in the renderer". Order follows
   CUTOVER_INVENTORY §8 steps 4–11 (window → ipc families → ssh → speech → updater).
4. **Shell specs (~30):** retire to Rust contract tests + marker smokes; delete the spec.
5. **Final config cleanup** (after 1–4): delete `electron-headless`/`electron-headful` projects,
   the `_electron` helper files, and rewrite `test:e2e*` scripts to the new lane
   (CUTOVER_INVENTORY §4 row for scripts).

## 5. Config cleanup checklist (execute after the lane is proven)

- [ ] `tests/playwright.config.ts`: remove `electron-headless`/`electron-headful` projects;
      add the tauri project (or a `wdio.conf.ts` if webdriverio wins).
- [ ] `package.json`: rewrite the 11 `test:e2e*` scripts; delete
      `ensure:electron-runtime`/`build:electron-vite*`/`rebuild:electron` when CUTOVER steps 2–3 land.
- [ ] `tests/e2e/global-setup.ts`: swap Electron build for `tauri build --debug` + `tauri-driver`
      startup; keep the seeded-repo logic verbatim.
- [ ] `tauri.conf.json`: no schema change needed for tauri-driver (WebDriver is capability-side);
      verify `build > beforeDevCommand/beforeBuildCommand` produce the renderer the specs expect
      (config/vite.tauri.config.ts already wired).
- [ ] `tests/e2e/helpers/*` (95 files): split into lane-agnostic (keep) vs `_electron`-coupled
      (14 files — rewrite against the harness).
- [ ] CI: Windows runner needs WebView2 runtime (preinstalled on hosted runners) + display for
      headful specs (Xvfb equivalent: use `@headful` smoke-markers instead on CI).

## 6. Risks

- **WebView2 + WebDriver quirks:** some `page.evaluate` probes hitting preload-only globals behave
  differently; the shim must be complete for a spec's namespace before its spec moves
  (77-namespace surface — track per-namespace in the shim).
- **Headful CI:** the 34 `@headful` specs need a real display; on CI prefer converting to
  smoke-marker equivalents rather than standing up interactive runners.
- **Per-test isolation:** Electron's fresh-userData-per-test pattern must map to per-test env +
  app-data dir; verify the Tauri app honors the env (smoke modes prove the plumbing exists).
- **Timing budgets:** Tauri cold-start ≈ Electron's; keep the 120 s budget and CI workers=1
  until measured.
- **@stablyai/playwright-test wrapper:** the fork may pin Electron-specific APIs; if its
  `_electron` helper is the only coupling, a plain `@playwright/test` + WebDriver combo is the
  fallback (verify the wrapper's extra features actually used before deciding).
