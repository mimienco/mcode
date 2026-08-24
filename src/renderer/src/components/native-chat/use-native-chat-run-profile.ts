import { useMemo, type Dispatch, type SetStateAction } from 'react'
import type { AgentType } from '../../../../shared/agent-status-types'
import {
  applyAgentRunProfile,
  type AgentInteractionMode,
  type AgentPermissionPreset
} from '../../../../shared/agent-run-profile'
import type { CodexLiveSessionOptionsSnapshot } from '../../../../shared/codex-live-session-options'
import type { SessionOptionDescriptor } from '../../../../shared/native-chat-session-options'
import { isTuiAgent } from '../../../../shared/tui-agent-config'
import { resolveTuiAgentPermissionMode } from '../../../../shared/tui-agent-permissions'
import { useAppStore } from '../../store'
import type { NativeChatPtySessionOptionsSurface } from './native-chat-pty-session-options'
import {
  buildNativeChatRunProfileSnapshot,
  isAgentInteractionMode,
  isAgentPermissionPreset
} from './native-chat-run-profile'

export function useNativeChatRunProfile(args: {
  agent: AgentType
  targetPtyId: string | null
  codexLiveSnapshot: CodexLiveSessionOptionsSnapshot
  setCodexLiveSnapshot: Dispatch<SetStateAction<CodexLiveSessionOptionsSnapshot>>
  modelSurface: NativeChatPtySessionOptionsSurface | null
}): {
  surface: NativeChatPtySessionOptionsSurface | null
  snapshot: SessionOptionDescriptor[]
} {
  const { agent, targetPtyId, codexLiveSnapshot, setCodexLiveSnapshot, modelSurface } = args
  const settings = useAppStore((state) => state.settings)
  const derivedPermission = isTuiAgent(agent)
    ? resolveTuiAgentPermissionMode({
        agent,
        agentArgs: settings?.agentDefaultArgs?.[agent],
        agentEnv: settings?.agentDefaultEnv?.[agent]
      })
    : 'ask'
  const customLaunchOverride = derivedPermission === 'mixed'
  const permission: AgentPermissionPreset = customLaunchOverride
    ? 'ask'
    : (settings?.agentPermissionPreset ?? derivedPermission)
  const interactionMode: AgentInteractionMode = settings?.agentInteractionMode ?? 'build'
  const snapshot = useMemo(
    () =>
      buildNativeChatRunProfileSnapshot({
        agent,
        permission,
        interactionMode,
        codexLiveSnapshot,
        customLaunchOverride
      }),
    [agent, codexLiveSnapshot, customLaunchOverride, interactionMode, permission]
  )

  const surface = useMemo<NativeChatPtySessionOptionsSurface | null>(() => {
    if (!modelSurface) {
      return null
    }
    const applyRunProfile = async (update: {
      permission?: AgentPermissionPreset
      interactionMode?: AgentInteractionMode
    }): Promise<void> => {
      if (
        agent === 'codex' &&
        targetPtyId &&
        codexLiveSnapshot.available &&
        window.api?.pty?.setCodexSessionOptions
      ) {
        const result = await window.api.pty.setCodexSessionOptions(targetPtyId, {
          ...update
        })
        if (result.outcome !== 'applied') {
          throw new Error('This Codex session does not support live run-profile changes.')
        }
        setCodexLiveSnapshot((current) =>
          current.available
            ? {
                ...current,
                current: {
                  ...(current.current ?? { model: current.models[0]?.id ?? '' }),
                  ...(result.permission ? { permission: result.permission } : {}),
                  ...(result.interactionMode ? { interactionMode: result.interactionMode } : {})
                }
              }
            : current
        )
        return
      }
      const latest = useAppStore.getState().settings
      const nextPermission = update.permission ?? permission
      const nextInteractionMode = update.interactionMode ?? interactionMode
      const rewritten = applyAgentRunProfile({
        permission: nextPermission,
        interactionMode: nextInteractionMode,
        agentDefaultArgs: latest?.agentDefaultArgs,
        agentDefaultEnv: latest?.agentDefaultEnv
      })
      await useAppStore.getState().updateSettings({
        ...rewritten,
        agentPermissionPreset: nextPermission,
        agentInteractionMode: nextInteractionMode
      })
    }
    const setRunProfileOption = async (id: string, value: string) => {
      if (id === 'permission' && isAgentPermissionPreset(value)) {
        await applyRunProfile({ permission: value })
      } else if (id === 'interaction-mode' && isAgentInteractionMode(value)) {
        await applyRunProfile({ interactionMode: value })
      } else {
        throw new Error(`Unsupported run-profile option: ${id}`)
      }
      return { snapshot: [...modelSurface.getSnapshot(), ...snapshot] }
    }
    return {
      getSnapshot: () => [...modelSurface.getSnapshot(), ...snapshot],
      setOption: (id, value) =>
        id === 'permission' || id === 'interaction-mode'
          ? setRunProfileOption(id, String(value))
          : modelSurface.setOption(id, value),
      invokeAction: (id) => modelSurface.invokeAction(id),
      subscribe: modelSurface.subscribe,
      recordOutgoingCommand: modelSurface.recordOutgoingCommand,
      reportSessionOptions: modelSurface.reportSessionOptions,
      replaceModels: modelSurface.replaceModels
    }
  }, [
    agent,
    codexLiveSnapshot,
    interactionMode,
    modelSurface,
    permission,
    setCodexLiveSnapshot,
    snapshot,
    targetPtyId
  ])

  return { surface, snapshot }
}
