# MCode skills registry & MCP

Install MCode agent skills with npx skills add. Hybrid stubs stay thin; `mcode skills get` serves the version-matched guide. MCode can update skills in the background.

MCode ships **skills** that agents install into their skill directories. Public install packages are **hybrid discovery stubs**: short `SKILL.md` files that tell the agent *when* to engage MCode and how to load the full guide from the running CLI. Command flags live in the binary so they cannot drift from the app version.

## Installable MCode skills

Use `npx skills add` with the public MCode repo and the skill name. Default agent setup usually installs `mcode-cli`, `computer-use`, and `orchestration`.

| Skill | Install | Use it for |
| --- | --- | --- |
| [`mcode-cli`](#mcode-cli) | `npx skills add https://github.com/mcode-ide/mcode --skill mcode-cli --global` | Worktrees, terminals, files, automations, embedded browser. |
| [`orchestration`](#orchestration) | `npx skills add https://github.com/mcode-ide/mcode --skill orchestration --global` | Multi-agent Runs, tasks, supervised workers, messages, gates. |
| [`computer-use`](#computer-use) | `npx skills add https://github.com/mcode-ide/mcode --skill computer-use --global` | Desktop apps via accessibility trees and safe UI actions. |
| [`mcode-linear`](#mcode-linear) | `npx skills add https://github.com/mcode-ide/mcode --skill mcode-linear --global` | Linear ticket read/write through `mcode linear`. |
| [`mcode-emulator`](#mcode-emulator) | `npx skills add https://github.com/mcode-ide/mcode --skill mcode-emulator --global` | iOS Simulator control. |
| [`mcode-emulator-android`](#mcode-emulator-android) | `npx skills add https://github.com/mcode-ide/mcode --skill mcode-emulator-android --global` | Android emulator/device via adb. |
| [`mcode-per-workspace-env`](#mcode-per-workspace-env) | `npx skills add https://github.com/mcode-ide/mcode --skill mcode-per-workspace-env --global` | Per-workspace environment recipes (`mcode.yaml`). |

## Hybrid stubs vs the live guide

After `npx skills add`, agents see a short stub that says:

1. Resolve the CLI executable for this session (`MCODE_CLI_COMMAND`, `mcode-dev`, Linux `mcode-ide`, else `mcode`).
2. Load the full guide: `mcode skills get <topic>` (or `--full` for the long guide).
3. Prefer `--json` and do not invent flags from memory.

```
mcode skills list
mcode skills get mcode-cli
mcode skills get orchestration --full
mcode skills get mcode-linear --json
```

Add `--json` when an agent needs deterministic output for automation. `skills show` is an alias for `skills get`.

## Keep skills up to date

When MCode ships a newer skill package than the one installed globally, the app can:

- Show an **update available** nudge (and **Needs attention** when a copy is out of date somewhere the updater cannot safely rewrite).
- Open **Update skills** — list of placements and skip reasons, then run **`npx --yes skills update <names> --global -y`** headlessly (no embedded terminal).
- Keep the run in the **background**: closing the dialog does not cancel it. A status-bar segment shows progress (spinner while running, brief success check, persistent failure until you act). Click the segment to reopen the dialog.
- Surface freshness on **Settings → Agents** for installed skills (global install rail), including **Details** when a row needs review.

Manual equivalent (desktop Settings still shows the same `npx` form):

```
npx skills update mcode-cli orchestration computer-use --global
```

On headless hosts (SSH, containers, CI, `mcode serve`) with no Settings UI, use the local CLI wrappers — they resolve the same `npx` commands, add non-interactive flags, and do **not** need a running MCode runtime:

```
mcode skills install                                      # list installable names
mcode skills install --skill mcode-cli --skill orchestration
mcode skills install --skill mcode-cli --agent claude-code,codex
mcode skills install --all --dry-run
mcode skills update --all
mcode skills update --skill mcode-cli --dry-run
```

- Default scope is **global** (`--global`); pass `--local` for the current project only.
- `install` targets agents MCode detects on the host (plus the shared `.agents/skills` directory). Use `--agent <name>[,<name>…]` or `--agent universal` to override; if no agent is detected, `--agent` is required.
- `update` only refreshes skills that are already installed.
- `--dry-run` prints the resolved command; `--json` is only valid with listing / `--dry-run`.

Prefer the in-app updater when MCode offers it so the same global placements the app scanned get rewritten. Rows marked **Skipped** explain why that skill cannot be updated automatically (for example missing source registration) — fix the placement, then re-check.

## Install and delete from the Skills page

- When an in-app install asks for a workspace target, search the picker by workspace name or by kind (**worktree** / **folder**).
- The agent picker separates directories MCode always includes from additional agent directories you can select, so you can review the exact installation targets before continuing.
- To remove a scanned skill, open its actions menu and choose **Delete…**. For several skills, open the page's **More actions** menu, choose **Delete skills…**, and select the eligible results.
- MCode explains why a copy cannot be deleted — for example because it is bundled, plugin-managed, read-only, or outside a removable skill folder — before you confirm.
- This is separate from **Manage installs**, which manages copies installed from shared links.

## Share private skills between hosts

- Open **Skills → Share skills** to publish one skill or a bundle behind one unlisted, revocable link. Publishing requires your MCode account.
- Review the included files, scripts, executables, digest, and optional release notes before you publish; each published version is immutable, so later local edits do not silently change what recipients install.
- Anyone with an active link can inspect and install the shared skills without signing in, so treat the link like a credential.
- A recipient reviews the exact version, chooses all or a subset of its skills, then selects global or workspace scope on this computer, a paired MCode runtime, WSL, or an SSH host. Existing modified skills default to **Keep local**.
- Use **Skills → Manage installs** to update, roll back, or remove copies MCode installed, and **Settings → Share Skills** to copy or revoke your active links. Revoking a link blocks future access but does not remove copies already installed.

Agents can publish an explicitly named set from the CLI after you enable the separate, default-off **Settings → Share Skills → Allow agents and the MCode CLI to publish skill links** permission:

```
mcode skills installed --json
mcode skills share --skill frontend --skill testing --bundle-name "Team Toolkit" --json
```

`skills installed` returns safe selectors without exposing local paths. `skills share` accepts exact discovery IDs or unambiguous names; it does not accept arbitrary paths or `--all`.

## mcode-cli

```
npx skills add https://github.com/mcode-ide/mcode --skill mcode-cli --global
```

After install, load the version-matched command guide with `mcode skills get mcode-cli`. See [MCode CLI](overview.md).

## orchestration

```
npx skills add https://github.com/mcode-ide/mcode --skill orchestration --global
```

Use this when an agent should coordinate other agents through Runs, tasks, supervised workers, and decision gates. See [Orchestration](orchestration.md). Always load `mcode skills get orchestration --full` before mutating orchestration state — the legacy `orchestration run` command is retired.

## computer-use

```
npx skills add https://github.com/mcode-ide/mcode --skill computer-use --global
```

Use this when an agent needs to inspect and operate local desktop app windows. See [Computer use](computer-use.md).

## mcode-linear

```
npx skills add https://github.com/mcode-ide/mcode --skill mcode-linear --global
```

Agents should load `mcode skills get mcode-linear` before mutating tickets. Covers `issue --full`, `save-issue`, `list-issues`, relations, completion attach+comment flow, and untrusted-ticket rules. Existing `linear-tickets` installs still resolve. See [CLI reference → Linear](reference.md#linear).

## mcode-emulator

```
npx skills add https://github.com/mcode-ide/mcode --skill mcode-emulator --global
```

Use this when an agent should control an iOS Simulator from inside MCode through `mcode emulator` commands.

## mcode-emulator-android

```
npx skills add https://github.com/mcode-ide/mcode --skill mcode-emulator-android --global
```

Use for adb-connected Android AVDs/devices: list/boot, tap/swipe/type, hardware buttons, install/launch, permissions, accessibility tree, logcat. Load details with `mcode skills get mcode-emulator-android`.

## mcode-per-workspace-env

```
npx skills add https://github.com/mcode-ide/mcode --skill mcode-per-workspace-env --global
```

Use when setting up or debugging per-workspace environment recipes in `mcode.yaml`. See [Ways to run MCode](../ways-to-run.md#4-cloud-vms-per-workspace-environments).

## Discovery sources

MCode's skill UI scans installed skill homes for Claude, Codex, Agent Skills, and **OMP** (`~/.omp/agent/skills`), so skills placed there show up without a manual symlink.

## Add your own skills

Any repo with a `skills/<name>/SKILL.md` file can be installed via `npx skills add`. Point your agent at internal repos to give it company-specific powers.

## MCP servers

Model Context Protocol (MCP) servers expose external tools to compatible agents. Register MCP endpoints under [Settings → Integrations → MCP](../settings.md); those tools appear inside agent CLIs that support MCP.
