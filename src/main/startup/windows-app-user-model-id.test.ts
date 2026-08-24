import { describe, expect, it, vi } from 'vitest'
import { setWindowsAppUserModelId } from './windows-app-user-model-id'

describe('setWindowsAppUserModelId', () => {
  it('preserves the resolved Windows identity instead of substituting the executable path', () => {
    const setAppUserModelId = vi.fn()

    setWindowsAppUserModelId(
      { setAppUserModelId },
      { appUserModelId: 'com.mcode.desktop.dev.0123456789' },
      'win32'
    )

    expect(setAppUserModelId).toHaveBeenCalledWith('com.mcode.desktop.dev.0123456789')
  })

  it('does nothing outside Windows', () => {
    const setAppUserModelId = vi.fn()

    setWindowsAppUserModelId(
      { setAppUserModelId },
      { appUserModelId: 'com.mcode.desktop' },
      'darwin'
    )

    expect(setAppUserModelId).not.toHaveBeenCalled()
  })
})
