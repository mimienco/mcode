# MCode CLI reference

Commands, selectors, and agent-friendly patterns for driving MCode from a shell.

The `mcode` CLI talks to a running MCode runtime. Use it when a shell script or agent needs to inspect worktrees, launch terminals, open files, automate the built-in browser, or report progress back into MCode.

## Verify the runtime

Register the CLI under [Settings → General → MCode CLI](../settings.md), then check that it can reach MCode:

```
command -v mcode
mcode status --json
```

If MCode is not already running:

```
mcode open --json
mcode status --json
```

Use `--json` when another tool will parse the result. Human-readable output is for quick terminal checks.

## Selectors

Most commands accept selectors instead of requiring long IDs:

```
mcode repo show --repo id:<repoId> --json
mcode worktree show --worktree active --json
mcode worktree show --worktree path:/abs/path/to/worktree --json
mcode worktree show --worktree branch:feature-name --json
mcode worktree show --worktree issue:123 --json
```

- `active` and `current` resolve to the enclosing MCode-managed worktree from the shell's current directory or terminal context. Use explicit selectors in scripts that may run outside the target worktree.
- For remote runtimes, prefer full server-side selectors such as `id:<repoId>::<absolute-worktree-path>` or `path:<absolute-server-path>` because the local shell's current directory may not exist on the runtime host.

## Choose a host

List every machine the current MCode host can target and the selector for each one:

```
mcode host list --json
```

The result includes this machine, its registered [SSH targets](../ssh.md), and paired [Remote MCode Servers](../remote-servers.md).

- Use `--host local` for this machine, `--host ssh:<target-id>` for an SSH target, and `--environment <server-name>` for a paired server.
- SSH labels and paired-server names also resolve when they are unique; use the IDs from `host list` when names collide.
- If you put a machine name on the wrong selector, MCode reports the matching machine and the flag to use instead of returning an empty result.

## Runtime commands

```
mcode open --json
mcode status --json
mcode serve --port 6768 --pairing-address 100.64.1.20 --json
```

`mcode serve` starts a runtime server in the foreground without opening the desktop window. Use it for [Remote MCode Servers](../remote-servers.md) or headless environments, and stop it with `Ctrl-C`.

## Repos

```
mcode repo list --json
mcode repo add --path /abs/path/to/repo --json
mcode repo show --repo id:<repoId> --json
mcode repo set-base-ref --repo id:<repoId> --ref origin/main --json
mcode repo search-refs --repo id:<repoId> --query main --limit 10 --json
```

Set the repo base ref before creating lots of worktrees so new tasks branch from the right place by default.

## Worktrees

```
mcode worktree list --repo id:<repoId> --json
mcode worktree ps --json
mcode worktree current --json
mcode worktree show --worktree active --json
mcode worktree create --repo id:<repoId> --name fix-login --json
mcode worktree create --name child-task --agent codex --prompt "Investigate the flaky login test" --json
mcode worktree set --worktree active --comment "reproduced failure; testing token refresh fix" --json
mcode worktree rm --worktree id:<worktreeId> --force --json
```

When `worktree create` runs from inside an MCode-managed worktree, MCode records the new worktree as a child when it can infer the relationship. Pass `--parent-worktree active` to be explicit, or `--no-parent` when the new work is independent.

Agent startup flags:

```
mcode worktree create --name review-api --agent claude --setup run --json
mcode worktree create --name quick-check --agent codex --prompt "Summarize the diff" --setup skip --json
mcode worktree create --name hidden-setup --setup inherit --json
```

- `--agent` launches the selected agent in the first terminal.
- `--prompt` sends initial work to that agent.
- `--setup run|skip|inherit` controls repo setup hooks; `inherit` follows the repo policy.

## Terminals

```
mcode terminal list --worktree active --json
mcode terminal show --terminal <handle> --json
mcode terminal read --terminal <handle> --json
mcode terminal read --terminal <handle> --screen --json
mcode terminal read --terminal <handle> --cursor <cursor> --limit 1000 --json
mcode terminal send --terminal <handle> --text "continue" --enter --json
mcode terminal wait --terminal <handle> --for tui-idle --timeout-ms 300000 --json
mcode terminal create --worktree active --title "tests" --command "npm test" --json
mcode terminal split --terminal <handle> --direction horizontal --command "npm run dev" --json
mcode terminal rename --terminal <handle> --title "runner" --json
mcode terminal switch --terminal <handle> --json
mcode terminal close --terminal <handle> --json
```

Omit `--terminal` to target the active terminal in the current worktree. Read before sending when you are not sure what the terminal is waiting for.

