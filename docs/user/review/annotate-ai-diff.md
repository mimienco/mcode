# Annotate AI Diff

Annotate AI Diff is MCode's inline review loop for agent-generated code: leave comments on any line of any AI-generated hunk, then send them back to the agent as a single batch for revision.

## Leave a comment

1. Hover any line in the diff — a **+** appears in the gutter.
2. Click it, or press `c` with the cursor on the line.
3. Type your feedback — markdown supported.
4. Press `Cmd-Enter` to save, `Esc` to cancel.

Comments pin to the exact line; MCode tracks them across edits so they follow the line if the diff shifts.

## Send the batch

- When your review is done, click **Send to agent** at the top of the diff.
- MCode composes a single prompt with all your comments, line-anchored, then opens a **Send notes to** menu listing the worktree's available agents.
- Pick the agent that should revise the change, or start a new agent from the same menu.

**Send Review Notes to Agent** is unbound by default so it won't collide with other chords. Assign it under [Settings → Shortcuts](../settings.md) to open the send menu from the keyboard — it works even while the editor is focused.

## Why batch?

Sending comments one at a time makes the agent swing back and forth. Batching keeps feedback coherent: one round of thinking, one revision pass, a much higher hit rate.

## Reply, resolve, re-review

- Comments stay pinned after the agent revises — use them to verify the fix.
- Click **Resolve** to collapse a thread.
- Unresolved comments are part of the next batch if you hit **Send** again.

## Related

- [Diff viewer](diff-viewer.md) — the surface annotations live on.
- [Attribution](attribution.md) — which lines an agent actually wrote.
