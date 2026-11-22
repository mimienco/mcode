# MCode (orca) dev cold-start performance review — shared brief

Read this first. It is the shared ground truth for a 20-agent performance review. Your own task spec names your slice.

## Symptom timeline (measured 2026-08-28, Windows 10, repo D:/ADE/orca)

- `dev` script = `ensure:electron-runtime` (2.4s) → `node config/scripts/run-electron-vite-dev.mjs --fresh-profile`
- Log: "[mcode-dev] Building web client for pairing..." → rolldown-vite **production build** of the web client: ~6,700 modules, **1m48s**, output `out/web/`. Biggest chunks: monaco `editor.api2` 3.6MB, `web-index` 1.75MB, locales `ja` 752KB / `ko` 660KB / `en` 634KB / `zh` 554KB, `App` 878KB, `TerminalPane` 666KB, `editor.api2`+`web-index` flagged >500KB. Transform log shows it pulling the ENTIRE renderer app graph (TaskPage, TerminalPane, editor, mermaid, cytoscape, tiptap, emoji-picker...).
- electron-vite dev builds: **main 17.42s** → `out/main/index.js` is a **10.76MB** single chunk (plus locale chunks zh 605KB/ko 712KB/ja 803KB), **preload 398ms**.
- Renderer dev server on localhost:5173, then "starting electron app...".
- Electron main process: ~40-60s after spawn before CDP port 9513 accepted connections; workingset 25MB → 238MB. Suggests heavy synchronous main-thread init (module parse + boot work).
- **FIRST PAINT: white screen.** `#root` EMPTY (0 children), body empty, readyState interactive, title "MCode". ZERO console errors/exceptions (only the standard Electron "no CSP" warning). Persisted until a manual CDP `Page.reload` — after reload the app mounted instantly.
- Then the main pane showed a "Loading…" spinner for **~3 minutes** (user screenshot; sidebar/shell rendered around it). Later resolved on its own.
- CDP probe after load: 250 resources, **0 pending network** → both stalls are NOT network; they are main-thread / IPC / state gated.
- Daemon sidecar runs alongside: `electron.exe out/main/daemon-entry.js --socket \\?\pipe\mcode-terminal-host-v36-... --token <temp>\daemon-v36.token --pid-record ... --launch-nonce ...`.
- Fresh disposable profile EVERY dev run (`--fresh-profile`): `%TEMP%\mcode-dev-S0Ch64`. Wrapper logs: "Prepared wrapper in D:\ADE\orca\out\bin", "Instance: orca @ rebrand/mcode", "Remote debugging on http://127.0.0.1:9513".
- Opened project dir is `D:/ADE` (parent of the repo). It contains ~30 junk entries: 12+ `test-results-core-flow-*` dirs, `node_modules.corrupt-*`, `node_modules.junction-recovery-*`, `_tmp_*` files, a file literally named `NUL`, plus normal dirs (mobile/, tools/, tests/, prof/, config/, docs/, resources/, skill-stubs/, skill-guides/).

## Repo layout

- electron-vite app. Config: `electron.vite.config.ts` (main/preload/renderer), `vite.web.config.ts` (web pairing client), orchestrators in `config/scripts/*.mjs`, playwright e2e in `tests/`, docs in `docs/`, `DEVELOPING.md`, `AGENTS.md`, `DESIGN.md`.
- Code: `src/main` (electron main incl. daemon-entry), `src/preload`, `src/renderer` (only html entries + `src/renderer/src/` app code), `src/shared` (imported as `../shared` from renderer), `src/relay`, `src/cli`, `src/types`.
- Runtime: Windows 10, node 22.16, electron 43.1.0, rolldown-vite 7.3.1. pnpm 10.24 pinned in package.json but **pnpm is NOT installed — never try to run pnpm or install anything**.

## Rules for every agent

- READ-ONLY. Use read/grep/glob only. No edits, no builds, no tests, no installs, no process spawning. bash only for a trivial count/fact if unavoidable.
- Repo is large: **scope greps to specific subdirs** (full-tree greps time out at 30s).
- Ground every claim as file:line. Label speculation `[INFERENCE]`. Label numbers measured vs estimated.
- For each finding state whether it hurts DEV ONLY, PROD, or BOTH.
- Return a markdown report ≤600 words with exactly three sections:
  - `## Findings` — each: evidence (file:line), what it costs, impact (high/med/low), confidence (high/med/low), dev/prod/both
  - `## Root causes` — ranked
  - `## Improvements` — each: concrete action → expected win, effort S/M/L, risk, files to change. No generic advice ("add caching" is not enough — name the file and mechanism).
