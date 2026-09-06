// ─── Presence shim for namespaces whose Tauri port has not landed ───────────
//
// Every `window.api.<ns>` the renderer touches must EXIST under Tauri or a
// bootstrap/effect dereference crashes the React tree (the blank-window class
// of bug). Namespaces with a real typed bridge (app, ui, speech, shell,
// updater, fs) keep their clients; everything else resolves through this
// lazy presence proxy:
//  - `on[A-Z]*` members return a no-op unsubscribe (listener contract,
//    preload returns () => void); nothing emits those channels yet.
//  - every other member resolves `undefined` after ONE console.warn per
//    method. Rejecting instead floods callers that retry in effects
//    (125k console errors in 16s during the boot check); resolving
//    undefined converts the failure into one guarded-dereference error the
//    render boundary contains. `window.api.<ns> is not ported` warnings in
//    the console are the audit trail for which backends to port next.

export const TAURI_NAMESPACE_UNPORTED = 'tauri_namespace_unported'

const warnedMethods = new Set<string>()

export function createUnportedNamespace(namespace: string): Record<string, unknown> {
  const cache = new Map<string, unknown>()
  return new Proxy({} as Record<string, unknown>, {
    get(_target, method: string) {
      if (typeof method !== 'string') {
        return undefined
      }
      const hit = cache.get(method)
      if (hit) {
        return hit
      }
      const warnKey = `${namespace}.${method}`
      const member = /^on[A-Z]/.test(method)
        ? (): (() => void) => () => undefined
        : (): Promise<undefined> => {
            if (!warnedMethods.has(warnKey)) {
              warnedMethods.add(warnKey)
              console.warn(
                `[tauri-shim] ${warnKey} is not ported to Tauri yet (${TAURI_NAMESPACE_UNPORTED}); resolving undefined`
              )
            }
            return Promise.resolve(undefined)
          }
      cache.set(method, member)
      return member
    }
  })
}
