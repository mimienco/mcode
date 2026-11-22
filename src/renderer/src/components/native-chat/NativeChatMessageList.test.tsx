// @vitest-environment happy-dom

import '@testing-library/jest-dom/vitest'

import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import type { NativeChatSession } from '../../../../shared/native-chat-types'
import { NativeChatMessageList } from './NativeChatMessageList'

afterEach(cleanup)

describe('NativeChatMessageList images', () => {
  it('renders generated assistant images as expandable previews', () => {
    const session: NativeChatSession = {
      agent: 'codex',
      sessionId: 'session-1',
      status: 'ready',
      messages: [
        {
          id: 'generated-image-1',
          role: 'assistant',
          timestamp: 1,
          source: 'transcript',
          blocks: [
            {
              type: 'image-ref',
              path: 'C:\\generated_images\\generated-image-1.png',
              url: 'data:image/png;base64,aW1hZ2UtYnl0ZXM=',
              alt: 'Generated image'
            }
          ]
        }
      ]
    }

    render(
      <NativeChatMessageList
        session={{
          ...session,
          hasMore: false,
          loadingEarlier: false,
          loadEarlier: () => undefined,
          readPhase: 'ready'
        }}
        isWorking={false}
        expandSignal={false}
        fontScale={1}
      />
    )

    expect(screen.getByRole('img', { name: 'Generated image' })).toHaveAttribute(
      'src',
      'data:image/png;base64,aW1hZ2UtYnl0ZXM='
    )
    expect(screen.getByRole('button', { name: 'Expand image' })).toBeInTheDocument()
  })

  it('structures user and assistant turns inside the focused reading column', () => {
    const session: NativeChatSession = {
      agent: 'codex',
      sessionId: 'session-1',
      status: 'ready',
      messages: [
        {
          id: 'user-1',
          role: 'user',
          timestamp: 1,
          source: 'transcript',
          blocks: [{ type: 'text', text: 'Please inspect the session picker.' }]
        },
        {
          id: 'assistant-1',
          role: 'assistant',
          timestamp: 2,
          source: 'transcript',
          blocks: [{ type: 'text', text: 'I found the interaction boundary.' }]
        }
      ]
    }

    const { container } = render(
      <NativeChatMessageList
        session={{
          ...session,
          hasMore: false,
          loadingEarlier: false,
          loadEarlier: () => undefined,
          readPhase: 'ready'
        }}
        isWorking={false}
        expandSignal={false}
        fontScale={1}
      />
    )

    expect(container.querySelector('[data-native-chat-transcript]')).toHaveClass(
      'max-w-3xl',
      '@5xl/native-chat:max-w-4xl'
    )
    expect(
      container.querySelector('[data-native-chat-role="user"]')?.firstElementChild
    ).toHaveClass('max-w-[min(42rem,88%)]')
    expect(container.querySelector('[data-native-chat-role="assistant"]')).toHaveClass(
      'max-w-[72ch]'
    )
    const messageActions = container.querySelector('[data-native-chat-message-actions]')
    expect(messageActions).toHaveClass('mt-2', 'border-t')
    expect(messageActions).not.toHaveClass('absolute')
    expect(screen.getByRole('button', { name: 'Copy message' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Scroll this message to top' })).toBeInTheDocument()
  })
})
