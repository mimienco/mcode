# Agents feed

A threaded feed of agent completions, blocking states, response previews, and worktree creation events across every worktree.

The **Agents** entry in the sidebar opens a threaded feed of agent events across every worktree — completions, blocking questions, unread state, and worktree creation. It complements the per-worktree [notifications](notifications.md) view by giving you one chronological surface to triage from.

## Opening and filtering

The entry is on by default. It shows an unread badge when new events have arrived since you last visited; clicking an entry jumps you to the matching worktree and pane — the same handoff MCode does from a system notification.

Use `Cmd+F` on macOS or `Ctrl+F` on Windows and Linux to focus the feed filter. When the embedded terminal has focus, the same shortcut stays with the terminal instead.

## What shows up

Each entry in the feed is one of:

- An agent finishing a turn, either idle or blocked on a question.
- A new worktree being created.
- An agent that's been waiting on input long enough to surface as blocking.
- A short preview of the agent's most recent response, so you can skim the feed without opening every thread.

Threads are grouped by status — running agents stay pinned at the top so you can spot the ones still working at a glance.

## Interplay with notifications

The Agents feed doesn't replace the [header bell](notifications.md) or system notifications — those still fire on agent-finished pings. The feed is the catch-up surface when you've been away: instead of scrolling each worktree to find what changed, scan it.

## Related

- [Notifications & Inbox](notifications.md) — system pings, the header bell, and per-category tuning.
- [Agents & sessions](model/agents-sessions.md) — how MCode tracks the working/idle/blocked transitions this feed surfaces.