- `terminal list` reports each terminal's `executionHostId` when MCode can verify it, plus a result-level `hostScope` with covered and omitted host IDs. Treat a missing host identity or scope as **unverifiable**, not local. A missing terminal is evidence that it exited only when its execution host is listed in `hostScope.hostIds`.
- By default, `terminal read` returns the accumulated output stream with terminal escapes stripped. Programs that redraw lines can therefore appear as stacked fragments. Use `--screen` when you need the currently rendered frame; the response's `source` identifies `stream`, `screen`, or `screen-unavailable`. Screen reads have no history to page, so `--screen` and `--cursor` are mutually exclusive.
- For long output, use cursor reads. Save `nextCursor` from one stream read, then pass it back with `--cursor` to fetch only new output.

## Files

```
mcode file open src/App.tsx --worktree active --json
mcode file diff src/App.tsx --staged --worktree active --json
mcode file open-changed --mode both --worktree active --json
```

Paths are relative to the selected worktree. `open-changed` reads git status and opens changed files in edit, diff, or both modes.

## Built-in browser

Browser commands control MCode's embedded browser tab for the selected worktree. They do not control Chrome, Safari, or the MCode desktop UI.

Use a snapshot -> act -> snapshot loop:

```
mcode goto --url http://localhost:3000 --worktree active --json
mcode snapshot --worktree active --json
mcode click --element @e3 --worktree active --json
mcode fill --element @e1 --value "search query" --worktree active --json
mcode wait --text "Welcome" --worktree active --json
mcode screenshot --worktree active --json
```

Refs such as `@e3` come from `snapshot`. Re-snapshot after navigation, tab switches, clicks that change the page, and any stale-ref error.

Tab and capture commands:

```
mcode tab list --worktree active --json
mcode tab create --url http://localhost:3000 --worktree active --json
mcode tab switch --index 1 --worktree active --json
mcode capture start --worktree active --json
mcode console --limit 50 --worktree active --json
mcode network --limit 50 --worktree active --json
mcode full-screenshot --worktree active --json
mcode pdf --worktree active --json
```

Use `mcode exec --command "<agent-browser command>" --json` only for browser actions that do not have a typed MCode command yet.

Browser device emulation:

```
mcode set device --name "iPhone 12" --worktree active --json
mcode screenshot --worktree active --json
```

## Desktop computer use

Use `mcode computer` for native desktop apps outside the built-in browser:

```
mcode computer permissions --json
mcode computer list-apps --json
mcode computer get-app-state --app com.apple.Safari --json
mcode computer click --app com.apple.Safari --element-index 12 --json
mcode computer paste-text --app com.apple.Safari --text "hello" --json
```

See [Computer use](computer-use.md) for the full workflow and permission setup.

## Mobile emulator

The mobile emulator commands control iOS Simulator devices through MCode's worktree-scoped bridge. Use them instead of raw `serve-sim` or `simctl` when an agent is operating from inside MCode, so lifecycle and active-device state stay attached to the current worktree.

```
mcode emulator list --worktree active --json
mcode emulator attach "<device-name-or-udid>" --worktree active --json
mcode emulator tap 0.5 0.7 --worktree active --json
mcode emulator type "hello" --worktree active --json
mcode emulator gesture '[{"type":"begin","x":0.5,"y":0.8},{"type":"move","x":0.5,"y":0.4},{"type":"end","x":0.5,"y":0.2}]' --worktree active --json
mcode emulator button home --worktree active --json
mcode emulator rotate landscape_left --worktree active --json
mcode emulator exec --command "tap 0.5 0.7" --worktree active --json
mcode emulator kill --worktree active --json
mcode emulator shutdown --worktree active --json
```

Coordinates are normalized from `0` to `1`. Prefer `tap` for single taps, and use `gesture` for drags or multi-step touch input. Pass `--device <udid-or-name>` or `--emulator <id>` when a script must target a specific simulator instead of the worktree's active emulator.

## Linear

The `mcode linear` surface is what agents use via the `mcode-linear` skill (legacy install name `linear-tickets` still works). Prefer `--json`. Linked worktrees resolve with `--current`.

### Read

```
mcode linear issue --current --full --json
mcode linear issue ENG-123 --comments --children --relations --activity --json
mcode linear search "auth bug" --workspace all --json
mcode linear list --filter assigned --limit 10 --json
mcode linear list-issues --team ENG --state started --assignee me --json
mcode linear list-issues --query auth --updated-at -P7D --cursor <cursor> --workspace <id> --json
mcode linear team list --json
mcode linear team states --team ENG --json
mcode linear team labels --team ENG --json
mcode linear project list --query launch --json
```

`--full` expands comments, children, attachments, relations, and activity. Section flags (`--comments`, `--children`, `--attachments`, `--relations`, `--activity`) work individually.

