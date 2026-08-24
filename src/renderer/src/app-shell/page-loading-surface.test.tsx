import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { PageLoadingSurface } from './page-loading-surface'

// PERF-03 stage A: the loading surface replaces null Suspense fallbacks and
// must be perceivable by assistive tech while a cold chunk loads.
describe('PageLoadingSurface', () => {
  it('renders a named status surface with aria-busy instead of an empty region', () => {
    const markup = renderToStaticMarkup(<PageLoadingSurface />)
    expect(markup).toContain('aria-busy="true"')
    expect(markup).toContain('role="status"')
    expect(markup.replace(/<[^>]+>/g, '').trim().length).toBeGreaterThan(0)
  })
})
