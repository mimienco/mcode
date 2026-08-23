import type { DashboardCard } from '../../../../shared/dashboard-snapshot'

export type MissionControlDecision = {
  id: string
  title: string
  options: string[]
  priority: 'high' | 'normal'
  unseen: boolean
  requestedAt: number
  card: DashboardCard
}

export type MissionControlTaskState = 'attention' | 'failed' | 'working' | 'completed' | 'idle'

export type MissionControlTask = {
  id: string
  title: string
  state: MissionControlTaskState
  attemptCount: number
  updatedAt: number
  card: DashboardCard
}

export type MissionControlMission = {
  id: string
  label: string
  tasks: MissionControlTask[]
  decisionCount: number
  attentionCount: number
  workingCount: number
  failedCount: number
  completedCount: number
  idleCount: number
  progressPercent: number
  workspaceCount: number
  knownHostCount: number
  lastChangedAt: number
}

export type MissionControlSummary = {
  missionCount: number
  decisionCount: number
  workingTaskCount: number
  completedTaskCount: number
  unassignedAgentCount: number
}

export type MissionControlProjection = {
  summary: MissionControlSummary
  decisions: MissionControlDecision[]
  missions: MissionControlMission[]
}

type DecisionCopy = Pick<MissionControlDecision, 'title' | 'options'>

const MAX_DECISION_TITLE_LENGTH = 240
const MAX_DECISION_OPTIONS = 8
const MAX_DECISION_OPTION_LENGTH = 120

