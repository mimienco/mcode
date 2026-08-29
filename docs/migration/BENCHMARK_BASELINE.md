# Electron Benchmark Baseline

## Host

- Date: 2026-08-29
- OS: Windows host in Europe/Zurich timezone
- Node: 22.16.0 (repository engine requires Node 24; all results carry this environment warning)
- pnpm: 10.24.0 via the repository's pinned `packageManager` field (the globally reported pnpm is 11.19.0)
- Source HEAD: `6740f9c538120796b687ffe2b6f798b488cdaf76`
- Baseline copy: `D:\ADE\orca-electron-backup-20260829-095236`

## Status

Dependencies were restored in the backup with `corepack pnpm install --frozen-lockfile` in 13m02s. The install created only documented regenerable artifacts; every source-manifest hash still matched afterward and no new source/status entry appeared.

The baseline is partially red. These results are constraints for the migration, not failures to conceal:

- Targeted release/runtime tests: 4 files passed; 58 tests passed and 4 were conditionally skipped.
- Full parallel typecheck: failed in node, CLI, and web projects on pre-existing source errors.
- Production unpack build: stopped at the same typecheck gate, so installer size/startup/idle RAM are not yet measurable from a production artifact.
- Direct Electron/Vite bundle: passed; 3,151 main modules and 10,642 renderer modules, with renderer compilation taking 1m12s on the first production run.
- Terminal performance E2E: 9 passed, 4 failed, 1 skipped, 7 did not run in 9.1 minutes.

Terminal failures:

1. ACK-backpressured background PTYs produced 389.3ms maximum timer drift against a strict `<250ms` budget.
2. Background-tab shared-drain scenario timed out after 120s while creating the shared page.
3. Hidden overflow did not restore its marker from main-owned terminal state within 10s.
4. Typing-latency setup lost its Electron execution context during navigation.

Passing terminal coverage included foreground line rewrites, active redraw bursts, one-terminal and multi-pane responsiveness, plain/rich hidden TUI restoration, hidden side effects, and the visible bulk-output high-priority drain. Failure screenshots, error contexts, and traces remain under the backup's `test-results` directory.

## Measurements to capture on the same machine

| Metric | Electron | Tauri | Gate |
| --- | ---: | ---: | --- |
| Cold and warm startup to interactive | blocked by existing production typecheck | pending | Tauri no slower without documented cause |
| Idle private working set / total process tree | pending | pending | target materially lower |
| Installer/package size | pending | pending | target materially smaller |
| PTY input latency and sustained throughput | suite partially red; 389.3ms drift observed | pending | no perceptible latency regression; no loss |
| 10+ PTY memory growth and output integrity | current suite partially exercised | pending | bounded, no loss |
| Resize storm and hidden-pane restore | hidden restore partly green, overflow restore red | pending | current reliability or better |
| Worktree create/status/switch | pending | pending | current reliability or better |
| Shutdown under active sessions | pending | pending | no orphaned descendants |

Raw reports, exact commands, iteration counts, machine load notes, and failures will be retained. Budgets will not be changed to manufacture a pass.
