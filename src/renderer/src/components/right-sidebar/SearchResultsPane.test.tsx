// @vitest-environment happy-dom
import '@testing-library/jest-dom/vitest'

import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { RefObject } from 'react'

const detectInstallCommandMock = vi.hoisted(() => vi.fn(async () => 'winget install ripgrep'))

vi.mock('../../../../shared/quick-open-install-rg', () => ({
  detectInstallCommand: detectInstallCommandMock
}))

import { SearchResultsPane } from './SearchResultsPane'
import type { SearchResult } from '../../../../shared/code-search-types'

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

const scrollRef: RefObject<HTMLDivElement | null> = { current: null }

function renderPane(results: SearchResult | null, query: string): void {
  render(
    <SearchResultsPane
      results={results}
      hasCommittedResults
      query={query}
      loading={false}
      rows={[]}
      scrollRef={scrollRef}
      onToggleCollapsedFile={() => {}}
      onMatchClick={() => {}}
    />
  )
}

describe('SearchResultsPane', () => {
  it('shows an install hint when rg is unavailable and results are empty', async () => {
    renderPane({ files: [], totalMatches: 0, truncated: false, rgUnavailable: true }, 'needle')

    expect(
      await screen.findByText(/ripgrep \(rg\) is not installed or not on PATH/)
    ).toBeInTheDocument()
    expect(await screen.findByText('winget install ripgrep')).toBeInTheDocument()
  })

  it('shows no hint when rg is available, even with zero matches', () => {
    renderPane({ files: [], totalMatches: 0, truncated: false }, 'needle')

    expect(screen.queryByText(/ripgrep \(rg\) is not installed/)).toBeNull()
  })
})
