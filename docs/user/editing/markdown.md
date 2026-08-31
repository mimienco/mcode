# Rich markdown editor

Markdown files open in a rich editor by default — slash menu, toolbar, inline previews, internal link autocomplete.

Toggle to raw Monaco whenever you want with `Cmd-Shift-M`.

## Large files

- For files over 300 KB MCode opens the raw editor first to keep typing responsive.
- Choose **Open anyway** in the fallback banner to force the rich editor for that file; the choice lasts until you close its tab.
- Files using Markdown constructs the rich editor cannot safely preserve stay in the raw editor.

## Slash menu

Type `/` on an empty line for headings, lists, code blocks, callouts, images, mermaid diagrams, and toggle blocks.

- `/toggle-text` for collapsible notes; `/toggle-h1` … `/toggle-h5` when the toggle summary should read like a heading level.
- Nested toggles edit in place.
- Toggles save as portable `<details>` / `<summary>` markdown, so the file still previews outside MCode.

## Internal links

Type `[[` to start a wiki-style link. MCode autocompletes file paths within the worktree and inserts a relative link.

## Search

In-editor markdown search respects rendered text, not raw markdown — searching "Install" finds a heading whether written as `# Install` or `<h1>Install</h1>`.

## Review annotations

- Select rendered text to add an annotation without switching back to raw markdown.
- The note stays tied to the selected source range, so the comment shows while you keep editing the rendered document.
- **Add Review Note** is a remappable shortcut (`Cmd+Shift+A` / `Ctrl+Shift+A` by default) under [Settings → Shortcuts](../settings.md).

## Front matter

- YAML and TOML front matter is shown in the rich editor and rendered preview by default.
- Open the editor's **More actions** (`…`) menu and choose **Hide front matter** for a body-only view; the same menu toggles back to **Show front matter**.
- The toggle is per-file.

## Tables

| Key | Behavior |
| --- | --- |
| **Tab** / **Shift-Tab** | Next / previous cell; Tab past the last cell inserts a row |
| **Enter** | Move to the cell below; on the last row, add a row |
| **Backspace** on a fully empty row | Delete the row (or the whole table if it is the last row) |
| **Backspace** in an empty cell when the row still has content | Step to the previous cell |

- **Shift-Tab** unindents a list item or the selected lines of a code block.
- With the caret in a table, the toolbar shows one-click **row and column insert/delete** controls.
- Right-click a cell for the same structure commands in the context menu (the Table submenu only appears when the target is a table cell).

## Table of contents

- Click the **Table of Contents** button (tree icon) in the editor header for a heading outline pinned to the left of the editor.
- Clicking a heading jumps to it; collapse/expand controls keep deeply nested sections readable.
- Available in rich and preview modes.

## Share as artifact

- From the editor header, **Share as artifact** publishes the open Markdown file as a public view link through your signed-in MCode account.
- Requires **Settings → Artifacts → Allow publishing public artifact links** (off by default).
- Republish updates the same link while MCode still has the mapping; manage or delete links from the sidebar **Artifacts** page.
- Agents and scripts can use `mcode artifacts share|update|list|delete` — see [CLI reference → Artifacts](../cli/reference.md#artifacts).
