# Diff viewer

MCode's diff viewer is built for serious review of AI-generated code, not a quick glance. Every worktree has a built-in diff against its start-from ref.

## Features

- **Combined diff** across all staged, unstaged, and untracked files.
- **Line numbers** for both sides, toggleable.
- **Image diffs** — side-by-side, swipe, and onion-skin modes for binary images.
- **HTML preview** — in **View all** / combined diffs, HTML sections that still exist in the working tree show **Open Preview to the Side** (eye) next to the always-visible open-file control. Preview opens the working-tree HTML in a side browser split. Deleted HTML and commit-only combined surfaces skip the eye. See [HTML viewers](../editing/viewers.md#html).
- **Merge-conflict UI** with three-way view and inline resolution.
- **Staging by hunk or line** — same as `git add -p` but visual.

## Scoping

Shows changes against the worktree's start-from ref by default. Switch to comparing against any commit, branch, or the base ref from the diff toolbar.

## Word wrap

- Long lines wrap in place when diff word wrap is on, so wide diffs read top-to-bottom without horizontal scrolling. Off by default.
- Toggle **Word Wrap** from the **⋯** actions menu in the diff editor header, or set the global default under **Settings → General → Diff Word Wrap**.
- Both controls share the same setting — toggling from the editor flips it everywhere.

## File tree

- Combined diffs can show a collapsible file tree beside the hunks.
- Drag the tree's resize handle (or use arrow keys on the handle; Shift multiplies the step) to set width — the size is remembered across sessions.
- Collapse or show the tree from the combined-diff toolbar.

## Keyboard shortcuts

- `j` / `k` — next / previous changed file.
- `n` / `p` — next / previous hunk.
- `F7` / `Shift+F7` — next / previous change in the active editor.
- `s` — stage the hunk under the cursor.
- `c` — start a comment ([Annotate AI Diff](annotate-ai-diff.md)).

## Related

- [Attribution](attribution.md) — which lines an agent wrote.
- [Commit & push](commit-push.md) — stage, commit, and push from the same panel.
