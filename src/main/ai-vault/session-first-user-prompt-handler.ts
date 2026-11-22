import type {
  AiVaultAgent,
  AiVaultFirstUserPromptArgs,
  AiVaultFirstUserPromptResult
} from '../../shared/ai-vault-types'
import { readAiVaultFirstUserPromptInBackground } from './session-scanner-background'
import { isTranscriptPathInsideAgentRoots } from './session-transcript-path-gate'

export async function handleAiVaultGetFirstUserPrompt(
  args?: AiVaultFirstUserPromptArgs
): Promise<AiVaultFirstUserPromptResult> {
  if (!args || typeof args.filePath !== 'string' || typeof args.agent !== 'string') {
    return Promise.resolve({ prompt: null })
  }
  // Why: the path is renderer-supplied; only read transcripts under the
  // agent's known WSL-aware session roots so a crafted path can't make the
  // main process parse arbitrary files — including opening arbitrary SQLite
  // databases on the opencode leg.
  if (
    !(await isTranscriptPathInsideAgentRoots({
      agent: args.agent as AiVaultAgent,
      filePath: args.filePath
    }))
  ) {
    return { prompt: null }
  }
  return readAiVaultFirstUserPromptInBackground({
    agent: args.agent,
    filePath: args.filePath,
    sessionId: typeof args.sessionId === 'string' ? args.sessionId : undefined,
    executionHostId: args.executionHostId,
    codexHome: args.codexHome
  })
}
