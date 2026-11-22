import { describe, expect, it } from 'vitest'
import { getDefaultSettings } from '../../../shared/constants'
import { resolveTauriSettings } from './tauri-settings-defaults'
import {
  createSettingsClient,
  createSettingsDocumentBridge
} from './tauri-settings-document-client'
import type { GlobalSettings } from '../../../shared/global-settings-types'

describe('Tauri persisted settings defaults', () => {
  it('restores the full shared settings contract for a fresh profile', () => {
    expect(resolveTauriSettings({}, '/home/fixture')).toEqual(getDefaultSettings('/home/fixture'))
  })

  it('fills nested missing defaults while preserving stored user choices', () => {
    const result = resolveTauriSettings(
      {
        settings: {
          workspaceDir: '/selected',
          theme: 'light',
          notifications: { enabled: false },
          workspaceDirHistory: []
        }
      },
      '/home/fixture'
    )
    expect(result.workspaceDir).toBe('/selected')
    expect(result.theme).toBe('light')
    expect(result.notifications).toEqual({
      ...getDefaultSettings('/home/fixture').notifications,
      enabled: false
    })
    expect(result.workspaceDirHistory).toEqual([])
  })

  it('restores required nested objects instead of exposing null to the sidebar', () => {
    expect(
      resolveTauriSettings({ settings: { notifications: null } }, '/home/fixture').notifications
    ).toEqual(getDefaultSettings('/home/fixture').notifications)
  })

  it('does not let consumers mutate the defaults used by later reads', () => {
    const first = resolveTauriSettings({}, '/home/fixture')
    first.notifications.enabled = false
    expect(resolveTauriSettings({}, '/home/fixture').notifications.enabled).toBe(true)
  })

  it('normalizes async, cached and changed snapshots through the actual settings client', async () => {
    let document: Record<string, unknown> = {}
    const bridge = createSettingsDocumentBridge(async (command, args) => {
      if (command === 'settings_set_document') {
        const patch = args?.patch as { settings: Record<string, unknown> }
        document = { settings: { ...(document.settings as object), ...patch.settings } }
      }
      return document
    })
    const client = createSettingsClient(bridge, async () => '/home/fixture') as {
      get: () => Promise<GlobalSettings>
      getSync: () => GlobalSettings | null
      set: (updates: Partial<GlobalSettings>) => Promise<GlobalSettings>
      onChanged: (listener: (settings: GlobalSettings) => void) => () => void
    }
    expect(client.getSync()).toBeNull()
    expect(await client.get()).toEqual(getDefaultSettings('/home/fixture'))
    expect(client.getSync()?.notifications.enabled).toBe(true)
    const changes: GlobalSettings[] = []
    const dispose = client.onChanged((value) => changes.push(value))
    const result = await client.set({ theme: 'dark' })
    await Promise.resolve()
    expect(result.theme).toBe('dark')
    expect(result.notifications.enabled).toBe(true)
    expect(changes.at(-1)?.notifications.enabled).toBe(true)
    expect(document).toEqual({ settings: { theme: 'dark' } })
    dispose()
  })
})
