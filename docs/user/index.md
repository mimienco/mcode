<!-- TODO(rebrand): simplified mirror of upstream docs, being adapted for MCode. Source: https://www.onorca.dev/docs -->

# What is MCode?

MCode is a desktop IDE for running multiple AI coding agents side by side.

Every task gets its own git worktree, its own agent terminal, and its own browser tab — so you can fan out work across Claude Code, Codex, Cursor CLI, and friends without stashing, branch-juggling, or losing flow.

## When to use MCode

- You want three agents trying the same bug in parallel and to pick the winner.
- You want to review AI-generated diffs seriously before you ship them.
- You already pay for Claude Code, Codex, or Cursor CLI and want one place to orchestrate them.
- You want agents to run remotely — over SSH, on a self-hosted MCode server, or in an on-demand VM — without giving up your IDE.

## Who it's for

Designed for people who already write code for a living and want to use AI as leverage — not as a replacement. It assumes you read diffs, care about commits, and keep a worktree tidy.

## How it works at a high level

- MCode is not a model: it runs agent CLIs you already use — bring your own Claude, Codex, or OpenCode subscription.
- Every worktree is a real git worktree; you can `cd` in and use plain git whenever you want.
- MCode runs on your desktop by default. Remote modes use machines and cloud accounts you control — [SSH targets](ssh.md), [self-hosted MCode servers](remote-servers.md), or [Cloud VMs / per-workspace environments](ways-to-run.md#4-cloud-vms-per-workspace-environments).

## Where to start

- [Install](install.md) — download and first launch.
- [Your first 3-agent session](first-session.md) — the core workflow end to end.
- [Worktrees](model/worktrees.md) — the unit of everything in MCode.
- [Agents & sessions](model/agents-sessions.md) — how agent state is tracked.
- [Settings reference](settings.md) — every pane, searchable.
- [Ways to run MCode](ways-to-run.md) — local, SSH, remote servers, cloud VMs.
