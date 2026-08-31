# MCode User Docs — In-Repo Mirror

This folder is a **simplified, in-repo mirror** of the published user documentation, being
adapted for MCode. Pages are condensed working copies: every step, keyboard shortcut, and
setting name is kept accurate, product naming is MCode, and there is no frontmatter. Layout
mirrors the upstream site path-for-path (`docs/user/<group>/<page>.md`).

The app's user-facing links resolve through `src/shared/docs-urls.ts`.

> TODO(rebrand): `src/shared/docs-urls.ts` currently points at the **temporary** origin
> `https://www.onorca.dev`. Once MCode's own docs site is published, replace that origin with
> the real one — every `DOCS_URL` / `CHANGELOG_URL` / `docsPageUrl()` link follows automatically.

## Contents

| Group | Pages |
| --- | --- |
| Getting started | [index](index.md) · [install](install.md) · [first-session](first-session.md) · [settings](settings.md) · [ways-to-run](ways-to-run.md) · [activity](activity.md) · [notifications](notifications.md) · [terminal](terminal.md) |
| Workspaces & model | [worktrees](model/worktrees.md) · [agents & sessions](model/agents-sessions.md) · [quick open](model/quick-open.md) · [session restore](model/session-restore.md) · [tabs, panes & splits](model/tabs-panes-splits.md) |
| Agents | [supported](agents/supported.md) · [claude-code](agents/claude-code.md) · [codex](agents/codex.md) · [codex hot-swap](agents/codex-hot-swap.md) · [cursor-cli](agents/cursor-cli.md) · [glm](agents/glm-agent.md) · [hibernation](agents/hibernation.md) · [hooks & memory](agents/hooks-memory.md) · [native chat](agents/native-chat.md) · [session history](agents/session-history.md) · [usage tracking](agents/usage-tracking.md) |
| CLI | [overview](cli/overview.md) · [reference](cli/reference.md) · [orchestration](cli/orchestration.md) · [computer use](cli/computer-use.md) · [automations](cli/automations.md) · [skills](cli/skills.md) · [worktree checkpoints](cli/worktree-checkpoints.md) |
| Browser | [overview](browser/overview.md) · [design mode](browser/design-mode.md) · [profiles](browser/profiles.md) |
| Editing | [file explorer](editing/file-explorer.md) · [markdown](editing/markdown.md) · [monaco](editing/monaco.md) · [viewers](editing/viewers.md) |
| Review | [annotate AI diff](review/annotate-ai-diff.md) · [attribution](review/attribution.md) · [commit & push](review/commit-push.md) · [diff viewer](review/diff-viewer.md) · [github](review/github.md) · [jira](review/jira.md) · [linear](review/linear.md) |
| Remote | [ssh](ssh.md) · [remote servers](remote-servers.md) · [mobile](mobile.md) · [android apk](android-apk.md) |
| Recipes | [design-mode fix](recipes/design-mode-fix.md) · [jump worktrees](recipes/jump-worktrees.md) · [parallel agents](recipes/parallel-agents.md) · [remote worktrees](recipes/remote-worktrees.md) · [review AI diff](recipes/review-ai-diff.md) |
| Troubleshooting | [troubleshooting](troubleshooting.md) · [github errors](github-errors.md) · [telemetry](telemetry.md) |
