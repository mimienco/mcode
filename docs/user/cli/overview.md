# MCode CLI overview

Use the MCode CLI to script MCode from a terminal, manage worktrees, control agent terminals, automate the built-in browser, and install agent skills.

The MCode CLI is the `mcode` command-line interface for scripting a running MCode editor from any shell. Use it to create and inspect worktrees, drive agent terminals, open files and diffs, automate the built-in browser, run scheduled automations, share HTML/Markdown artifacts, and control MCode-native tools from scripts or AI agents.

It ships with the desktop app; register it under [Settings → General → MCode CLI](../settings.md).

Agents can install the matching MCode CLI skill with:

```
npx skills add https://github.com/mcode-ide/mcode --skill mcode-cli
# headless / no Settings UI:
mcode skills install --skill mcode-cli
```

See [Skills registry & MCP](skills.md) for every installable MCode skill, including `mcode skills install` / `mcode skills update`.

## Install & verify

```
command -v mcode
mcode status --json
```

## Worktree commands

```
mcode worktree ps --json
mcode worktree create --repo id:<repoId> --name my-task --issue 123 --json
mcode worktree current --json
mcode worktree set --worktree active --comment "reproduced bug" --json
mcode worktree rm --worktree id:<id> --force --json
```

For selectors, setup flags, parent/child worktrees, and the broader command map, see [MCode CLI reference](reference.md).

## Terminal commands

```
mcode terminal list --json
mcode terminal read --json
mcode terminal send --text "continue" --enter --json
mcode terminal wait --for tui-idle --timeout-ms 30000 --json
mcode terminal create --worktree path:/projects/app --command "npm test" --json
mcode terminal split --direction vertical --command "npm run dev" --json
```

For tracked multi-agent work, use [Orchestration](orchestration.md) instead of plain terminal prompts.

## File commands

Open files and diffs in the active MCode worktree from a shell:

```
mcode file open src/App.tsx
mcode file diff src/App.tsx --staged
mcode file open-changed --mode both
```

Use `--worktree <selector>` when the shell's current directory is not inside the target worktree.

## Browser profiles

Browser profiles isolate tab session state, so a script or agent can test with different cookies, local storage, and logged-in identities. The CLI exposes profile commands under `mcode tab profile`; start with `mcode tab profile list --json`, then use `create`, `set`, `clone`, or `use-default` as needed.

## Scheduled automations

Use `mcode automations` to create, inspect, run, and remove scheduled MCode tasks from a shell. Start with [Scheduled automations](automations.md) when you want a recurring prompt to run against a repo or an existing worktree.

## Artifacts

Share HTML or Markdown as public view links through the signed-in MCode account (`mcode artifacts share|update|list|delete`). Publishing is **opt-in** under Settings → Artifacts. Command details: [CLI reference → Artifacts](reference.md#artifacts).

## Browser automation

The CLI also drives the built-in browser with a snapshot-interact-re-snapshot loop:

```
mcode goto --url https://example.com --json
mcode snapshot --json     # returns refs like @e1, @e3
mcode click --element @e3 --json
mcode fill --element @e1 --value "search query" --json
mcode screenshot --json
```

For responsive browser checks, switch the active tab to a named device profile:

```
mcode set device --name "iPhone 12" --json
mcode screenshot --json
```

## Mobile emulator

The CLI can also drive an iOS Simulator through MCode's mobile emulator bridge. It is scoped to the active worktree, so agents and scripts can attach a simulator from `mcode emulator list`, tap normalized coordinates, type text, send gestures, rotate the device, and shut it down without leaving MCode.

```
mcode emulator list --json
mcode emulator attach "<device-name-or-udid>" --json
mcode emulator tap 0.5 0.7 --json
mcode emulator type "hello" --json
mcode emulator gesture '[{"type":"begin","x":0.5,"y":0.8},{"type":"move","x":0.5,"y":0.4},{"type":"end","x":0.5,"y":0.2}]' --json
mcode emulator rotate landscape_left --json
mcode emulator kill --json
```

Use `--worktree <selector>`, `--device <udid-or-name>`, or `--emulator <id>` when a script needs an explicit target.

## Next steps

- [CLI reference](reference.md) — commands, selectors, and agent-friendly patterns.
- [Skills registry & MCP](skills.md) — what agents install.
- [Orchestration](orchestration.md) — supervised multi-agent runs.
- [Computer use](computer-use.md) — native desktop apps.
- [Scheduled automations](automations.md) — recurring prompts.
- [Worktree checkpoints](worktree-checkpoints.md) — progress notes agents can read.
