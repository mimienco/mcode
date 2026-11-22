import { describe, expect, it } from 'vitest'
import { getAgentSessionOptionCatalog } from '../../../../shared/agent-session-option-catalog'
import { readOmpSessionOptionsFromTerminalScreen } from './omp-terminal-session-options'

describe('readOmpSessionOptionsFromTerminalScreen', () => {
  it('recovers OMP model and effort from its live status line', () => {
    expect(
      readOmpSessionOptionsFromTerminalScreen(
        'tips\r\npi > [high] GPT-5.6-Sol > [D] …2-82d2-c726b9ee1669 > (sub) > 7%',
        getAgentSessionOptionCatalog('omp')!.models
      )
    ).toEqual({ model: 'openai-codex/gpt-5.6-sol', effort: 'high' })
  })

  it('recovers OMP state from the ANSI-colored terminal buffer', () => {
    expect(
      readOmpSessionOptionsFromTerminalScreen(
        '\u001b[38;2;102;102;102mpi \u001b[39m > \u001b[38;2;0;255;0m[high] GPT-5.6-Sol\u001b[39m \u001b[38;2;102;102;102m>\u001b[39m [D] workspace',
        getAgentSessionOptionCatalog('omp')!.models
      )
    ).toEqual({ model: 'openai-codex/gpt-5.6-sol', effort: 'high' })
  })
})
