import { extname } from 'node:path'
import { hasUnsafeProviderSessionIdChars } from '../../shared/agent-session-resume'
import {
  AI_VAULT_SESSION_TITLE_REQUEST_MAX_COUNT,
  type AiVaultSessionTitleRequest,
  type AiVaultSessionTitlesResult
} from '../../shared/ai-vault-session-title'
import { resolveAiVaultSessionTitlesInBackground } from './session-scanner-background'
import { isTranscriptPathInsideAgentRoots } from './session-transcript-path-gate'

const TRANSCRIPT_PATH_MAX_LENGTH = 32_768

function normalizeRequest(request: AiVaultSessionTitleRequest): AiVaultSessionTitleRequest | null {
  const sessionId = request.sessionId.trim()
  if (!sessionId || sessionId.length > 512 || hasUnsafeProviderSessionIdChars(sessionId)) {
    return null
  }
  const transcriptPath = request.transcriptPath?.trim()
  if (
    !transcriptPath ||
    transcriptPath.length > TRANSCRIPT_PATH_MAX_LENGTH ||
    hasUnsafeProviderSessionIdChars(transcriptPath) ||
    extname(transcriptPath).toLowerCase() !== '.jsonl'
  ) {
    return { agent: request.agent, sessionId }
  }
  return { agent: request.agent, sessionId, transcriptPath }
}

export async function resolveLocalAiVaultSessionTitles(
  requests: AiVaultSessionTitleRequest[],
  signal?: AbortSignal
): Promise<AiVaultSessionTitlesResult> {
  const deduped = new Map<string, AiVaultSessionTitleRequest>()
  for (const request of requests.slice(0, AI_VAULT_SESSION_TITLE_REQUEST_MAX_COUNT)) {
    const normalized = normalizeRequest(request)
    if (!normalized) {
      continue
    }
    const key = `${normalized.agent}\0${normalized.sessionId}`
    const previous = deduped.get(key)
    if (!previous?.transcriptPath || normalized.transcriptPath) {
      deduped.set(key, normalized)
    }
  }
  // Why: transcriptPath is renderer-supplied; anything outside the agent's
  // known session roots degrades to the id-only shape before any reader can
  // stat or open it.
  const gated = await Promise.all(
    [...deduped.values()].map(async (request): Promise<AiVaultSessionTitleRequest> => {
      if (
        !request.transcriptPath ||
        (await isTranscriptPathInsideAgentRoots({
          agent: request.agent,
          filePath: request.transcriptPath
        }))
      ) {
        return request
      }
      return { agent: request.agent, sessionId: request.sessionId }
    })
  )
  return resolveAiVaultSessionTitlesInBackground(gated, signal)
}
