import { EventEmitter } from 'node:events'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  handlers,
  store,
  showSaveDialogMock,
  openMock,
  resetFilesystemIpcMocks
} from './filesystem-test-harness'

vi.mock('electron', async () => (await import('./filesystem-test-harness')).electronMock)
vi.mock('fs/promises', async () => (await import('./filesystem-test-harness')).fsPromisesMock)
vi.mock(
  '../wsl-unc-delete',
  async () => (await import('./filesystem-test-harness')).wslUncDeleteMock
)
vi.mock(
  '../crash-reporting/crash-breadcrumb-store',
  async () => (await import('./filesystem-test-harness')).crashBreadcrumbMock
)
vi.mock(
  '../local-downloaded-folder-promotion',
  async () => (await import('./filesystem-test-harness')).folderPromotionMock
)
vi.mock(
  '../git/status',
  async () => (await import('./filesystem-test-harness')).gitStatusModuleMock
)
vi.mock(
  '../git/check-ignored-paths',
  async () => (await import('./filesystem-test-harness')).gitIgnoredPathsMock
)
vi.mock('../git/worktree', async () => (await import('./filesystem-test-harness')).gitWorktreeMock)
vi.mock(
  '../providers/ssh-filesystem-dispatch',
  async () => (await import('./filesystem-test-harness')).sshFilesystemDispatchMock
)

import { registerFilesystemHandlers } from './filesystem'

// PERF-14: each streamed download used to pin a fresh sender.once('destroyed')
// listener until renderer destruction. One listener must serve any number of
// transfers and be removed once the sender has no transfers left.
describe('download session destroyed-listener lifetime', () => {
  const sender = Object.assign(new EventEmitter(), { id: 42 })

  beforeEach(() => {
    sender.removeAllListeners()
    resetFilesystemIpcMocks()
    showSaveDialogMock.mockResolvedValue({ canceled: false, filePath: '/downloads/movie.mp4' })
    openMock.mockResolvedValue({ close: vi.fn().mockResolvedValue(undefined), writeFile: vi.fn() })
    registerFilesystemHandlers(store as never)
  })

  async function startTransfer(): Promise<string> {
    const result = (await handlers.get('fs:startDownloadedFile')!(
      { sender },
      { suggestedName: 'movie.mp4' }
    )) as { canceled: boolean; transferId?: string }
    if (result.canceled || !result.transferId) {
      throw new Error('expected save dialog to succeed')
    }
    return result.transferId
  }

  it('keeps a single destroyed listener across concurrent transfers and drops it when idle', async () => {
    const transferIds: string[] = []
    for (let i = 0; i < 100; i += 1) {
      transferIds.push(await startTransfer())
    }
    expect(sender.listenerCount('destroyed')).toBe(1)

    await Promise.all(
      transferIds.map((transferId) =>
        handlers.get('fs:cancelDownloadedFile')!(null, { transferId })
      )
    )
    expect(sender.listenerCount('destroyed')).toBe(0)
  })

  it('removes the listener again after the last transfer finishes normally', async () => {
    const transferId = await startTransfer()
    expect(sender.listenerCount('destroyed')).toBe(1)
    await handlers.get('fs:cancelDownloadedFile')!(null, { transferId })
    expect(sender.listenerCount('destroyed')).toBe(0)

    // A later transfer re-arms exactly one listener.
    await startTransfer()
    expect(sender.listenerCount('destroyed')).toBe(1)
  })
})
