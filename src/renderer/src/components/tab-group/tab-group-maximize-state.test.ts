import { afterEach, describe, expect, it } from 'vitest'
import {
  getMaximizedTabGroupId,
  resetMaximizedTabGroupsForTests,
  setMaximizedTabGroupId
} from './tab-group-maximize-state'

afterEach(resetMaximizedTabGroupsForTests)

describe('tab group maximize state', () => {
  it('tracks the maximized middle pane independently per worktree', () => {
    setMaximizedTabGroupId('wt-1', 'group-top')
    setMaximizedTabGroupId('wt-2', 'group-bottom')

    expect(getMaximizedTabGroupId('wt-1')).toBe('group-top')
    expect(getMaximizedTabGroupId('wt-2')).toBe('group-bottom')

    setMaximizedTabGroupId('wt-1', null)

    expect(getMaximizedTabGroupId('wt-1')).toBeNull()
    expect(getMaximizedTabGroupId('wt-2')).toBe('group-bottom')
  })
})
