import assert from 'node:assert/strict'
import { expect, it as test } from 'vitest'
import { DesktopCommandError } from './desktop-client'
import { createTauriAgentHooksClient } from './tauri-agent-hooks-client'
import type { AgentHookInstallStatus } from '../../../shared/agent-hook-types'

function recordedInvoke(handlers: Record<string, (args?: Record<string, unknown>) => unknown>) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    const handler = handlers[command]
    if (!handler) {
      throw new Error(`unexpected command: ${command}`)
    }
    return handler(args)
  }
  return { calls, invoke }
}

function status(
  agent: string,
  overrides: Partial<AgentHookInstallStatus> = {}
): AgentHookInstallStatus {
  return {
    agent: agent as AgentHookInstallStatus['agent'],
    state: 'skipped',
    configPath: '',
    managedHooksPresent: false,
    detail: 'not yet ported to Tauri (wave 38)',
    ...overrides
  }
}

test('each method invokes agent_hooks_status with the matching agent literal', async () => {
  const { calls, invoke } = recordedInvoke({
    agent_hooks_status: (args) => status(String(args?.agent ?? ''))
  })
  const client = createTauriAgentHooksClient(invoke)

  const expected: [keyof typeof client, string][] = [
    ['claudeStatus', 'claude'],
    ['openClaudeStatus', 'openclaude'],
    ['codexStatus', 'codex'],
    ['geminiStatus', 'gemini'],
    ['antigravityStatus', 'antigravity'],
    ['ampStatus', 'amp'],
    ['cursorStatus', 'cursor'],
    ['droidStatus', 'droid'],
    ['commandCodeStatus', 'command-code'],
    ['grokStatus', 'grok'],
    ['copilotStatus', 'copilot'],
    ['hermesStatus', 'hermes'],
    ['devinStatus', 'devin'],
    ['kimiStatus', 'kimi']
  ]

  for (const [method, agent] of expected) {
    await expect(client[method]()).resolves.toEqual(status(agent))
  }
  expect(calls.map((call) => call.args)).toEqual(expected.map(([, agent]) => ({ agent })))
  expect(calls.every((call) => call.command === 'agent_hooks_status')).toBe(true)
})

test('coerces a missing detail field to null', async () => {
  const { invoke } = recordedInvoke({
    agent_hooks_status: () => ({
      agent: 'claude',
      state: 'installed',
      configPath: '/home/.claude/settings.json',
      managedHooksPresent: true
    })
  })
  const client = createTauriAgentHooksClient(invoke)
  await expect(client.claudeStatus()).resolves.toEqual({
    agent: 'claude',
    state: 'installed',
    configPath: '/home/.claude/settings.json',
    managedHooksPresent: true,
    detail: null
  })
})

test('rejects a malformed install-status payload as DesktopCommandError', async () => {
  const { invoke } = recordedInvoke({
    agent_hooks_status: () => ({ agent: 'claude', state: 'wired' })
  })
  const client = createTauriAgentHooksClient(invoke)
  const error = await client.claudeStatus().then(
    () => null,
    (thrown: unknown) => thrown
  )
  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, 'agent_hooks_status_invalid_response')
})

test('invoke failures become DesktopCommandError', async () => {
  const { invoke } = recordedInvoke({
    agent_hooks_status: () => {
      throw {
        code: 'agent_hooks_unknown_agent',
        context: { operation: 'agent_hooks_status' },
        message: 'unknown agent hook target: bogus',
        userMessage: ''
      }
    }
  })
  const client = createTauriAgentHooksClient(invoke)
  const error = await client.claudeStatus().then(
    () => null,
    (thrown: unknown) => thrown
  )
  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, 'agent_hooks_unknown_agent')
})
