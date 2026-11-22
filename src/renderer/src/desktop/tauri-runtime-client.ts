import { z } from 'zod'
import type { RuntimeApi } from '../../../preload/api/runtime-api'
import type { RuntimeRpcFailure } from '../../../shared/runtime-rpc-envelope'
import {
  KnownRuntimeEnvironmentSchema,
  PublicRuntimeAccessEndpointSchema
} from '../../../shared/runtime-environments'
import { RuntimeRpcCallError } from '../runtime/runtime-rpc-result'
import { createDesktopInvoke, type DesktopInvoke } from './desktop-client'

const PublicEnvironmentList = z.array(
  KnownRuntimeEnvironmentSchema.omit({ endpoints: true }).extend({
    endpoints: z.array(PublicRuntimeAccessEndpointSchema).min(1)
  })
)

function unsupportedLocalMethod(method: string): RuntimeRpcFailure {
  return {
    id: 'desktop-ipc',
    ok: false,
    error: { code: 'method_not_found', message: `Unknown method: ${method}` },
    _meta: { runtimeId: null }
  }
}

function remoteUnavailable(): RuntimeRpcFailure {
  return {
    id: 'desktop-ipc',
    ok: false,
    error: {
      code: 'runtime_unavailable',
      message: 'Remote runtime transport is not yet available in Tauri.'
    },
    _meta: { runtimeId: null }
  }
}

async function rejectLocal(method: string): Promise<never> {
  throw new RuntimeRpcCallError(unsupportedLocalMethod(method))
}

async function rejectRemote(): Promise<never> {
  throw new RuntimeRpcCallError(remoteUnavailable())
}

const inertListener = (): (() => void) => () => undefined

// Unsupported RPCs must reach the renderer's existing method/capability guards.
export function createTauriRuntimeClient(): RuntimeApi['runtime'] {
  return {
    call: async ({ method }) => unsupportedLocalMethod(method),
    getStatus: () => rejectLocal('status.get'),
    syncWindowGraph: () => rejectLocal('runtime.syncWindowGraph'),
    getTerminalFitOverrides: () => rejectLocal('runtime.getTerminalFitOverrides'),
    getTerminalDrivers: () => rejectLocal('runtime.getTerminalDrivers'),
    getBrowserDrivers: () => rejectLocal('runtime.getBrowserDrivers'),
    restoreTerminalFit: () => rejectLocal('runtime.restoreTerminalFit'),
    reclaimBrowserForDesktop: () => rejectLocal('runtime.reclaimBrowserForDesktop'),
    onTerminalFitOverrideChanged: inertListener,
    onTerminalDriverChanged: inertListener,
    onBrowserDriverChanged: inertListener
  }
}

export function createTauriRuntimeEnvironmentsClient(
  invoke: DesktopInvoke
): RuntimeApi['runtimeEnvironments'] {
  const call = createDesktopInvoke(invoke)
  return {
    list: async () => {
      const environments = PublicEnvironmentList.parse(await call('runtime_environments_list'))
      return environments.sort((left, right) => left.name.localeCompare(right.name))
    },
    call: async () => remoteUnavailable(),
    connect: async () => remoteUnavailable(),
    getStatus: async () => remoteUnavailable(),
    addFromPairingCode: rejectRemote,
    verifyAndAddFromPairingCode: rejectRemote,
    resolve: rejectRemote,
    remove: rejectRemote,
    disconnect: rejectRemote,
    subscribe: rejectRemote
  }
}
