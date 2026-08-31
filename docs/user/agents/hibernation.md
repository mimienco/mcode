# Agent hibernation

Let MCode pause idle background agent terminals and auto-resume them when you reopen the worktree.

When you keep dozens of worktrees open, idle agents add up — each one is a live PTY holding a model session in memory. Agent hibernation lets MCode quietly stop those terminals once they've been done and untouched long enough, then resume the same session the next time you open the worktree.

## What gets hibernated

MCode only hibernates an agent terminal when **all** of the following are true:

- The agent is in a **done** state — it has finished its last turn and isn't waiting on input.
- The terminal isn't in the active worktree or any worktree currently rendering a foreground terminal.
- It hasn't received keystrokes since the agent finished.
- The agent is one with a [resumable session](session-history.md): Claude, Codex, Gemini, Antigravity, OpenCode, Pi, MiMo Code, Droid, Grok, Devin, or OMP.
- It's been idle for at least the configured idle window (default 30 minutes).
- No mobile session is currently driving the terminal.
- No [orchestration](../cli/orchestration.md) Dispatch is still unsettled (`pending`, `dispatched`, or unknown). Sleep is allowed again after runtime-confirmed `completed`, `failed`, or `circuit_broken`.
- No live subagent / teammate roster remains on the pane (provider "done" alone is not enough while children are still attached).

A terminal that fails any check stays running. If a worktree has multiple agent panes, they hibernate together as a unit so a partially-paused worktree never ships.

You can also sleep worktrees manually from the sidebar (including **Sleep with Descendants** when nested children exist). See [Worktrees](../model/worktrees.md). Manual sleep keeps **finished** and **interrupted** resumable sessions so reopening the worktree can still relaunch with the same resume flags — it does not wipe those session records just because the pane was no longer "live."

## Tuning the idle window

Under the **Agent hibernation** toggle, **Hibernate after** sets how many idle minutes a finished background agent has to wait before MCode pauses it.

- Default: **30 minutes**.
- Range: **1 minute** to **24 hours**.
- The clock starts from the agent's last `done` update; any keystroke, new output, or returning to the agent's terminal tab resets it.

Longer windows trade memory savings for fewer pauses; shorter windows pause more aggressively but cost a resume next time you switch back.

## Finding sleeping worktrees

The sidebar filter menu controls whether sleeping worktrees are shown. If you hide them often, assign **Toggle Sleeping Workspaces** under [Settings → Shortcuts](../settings.md) to show or hide them without opening the filter menu.

## Resuming

When you open a hibernated worktree, MCode relaunches the agent CLI with the same resume flags it would use from [Agent Session History](session-history.md) — `claude --resume <id>`, `codex resume <id>`, and so on — so the conversation, working directory, and provider session pick up where they left off. There's nothing to click; the resume happens as part of bringing the terminal back into the foreground.

Resume also reuses the launch command, arguments, and private environment captured when MCode first opened the agent.

If the agent CLI can't resume the session for any reason (the transcript was deleted, the provider rotated the session ID), the terminal opens to a fresh prompt and the previous transcript stays available under the agent's session history.

## Limitations

Only the listed resumable agents hibernate. Cursor CLI, Hermes, Copilot, Trae, and other non-resumable terminals are left running.

## Next steps

- [Agent session history](session-history.md) — manually resume any past session, including ones MCode didn't hibernate.
- [Hot-swap Codex accounts](codex-hot-swap.md) — swap the active Codex login without restarting the session, which works alongside hibernation.
