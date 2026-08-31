# File explorer & external drag-drop

The file explorer on the left of each worktree mirrors your on-disk files in real time, so external changes show up instantly.

## Ordering and live tracking

- Create, rename, delete, and move all map to real filesystem operations.
- Listings sort directories first, then files, in **natural (numeric-aware) name order** — `9`, `99`, `100`, not lexicographic `100`, `99`, `10`.
- The same ordering applies to SSH, remote runtime, Source Control tree nodes, folder pickers, and the mobile file tree.

## External drag-drop

- Drop files from Finder/Explorer into the file tree to copy them in.
- Drop an image into a markdown editor to insert it at the cursor.
- Drop files onto an agent terminal to paste their paths at the prompt.
- On [SSH worktrees](../ssh.md), drag-drop uploads the file to the remote host before completing the drop, so the agent sees a real on-disk path.

## Git status

- Files are colored by git status: untracked, modified, staged, ignored.
- Right-click for discard, stage, rename, **Copy Path**, and **Copy Relative Path** (`Cmd+Option+Shift+C` / `Ctrl+Alt+Shift+C` by default; remappable).
- Right-click a single file and choose **Copy** to place the file itself on the OS clipboard. On SSH worktrees MCode stages the remote file locally first, then writes that staged file reference to the clipboard; remote folders are excluded.

## Download (SSH / remote)

- Right-click a remote file → **Download**, or a remote folder → **Download Folder** when the connection supports recursive transfer.
- MCode opens a native save/folder dialog.
- Desktop app only — not available in the web client. Local worktrees do not show this action.

## Search a folder

- Right-click a folder and choose **Find in Folder** to open Search already scoped to that folder.
- Or select a folder in the file explorer and press `Cmd-Shift-F` (macOS) / `Ctrl-Shift-F` (Windows and Linux).

## Related

- [Monaco editor & autosave](monaco.md) — edit files after you find the match you need.
- [Diff viewer](../review/diff-viewer.md) — review the changes your agent or editor made.
