// @vitest-environment happy-dom

import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { SessionOptionDescriptor } from '../../../../shared/native-chat-session-options'
import { TooltipProvider } from '../ui/tooltip'
import { NativeChatSessionOptionPickers } from './NativeChatSessionOptionPickers'

const model: SessionOptionDescriptor = {
  id: 'model',
  label: 'Model',
  category: 'model',
  kind: {
    type: 'select',
    currentValue: 'opus',
    choices: [
      { value: 'opus', label: 'Opus 4.8' },
      { value: 'sonnet', label: 'Sonnet 5' }
    ]
  },
  valueSource: 'applied',
  settable: true
}

const effort: SessionOptionDescriptor = {
  id: 'effort',
  label: 'Effort',
  category: 'thought_level',
  kind: {
    type: 'select',
    currentValue: 'high',
    choices: [
      { value: 'low', label: 'Low' },
      { value: 'high', label: 'High' }
    ]
  },
  valueSource: 'applied',
  settable: true
}

const surface = {
  getSnapshot: vi.fn(() => []),
  setOption: vi.fn(),
  invokeAction: vi.fn(),
  subscribe: vi.fn(() => vi.fn())
}

afterEach(() => cleanup())

describe('NativeChatSessionOptionPickers popup handoff', () => {
  it('switches directly from Model to Effort without restoring focus to Model', async () => {
    const user = userEvent.setup()
    render(
      <TooltipProvider>
        <NativeChatSessionOptionPickers
          surface={surface}
          snapshot={[model, effort]}
          isWorking={false}
        />
      </TooltipProvider>
    )

    const modelTrigger = screen.getByRole('button', { name: 'Model Opus 4.8' })
    const effortTrigger = screen.getByRole('button', { name: 'Effort: High' })

    await user.click(modelTrigger)
    expect(screen.getByRole('menu')).not.toBeNull()

    await user.click(effortTrigger)

    await waitFor(() => {
      expect(document.querySelector('.native-chat-effort-popover')).not.toBeNull()
      expect(screen.queryByRole('menu')).toBeNull()
      expect(document.activeElement).not.toBe(modelTrigger)
    })
  })
})
