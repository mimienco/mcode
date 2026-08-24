import type { AgentInteractionMode, AgentPermissionPreset } from '../../shared/agent-run-profile'
import type {
  CodexLiveSessionModel,
  CodexLiveSessionOptionsUpdate
} from '../../shared/codex-live-session-options'
import {
  buildCodexRunProfileSettingsUpdate,
  filterCodexPermissionProfilesByRequirements,
  parseCodexCollaborationModes,
  parseCodexPermissionProfiles,
  type CodexCollaborationModePreset
} from './codex-live-session-controls'
import { parseCodexLiveModels } from './codex-live-session-planning'

type CatalogRpc = {
  request(method: string, params: Record<string, unknown>): Promise<unknown>
}

export type CodexLiveSessionCatalog = {
  models: CodexLiveSessionModel[]
  permissionProfiles: AgentPermissionPreset[]
  collaborationModes: CodexCollaborationModePreset[]
}

export async function loadCodexLiveSessionCatalog(
  rpc: CatalogRpc,
  cwd?: string
): Promise<CodexLiveSessionCatalog> {
  const models: CodexLiveSessionModel[] = []
  let cursor: string | null = null
  do {
    const response = await rpc.request('model/list', {
      limit: 100,
      includeHidden: false,
      ...(cursor ? { cursor } : {})
    })
    models.push(...parseCodexLiveModels(response))
    cursor =
      response &&
      typeof response === 'object' &&
      typeof (response as { nextCursor?: unknown }).nextCursor === 'string'
        ? (response as { nextCursor: string }).nextCursor || null
        : null
  } while (cursor)
  if (models.length === 0) {
    throw new Error('Codex App Server returned no selectable models.')
  }
  const [permissionResponse, collaborationResponse, requirementsResponse] = await Promise.all([
    rpc
      .request('permissionProfile/list', { limit: 100, ...(cwd ? { cwd } : {}) })
      .catch(() => null),
    rpc.request('collaborationMode/list', {}).catch(() => null),
    rpc.request('configRequirements/read', {}).catch(() => null)
  ])
  return {
    models,
    permissionProfiles: filterCodexPermissionProfilesByRequirements(
      parseCodexPermissionProfiles(permissionResponse),
      requirementsResponse
    ),
    collaborationModes: parseCodexCollaborationModes(collaborationResponse)
  }
}

export function codexRunProfileControls(catalog: CodexLiveSessionCatalog): {
  permissions: AgentPermissionPreset[]
  interactionModes: AgentInteractionMode[]
} {
  return {
    permissions: [...catalog.permissionProfiles],
    interactionModes: catalog.collaborationModes.map((mode) => mode.interactionMode)
  }
}

export function catalogSupportsCodexRunProfile(
  catalog: CodexLiveSessionCatalog,
  update: { permission?: AgentPermissionPreset; interactionMode?: AgentInteractionMode }
): boolean {
  return (
    (!update.permission || catalog.permissionProfiles.includes(update.permission)) &&
    (!update.interactionMode ||
      catalog.collaborationModes.some((mode) => mode.interactionMode === update.interactionMode))
  )
}

export function prepareCodexLiveSessionUpdate(args: {
  catalog: CodexLiveSessionCatalog
  update: CodexLiveSessionOptionsUpdate
  current?: { model: string; effort?: string } | null
}): { currentModel: string; effort?: string; runProfileUpdate: Record<string, unknown> } | null {
  if (!catalogSupportsCodexRunProfile(args.catalog, args.update)) {
    return null
  }
  const selectedModel = args.update.model
    ? args.catalog.models.find((model) => model.id === args.update.model)
    : undefined
  if (args.update.model && !selectedModel) {
    return null
  }
  const effort = args.update.effort ?? selectedModel?.defaultEffort
  if (
    selectedModel &&
    effort &&
    selectedModel.efforts.length > 0 &&
    !selectedModel.efforts.some((entry) => entry.id === effort)
  ) {
    return null
  }
  const currentModel =
    args.update.model ?? args.current?.model ?? selectedModel?.id ?? args.catalog.models[0]?.id
  if (!currentModel) {
    return null
  }
  const runProfileUpdate = buildCodexRunProfileSettingsUpdate({
    permission: args.update.permission,
    interactionMode: args.update.interactionMode,
    currentModel,
    currentEffort: effort ?? args.current?.effort,
    collaborationModes: args.catalog.collaborationModes
  })
  if ((args.update.permission || args.update.interactionMode) && !runProfileUpdate) {
    return null
  }
  return { currentModel, ...(effort ? { effort } : {}), runProfileUpdate: runProfileUpdate ?? {} }
}
