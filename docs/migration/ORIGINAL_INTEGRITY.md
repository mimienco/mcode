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

| Check | Result |
| --- | --- |
| Backup manifest | PASS: 16,748 present hashes, 3 expected deletions, 0 mismatches |
| Rewrite manifest before migration | PASS: 16,748 present hashes, 3 expected deletions, 0 mismatches |
| Original HEAD and branch after guardrail work | PASS |
| Original staged diff and porcelain status | PASS |
| Original unstaged diff | PASS using the snapshot's actual `git diff` capture mode |
| Original full source manifest after guardrail work | PASS: 16,751 entries |
| Original full source manifest after Electron baseline | PASS: 16,748 present hashes, 3 expected deletions, 0 mismatches |
| Original full source manifest after Tauri bootstrap | PASS: 16,748 present hashes, 3 expected deletions, 0 mismatches |
| Backup source-file protection | PASS: all 16,748 present manifest files marked read-only; 3 expected missing |

The file named `diff-binary.patch` was captured without Git binary payload bodies: it matches `git diff`, not fresh `git diff --binary` output. The integrity checker records this capture-mode fact and relies on the independent file manifest for binary content integrity.

Run from the rewrite repository after each major phase:

```powershell
node config/scripts/verify-original-integrity.mjs --source D:\ADE\orca --snapshot docs\migration\source-snapshot --json
```

No migration command has written to the original repository.
