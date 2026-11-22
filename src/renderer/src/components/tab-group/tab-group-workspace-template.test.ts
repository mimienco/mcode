import { describe, expect, it, vi } from 'vitest'
import { launchTabGroupWorkspaceTemplate } from './tab-group-workspace-template'

function templateHarness() {
  const openAgent = vi.fn(async () => {})
  const openBrowser = vi.fn(async () => {})
  const openMarkdown = vi.fn(async () => {})
  const openTerminal = vi.fn(async () => {})
  const createEmptySplitGroup = vi.fn((): string | null => 'group-2')
  const closeEmptyGroup = vi.fn(() => true)

  return {
    createEmptySplitGroup,
    closeEmptyGroup,
    openAgent,
    openBrowser,
    openMarkdown,
    openTerminal,
    args: {
      agent: 'claude' as const,
      worktreeId: 'wt-1',
      sourceGroupId: 'group-1',
      createEmptySplitGroup,
      closeEmptyGroup,
      openAgent,
      openBrowser,
      openMarkdown,
      openTerminal
    }
  }
}

describe('launchTabGroupWorkspaceTemplate', () => {
  it('builds Research as Browser + agent in a right split', async () => {
    const harness = templateHarness()

    await expect(
      launchTabGroupWorkspaceTemplate({ ...harness.args, templateId: 'research' })
    ).resolves.toBe('launched')

    expect(harness.createEmptySplitGroup).toHaveBeenCalledWith('wt-1', 'group-1', 'right')
    expect(harness.openBrowser).toHaveBeenCalledWith('group-1')
    expect(harness.openAgent).toHaveBeenCalledWith('claude', 'group-2')
  })

  it('builds Build as agent + Terminal', async () => {
    const harness = templateHarness()

    await launchTabGroupWorkspaceTemplate({ ...harness.args, templateId: 'build' })

    expect(harness.openAgent).toHaveBeenCalledWith('claude', 'group-1')
    expect(harness.openTerminal).toHaveBeenCalledWith('group-2')
  })

  it('builds Review as Markdown + agent', async () => {
    const harness = templateHarness()

    await launchTabGroupWorkspaceTemplate({ ...harness.args, templateId: 'review' })

    expect(harness.openMarkdown).toHaveBeenCalledWith('group-1')
    expect(harness.openAgent).toHaveBeenCalledWith('claude', 'group-2')
  })

  it('starts independent template panes concurrently', async () => {
    const harness = templateHarness()
    const { promise: browserPending, resolve: finishBrowser } = Promise.withResolvers<void>()
    harness.openBrowser.mockReturnValueOnce(browserPending)

    const launch = launchTabGroupWorkspaceTemplate({
      ...harness.args,
      templateId: 'research'
    })

    expect(harness.openBrowser).toHaveBeenCalledWith('group-1')
    expect(harness.openAgent).toHaveBeenCalledWith('claude', 'group-2')
    finishBrowser()
    await expect(launch).resolves.toBe('launched')
  })

  it('does not open anything when a split cannot be created', async () => {
    const harness = templateHarness()
    harness.createEmptySplitGroup.mockReturnValue(null)

    await expect(
      launchTabGroupWorkspaceTemplate({ ...harness.args, templateId: 'research' })
    ).resolves.toBe('split-unavailable')

    expect(harness.openBrowser).not.toHaveBeenCalled()
    expect(harness.openAgent).not.toHaveBeenCalled()
  })

  it('removes the empty split when a template opener fails', async () => {
    const harness = templateHarness()
    harness.openAgent.mockRejectedValue(new Error('agent failed'))

    await expect(
      launchTabGroupWorkspaceTemplate({ ...harness.args, templateId: 'research' })
    ).rejects.toThrow('agent failed')

    expect(harness.closeEmptyGroup).toHaveBeenCalledWith('wt-1', 'group-2')
  })
})
