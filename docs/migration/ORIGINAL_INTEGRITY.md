# Original Repository Integrity

## Paths

- Original: `D:\ADE\orca`
- Verified backup: `D:\ADE\orca-electron-backup-20260829-095236`
- Rewrite: `D:\ADE\orca-tauri-rewrite-20260829-095236`
- Snapshot: `docs/migration/source-snapshot`

## Captured identity

- HEAD: `6740f9c538120796b687ffe2b6f798b488cdaf76`
- Branch: `rebrand/mcode`
- Remote: `https://github.com/mimienco/mcode.git`
- Submodules: none reported by `git submodule status --recursive`.
- The source contained extensive pre-existing tracked and untracked changes; they were preserved verbatim rather than cleaned or committed.

## Verification results

| Check                                                                            | Result                                                                                                                            |
| -------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Backup manifest                                                                  | PASS: 16,748 present hashes, 3 expected deletions, 0 mismatches                                                                   |
| Rewrite manifest before migration                                                | PASS: 16,748 present hashes, 3 expected deletions, 0 mismatches                                                                   |
| Original HEAD and branch after guardrail work                                    | PASS                                                                                                                              |
| Original staged diff and porcelain status                                        | PASS                                                                                                                              |
| Original unstaged diff                                                           | PASS using the snapshot's actual `git diff` capture mode                                                                          |
| Original full source manifest after guardrail work                               | PASS: 16,751 entries                                                                                                              |
| Original full source manifest after Electron baseline                            | PASS: 16,748 present hashes, 3 expected deletions, 0 mismatches                                                                   |
| Original full source manifest after Tauri bootstrap                              | PASS: 16,748 present hashes, 3 expected deletions, 0 mismatches                                                                   |
| Backup source-file protection                                                    | PASS: all 16,748 present manifest files marked read-only; 3 expected missing                                                      |
| Original full source manifest after atomic shutdown/restart slice (`7284f6377f`) | PASS: unchanged HEAD/branch/diffs/status; 16,751 entries, 16,748 checked, 3 expected deletions, 0 mismatches                      |
| Original integrity after renderer-import inventory                               | EXTERNAL DELTA: HEAD, branch, and staged diff unchanged; unstaged diff/status changed; four manifest hash mismatches listed below |

## Concurrent original delta detected on 2026-08-30

The guard detected changes made outside the rewrite while this migration slice was running. No migration command wrote to the original. The original file timestamps and the read-only backup comparison identify:

- `AGENTS.md` — modified 14:07:23; 96 inserted lines versus the captured backup.
- `src/renderer/src/components/settings/Settings.tsx` — modified 14:18:37; 4 changed lines versus the captured backup.
- `src/renderer/src/components/settings/settings-section-registry.ts` — modified 14:20:34; 8 inserted lines versus the captured backup.
- `src/renderer/src/components/settings/settings-section-registry.test.ts` — modified 14:25:02; 24 inserted lines versus the captured backup.

The verified read-only backup remains the authoritative captured-source input for subsequent inventories and behavioral comparison. The external original delta is preserved; it is neither reverted nor silently incorporated into the rewrite.

The file named `diff-binary.patch` was captured without Git binary payload bodies: it matches `git diff`, not fresh `git diff --binary` output. The integrity checker records this capture-mode fact and relies on the independent file manifest for binary content integrity.

Run from the rewrite repository after each major phase:

```powershell
node config/scripts/verify-original-integrity.mjs --source D:\ADE\orca --snapshot docs\migration\source-snapshot --json
```

No migration command has written to the original repository.

## 2026-08-30 session checkpoint (orchestrator)

- HEAD, branch, staged diff, source manifest: unchanged (PASS).
- Working tree: user-driven development continued in the original during migration
  (28 modified tracked files + 7 new untracked test/source files, e.g. AGENTS.md
  workflow addendum dated 2026-08-30 14:07, worktree-name.ts, repo-clone-result.ts).
  HEAD commit is still 6740f9c538 — none of this is migration-caused; migration
  commands remain read-only in the original. The snapshot manifest continues to
  track the frozen baseline; live drift is expected user work, not integrity loss.
- Untracked `.dev-launch.cmd` (created 2026-08-30 13:50) is a user dev launcher.

## 2026-08-31 final session check (orchestrator)

- The user continued development in the original and committed `b430167f25`
  ("fix: rebrand user-facing docs/changelog/feedback URLs behind shared contract",
  2026-08-31 08:15, 74 files) on top of the migration baseline `6740f9c538`.
- This is user-authored work in the ORIGINAL repo, created after the migration
  snapshot froze the baseline. It is NOT migration-caused: every migration
  command against the original has been read-only, and no migration agent has
  write access there. The frozen snapshot (HEAD 6740f9c538 + manifest) remains
  the verified migration oracle; the divergence is documented, expected user
  progress, and does not affect the backup or rewrite copies.
- Branch: rebrand/mcode (unchanged). Migration integrity verdict: original
  unchanged by migration activity itself; forward drift = user commits.

## 2026-08-31 session-final check (orchestrator, wave 4)

- Snapshot dir was found missing from D:/ADE/_migration-snapshot (external
  cleanup between sessions); restored verbatim from the committed copy at
  docs/migration/source-snapshot and re-verified against it.
- The user continued development in the original: 4 more commits since the
  migration baseline (b430167f25..65112549da — Replay guided tours, rebrand
  contract, palette feedback), plus live working-tree changes. HEAD moved
  6740f9c538 -> 65112549da; branch rebrand/mcode unchanged.
- Verdict: identical in kind to the previously documented drift — user work,
  not migration activity. Every migration command against the original is
  read-only. The frozen snapshot remains the migration oracle; forward
  divergence is expected and recorded.
