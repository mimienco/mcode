import { describe, expect, it, vi } from 'vitest'

const gateMock = vi.hoisted(() => vi.fn<(args: { agent: string; filePath: string }) => Promise<boolean>>())
const backgroundReadMock = vi.hoisted(() => vi.fn())

vi.mock('./session-transcript-path-gate', () => ({
  isTranscriptPathInsideAgentRoots: (args: { agent: string; filePath: string }) => gateMock(args)
}))
vi.mock('./session-scanner-background', () => ({
  readAiVaultFirstUserPromptInBackground: backgroundReadMock
}))

import { handleAiVaultGetFirstUserPrompt } from './session-first-user-prompt-handler'

describe('handleAiVaultGetFirstUserPrompt', () => {
  it('resolves null without reading when the path is outside every agent root', async () => {
    gateMock.mockResolvedValue(false)

    await expect(
      handleAiVaultGetFirstUserPrompt({
        agent: 'opencode',
        filePath: '/home/ada/evil/opencode.db',
        sessionId: 'ses_1'
      })
    ).resolves.toEqual({ prompt: null })

    expect(gateMock).toHaveBeenCalledWith({ agent: 'opencode', filePath: '/home/ada/evil/opencode.db' })
    expect(backgroundReadMock).not.toHaveBeenCalled()
  })

  it('delegates to the background reader once containment passes', async () => {
    gateMock.mockResolvedValue(true)
    backgroundReadMock.mockResolvedValue({ prompt: 'ship it' })

    await expect(
      handleAiVaultGetFirstUserPrompt({
        agent: 'claude',
        filePath: '/home/ada/.claude/projects/repo/ses_1.jsonl'
      })
    ).resolves.toEqual({ prompt: 'ship it' })

    expect(backgroundReadMock).toHaveBeenCalledWith({
      agent: 'claude',
      filePath: '/home/ada/.claude/projects/repo/ses_1.jsonl',
      sessionId: undefined,
      executionHostId: undefined,
      codexHome: undefined
    })
  })

  it('resolves null for malformed payloads without consulting the gate', async () => {
    gateMock.mockClear()
    await expect(handleAiVaultGetFirstUserPrompt(undefined)).resolves.toEqual({ prompt: null })
    await expect(handleAiVaultGetFirstUserPrompt({ agent: 7 } as never)).resolves.toEqual({ prompt: null })

    expect(gateMock).not.toHaveBeenCalled()
  })
})
