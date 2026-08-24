import { describe, expect, it, vi } from 'vitest'
import {
  findSectionEntry,
  getSettingsSectionRegistry,
  prefetchDefaultSections
} from './settings-section-registry'

// PERF-03 stage B / T1: the registry is the canonical lazy-loader map. It must
// cover every nav section, never load eagerly, dedupe prefetches, and fall
// back to the repository pane for repo-* deep links.
const NAV_SECTION_IDS = [
  'general',
  'setup-guide',
  'agents',
  'accounts',
  'orchestration',
  'linear',
  'computer-use',
  'voice',
  'mcode-account',
  'integrations',
  'mobile',
  'automations',
  'artifacts',
  'share-skills',
  'git',
  'tasks',
  'terminal',
  'quick-commands',
  'browser',
  'mobile-emulator',
  'floating-workspace',
  'appearance',
  'input',
  'notifications',
  'shortcuts',
  'stats',
  'ssh',
  'servers',
  'developer-permissions',
  'privacy',
  'advanced',
  'dev',
  'experimental',
  'plugins'
]

describe('settings section registry', () => {
  it('covers every nav section id', () => {
    const registry = getSettingsSectionRegistry()
    const missing = NAV_SECTION_IDS.filter((id) => !registry.has(id))
    expect(missing).toEqual([])
  })

  it('does not call any loader when the registry is built or read', () => {
    for (const [, sectionEntry] of getSettingsSectionRegistry()) {
      expect(sectionEntry.peekLoadCount()).toBe(0)
    }
    findSectionEntry('terminal')
    findSectionEntry('repo-abc')
    for (const [, sectionEntry] of getSettingsSectionRegistry()) {
      expect(sectionEntry.peekLoadCount()).toBe(0)
    }
  })

  it('falls back to the repository pane for repo-* deep links', () => {
    expect(findSectionEntry('repo-abc-123')?.id).toBe('repository')
    expect(findSectionEntry('does-not-exist')).toBeNull()
  })

  it('dedupes the default-section idle prefetch into one loader call', async () => {
    vi.stubGlobal('requestIdleCallback', undefined)
    vi.stubGlobal(
      'setTimeout',
      vi.fn((callback: () => void) => {
        callback()
        return 0
      })
    )
    try {
      const general = findSectionEntry('general')
      expect(general).not.toBeNull()
      prefetchDefaultSections()
      prefetchDefaultSections()
      expect(general?.peekLoadCount()).toBe(1)
      // Other sections stay untouched by the default prefetch.
      const terminal = findSectionEntry('terminal')
      expect(terminal?.peekLoadCount()).toBe(0)
    } finally {
      vi.unstubAllGlobals()
    }
  })
})
