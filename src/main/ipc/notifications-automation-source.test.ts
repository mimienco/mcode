import { beforeEach, describe, expect, it, vi } from 'vitest'

import {
  getAllWindowsMock,
  getDispatchHandler,
  notificationCtorMock,
  notificationIsSupportedMock,
  resetNotificationDispatchMocks,
  setTrayAttentionMock
} from './notifications-test-harness'

vi.mock('electron', async () =>
  (await import('./notifications-test-harness')).createElectronModuleMock()
)

vi.mock('./notification-authorization-status', async () =>
  (await import('./notifications-test-harness')).createNotificationAuthorizationModuleMock()
)

vi.mock('./ui', async () =>
  (await import('./notifications-test-harness')).createTrustedUIRendererModuleMock()
)

vi.mock('../tray/system-tray', async () =>
  (await import('./notifications-test-harness')).createSystemTrayModuleMock()
)

import { registerNotificationHandlers } from './notifications'
import type { NotificationDispatchRequest } from '../../shared/notification-settings-types'

function makeRequest(overrides?: Partial<NotificationDispatchRequest>): NotificationDispatchRequest {
  return {
    source: 'automation-run-complete',
    automationName: 'Nightly review',
    automationOutcome: 'completed',
    automationError: null,
    ...overrides
  }
}

function register(storeOverrides?: Record<string, unknown>, runtime?: unknown): void {
  registerNotificationHandlers(
    {
      getSettings: () => ({
        notifications: {
          enabled: true,
          agentTaskComplete: true,
          automationRunComplete: true,
          terminalBell: true,
          suppressWhenFocused: false,
          ...storeOverrides
        }
      })
    } as never,
    runtime as never
  )
}

describe('automation-run-complete notifications', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-08-23T04:00:00Z'))
    resetNotificationDispatchMocks()
    notificationIsSupportedMock.mockReturnValue(true)
    getAllWindowsMock.mockReturnValue([])
  })

  it('is disabled when the automationRunComplete setting is off', () => {
    register({ automationRunComplete: false })

    const result = getDispatchHandler()({}, makeRequest())

    expect(result).toEqual({ delivered: false, reason: 'source-disabled' })
    expect(notificationCtorMock).not.toHaveBeenCalled()
  })

  it('delivers a desktop notification with the automation name and outcome', () => {
    register()

    const result = getDispatchHandler()({}, makeRequest()) as { delivered: boolean }

    expect(result.delivered).toBe(true)
    expect(notificationCtorMock).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'Nightly review finished' })
    )
  })

  it('labels failed runs and surfaces the error snippet in the body', () => {
    register()

    getDispatchHandler()({},
      makeRequest({ automationOutcome: 'failed', automationError: 'git rebase conflict' })
    )

    expect(notificationCtorMock).toHaveBeenCalledWith(
      expect.objectContaining({
        title: 'Nightly review failed',
        body: 'git rebase conflict'
      })
    )
  })

  it('labels skipped runs', () => {
    register()

    getDispatchHandler()({}, makeRequest({ automationOutcome: 'skipped' }))

    expect(notificationCtorMock).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'Nightly review was skipped' })
    )
  })

  it('fans the alert out to the paired phone through the runtime', () => {
    const dispatchMobileNotification = vi.fn()
    register(undefined, { dispatchMobileNotification })

    getDispatchHandler()({}, makeRequest())

    expect(dispatchMobileNotification).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'notification',
        source: 'automation-run-complete',
        title: 'Nightly review finished'
      })
    )
  })

  it('never lights the tray attention dot for automation completions', () => {
    register()

    getDispatchHandler()({}, makeRequest())

    expect(setTrayAttentionMock).not.toHaveBeenCalled()
  })

  it('suppresses while the window is focused when suppressWhenFocused is on', () => {
    getAllWindowsMock.mockReturnValue([
      { isDestroyed: () => false, isFocused: () => true } as never
    ])
    register({ suppressWhenFocused: true })

    const result = getDispatchHandler()(
      {},
      makeRequest({ isActiveWorktree: true })
    ) as { delivered: boolean }

    expect(result.delivered).toBe(false)
    expect(notificationCtorMock).not.toHaveBeenCalled()
  })
})