function clipped(value: string, maxLength: number): string {
  return value.trim().slice(0, maxLength)
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function readDecisionCopy(card: DashboardCard): DecisionCopy {
  const fallbackTitle = clipped(
    card.askSummary || card.task || 'Agent needs a decision',
    MAX_DECISION_TITLE_LENGTH
  )
  const rawPrompt = card.askSummary?.trim()
  if (!rawPrompt?.startsWith('{')) {
    return { title: fallbackTitle, options: [] }
  }
  const structuredFallbackTitle = clipped(
    card.task || 'Agent needs a decision',
    MAX_DECISION_TITLE_LENGTH
  )

  try {
    const parsed: unknown = JSON.parse(rawPrompt)
    if (!isRecord(parsed)) {
      return { title: structuredFallbackTitle, options: [] }
    }

    const firstQuestion = Array.isArray(parsed.questions) ? parsed.questions[0] : undefined
    const question = isRecord(firstQuestion) ? firstQuestion : parsed
    const title = clipped(
      typeof question.question === 'string' && question.question.trim()
        ? question.question.trim()
        : structuredFallbackTitle,
      MAX_DECISION_TITLE_LENGTH
    )
    const options = Array.isArray(question.options)
      ? question.options
          .flatMap((option) => {
            if (typeof option === 'string' && option.trim()) {
              return [clipped(option, MAX_DECISION_OPTION_LENGTH)]
            }
            if (!isRecord(option) || typeof option.label !== 'string' || !option.label.trim()) {
              return []
            }
            return [clipped(option.label, MAX_DECISION_OPTION_LENGTH)]
          })
          .slice(0, MAX_DECISION_OPTIONS)
      : []

    return { title, options }
  } catch {
    return { title: structuredFallbackTitle, options: [] }
  }
}

const TASK_STATE_ORDER: Readonly<Record<MissionControlTaskState, number>> = {
  attention: 0,
  failed: 1,
  working: 2,
  completed: 3,
  idle: 4
}

function compareCardsNewestFirst(left: DashboardCard, right: DashboardCard): number {
  return right.stateChangedAt - left.stateChangedAt || left.paneKey.localeCompare(right.paneKey)
}

function taskState(card: DashboardCard): MissionControlTaskState {
  if (card.bucket === 'attention') {
    return 'attention'
  }
  if (
    card.orchestration?.dispatchStatus === 'failed' ||
    card.orchestration?.dispatchStatus === 'circuit_broken'
  ) {
    return 'failed'
  }
  if (
    card.bucket === 'working' ||
    card.orchestration?.dispatchStatus === 'pending' ||
    card.orchestration?.dispatchStatus === 'dispatched'
  ) {
    return 'working'
  }
  if (card.bucket === 'done' || card.orchestration?.dispatchStatus === 'completed') {
    return 'completed'
  }
  return 'idle'
}

function buildMissions(cards: readonly DashboardCard[]): MissionControlMission[] {
  const byRun = new Map<string, DashboardCard[]>()
  for (const card of cards) {
    const runId = card.orchestration?.orchestrationRunId
    if (!runId) {
      continue
    }
    const runCards = byRun.get(runId)
    if (runCards) {
      runCards.push(card)
    } else {
      byRun.set(runId, [card])
    }
  }

  return [...byRun.entries()]
    .map(([runId, runCards]) => {
      const byTask = new Map<string, DashboardCard[]>()
      for (const card of runCards) {
        const taskId = card.orchestration?.taskId
        if (!taskId) {
          continue
        }
        const attempts = byTask.get(taskId)
        if (attempts) {
          attempts.push(card)
        } else {
          byTask.set(taskId, [card])
        }
      }

      const tasks = [...byTask.entries()]
        .map(([taskId, attempts]): MissionControlTask => {
          attempts.sort(compareCardsNewestFirst)
          const primary = attempts[0]
          return {
            id: taskId,
            title: primary.orchestration?.taskTitle || primary.task || `Task ${taskId.slice(0, 8)}`,
            state: taskState(primary),
            attemptCount: attempts.length,
            updatedAt: primary.stateChangedAt,
            card: primary
          }
        })
        .sort(
          (left, right) =>
            TASK_STATE_ORDER[left.state] - TASK_STATE_ORDER[right.state] ||
            right.updatedAt - left.updatedAt ||
            left.id.localeCompare(right.id)
        )
      const attentionCount = tasks.filter((task) => task.state === 'attention').length
      const workingCount = tasks.filter((task) => task.state === 'working').length
      const failedCount = tasks.filter((task) => task.state === 'failed').length
      const completedCount = tasks.filter((task) => task.state === 'completed').length
      const idleCount = tasks.filter((task) => task.state === 'idle').length
      const workspaces = new Set(runCards.map((card) => `${card.repoId}\u0000${card.worktreeId}`))
      const knownHosts = new Set(
        runCards.flatMap((card) => (card.executionHostId ? [card.executionHostId] : []))
      )

      return {
        id: runId,
        label: `Mission ${runId.slice(0, 8)}`,
        tasks,
        decisionCount: attentionCount,
        attentionCount,
        workingCount,
        failedCount,
        completedCount,
        idleCount,
        progressPercent: tasks.length === 0 ? 0 : Math.round((completedCount / tasks.length) * 100),
        workspaceCount: workspaces.size,
        knownHostCount: knownHosts.size,
        lastChangedAt: Math.max(0, ...tasks.map((task) => task.updatedAt))
      }
    })
    .sort(
      (left, right) =>
        Number(right.decisionCount > 0) - Number(left.decisionCount > 0) ||
        Number(right.failedCount + right.workingCount > 0) -
          Number(left.failedCount + left.workingCount > 0) ||
        right.lastChangedAt - left.lastChangedAt ||
        left.id.localeCompare(right.id)
    )
}

export function buildMissionControlProjection(
  cards: readonly DashboardCard[]
): MissionControlProjection {
  const decisions = cards
    .filter((card) => card.bucket === 'attention')
    .map((card) => {
      const copy = readDecisionCopy(card)
      return {
        id: card.paneKey,
        ...copy,
        priority: card.dotState === 'blocked' ? ('high' as const) : ('normal' as const),
        unseen: card.unseen,
        requestedAt: card.stateChangedAt,
        card
      }
    })
    .sort(
      (left, right) =>
        Number(right.unseen) - Number(left.unseen) ||
        Number(right.priority === 'high') - Number(left.priority === 'high') ||
        right.requestedAt - left.requestedAt ||
        left.id.localeCompare(right.id)
    )
  const missions = buildMissions(cards)
  const missionTasks = missions.flatMap((mission) => mission.tasks)

  return {
    summary: {
      missionCount: missions.length,
      decisionCount: decisions.length,
      workingTaskCount: missionTasks.filter((task) => task.state === 'working').length,
      completedTaskCount: missionTasks.filter((task) => task.state === 'completed').length,
      unassignedAgentCount: cards.filter(
        (card) => !card.orchestration?.orchestrationRunId
      ).length
    },
    decisions,
    missions
  }
}
