import { basename } from 'node:path'
import type { AgentStartupShell } from '../../shared/tui-agent-startup-shell'
import {
  buildShellCommandFromArgv,
  tokenizeStartupCommand
} from '../../shared/tui-agent-startup-shell'
import type { CodexLiveSessionModel } from '../../shared/codex-live-session-options'

export type CodexRemoteLaunchPlan = {
  appServerArgv: string[]
  tuiCommand: string
}

function normalizedExecutableName(value: string): string {
  return basename(value.replaceAll('\\', '/')).toLowerCase()
}

function codexEntrypointLength(tokens: readonly string[]): number | null {
  const first = normalizedExecutableName(tokens[0] ?? '')
  if (/^codex(?:\.exe|\.cmd|\.bat)?$/.test(first)) {
    return 1
  }
  if (/^(?:node|node\.exe)$/.test(first)) {
    const second = normalizedExecutableName(tokens[1] ?? '')
    return /^codex(?:\.js|\.mjs|\.cjs)?$/.test(second) ? 2 : null
  }
  return null
}

function codexAppServerGlobalArgs(tokens: readonly string[]): string[] | null {
  const args: string[] = []
  const valueFlags = new Set(['-c', '--config', '--enable', '--disable', '-p', '--profile'])
  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index]
    if (valueFlags.has(token)) {
      const value = tokens[index + 1]
      if (value === undefined) {
        return null
      }
      args.push(token, value)
      index += 1
      continue
    }
    if (token === '--strict-config' || /^(?:--config|--enable|--disable|--profile)=/.test(token)) {
      args.push(token)
    }
  }
  return args
}

/** Pure, fail-closed launch rewrite. Shell expressions and unknown wrappers
 * keep the ordinary embedded TUI path rather than being reinterpreted. */
export function planCodexRemoteLaunch(args: {
  command: string
  shell: AgentStartupShell
  endpoint: string
}): CodexRemoteLaunchPlan | null {
  const parsed = tokenizeStartupCommand(args.command, args.shell)
  if (
    !parsed.ok ||
    parsed.tokens.length === 0 ||
    parsed.spans.some((span) => span.divergesFromShell)
  ) {
    return null
  }
  const entrypointLength = codexEntrypointLength(parsed.tokens)
  if (entrypointLength === null || parsed.tokens.includes('--remote')) {
    return null
  }
  const entrypoint = parsed.tokens.slice(0, entrypointLength)
  const trailing = parsed.tokens.slice(entrypointLength)
  const appServerGlobalArgs = codexAppServerGlobalArgs(trailing)
  if (!appServerGlobalArgs) {
    return null
  }
  return {
    appServerArgv: [...entrypoint, ...appServerGlobalArgs, 'app-server', '--listen', args.endpoint],
    tuiCommand: buildShellCommandFromArgv(
      [...entrypoint, '--remote', args.endpoint, ...trailing],
      args.shell
    )
  }
}

function effortLabel(value: string): string {
  return value === 'xhigh'
    ? 'Extra High'
    : value.length > 0
      ? `${value[0].toUpperCase()}${value.slice(1)}`
      : value
}

export function parseCodexLiveModels(value: unknown): CodexLiveSessionModel[] {
  const data =
    value && typeof value === 'object' && Array.isArray((value as { data?: unknown }).data)
      ? (value as { data: unknown[] }).data
      : []
  const models: CodexLiveSessionModel[] = []
  for (const candidate of data) {
    if (!candidate || typeof candidate !== 'object') {
      continue
    }
    const model = candidate as Record<string, unknown>
    if (
      model.hidden === true ||
      typeof model.model !== 'string' ||
      !model.model ||
      typeof model.displayName !== 'string' ||
      typeof model.defaultReasoningEffort !== 'string'
    ) {
      continue
    }
    const efforts = Array.isArray(model.supportedReasoningEfforts)
      ? model.supportedReasoningEfforts.flatMap((entry) => {
          if (!entry || typeof entry !== 'object') {
            return []
          }
          const effort = entry as Record<string, unknown>
          return typeof effort.reasoningEffort === 'string'
            ? [
                {
                  id: effort.reasoningEffort,
                  label: effortLabel(effort.reasoningEffort),
                  ...(typeof effort.description === 'string'
                    ? { description: effort.description }
                    : {})
                }
              ]
            : []
        })
      : []
    models.push({
      id: model.model,
      label: model.displayName,
      ...(typeof model.description === 'string' && model.description
        ? { description: model.description }
        : {}),
      efforts,
      defaultEffort: model.defaultReasoningEffort,
      ...(model.isDefault === true ? { isDefault: true } : {})
    })
  }
  return models
}
