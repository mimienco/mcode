// @vitest-environment happy-dom

import { act, cleanup, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { SearchResult } from '../../../../shared/code-search-types'
import type { ReplaceSummary } from '../../../../shared/code-replace-types'
import { useFileSearchReplace } from './use-file-search-replace'

const mocks = vi.hoisted(() => ({
  getConnectionId: vi.fn(),
  getRuntimeSettings: vi.fn(),
  replaceRuntimeFiles: vi.fn(),
  toastSuccess: vi.fn(),
  toastWarning: vi.fn(),
  toastError: vi.fn(),
  storeGetState: vi.fn(),
  useAppStoreImpl: vi.fn()
}))
vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string, options?: Record<string, string>) =>
    fallback.replace(/\{\{value(\d+)\}\}/g, (_m, index: string) => options?.[`value${index}`] ?? '')
}))
vi.mock('@/lib/connection-context', () => ({
  getConnectionId: mocks.getConnectionId
}))

vi.mock('./file-explorer-runtime-owner', () => ({
  getRightSidebarWorktreeRuntimeSettings: mocks.getRuntimeSettings
}))

vi.mock('@/runtime/runtime-file-client', () => ({
  replaceRuntimeFiles: mocks.replaceRuntimeFiles
}))

vi.mock('sonner', () => ({
  toast: {
    success: mocks.toastSuccess,
    warning: mocks.toastWarning,
    error: mocks.toastError
  }
}))

vi.mock('@/store', () => ({
  useAppStore: Object.assign(
    (selector: (state: Record<string, unknown>) => unknown) =>
      mocks.useAppStoreImpl(selector),
    { getState: mocks.storeGetState }
  )
}))

const RESULTS: SearchResult = {
  files: [
    {
      filePath: '/repo/src/a.ts',
      relativePath: 'src/a.ts',
      matches: [{ line: 1, column: 0, matchLength: 3, lineContent: 'foo' }],
      matchCount: 2
    },
    {
      filePath: '/repo/docs/b.md',
      relativePath: 'docs/b.md',
      matches: [{ line: 4, column: 1, matchLength: 3, lineContent: 'foo' }],
      matchCount: 1
    }
  ],
  totalMatches: 3,
  truncated: false
}

const SUMMARY: ReplaceSummary = {
  outcomes: [{ relativePath: 'src/a.ts', status: 'written', occurrences: 2 }],
  writtenFiles: 1,
  writtenOccurrences: 2,
  skippedFiles: 0,
  failedFiles: 0
}

type StoreShape = {
  fileSearchStateByWorktree: Record<string, unknown>
  updateFileSearchState: ReturnType<typeof vi.fn>
  openFiles: { worktreeId: string; filePath: string; isDirty: boolean; isUntitled?: boolean }[]
}

function makeStore(overrides: Partial<StoreShape> = {}): StoreShape {
  return {
    fileSearchStateByWorktree: {
      wt1: { replaceOpen: false, replaceValue: 'bar', preserveCase: 'none' }
    },
    updateFileSearchState: vi.fn(),
    openFiles: [],
    ...overrides
  }
}

function renderReplaceHook(
  store: StoreShape,
  args: Partial<Parameters<typeof useFileSearchReplace>[0]> = {}
) {
  mocks.useAppStoreImpl.mockImplementation(
    (selector: (state: StoreShape) => unknown) => selector(store)
  )
  mocks.storeGetState.mockReturnValue(store)
  mocks.getRuntimeSettings.mockReturnValue({ activeRuntimeEnvironmentId: null })
  mocks.getConnectionId.mockReturnValue(undefined)
  mocks.replaceRuntimeFiles.mockResolvedValue(SUMMARY)

  const hook = renderHook(() =>
    useFileSearchReplace({
      activeWorktreeId: 'wt1',
      worktreePath: '/repo',
      results: RESULTS,
      query: 'foo',
      caseSensitive: false,
      wholeWord: false,
      useRegex: false,
      rerunSearch: vi.fn(),
      ...args
    })
  )
  return { hook, rerunSearch: args.rerunSearch ?? vi.fn() }
}

