import { describe, expect, it, vi } from 'vitest'
import type { GlobalSettings } from '../../../../shared/global-settings-types'
import { buildCodexSessionSourceHomeControl } from './codex-session-source-home-control'

type CodexSourceHomeSettings = Pick<
  GlobalSettings,
  'codexSessionSourceHome' | 'localWindowsRuntimeDefault'
>

function settingsWith(
  overrides: Partial<CodexSourceHomeSettings>
): CodexSourceHomeSettings {
  return {
    codexSessionSourceHome: overrides.codexSessionSourceHome,
    localWindowsRuntimeDefault: overrides.localWindowsRuntimeDefault ?? { kind: 'windows-host' }
  }
}

// The save path runs through the serialized write queue, so payloads land one
// microtask after onSave.
async function flushWrites(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

describe('buildCodexSessionSourceHomeControl', () => {
  it('reads and writes the host override on the host runtime', async () => {
    const updateSettings = vi.fn()
    const control = buildCodexSessionSourceHomeControl(
      settingsWith({ codexSessionSourceHome: { host: '/custom/codex' } }),
      updateSettings,
      () => null
    )

    expect(control.runtimeLabel).toBe('~/.codex')
    expect(control.value).toBe('/custom/codex')

    control.onSave('/new/codex')
    await flushWrites()
    expect(updateSettings).toHaveBeenCalledWith({
      codexSessionSourceHome: { host: '/new/codex' }
    })
  })

  it('scopes to the selected WSL distro', async () => {
    const updateSettings = vi.fn()
    const control = buildCodexSessionSourceHomeControl(
      settingsWith({
        codexSessionSourceHome: { wsl: { Ubuntu: '/home/me/.codex' } },
        localWindowsRuntimeDefault: { kind: 'wsl', distro: 'Ubuntu' }
      }),
      updateSettings,
      () => null
    )

    expect(control.value).toBe('/home/me/.codex')
    control.onSave('/home/me/.config/codex')
    await flushWrites()
    expect(updateSettings).toHaveBeenCalledWith({
      codexSessionSourceHome: { wsl: { Ubuntu: '/home/me/.config/codex' } }
    })
  })

  it('reads a stored WSL value when the reported distro casing differs', () => {
    // Stored as "Ubuntu"; the runtime reports "ubuntu". The launch-time resolver
    // matches case-insensitively, so the UI must surface the active value too.
    const control = buildCodexSessionSourceHomeControl(
      settingsWith({
        codexSessionSourceHome: { wsl: { Ubuntu: '/home/me/.codex' } },
        localWindowsRuntimeDefault: { kind: 'wsl', distro: 'ubuntu' }
      }),
      vi.fn(),
      () => null
    )

    expect(control.value).toBe('/home/me/.codex')
  })

  it('updates the existing WSL key in place instead of creating a duplicate casing', async () => {
    const updateSettings = vi.fn()
    const control = buildCodexSessionSourceHomeControl(
      settingsWith({
        codexSessionSourceHome: { wsl: { Ubuntu: '/home/me/.codex' } },
        localWindowsRuntimeDefault: { kind: 'wsl', distro: 'ubuntu' }
      }),
      updateSettings,
      () => null
    )

    control.onSave('/home/me/.config/codex')
    await flushWrites()
    // No stray "ubuntu" key alongside "Ubuntu".
    expect(updateSettings).toHaveBeenCalledWith({
      codexSessionSourceHome: { wsl: { Ubuntu: '/home/me/.config/codex' } }
    })
  })

  it('clears the matching WSL key regardless of casing when reset', async () => {
    const updateSettings = vi.fn()
    const control = buildCodexSessionSourceHomeControl(
      settingsWith({
        codexSessionSourceHome: { wsl: { Ubuntu: '/home/me/.codex' } },
        localWindowsRuntimeDefault: { kind: 'wsl', distro: 'ubuntu' }
      }),
      updateSettings,
      () => null
    )

    control.onSave('')
    await flushWrites()
    // Removing the only key leaves no dangling wsl object.
    expect(updateSettings).toHaveBeenCalledWith({
      codexSessionSourceHome: { wsl: undefined }
    })
  })

  it('falls back to the host control when a WSL scope has no selected distro', () => {
    const control = buildCodexSessionSourceHomeControl(
      settingsWith({
        codexSessionSourceHome: { host: '/custom/codex' },
        localWindowsRuntimeDefault: { kind: 'wsl', distro: null }
      }),
      vi.fn(),
      () => null
    )

    expect(control.runtimeLabel).toBe('~/.codex')
    expect(control.value).toBe('/custom/codex')
  })

  it('keeps a rapid host + WSL pair of commits from clobbering each other', async () => {
    // Mimics the store: each write merges only after the persist resolves.
    let latest = settingsWith({})
    const updateSettings = vi.fn((update: Partial<GlobalSettings>) =>
      Promise.resolve().then(() => {
        latest = { ...latest, ...update }
      })
    )
    const hostControl = buildCodexSessionSourceHomeControl(
      settingsWith({}),
      updateSettings,
      () => latest
    )
    const wslControl = buildCodexSessionSourceHomeControl(
      settingsWith({ localWindowsRuntimeDefault: { kind: 'wsl', distro: 'Ubuntu' } }),
      updateSettings,
      () => latest
    )

    hostControl.onSave('/custom/codex')
    wslControl.onSave('/home/me/.codex')
    await vi.waitFor(() => expect(updateSettings).toHaveBeenCalledTimes(2))
    await flushWrites()

    expect(updateSettings).toHaveBeenNthCalledWith(1, {
      codexSessionSourceHome: { host: '/custom/codex' }
    })
    // Second write rebuilt from the reconciled state, so both fields survive.
    expect(updateSettings).toHaveBeenNthCalledWith(2, {
      codexSessionSourceHome: {
        host: '/custom/codex',
        wsl: { Ubuntu: '/home/me/.codex' }
      }
    })
    expect(latest.codexSessionSourceHome).toEqual({
      host: '/custom/codex',
      wsl: { Ubuntu: '/home/me/.codex' }
    })
  })
})
