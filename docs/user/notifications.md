# Notifications & Inbox

How MCode pings you when an agent actually finishes, and how to tune the pings.

MCode runs agents, not just terminals, so it knows when an agent actually finishes versus when it's just paused. Notifications lean on that signal so you can queue three agents, walk away, and come back when the first one is done.

## Agent-finished pings

When an agent transitions from working to idle, MCode fires a notification — system notification, sound, and a chip on the worktree.

## Persistent bell

The header bell shows unread notifications across all worktrees. Clicking a notification jumps to the matching worktree and pane.

On macOS, the same unread count is mirrored as a badge on the Dock icon, so you can spot pending agent pings without bringing MCode to the foreground.

## Mark unread

Right-click a notification to mark unread — useful when you've triaged something but want to come back to it later.

## Tuning

Turn off specific categories (system, sound, chip-only) under [Settings → Notifications](settings.md).

## Custom sounds

Pick a custom desktop notification sound per category under [Settings → Notifications](settings.md) — point at any audio file on disk, or pick from the built-in set. Useful when you want agent-finished pings to stand out from system mail and chat apps.

Supported formats: MP3, WAV, OGG, M4A, AAC, FLAC. One file applies to all delivered desktop notifications.

When you use a custom sound, set its playback volume from the same settings pane.

## Related

- [Agents feed](activity.md) — the chronological catch-up surface across every worktree.
- [Agents & sessions](model/agents-sessions.md) — how working/idle/blocked transitions are detected.
