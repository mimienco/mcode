# Original Integrity Check

## Source repo
- Path: `D:/ADE/orca`
- HEAD at snapshot: `6740f9c538120796b687ffe2b6f798b488cdaf76`
- Branch: `rebrand/mcode`
- Remote: `origin https://github.com/mimienco/mcode.git` (fetch/push)
- Stashes: 2 (see `stashes.txt`)

## Snapshot artifacts (in `docs/migration/source-snapshot/`)
- `HEAD.txt`, `branch.txt`, `remotes.txt`, `stashes.txt`
- `status-full.txt` (porcelain v1, all untracked incl. node_modules)
- `status-relevant.txt` (358 entries, node_modules/NUL excluded)
- `diff-binary.patch`, `diff-cached-binary.patch` (empty)
- `manifest.sha256` (16.751 files: 16.652 tracked source + 358 untracked source + 3 recovered-deleted + normalized; binary assets excluded by extension)
- 3 tracked files were deleted in working tree (`prof/attribute-lib-edge.cjs`, 2× `native-chat-display-pipeline*`) — hashed from `git show HEAD`.

## Backup
- `D:/ADE/orca-electron-backup-20260829-095236` — robocopy /E, exclusions: node_modules*, out, dist, playwright-report, test-results, .vite, .turbo, target, NUL. 26.958 files / 822 MB, 0 errors.
- Verification: `sha256sum -c` against full manifest (see result logged below in STATUS.md).

## Rewrite copy
- `D:/ADE/orca-tauri-rewrite-20260829-095236` — robocopy /E, same exclusions.
- All migration work happens exclusively inside this directory, on branch `rewrite/tauri-full`.

## Original protection rule
Only read-only commands are ever run in `D:/ADE/orca`. No builds, no installs, no formatting, no writes. Integrity is re-checked after every major phase and at the end (HEAD, branch, status hash compare, manifest spot-check).
