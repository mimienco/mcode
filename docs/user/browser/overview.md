# Per-worktree browser

Every MCode worktree has its own browser: a real Chromium window — address bar, history, devtools — embedded in a pane, with tabs scoped to the worktree.

## Controls

- Address bar with history and fuzzy URL completion. Non-URL text searches with your [Default Search Engine](../settings.md) — the same engine the [new-tab omnibox](../model/quick-open.md#new-tab-omnibox) uses. Prefix a query with `?` in the **+** field to force search.
- Back / forward / reload / stop. Hover the reload control for the normal-reload shortcut; right-click (or long-press) for **Reload** and **Hard Reload** with their shortcuts — hard reload bypasses the cache when iterating on local frontend assets.
- `Cmd-F` — find in page.
- `Cmd-T` — new tab, scoped to this worktree.
- `Cmd-Shift-T` — reopen last closed tab.
- Page links with `target=_blank` and ordinary unnamed popups open in new MCode tabs using the opener tab's browser profile. Named or OAuth-style popups can still open in a separate window.

## Worktree scoping

Tabs are filtered per worktree. Switching worktrees restores that worktree's browser tabs and scroll positions. One click imports cookies from Chrome or Edge; Google logins are not imported — sign in to Google directly in MCode.

## Remote workspaces

For a workspace on a paired [Remote MCode Server](../remote-servers.md), new browser pages render on this desktop by default while HTTP(S), WebSocket, DNS, and loopback traffic still go through the remote host. Input, selection, and popups stay native to this device without changing the page's network identity. The host indicator in the browser toolbar shows where traffic is going.

Under [Settings → Browser → Remote server workspaces](../settings.md), choose **This device** or **Server (streamed)**. The choice applies to new pages only; it does not move a page that is already open. If a locally rendered remote page cannot start, **Reopen on server** opens a new server-hosted page at its last address (signed-in or form state may differ).

For SSH workspaces, **Browse through SSH workspace hosts** controls whether browser traffic and DNS go through the workspace's SSH host or this device. Downloads from a client-hosted remote page stay on the remote workspace host, and uploads are staged there before the page receives them.

## Link routing

Under [Settings → Browser → Link Routing](../settings.md), choose whether http(s) links from the terminal, markdown, and editor open in MCode's per-worktree browser or the system browser.

A nested **Hold Shift…** toggle inverts that default for one click with the platform modifier (`⇧⌘-click` on macOS, `Shift+Ctrl+click` on Windows / Linux):

- When links open in **MCode**, the modifier sends one link to the system browser.
- When links open in the **system browser**, turn the toggle on so the modifier opens one link in MCode's built-in browser instead (plain click stays system).

A capability-verified paired runtime can offer the same MCode/system-browser choice for its terminal links. SSH-owned terminal links and links from older runtimes remain system-browser only. See [Terminal → Link actions](../terminal.md#link-actions).

## Downloads

Browser downloads appear in a shelf under the toolbar while active or recently completed, with actions to cancel an in-progress download, open a finished file, show it in its folder, or dismiss the row.

## Share as artifact

Local HTML files opened in the worktree browser can use **Share as artifact** in the toolbar to mint a public view link through your signed-in MCode account (same opt-in gate and management surface as Markdown). Relative HTML assets are not uploaded — share a self-contained file or use absolute asset URLs. See [Settings → Artifacts](../settings.md#artifacts) and [CLI reference → Artifacts](../cli/reference.md#artifacts).

## Viewport-size emulation

Set a custom viewport size on a browser tab to test responsive layouts without resizing the whole pane. MCode uses Chrome DevTools Protocol device emulation under the hood, so the page sees the emulated dimensions in `window.innerWidth` and media queries.

## Automation

The browser is also scriptable by agents via the [MCode CLI](../cli/overview.md) — `mcode snapshot`, `mcode click`, `mcode fill`, and so on. Same browser, same tabs.

## Next steps

- [Design Mode](design-mode.md) — turn the browser into a pointer-to-code feedback loop.
- [Browser-use profiles](profiles.md) — run the browser with a specific login, cookie jar, or user agent.