describe('useFileSearchReplace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    cleanup()
  })

  it('captures the occurrence total when replace is requested', () => {
    const store = makeStore()
    const { hook } = renderReplaceHook(store)
    act(() => hook.result.current.onRequestReplace())
    expect(hook.result.current.confirmOccurrences).toBe(3)
  })

  it('does not request replace without results or a replacement value', () => {
    const store = makeStore()
    const { hook } = renderReplaceHook(store, { results: null })
    act(() => hook.result.current.onRequestReplace())
    expect(hook.result.current.confirmOccurrences).toBeNull()

    const emptyValueStore = makeStore({
      fileSearchStateByWorktree: { wt1: { replaceOpen: true, replaceValue: '', preserveCase: 'none' } }
    })
    const second = renderReplaceHook(emptyValueStore)
    act(() => second.hook.result.current.onRequestReplace())
    expect(second.hook.result.current.confirmOccurrences).toBeNull()
  })

  it('runs replace with dirty editor tabs excluded and announces the summary', async () => {
    const store = makeStore({
      openFiles: [
        { worktreeId: 'wt1', filePath: '/repo/src/a.ts', isDirty: true },
        { worktreeId: 'wt1', filePath: '/repo/other.ts', isDirty: false },
        { worktreeId: 'wt2', filePath: '/elsewhere/dirty.ts', isDirty: true }
      ]
    })
    const rerunSearch = vi.fn()
    const { hook } = renderReplaceHook(store, { rerunSearch })

    act(() => hook.result.current.onRequestReplace())
    await act(async () => hook.result.current.onConfirmReplace())

    expect(mocks.replaceRuntimeFiles).toHaveBeenCalledTimes(1)
    const [, params] = mocks.replaceRuntimeFiles.mock.calls[0]
    expect(params.skipPaths).toEqual(['/repo/src/a.ts'])
    expect(params.targets).toHaveLength(2)
    expect(params.replacement).toBe('bar')
    expect(mocks.toastSuccess).toHaveBeenCalledWith(
      expect.stringContaining('Replaced 2')
    )
    expect(rerunSearch).toHaveBeenCalled()
    expect(hook.result.current.confirmOccurrences).toBeNull()
  })

  it('reports failures through the error toast and skips rerun', async () => {
    const store = makeStore()
    const rerunSearch = vi.fn()
    const { hook } = renderReplaceHook(store, { rerunSearch })
    // Why: set after render — renderReplaceHook seeds a resolved default.
    mocks.replaceRuntimeFiles.mockRejectedValue(new Error('remote host outdated'))

    act(() => hook.result.current.onRequestReplace())
    await act(async () => hook.result.current.onConfirmReplace())

    expect(mocks.toastError).toHaveBeenCalledWith('remote host outdated')
    expect(rerunSearch).not.toHaveBeenCalled()
  })

  it('warns instead of succeeding when files failed', async () => {
    const store = makeStore()
    const { hook } = renderReplaceHook(store)
    // Why: set after render — renderReplaceHook seeds a resolved default.
    mocks.replaceRuntimeFiles.mockResolvedValue({
      ...SUMMARY,
      failedFiles: 1,
      outcomes: [
        ...SUMMARY.outcomes,
        { relativePath: 'docs/b.md', status: 'failed', reason: 'write-error' }
      ]
    })

    act(() => hook.result.current.onRequestReplace())
    await act(async () => hook.result.current.onConfirmReplace())

    expect(mocks.toastWarning).toHaveBeenCalled()
    expect(mocks.toastSuccess).not.toHaveBeenCalled()
  })
  it('lists dirty-tab skips inside the announcement', async () => {
    const store = makeStore({
      openFiles: [{ worktreeId: 'wt1', filePath: '/repo/src/a.ts', isDirty: true }]
    })
    const { hook } = renderReplaceHook(store)
    // Why: set after render — renderReplaceHook seeds a resolved default.
    mocks.replaceRuntimeFiles.mockResolvedValue({
      outcomes: [
        { relativePath: 'src/a.ts', status: 'skipped', reason: 'dirty-editor-tab' },
        { relativePath: 'docs/b.md', status: 'written', occurrences: 1 }
      ],
      writtenFiles: 1,
      writtenOccurrences: 1,
      skippedFiles: 1,
      failedFiles: 0
    })

    act(() => hook.result.current.onRequestReplace())
    await act(async () => hook.result.current.onConfirmReplace())

    expect(mocks.toastSuccess).toHaveBeenCalledWith(
      expect.stringContaining('1 skipped')
    )
  })

  it('toggles open state and writes value changes through the store', () => {
    const store = makeStore()
    const { hook } = renderReplaceHook(store)

    act(() => hook.result.current.onOpenToggle())
    expect(store.updateFileSearchState).toHaveBeenCalledWith('wt1', { replaceOpen: true })

    act(() => hook.result.current.onValueChange('baz'))
    expect(store.updateFileSearchState).toHaveBeenCalledWith('wt1', { replaceValue: 'baz' })

    act(() => hook.result.current.onPreserveCaseToggle())
    expect(store.updateFileSearchState).toHaveBeenCalledWith('wt1', { preserveCase: 'match' })
  })

  it('cancels a pending confirmation', () => {
    const store = makeStore()
    const { hook } = renderReplaceHook(store)
    act(() => hook.result.current.onRequestReplace())
    act(() => hook.result.current.onCancelConfirm())
    expect(hook.result.current.confirmOccurrences).toBeNull()
    expect(mocks.replaceRuntimeFiles).not.toHaveBeenCalled()
  })
})

