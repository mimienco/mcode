import type { WorkspaceSessionApi } from '../../../preload/api/workspace-session-api'
import type { WorkspaceSessionState } from '../../../shared/workspace-session-state-types'
import { createDesktopInvoke, DesktopCommandError, type DesktopInvoke } from './desktop-client'

type SessionApi = WorkspaceSessionApi['session']

function unsupportedSync(operation: string): never {
  throw new DesktopCommandError(
    'session_sync_unsupported',
    { operation },
    `${operation} is unavailable on the asynchronous Tauri bridge`,
    'This operation needs the asynchronous desktop session bridge.'
  )
}

function isSession(value: unknown): value is WorkspaceSessionState {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return false
  }
  const fields = value as Record<string, unknown>
  return (
    ['activeRepoId', 'activeWorktreeId', 'activeTabId'].every(
      (key) => fields[key] === null || typeof fields[key] === 'string'
    ) &&
    ['tabsByWorktree', 'terminalLayoutsByTabId'].every(
      (key) => !!fields[key] && typeof fields[key] === 'object' && !Array.isArray(fields[key])
    )
  )
}

export function createTauriSessionClient(invoke: DesktopInvoke): SessionApi {
  const call = createDesktopInvoke(invoke)
  let pending: Promise<void> = Promise.resolve()
  const failures: unknown[] = []
  const scrollback = new Map<string, string | null>()
  const hostRefs = new Map<string, Set<string>>()
  const hostReadGenerations = new Map<string, number>()

  async function prefetchScrollback(
    session: WorkspaceSessionState,
    hostId: string,
    generation: number
  ): Promise<void> {
    const refs = new Set<string>()
    const fetched = new Map<string, string | null>()
    for (const layout of Object.values(session.terminalLayoutsByTabId)) {
      for (const ref of Object.values(layout.scrollbackRefsByLeafId ?? {})) {
        if (/^v1-[0-9a-f]{32}$/.test(ref)) {
          refs.add(ref)
        }
      }
    }
    const references = [...refs]
    // Bound concurrent disk/IPC work; sync pane hydration runs only after get resolves.
    for (let index = 0; index < references.length; index += 4) {
      await Promise.all(references.slice(index, index + 4).map(async (reference) => {
        try {
          const value = await call('session_read_terminal_scrollback', { reference })
          fetched.set(reference, typeof value === 'string' ? value : null)
        } catch (error) {
          fetched.set(reference, null)
          console.warn('[session] Could not prefetch terminal scrollback', error)
        }
      }))
    }
    // A slower earlier snapshot must not evict a newer host's restored buffers.
    if (hostReadGenerations.get(hostId) !== generation) {
      return
    }
    hostRefs.set(hostId, refs)
    for (const [ref, payload] of fetched) {
      scrollback.set(ref, payload)
    }
    const retained = new Set([...hostRefs.values()].flatMap((entries) => [...entries]))
    for (const ref of scrollback.keys()) {
      if (!retained.has(ref)) {
        scrollback.delete(ref)
      }
    }
  }

  function enqueue(write: () => Promise<unknown>, retainFailure = true): Promise<void> {
    const result = pending.then(write).then(() => undefined)
    pending = result.catch((error: unknown) => {
      if (retainFailure && failures.length === 0) {
        failures.push(error)
      }
    })
    return result
  }

  return {
    async get(hostId) {
      const hostKey = hostId ?? 'local'
      const generation = (hostReadGenerations.get(hostKey) ?? 0) + 1
      hostReadGenerations.set(hostKey, generation)
      await pending
      const result = await call('session_get', { hostId: hostId ?? null })
      if (!isSession(result)) {
        throw new DesktopCommandError(
          'session_invalid_response',
          { operation: 'session_get' },
          'session_get returned an invalid session',
          'MCode could not restore this session. Try again.',
          result
        )
      }
      await prefetchScrollback(result, hostKey, generation)
      return result
    },
    set: (args, hostId) => enqueue(() => call('session_set', { args, hostId: hostId ?? null })),
    patch: (args, hostId) => enqueue(() => call('session_patch', { args, hostId: hostId ?? null })),
    flush: () =>
      enqueue(async () => {
        if (failures.length > 0) {
          const first = failures[0]
          failures.length = 0
          throw first
        }
        await call('session_flush')
      }, false),
    // Tauri's async-close-guard stages the versioned checkpoint before permitting shutdown.
    setSync: () => unsupportedSync('session_set_sync'),
    readTerminalScrollback: ({ ref }) => scrollback.get(ref) ?? null
  }
}
