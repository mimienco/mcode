# Agent hooks & memory

MCode plays nicely with the agent hook and memory conventions Claude Code and Codex already use — it reads them, respects them, and gives you a UI for the ones that make sense in an IDE context.

## Per-repo hooks

MCode reads each repo's `.claude/` and `.codex/` configuration. Hooks you already have will run when MCode launches the agent in a worktree of that repo.

## Worktree setup hooks

Configure commands to run automatically after a worktree is created — e.g. `pnpm install`, `direnv allow`, or a script that restores `.env` files. Set them under [Settings → Repository → Hooks](../settings.md).

## Memory files

Claude's `CLAUDE.md` and Codex's `AGENTS.md` (at repo root or nested) are left alone — they belong to the agent. MCode surfaces them in the file explorer like any other file so you can edit them inline.

## Agent status hooks

**Settings → Agents → Agent status hooks** controls the MCode-managed hooks that report working / waiting / done into the UI. Turning the setting off removes those managed hooks and stops reinstalling them; turning it back on restores them **without restarting MCode** (on Windows, the WSL hook relay follows the same live gate). CLI equivalents: `mcode agent hooks status|on|off --json`.

## Surviving a restart

Hook endpoints are written to disk (`{userData}/agent-hooks/endpoint.env` on POSIX, `endpoint.cmd` on Windows) and re-sourced on every hook invocation, so long-lived agent sessions keep reaching the live MCode server even after an app restart — no more dead-port POSTs from a PTY that outlived the previous session.
