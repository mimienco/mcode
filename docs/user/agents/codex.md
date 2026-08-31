# Codex in MCode

Codex is OpenAI's agentic CLI. MCode's Codex integration is one of the deepest in the app — usage, hot-swap, and restart all preserve account identity.

## Setup

1. Install Codex per OpenAI's docs.
2. Log in from any terminal.
3. MCode reads `~/.codex` for accounts and credentials.

## Launching

Pick **Codex** from the agent combobox. MCode launches `codex` with the worktree as `cwd` and routes auth through the currently selected account.

## Account hot-swap

Many Codex users run multiple accounts to stretch rate limits. MCode's account switcher swaps the active account without re-login or config edits. See [Hot-swap Codex accounts](codex-hot-swap.md).

## System default vs extra accounts

**System default** uses your real `~/.codex` login (the same home a bare `codex` outside MCode would use). Extra accounts MCode manages get their own home under MCode's account data so credentials and rollouts stay isolated. New Codex launches follow the active account; already-running sessions keep the home they started with until you restart them.

## Nested Task subagents

When Codex spawns Task subagents, MCode can show them as child rows under the parent agent on the worktree agent list and [Agent Dashboard](../model/agents-sessions.md#agent-dashboard). Expand the chevron to see each child; click a child to focus the parent terminal (subagents do not own a separate pane).

## Continue in a new session

From an agent terminal's header or context menu, choose **Continue in New Session…**. MCode starts a fresh agent session (same or different CLI) and injects a bounded handoff prompt from the prior transcript or captured context. The original session is left alone — this is not `codex resume`.

## Restart chip

When Codex exits, the restart chip relaunches the agent with the same account. If you swapped accounts mid-session and want to restart with the new one, use the account switcher first, then restart.

## Usage & rate limits

MCode reads local Codex usage state for the active account and surfaces it in the status bar. See [Usage & rate-limit tracking](usage-tracking.md).

## Codex on Windows (WSL)

On Windows, MCode can run Codex either from your host install or from a WSL distro. Add a WSL-hosted Codex account from the account switcher — MCode creates an isolated account home inside the distro (under `~/.local/share/mcode/codex-accounts/<id>/home`), maps it back to the host as a `\\wsl.localhost\<distro>\...` path for auth reads, and routes launches, hot-swap, and rate-limit fetches through the selected distro. If Codex isn't installed in the target distro, the **Add account** dialog fails with an actionable message telling you which distro is missing the binary.
