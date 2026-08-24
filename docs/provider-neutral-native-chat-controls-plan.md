# Provider-neutral native Chat controls implementation plan

## Goal

Add the permission and Plan/Build controls shown beside the native Chat composer while keeping MCode—not a model or provider—as the product-level owner of their meaning.

MCode exposes two independent settings:

```ts
type AgentPermissionPreset = 'ask' | 'auto-edits' | 'bypass'
type AgentInteractionMode = 'plan' | 'build'
```

Provider adapters translate those values into native protocols or launch flags. Unsupported values remain visibly unavailable and are never widened to a more dangerous setting.

## Structure

```text
shared/agent-run-profile
  ├─ stable MCode enums and capability contract
  ├─ provider launch mappings
  ├─ safe rewrite/detection of MCode-owned launch arguments
  └─ provider-neutral labels and support metadata

main/codex live session bridge
  ├─ permissionProfile/list and collaborationMode/list discovery
  ├─ thread/settings/update mapping
  └─ effective thread/settings/updated reconciliation

renderer/native chat
  ├─ combined model-options + run-profile surface
  ├─ Permission picker with bypass confirmation
  ├─ Plan/Build picker
  └─ live versus next-session status

settings/agents
  └─ the same global permission preset used by new sessions
```

## Required behavior

1. **Ask before edits** never means full access.
2. **Auto-accept edits** never falls back to bypass when a provider lacks it.
3. **Bypass permissions** requires explicit confirmation in Chat.
4. **Plan** and **Build** do not overwrite the saved Build permission intent.
5. A live-capable provider updates the active task and reconciles the provider-reported value.
6. A launch-only provider updates new-session defaults and clearly reports that the running task was not changed.
7. Managed or unsupported capabilities are disabled with a reason.
8. Free-form user launch arguments are preserved; MCode rewrites only argument strings it previously generated.

## Provider mappings for the first release

| Provider     | Ask                    | Auto-edits             | Bypass                     | Plan/Build                                                        |
| ------------ | ---------------------- | ---------------------- | -------------------------- | ----------------------------------------------------------------- |
| Codex        | read-only + on-request | workspace + on-request | danger-full-access + never | live app-server `plan` / `default`                                |
| Claude Code  | `default`              | `acceptEdits`          | `bypassPermissions`        | combined launch enum `plan`, restore Build permission on exit     |
| Gemini CLI   | `default`              | `auto_edit`            | `yolo`                     | combined launch enum `plan`, restore Build permission on exit     |
| Cursor Agent | default                | unsupported            | `--force`/`--yolo`         | `--mode plan`, default Agent mode for Build                       |
| Grok Build   | `default`              | `acceptEdits`          | `bypassPermissions`        | provider-owned interactive Plan fallback; no claimed launch state |

## Step-by-step delivery

### 1. Domain and launch adapters

- Add stable run-profile types and per-provider capability metadata.
- Add safe launch argument/environment generation and detection.
- Migrate the old Manual/YOLO setting vocabulary without changing existing effective defaults.
- Test every supported provider and the Cursor auto-edits safety case.

### 2. Codex live protocol

- Discover allowed permission profiles and collaboration presets.
- Extend the live snapshot with effective permission and interaction mode.
- Apply validated profile/mode updates atomically through `thread/settings/update`.
- Reconcile `thread/settings/updated`; fail closed when the alpha API is absent.

### 3. Native Chat UI

- Merge two run-profile descriptors into the existing session-options surface.
- Render permission and Plan/Build as distinct compact composer pickers.
- Require confirmation for bypass.
- Disable changes while a turn is working.
- Explain `live`, `next session`, and unsupported behavior in the picker copy.

### 4. General Settings

- Replace Manual/YOLO with Ask, Auto-accept edits, and Bypass.
- Persist the stable MCode preset and update provider launch defaults through the same adapter.
- Preserve custom per-agent overrides and show the existing mixed-state warning.

### 5. Verification

- Run focused shared, Codex bridge, renderer hook, picker, composer, Settings, and persistence tests.
- Run Node/web typechecking regularly.
- Run the full unit test suite once.
- Review the final diff for provider semantics, security regressions, remote degradation, and unrelated dirty-worktree changes.

## Reusable implementation-plan template

Use this section for later provider adapters:

```md
### Provider: <name>

- Runtime/version probe:
- MCode permission mapping:
  - ask:
  - auto-edits:
  - bypass:
- MCode interaction mapping:
  - plan:
  - build:
- Apply channel: live | launch-only | interactive-fallback | unsupported
- Effective-state channel:
- Managed-policy restrictions:
- Conflicting flags removed:
- Unsupported values and UI reason:
- Security invariants:
- Focused tests:
```

## Done criteria

- The composer contains separate permission and Plan/Build controls.
- The controls use provider-neutral MCode values.
- Codex changes apply live when its bridge is available.
- Claude, Gemini, and Cursor receive verified new-session Plan mappings; Grok keeps Plan as an explicit provider-owned fallback while its permission presets remain launch-controlled.
- Unsupported mappings cannot silently increase access.
- Tests, typechecking, final review, and the implementation commit complete successfully.
