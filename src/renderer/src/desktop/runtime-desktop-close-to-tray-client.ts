import { isTauri } from '@tauri-apps/api/core'

type TauriCloseToTrayClient = {
  setEnabled: (enabled: boolean) => Promise<void>
}

type PersistCloseToTrayPreference = (enabled: boolean) => void | Promise<void>

type RuntimeDesktopCloseToTrayClient = {
  updatePreference: (
    enabled: boolean,
    previousEnabled: boolean,
    persist: PersistCloseToTrayPreference
  ) => Promise<void>
}

type RuntimeDesktopCloseToTrayDependencies = {
  isTauri: () => boolean
  loadTauri: () => Promise<TauriCloseToTrayClient>
}

export function createRuntimeDesktopCloseToTrayClient(
  dependencies: RuntimeDesktopCloseToTrayDependencies
): RuntimeDesktopCloseToTrayClient {
  let pendingTransaction = Promise.resolve()
  let effectiveEnabled: boolean | undefined

  const runTransaction = async (
    enabled: boolean,
    previousEnabled: boolean,
    persist: PersistCloseToTrayPreference
  ): Promise<void> => {
    await persist(enabled)
    if (!dependencies.isTauri()) {
      return
    }
    try {
      await (await dependencies.loadTauri()).setEnabled(enabled)
    } catch (applyError) {
      try {
        await persist(previousEnabled)
      } catch (rollbackError) {
        throw new AggregateError(
          [applyError, rollbackError],
          'Failed to apply and roll back the close-to-tray preference'
        )
      }
      throw applyError
    }
  }

  return {
    updatePreference(enabled, previousEnabled, persist) {
      const transaction = pendingTransaction.then(async () => {
        effectiveEnabled ??= previousEnabled
        await runTransaction(enabled, effectiveEnabled, persist)
        effectiveEnabled = enabled
      })
      pendingTransaction = transaction.catch(() => undefined)
      return transaction
    }
  }
}

export const runtimeDesktopCloseToTrayClient = createRuntimeDesktopCloseToTrayClient({
  isTauri,
  loadTauri: async () => (await import('./tauri-desktop-client')).tauriCloseToTrayClient
})
