import { describe, expect, it, vi } from 'vitest'

const gateMock = vi.hoisted(() => vi.fn<(args: { agent: string; filePath: string }) => Promise<boolean>>())
const backgroundMock = vi.hoisted(() => vi.fn())

vi.mock('./session-transcript-path-gate', () => ({
  isTranscriptPathInsideAgentRoots: (args: { agent: string; filePath: string }) => gateMock(args)
}))
vi.mock('./session-scanner-background', () => ({
  resolveAiVaultSessionTitlesInBackground: backgroundMock
}))

import { resolveLocalAiVaultSessionTitles } from './session-title-resolver'
import type { AiVaultSessionTitleRequest } from '../../shared/ai-vault-session-title'

describe('resolveLocalAiVaultSessionTitles', () => {
  it('drops a transcriptPath outside the agent roots down to the id-only shape', async () => {
    gateMock.mockResolvedValue(false)
    backgroundMock.mockResolvedValue({ titles: [] })

    await resolveLocalAiVaultSessionTitles([
      { agent: 'claude', sessionId: 'ses_1', transcriptPath: '/etc/passwd.jsonl' }
    ])

    expect(gateMock).toHaveBeenCalledWith({ agent: 'claude', filePath: '/etc/passwd.jsonl' })
    expect(backgroundMock).toHaveBeenCalledWith([{ agent: 'claude', sessionId: 'ses_1' }], undefined)
  })

  it('keeps a transcriptPath inside the agent roots intact', async () => {
    gateMock.mockResolvedValue(true)
    backgroundMock.mockResolvedValue({ titles: [] })
    const transcriptPath = '/home/ada/.codex/sessions/ses_2.jsonl'

    await resolveLocalAiVaultSessionTitles([{ agent: 'codex', sessionId: 'ses_2', transcriptPath }])

    expect(gateMock).toHaveBeenCalledWith({ agent: 'codex', filePath: transcriptPath })
    expect(backgroundMock).toHaveBeenCalledWith(
      [{ agent: 'codex', sessionId: 'ses_2', transcriptPath }],
      undefined
    )
  })

  it('never gates requests without a transcriptPath', async () => {
    backgroundMock.mockClear()
    gateMock.mockClear()
    const request: AiVaultSessionTitleRequest = { agent: 'claude', sessionId: 'ses_3' }

    await resolveLocalAiVaultSessionTitles([request])

    expect(gateMock).not.toHaveBeenCalled()
    expect(backgroundMock).toHaveBeenCalledWith([request], undefined)
  })
})
