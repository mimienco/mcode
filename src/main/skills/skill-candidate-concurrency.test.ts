import { describe, expect, it } from 'vitest'
import { runSkillCandidateTasks } from './skill-candidate-concurrency'

describe('runSkillCandidateTasks', () => {
  it('bounds concurrency and preserves task order', async () => {
    let active = 0
    let maxActive = 0
    const results = await runSkillCandidateTasks(
      Array.from({ length: 12 }, (_, index) => async () => {
        active += 1
        maxActive = Math.max(maxActive, active)
        await new Promise((resolve) => setTimeout(resolve, 12 - index))
        active -= 1
        return index
      })
    )

    expect(maxActive).toBe(4)
    expect(results).toEqual(Array.from({ length: 12 }, (_, index) => index))
  })
  it('yields to the event loop while immediately resolved candidates drain', async () => {
    let timerRan = false
    const timer = setImmediate(() => {
      timerRan = true
    })

    try {
      await runSkillCandidateTasks(Array.from({ length: 32 }, () => async () => undefined))
      expect(timerRan).toBe(true)
    } finally {
      clearImmediate(timer)
    }
  })
})
