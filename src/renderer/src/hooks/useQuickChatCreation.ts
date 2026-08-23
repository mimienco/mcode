import { useCallback, useState } from 'react'
import { toast } from 'sonner'
import { useAppStore } from '@/store'
import { createBrowserUuid } from '@/lib/browser-uuid'
import { prepareQuickChatWorkspace } from '@/lib/quick-chat-create'
import { submitFolderWorkspaceCreate } from '@/components/sidebar/folder-workspace-composer-submit'
import { QUICK_CHAT_DEFAULT_NAME } from '../../../shared/quick-chat-workspaces'
import { translate } from '@/i18n/i18n'
import { isQuickChatCreationAvailable } from '@/lib/quick-chat-capability'
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
  try {
    workspaceCreated = await submitFolderWorkspaceCreate({
      projectGroup: prepared.projectGroup,
      name: QUICK_CHAT_DEFAULT_NAME,
      lastAutoName: '',
      linkedWorkItem: null,
      note: '',
      quickAgent: null,
      autoRenameBranchFromWork: false,
      agentCmdOverrides: undefined,
      terminalWindowsShell: settings?.terminalWindowsShell,
      isRemote: false,
      launchSource: 'sidebar',
      runtimeEnvironmentId: null,
      createFolderWorkspace: async (input) => {
        const workspace = await window.api.folderWorkspaces.create({
          ...input,
          folderPath: prepared.folderPath,
          kind: 'quick-chat'
        })
        useAppStore.setState((current) => ({
          folderWorkspaces: current.folderWorkspaces.some((entry) => entry.id === workspace.id)
            ? current.folderWorkspaces
            : [workspace, ...current.folderWorkspaces]
        }))
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
}

export function useQuickChatCreation(): {
  createQuickChat: () => void
  creating: boolean
  available: boolean
} {
  const [creating, setCreating] = useState(false)
  const available = isQuickChatCreationAvailable()
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
