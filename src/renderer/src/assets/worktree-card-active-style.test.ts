import fs from 'node:fs'
import { describe, expect, it } from 'vitest'

// Why: Windows checkouts use CRLF; multi-line needles below are \n-based.
const mainCss = fs.readFileSync(new URL('./main.css', import.meta.url), 'utf8').replace(/\r\n/g, '\n')

function getCssRuleBody(selector: string): string {
  const ruleMarker = mainCss.indexOf(`\n${selector} {`)
  expect(ruleMarker).toBeGreaterThanOrEqual(0)

  const ruleStart = ruleMarker + 1
  const bodyStart = mainCss.indexOf('{', ruleStart) + 1
  const bodyEnd = mainCss.indexOf('}', bodyStart)
  return mainCss.slice(bodyStart, bodyEnd)
}

describe('worktree card active styling', () => {
  it('keeps the primary selection wash translucent so card text stays legible', () => {
    const primary = getCssRuleBody(
      "[data-worktree-card-surface][data-worktree-card-active='primary']"
    )
    const darkPrimary = getCssRuleBody(
      ".dark [data-worktree-card-surface][data-worktree-card-active='primary']"
    )

    // Test Bench redesign: primary wash moved to --ai-action-accent. Still more
    // translucent than the old foreground washes (5%/7% vs 8%/10%), so the
    // legibility guard this test exists for still holds.
    expect(primary).toContain(
      'background: color-mix(in srgb, var(--ai-action-accent) 5%, transparent)'
    )
    expect(darkPrimary).toContain(
      'background: color-mix(in srgb, var(--ai-action-accent) 7%, transparent)'
    )
  })

  it('keeps the secondary selection ring when CSS owns the active state', () => {
    const secondary = getCssRuleBody(
      "[data-worktree-card-surface][data-worktree-card-active='secondary']"
    )
    const darkSecondary = getCssRuleBody(
      ".dark [data-worktree-card-surface][data-worktree-card-active='secondary']"
    )

    expect(secondary).toContain('var(--sidebar-ring) 15%')
    expect(darkSecondary).toContain('var(--sidebar-ring) 18%')
  })
})
