import { createDesktopInvoke, DesktopCommandError } from './desktop-client'
import type { AgentHooksApi } from '../../../preload/api/agent-hook-api'
import type {
  AgentHookInstallSkipReason,
  AgentHookInstallState,
  AgentHookInstallStatus,
  AgentHookTarget
} from '../../../shared/agent-hook-types'
import { AGENT_HOOK_TARGETS } from '../../../shared/agent-hook-types'

type AgentHooksInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

const INSTALL_STATES: ReadonlySet<string> = new Set([
  'installed',
  'not_installed',
  'partial',
  'error',
  'skipped'
])

const SKIP_REASONS: ReadonlySet<string> = new Set([
  'agent_disabled',
  'cli_not_found',
  'cli_presence_unknown',
  'hooks_disabled'
])

const TARGETS: ReadonlySet<string> = new Set<string>(AGENT_HOOK_TARGETS)

/** Preload `agentHooks` plus `kimiStatus` (present on the Electron object, omitted from AgentHooksApi). */
export type TauriAgentHooksApi = AgentHooksApi & {
  kimiStatus: () => Promise<AgentHookInstallStatus>
}

function isInstallState(value: unknown): value is AgentHookInstallState {
  return typeof value === 'string' && INSTALL_STATES.has(value)
}

function isHookTarget(value: unknown): value is AgentHookTarget {
  return typeof value === 'string' && TARGETS.has(value)
}

function isSkipReason(value: unknown): value is AgentHookInstallSkipReason {
  return typeof value === 'string' && SKIP_REASONS.has(value)
}

export function asAgentHookInstallStatus(value: unknown): AgentHookInstallStatus | undefined {
  if (typeof value !== 'object' || value === null) {
    return undefined
  }
  const payload = value as Record<string, unknown>
  if (!isHookTarget(payload.agent) || !isInstallState(payload.state)) {
    return undefined
  }
  if (typeof payload.configPath !== 'string' || typeof payload.managedHooksPresent !== 'boolean') {
    return undefined
  }
  if (
    payload.detail !== undefined &&
    payload.detail !== null &&
    typeof payload.detail !== 'string'
  ) {
    return undefined
  }
  if (payload.skipReason !== undefined && !isSkipReason(payload.skipReason)) {
    return undefined
  }
  return {
    agent: payload.agent,
    state: payload.state,
    configPath: payload.configPath,
    managedHooksPresent: payload.managedHooksPresent,
    detail: typeof payload.detail === 'string' ? payload.detail : null,
    ...(payload.skipReason !== undefined ? { skipReason: payload.skipReason } : {})
  }
}

function invalidStatus(value: unknown): DesktopCommandError {
  return new DesktopCommandError(
    'agent_hooks_status_invalid_response',
    { operation: 'agent_hooks_status' },
    'agent_hooks_status returned an unexpected payload',
    'MCode could not complete the desktop request. Try again.',
    value
  )
}

/**
 * Typed `window.api.agentHooks` over `agent_hooks_status`. Each method passes
 * the matching agent literal from `AGENT_HOOK_TARGETS`.
 */
export function createTauriAgentHooksClient(invoke: AgentHooksInvoke): TauriAgentHooksApi {
  const call = createDesktopInvoke(invoke)

  const status = (agent: AgentHookTarget) => async (): Promise<AgentHookInstallStatus> => {
    const payload = await call('agent_hooks_status', { agent })
    const parsed = asAgentHookInstallStatus(payload)
    if (!parsed) {
      throw invalidStatus(payload)
    }
    return parsed
  }

  return {
    claudeStatus: status('claude'),
    openClaudeStatus: status('openclaude'),
    codexStatus: status('codex'),
    geminiStatus: status('gemini'),
    antigravityStatus: status('antigravity'),
    ampStatus: status('amp'),
    cursorStatus: status('cursor'),
    droidStatus: status('droid'),
    commandCodeStatus: status('command-code'),
    grokStatus: status('grok'),
    copilotStatus: status('copilot'),
    hermesStatus: status('hermes'),
    devinStatus: status('devin'),
    kimiStatus: status('kimi')
  }
}
