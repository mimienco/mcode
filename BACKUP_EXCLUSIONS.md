# Backup Exclusions

The verified backup and rewrite copy were created from `D:\ADE\orca` at snapshot `20260829-095236`.

Only regenerable or non-source artifacts were excluded from the copy operation:

- `node_modules` and nested dependency installations
- `out`, `dist`, and `build`
- `playwright-report` and `test-results`
- `.vite` and `.turbo`
- Rust `target` directories
- the Windows-reserved `NUL` path

Hidden files, `.git`, tracked files, untracked source files, local scripts, patches, and repository configuration were retained. Both copies were checked against `docs/migration/source-snapshot/manifest.sha256`: 16,748 present files matched, three tracked deletions remained absent as expected, and no hash mismatch or unexpected missing file was found.

Controlled Electron baseline execution is complete. All 16,748 present files represented by the source manifest are now marked read-only in the backup; the three expected tracked deletions remain absent. Test-created dependency caches, build outputs, and test artifacts are regenerable and deliberately remain outside this source-file protection so the captured evidence can be inspected or rerun without modifying the preserved source snapshot.
