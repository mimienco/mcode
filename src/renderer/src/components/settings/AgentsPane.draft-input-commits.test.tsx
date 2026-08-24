// @vitest-environment happy-dom

import { cleanup, fireEvent, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  AgentCommandOverrideInput,
  AgentDefaultArgsInput,
  AgentDefaultEnvInput
} from './AgentsPane'

// PERF-16: Enter used to run the commit twice (keydown handler + the blur it
// triggered). Each confirmed change must produce exactly one save call, and a
// no-change commit must be skipped entirely.
afterEach(() => {
  cleanup()
})

function renderCommandInput(onSaveOverride: (value: string) => void) {
  const { container } = render(
    <AgentCommandOverrideInput
      defaultCmd="claude"
      cmdOverride=""
      onSaveOverride={onSaveOverride}
    />
  )
  const input = container.querySelector('input')!
  input.focus()
  return input
}

function renderArgsInput(onSaveArgs: (value: string) => void) {
  const { container } = render(
    <AgentDefaultArgsInput
      defaultArgs="--verbose"
      argsOverride="--verbose"
      onSaveArgs={onSaveArgs}
    />
  )
  const input = container.querySelector('input')!
  input.focus()
  return input
}

function renderEnvInput(onSaveEnv: (value: Record<string, string>) => void) {
  const { container } = render(
    <AgentDefaultEnvInput defaultEnv={{ A: '1' }} envOverride={{ B: '2' }} onSaveEnv={onSaveEnv} />
  )
  const input = container.querySelector('input')!
  input.focus()
  return input
}

describe('agent draft input single-commit', () => {
  it('commits once on Enter and skips redundant re-commits', () => {
    const onSaveOverride = vi.fn()
    const input = renderCommandInput(onSaveOverride)

    fireEvent.change(input, { target: { value: 'claude --fast' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(onSaveOverride).toHaveBeenCalledTimes(1)
    expect(onSaveOverride).toHaveBeenLastCalledWith('claude --fast')

    // A second blur with no change must not re-commit.
    fireEvent.blur(input)
    expect(onSaveOverride).toHaveBeenCalledTimes(1)
  })

  it('commits args once on Enter and once on a genuine later change', () => {
    const onSaveArgs = vi.fn()
    const input = renderArgsInput(onSaveArgs)

    fireEvent.change(input, { target: { value: '--verbose --json' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(onSaveArgs).toHaveBeenCalledTimes(1)
    expect(onSaveArgs).toHaveBeenLastCalledWith('--verbose --json')

    fireEvent.blur(input)
    expect(onSaveArgs).toHaveBeenCalledTimes(1)

    fireEvent.change(input, { target: { value: '--quiet' } })
    fireEvent.blur(input)
    expect(onSaveArgs).toHaveBeenCalledTimes(2)
    expect(onSaveArgs).toHaveBeenLastCalledWith('--quiet')
  })

  // Why: single-line inputs normalize newlines away (DOM spec), so the draft
  // uses a space separator like stringifyAgentDefaultEnvDraft emits.
  it('commits env once on Enter and skips an unchanged blur', () => {
    const onSaveEnv = vi.fn()
    const input = renderEnvInput(onSaveEnv)

    fireEvent.change(input, { target: { value: 'C=3 D=4' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(onSaveEnv).toHaveBeenCalledTimes(1)
    expect(onSaveEnv).toHaveBeenLastCalledWith({ C: '3', D: '4' })

    fireEvent.blur(input)
    expect(onSaveEnv).toHaveBeenCalledTimes(1)
  })
})
