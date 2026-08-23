# Mission Cockpit and Decision Inbox implementation plan

Status: complete
Owner: MCode
Last updated: 2026-08-23

## Outcome

Add a first-class Mission Control view to the existing Agent Dashboard that answers:

1. Which orchestrated missions are active?
2. What is each mission doing, where, and with which agent?
3. Which decisions need the operator now?
4. How does the operator reach the exact live context safely?

The first release is an operational read model over existing MCode truth. It does not add a second scheduler or bypass terminal/provider approval contracts.

## Product scope

### In this implementation

- A Mission Control view beside Dashboard and Agent Map.
- Mission grouping from existing Run/Task/Dispatch context already attached to agent status.
- Mission progress and counts for working, attention, failed, completed, and idle tasks.
- A global Decision Inbox for every attention card, including non-orchestrated agents.
- Human-readable parsing of plain and JSON interactive prompts.
- Stable sorting: unseen decisions first, then blocked, then newest.
- Click-to-open the exact existing terminal preview; no new PTY or agent process.
- Cross-host/worktree labels carried from the existing dashboard snapshot.
- Empty states, accessible headings, progress semantics, keyboard focus, and status text that does not rely on color.
- Pop-out and in-window dashboard parity.
- Main-process validation of the added orchestration snapshot field.

### Explicitly not in this implementation

- Approve/Deny buttons that write directly to provider prompts.
- Orchestration Gate mutation from the renderer.
- Head-SHA, diff-digest, policy, expiry, or immutable approval receipts.
- Queued tasks that do not yet have an agent-status card.
- Automatic task scheduling or dependency execution.
- Multi-repo atomic missions.
- Canvas geometry.

Those require an authoritative Mission/Evidence/Decision snapshot from the runtime. Shipping mutation controls before that seam exists would create unsafe UI-only approvals.

## Domain language

- **Mission**: one orchestration Run visible through its active Task/Dispatch cards.
- **Task**: one logical orchestration Task, possibly with more than one Dispatch attempt.
- **Decision**: an agent in the attention bucket with a pending interactive prompt or blocker.
- **Independent work**: an agent without an orchestration Run; it can appear in the Decision Inbox but is not invented as a Mission.
- **Mission Control projection**: disposable UI read model built from a dashboard snapshot. It is not a system of record.

## Module design

### Shared snapshot interface

`DashboardCard.orchestration?` carries the bounded, structured-clone-safe subset already present on `AgentStatusEntry.orchestration`:

- `taskId`
- `dispatchId`
- `dispatchStatus?`
- `taskTitle?`
- `displayName?`
- `orchestrationRunId?`

The existing dashboard builder owns normalization. The main-process dashboard validator owns admission.

### Deep projection module

External interface:

```ts
buildMissionControlProjection(cards: readonly DashboardCard[]): MissionControlProjection
```

The caller learns one function and one result shape. The module hides:

- prompt parsing
- decision priority
- mission/task grouping
- repeated dispatch-attempt collapse
- task state precedence
- mission progress
- labels and stable ordering

Tests use the same interface as the UI. No store, Electron, or React mocks are required.

### Presentational module

`MissionControlView` accepts cards and `onOpenTerminal(card)`. It does not query stores, call RPC, or mutate orchestration.

This keeps both dashboard hosts identical:

- In-window drawer uses local store routing.
- Pop-out uses the existing validated IPC relay.

## State rules

### Decision order

1. Unseen before seen.
2. Blocked before waiting.
3. Newer `stateChangedAt` before older.
4. Stable pane-key tie-breaker.

### Task collapse

- Group cards by `taskId` within a Run.
- Keep all cards as attempts.
- Choose the newest attempt as the primary card for navigation.
- State precedence: attention → failed/circuit-broken → working/dispatched → completed/done → idle.

### Mission order

1. Missions with pending decisions.
2. Missions with active work.
3. Newest activity.
4. Stable Run-id tie-breaker.