### MCP-style write

```
# Create or update (omit id/--current to create; requires --team and --title on create)
mcode linear save-issue --team ENG --title "Fix auth" --priority high --json
mcode linear save-issue ENG-123 --state "In Progress" --assignee me --json
mcode linear save-issue --current --project null --due-date null --json

mcode linear relation add ENG-1 --related ENG-2 --type blocks --json
mcode linear relation remove ENG-1 --related ENG-2 --type related --json
```

`save-issue` labels **replace** the full label set (Linear MCP `save_issue` semantics). Literal `null` clears assignee, estimate, due date, project, or parent.

### Field helpers (still valid)

```
mcode linear status set --current --to "In Progress" --json
mcode linear assignee set --current --me --json
mcode linear priority set ENG-123 --to high --json
mcode linear estimate set --current --to 3 --json
mcode linear due-date set --current --to 2026-08-01 --json
mcode linear label add --current --label backend --json
mcode linear comment add --current --body "Investigating regression" --json
mcode linear attach --current --url https://example.com/repro --title "Repro" --json
mcode linear create --title "Flaky login test" --team ENG --priority high --json
```

Run `mcode linear --help` or `mcode skills get mcode-linear` for the version-matched list. Pass an explicit issue id (e.g. `ENG-123`) when a script may run outside an MCode-linked worktree.

## Skills (local, no runtime required)

List bundled guides, print a version-matched guide, or install/update hybrid skill packages without the desktop Settings UI:

```
mcode skills list
mcode skills get mcode-cli
mcode skills get orchestration --full
mcode skills install --skill mcode-cli --skill orchestration
mcode skills install --all --dry-run
mcode skills update --all
```

`install` / `update` shell out to the same `npx skills` commands Settings uses. They do not contact the MCode runtime. See [MCode skills](skills.md#keep-skills-up-to-date).

## Account (host-local runtime)

On a headless host running MCode (`mcode serve` or the desktop app), add managed Claude/Codex accounts when the remote client cannot use **Add account** (remote runtime scope disables that button):

```
mcode account list
mcode account add                 # Claude by default
mcode account add --agent codex
```

`account add` runs `claude login` / `codex login` in **this** terminal on the host, then registers the captured credentials with the local runtime. Codex uses device authorization so the browser can finish on another machine. Run these on the machine that owns the accounts — not through a client-only remote session.

## Artifacts

Publish HTML or Markdown through the signed-in MCode account. Viewing a public link does not require sign-in; create/list/update/delete do. **Publishing is off by default** — a human must enable **Settings → Artifacts → Allow publishing public artifact links** on the device. There is no CLI flag that grants the gate. `list`, `unshare`, and `delete` stay available so you can audit or revoke links after turning publishing off.

```
mcode artifacts share ./report.html --json
mcode artifacts share ./notes.md --json
mcode artifacts update ./notes.md --json
mcode artifacts unshare ./notes.md --json
mcode artifacts list --json
mcode artifacts list --cursor <cursor> --json
mcode artifacts delete <id> --json
```

- Accepted files: `.html`, `.htm`, `.md`, `.markdown`.
- `share` stores the edit token in the active MCode profile and does not print it. `update` / `unshare` resolve by the same local path and profile that originally shared the file.
- `list` is paged (`nextCursor` → `--cursor`). `delete` takes the artifact id from `list` and does not need the original file.
- Relative HTML assets are not uploaded — share self-contained HTML or absolute asset URLs.
- Denied publish/update fails with `artifact_sharing_disabled`; fix Settings instead of retrying.
- Desktop: open a local HTML or Markdown file and use **Share as artifact**, or manage links from the sidebar **Artifacts** page.

## Automations, environments, and hooks

Scheduled prompts:

```
mcode automations list --json
mcode automations create --name "Daily review" --trigger daily --time 09:00 --prompt "Review open changes" --provider codex --repo id:<repoId> --disabled --json
mcode automations run <automationId> --json
```

Remote runtime environments:

```
mcode environment add --name work-laptop --pairing-code "mcode://pair?code=..." --json
mcode environment list --json
mcode environment rm --environment <selector> --json
```

Agent status hooks:

```
mcode agent hooks status --json
mcode agent hooks on --json
mcode agent hooks off --json
```

## Agent habits

- Prefer `--json` for automation and agent calls.
- Prefer selectors over parsing UI labels.
- Read terminal state before sending input unless the next input is obvious.
- Use worktree comments for progress checkpoints. See [Worktree checkpoints](worktree-checkpoints.md).
- Use [Orchestration](orchestration.md) for tracked multi-agent dispatches instead of ad hoc terminal prompts.
