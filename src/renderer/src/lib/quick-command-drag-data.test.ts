// @vitest-environment happy-dom

import { describe, expect, it, vi } from 'vitest'
import {
  QUICK_COMMAND_DRAG_MIME,
  finishQuickCommandDrag,
  hasQuickCommandDragData,
  markQuickCommandDropHandled,
  readQuickCommandDragData,
  subscribeQuickCommandDragFinish,
  writeQuickCommandDragData
} from './quick-command-drag-data'

function createDataTransfer(): DataTransfer {
  const values = new Map<string, string>()
  return {
    effectAllowed: 'uninitialized',
    dropEffect: 'none',
    get types() {
      return [...values.keys()]
    },
    getData: vi.fn((type: string) => values.get(type) ?? ''),
    setData: vi.fn((type: string, value: string) => {
      values.set(type, value)
    })
  } as unknown as DataTransfer
}

describe('quick command drag data', () => {
  it('round-trips a bounded internal payload and publishes plain text interoperability', () => {
    const dataTransfer = createDataTransfer()

    expect(writeQuickCommandDragData(dataTransfer, 'Review changes', 'git diff --check')).toBe(true)

    expect(dataTransfer.effectAllowed).toBe('copy')
    expect(dataTransfer.getData('text/plain')).toBe('git diff --check')
    expect(dataTransfer.types).toContain(QUICK_COMMAND_DRAG_MIME)
    expect(hasQuickCommandDragData(dataTransfer)).toBe(true)
    expect(readQuickCommandDragData(dataTransfer)).toEqual({
      label: 'Review changes',
      text: 'git diff --check'
    })
  })

  it('ignores arbitrary plain text and malformed internal payloads', () => {
    const plainText = createDataTransfer()
    plainText.setData('text/plain', 'rm -rf important')
    expect(hasQuickCommandDragData(plainText)).toBe(false)
    expect(readQuickCommandDragData(plainText)).toBeNull()

    const malformed = createDataTransfer()
    malformed.setData(QUICK_COMMAND_DRAG_MIME, '{bad json')
    expect(readQuickCommandDragData(malformed)).toBeNull()
  })

  it('lets an eligible destination accept the Electron dragend fallback', () => {
    writeQuickCommandDragData(createDataTransfer(), 'Status', 'git status')
    const destination = vi.fn(() => true)
    const release = subscribeQuickCommandDragFinish(destination)

    expect(finishQuickCommandDrag(12, 34)).toBe(true)
    expect(destination).toHaveBeenCalledWith(
      { label: 'Status', text: 'git status' },
      { clientX: 12, clientY: 34 }
    )
    release()
  })

  it('does not treat a rejected fallback destination as a successful drop', () => {
    writeQuickCommandDragData(createDataTransfer(), 'Status', 'git status')
    const release = subscribeQuickCommandDragFinish(() => false)

    expect(finishQuickCommandDrag(12, 34)).toBe(false)
    release()
  })

  it('does not dispatch a fallback after a real destination handled the drop', () => {
    writeQuickCommandDragData(createDataTransfer(), 'Status', 'git status')
    const destination = vi.fn(() => true)
    const release = subscribeQuickCommandDragFinish(destination)
    markQuickCommandDropHandled()

    expect(finishQuickCommandDrag(12, 34)).toBe(true)
    expect(destination).not.toHaveBeenCalled()
    release()
  })

  it('refuses empty or oversized text at both write and read boundaries', () => {
    const empty = createDataTransfer()
    expect(writeQuickCommandDragData(empty, 'Empty', '')).toBe(false)

    const oversized = createDataTransfer()
    const text = 'x'.repeat(6001)
    expect(writeQuickCommandDragData(oversized, 'Too large', text)).toBe(false)

    oversized.setData(
      QUICK_COMMAND_DRAG_MIME,
      JSON.stringify({ version: 1, label: 'Too large', text })
    )
    expect(readQuickCommandDragData(oversized)).toBeNull()
  })
})