### Progress

`completed logical tasks / total logical tasks`. A failed or waiting task is not completed.

## TDD tracer bullets

### Slice 1 — Decision projection

- [x] RED: plain attention prompt becomes one decision.
- [x] GREEN: minimum projection implementation.
- [x] RED: JSON AskUserQuestion becomes readable question and options.
- [x] GREEN: bounded defensive parser.
- [x] RED: unseen/blocked/newest ordering is stable.
- [x] GREEN: priority comparator.

### Slice 2 — Mission projection

- [x] RED: cards sharing a Run become one mission.
- [x] GREEN: Run grouping.
- [x] RED: repeated attempts for one Task collapse into one logical task.
- [x] GREEN: task aggregation and primary attempt selection.
- [x] RED: mission progress and priority order reflect task states.
- [x] GREEN: summary aggregation.
- [x] RED: agents without a Run do not become fake missions.
- [x] GREEN: explicit unassigned count.

### Slice 3 — Snapshot transport

- [x] RED: dashboard builder publishes orchestration metadata.
- [x] GREEN: attach normalized context to cards.
- [x] RED: main-process validator accepts bounded context and rejects malformed context per card.
- [x] GREEN: validator implementation.
- [x] RED: live pop-out status patch refreshes orchestration metadata.
- [x] GREEN: patch implementation.

### Slice 4 — Mission Control UI

- [x] RED: Mission Control is a third dashboard view.
- [x] GREEN: view switch and default entry.
- [x] RED: Decision Inbox shows readable pending decisions and opens the exact card.
- [x] GREEN: inbox UI.
- [x] RED: missions show task progress, status, workspace, agent, and open behavior.
- [x] GREEN: cockpit UI.
- [x] RED: empty states distinguish no missions from no decisions.
- [x] GREEN: empty-state UI.

### Slice 5 — Accessibility and navigation

- [x] Heading hierarchy and labelled regions.
- [x] Native buttons for every actionable row.
- [x] `aria-live="polite"` summary for decision-count changes without message flooding.
- [x] Progress bars with numeric values and text labels.
- [x] Focus-visible treatment using existing design tokens.
- [x] Platform-safe existing search shortcut remains unchanged.

## Review checklist

- [x] No new scheduler or duplicated orchestration state.
- [x] No decision mutation without authoritative validation.
- [x] No local/SSH/remote identity guessing.
- [x] Pop-out payload remains bounded and backward compatible through optional fields.
- [x] One malformed orchestration context drops only its card, not the board.
- [x] Mission UI never implies queued tasks are complete inventory.
- [x] Status meaning is available in text, not color alone.
- [x] Existing Dashboard and Agent Map behavior remains available.
- [x] No live terminal is remounted or reparented.

## Follow-up architecture after this MVP

1. Add an authoritative runtime `mission.snapshot` read method with Runs, queued Tasks, dependencies, Gates, receipts, budgets, and execution-host identity.
2. Merge that read model with live card routing by Run/Task/Dispatch ids.
3. Add head-bound Decision Receipts in main/runtime before any inline Approve/Deny UI.
4. Add dependency graph and ready/blocked explanations.
5. Add per-Mission cost/time budgets and bounded recovery.
6. Reuse the same Mission projection for Grid/DAG/Attention Canvas layouts.

## Verification

- Feature suite: 11 test files, 131 tests passed.
- Changed-file lint: `oxlint --deny-warnings` passed.
- Two-axis Standards/Spec review: no remaining findings after fixes.
- Web and Node typechecks are currently blocked before the changed modules by existing syntax errors in `src/shared/notification-settings-types.ts` (lines 59, 61, and 63).
- The repository-wide test run was attempted and stopped after widespread unrelated baseline failures across SSH, runtime, updater, filesystem-watcher, VM, Git, and native-chat suites. The scoped feature suite was rerun afterward and remained green.
