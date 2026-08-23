import { useCallback, useState } from 'react'
import { toast } from 'sonner'
import { useAppStore } from '@/store'
import { createBrowserUuid } from '@/lib/browser-uuid'
import { prepareQuickChatWorkspace } from '@/lib/quick-chat-create'
import { submitFolderWorkspaceCreate } from '@/components/sidebar/folder-workspace-composer-submit'
import { resolveInitialNativeChatSessionOptions } from '@/components/native-chat/native-chat-launch-session-options'
import { pickTuiAgent } from '../../../shared/tui-agent-selection'
import {
  resolveTuiAgentLaunchArgs,
  resolveTuiAgentLaunchEnv
} from '../../../shared/tui-agent-launch-defaults'
import { QUICK_CHAT_DEFAULT_NAME } from '../../../shared/quick-chat-workspaces'
import { translate } from '@/i18n/i18n'
import { isNativeChatSupportedAgent } from '@/lib/native-chat-supported-agent'
import { isWebClientLocation } from '@/lib/web-client-location'
import { folderWorkspaceKey } from '../../../shared/workspace-scope'
import { FLOATING_TERMINAL_WORKTREE_ID } from '../../../shared/constants'
import { isWslUncPath } from '../../../shared/wsl-paths'

let quickChatCreationQueue: Promise<void> = Promise.resolve()

function queueQuickChatCreation(run: () => Promise<void>): Promise<void> {
  const result = quickChatCreationQueue.then(run, run)
  quickChatCreationQueue = result.catch(() => undefined)
  return result
}

async function createAndOpenQuickChat(): Promise<void> {
  const state = useAppStore.getState()
  const settings = state.settings
  const preferredAgent =
    settings?.defaultTuiAgent && settings.defaultTuiAgent !== 'blank'
      ? settings.defaultTuiAgent
      : null
  const detectedAgents = await state.ensureDetectedAgents(FLOATING_TERMINAL_WORKTREE_ID)
  const chatCapableAgents = detectedAgents.filter(isNativeChatSupportedAgent)
  const agent = pickTuiAgent(preferredAgent, chatCapableAgents, settings?.disabledTuiAgents)
  if (!agent) {
    throw new Error(
      translate(
        'auto.hooks.useQuickChatCreation.noAgent',
        'No enabled local chat-capable coding agent is available. Choose an agent in Settings, then try again.'
      )
    )
  }

  const prepared = await prepareQuickChatWorkspace({
    quickChatId: createBrowserUuid(),
    projectGroups: useAppStore.getState().projectGroups,
    getFloatingWorkspacePath: async () => {
      const configuredPath = await window.api.app.getFloatingTerminalCwd({
        path: settings?.floatingTerminalCwd,
        requireTrusted: true
      })
      return isWslUncPath(configuredPath)
        ? window.api.app.getFloatingTerminalCwd({ requireTrusted: true })
        : configuredPath
    },
    ensureDir: async (dirPath) => {
      if (!(await window.api.fs.pathExists({ filePath: dirPath }))) {
        await window.api.fs.createDir({ dirPath })
      }
    },
    createGroup: async (input) => {
      const group = await window.api.projectGroups.create(input)
      useAppStore.setState((current) => ({
        projectGroups: current.projectGroups.some((entry) => entry.id === group.id)
          ? current.projectGroups
          : [...current.projectGroups, group]
      }))
      return group
    }
  })

  let workspaceCreated = false
  let createdWorkspaceId: string | null = null
  try {
    workspaceCreated = await submitFolderWorkspaceCreate({
      projectGroup: prepared.projectGroup,
      name: QUICK_CHAT_DEFAULT_NAME,
      lastAutoName: '',
      linkedWorkItem: null,
      note: '',
      quickAgent: agent,
      autoRenameBranchFromWork: true,
      agentCmdOverrides: settings?.agentCmdOverrides,
      agentArgs: resolveTuiAgentLaunchArgs(agent, settings?.agentDefaultArgs),
      agentEnv: resolveTuiAgentLaunchEnv(agent, settings?.agentDefaultEnv),
      sessionOptions: resolveInitialNativeChatSessionOptions(
        {
          experimentalNativeChat: true,
          openAgentTabsInChatByDefault: true,
          nativeChatSessionOptions: settings?.nativeChatSessionOptions
        },
        { agent, nativeChatTranscriptIsLocalReadable: true }
      ),
      terminalWindowsShell: settings?.terminalWindowsShell,
      isRemote: false,
      launchSource: 'sidebar',
      runtimeEnvironmentId: null,
      createFolderWorkspace: async (input) => {
        const workspace = await window.api.folderWorkspaces.create({
          ...input,
          folderPath: prepared.folderPath,
          kind: 'quick-chat',
          pendingFirstAgentMessageRename: true
        })
        useAppStore.setState((current) => ({
          folderWorkspaces: current.folderWorkspaces.some((entry) => entry.id === workspace.id)
            ? current.folderWorkspaces
            : [workspace, ...current.folderWorkspaces]
        }))
        createdWorkspaceId = workspace.id
        return workspace
      },
      onOpenChange: () => undefined
    })
  } finally {
    if (!workspaceCreated) {
      await window.api.fs
        .deletePath({ targetPath: prepared.folderPath, recursive: true })
        .catch(() => undefined)
    }
  }
  if (!workspaceCreated) {
    throw new Error(
      translate(
        'auto.hooks.useQuickChatCreation.createFailed',
        'The no-project chat could not be created.'
      )
    )
  }
  if (createdWorkspaceId) {
    const latestState = useAppStore.getState()
    const activeTabId = latestState.activeTabIdByWorktree[folderWorkspaceKey(createdWorkspaceId)]
    if (activeTabId) {
      latestState.setTabViewMode(activeTabId, 'chat')
    }
  }
}

export function useQuickChatCreation(): {
  createQuickChat: () => void
  creating: boolean
  available: boolean
} {
  const [creating, setCreating] = useState(false)
  const available = !isWebClientLocation()
  const createQuickChat = useCallback(() => {
    if (creating || !available) {
      return
    }
    setCreating(true)
    void queueQuickChatCreation(createAndOpenQuickChat)
      .catch((error) => {
        toast.error(
          translate('auto.hooks.useQuickChatCreation.failedTitle', 'Could not start a new chat'),
          { description: error instanceof Error ? error.message : String(error) }
        )
      })
      .finally(() => setCreating(false))
  }, [available, creating])
  return { createQuickChat, creating, available }
}
